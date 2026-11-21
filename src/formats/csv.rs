//! CSV and friends (semicolon, tab, pipe delimited).
//!
//! Field content is preserved as written (RFC 4180: spaces are part of the
//! field); only control characters are cleaned. Encoding is detected from
//! the BOM (UTF-8, UTF-16LE/BE), then UTF-8, then Mozilla's `chardetng`
//! (Windows-1251, KOI8-R, Windows-1252 and the other legacy encodings a
//! spreadsheet exports in), whose fallback is Windows-1252. The
//! delimiter is chosen by trial-parsing candidates and scoring record
//! consistency, so delimiters inside quoted fields don't skew the choice. The
//! format marks no header row, so the shape of the data decides whether the
//! first record is one.

use crate::error::ConvertError;
use crate::model::{Block, Cell, Document, Inline, Table, TableKind};
use crate::shared::header::resolve_header_rows;
use crate::shared::text::clean_text;
use csv::ReaderBuilder;
use std::borrow::Cow;

pub fn parse(bytes: &[u8]) -> Result<Document, ConvertError> {
    let text = decode(bytes);
    let delimiter = sniff_delimiter(&text);

    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.as_bytes());

    let mut rows: Vec<Vec<Cell>> = Vec::new();
    for record in reader.records() {
        let record = match record {
            Ok(r) => r,
            Err(e) => {
                log::warn!("skipping unreadable csv record: {e}");
                continue;
            }
        };
        let cells: Vec<Cell> =
            record.iter().map(|f| Cell::from_inlines(vec![Inline::plain(clean_text(f))])).collect();
        rows.push(cells);
    }

    let mut doc = Document::default();
    let mut table = Table::from_rows(rows, 0, TableKind::Data);
    table.header_rows = resolve_header_rows(&table, 0);
    if !table.grid.is_empty() {
        doc.blocks.push(Block::Table(table));
    }
    Ok(doc)
}

fn decode(bytes: &[u8]) -> Cow<'_, str> {
    match bytes {
        [0xFF, 0xFE, ..] => {
            let (s, _, _) = encoding_rs::UTF_16LE.decode(bytes);
            Cow::Owned(s.into_owned())
        }
        [0xFE, 0xFF, ..] => {
            let (s, _, _) = encoding_rs::UTF_16BE.decode(bytes);
            Cow::Owned(s.into_owned())
        }
        _ => {
            let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
            match std::str::from_utf8(bytes) {
                Ok(s) => Cow::Borrowed(s),
                Err(_) => {
                    let (s, _) = guess_legacy(bytes).decode_without_bom_handling(bytes);
                    Cow::Owned(s.into_owned())
                }
            }
        }
    }
}

/// How much of a non-UTF-8 input the detector reads. A spreadsheet export
/// repeats its encoding's letters from the first rows on, so a prefix decides
/// as well as the whole file and keeps detection cheap on a large one.
const DETECT_PREFIX: usize = 1 << 20;

/// The Cyrillic encodings `chardetng` can answer: KOI8-U is its KOI8 family
/// (the same letters as KOI8-R), IBM866 the DOS code page.
const CYRILLIC: [&encoding_rs::Encoding; 4] =
    [encoding_rs::WINDOWS_1251, encoding_rs::KOI8_U, encoding_rs::ISO_8859_5, encoding_rs::IBM866];

/// The legacy encoding of input that isn't UTF-8 and has no BOM: a Cyrillic
/// encoding when `chardetng` finds one, else Windows-1252, the encoding this
/// format assumed before it had a detector. Its other answers aren't taken:
/// on a short Western CSV whose only non-ASCII is a symbol or two it guesses
/// Central European, Baltic or Big5 (`£8` becomes `Ł8`, `naïve` `naīve`).
fn guess_legacy(bytes: &[u8]) -> &'static encoding_rs::Encoding {
    use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
    let prefix = &bytes[..bytes.len().min(DETECT_PREFIX)];
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    detector.feed(prefix, prefix.len() == bytes.len());
    let guess = detector.guess(None, Utf8Detection::Deny);
    if CYRILLIC.contains(&guess) { guess } else { encoding_rs::WINDOWS_1252 }
}

/// Trial-parse each candidate over the leading records and score by field
/// consistency; comma wins ties.
fn sniff_delimiter(text: &str) -> u8 {
    const CANDIDATES: [u8; 4] = *b",;\t|";
    let mut best = (b',', 0u32);
    for d in CANDIDATES {
        let mut reader = ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .delimiter(d)
            .from_reader(text.as_bytes());
        // Sample complete records (the reader is streaming, so this reads
        // only as much input as the records span): sampling physical lines
        // would cut a quoted multiline field in half.
        let counts: Vec<usize> =
            reader.records().map_while(Result::ok).take(20).map(|r| r.len()).collect();
        if counts.is_empty() {
            continue;
        }
        // Modal field count and how dominant it is.
        let mut tally: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
        for &c in &counts {
            *tally.entry(c).or_default() += 1;
        }
        // Frequency ties break toward the wider record shape so the choice
        // never depends on hash-map iteration order.
        let (&modal, &freq) = tally.iter().max_by_key(|&(&count, &f)| (f, count)).unwrap();
        if modal < 2 {
            continue; // a delimiter that never splits carries no signal
        }
        // Consistency first, then wider records break the tie.
        let score = freq * 1000 + modal.min(500) as u32;
        if score > best.1 {
            best = (d, score);
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_fields_keep_padding() {
        let doc = parse(b"a,b\n\"  padded  \",x\n").unwrap();
        let Block::Table(t) = &doc.blocks[0] else { panic!() };
        let crate::model::CellSlot::Origin(cell) = &t.grid[1][0] else { panic!() };
        let Block::Paragraph(inlines) = &cell.blocks[0] else { panic!() };
        let crate::model::Inline::Text { text, .. } = &inlines[0] else { panic!() };
        assert_eq!(text, "  padded  ");
    }

    #[test]
    fn quoted_delimiters_do_not_skew_sniffing() {
        // Semicolon-delimited with commas inside quoted fields.
        assert_eq!(sniff_delimiter("a;b;c\n\"1,5\";\"2,5\";x\n\"3,0\";y;z\n"), b';');
        assert_eq!(sniff_delimiter("a,b,c\n1,2,3\n"), b',');
        assert_eq!(sniff_delimiter("a\tb\n1\t2\n"), b'\t');
    }

    #[test]
    fn multiline_quoted_field_does_not_break_sniffing() {
        // A quoted field spanning more physical lines than the old 20-line
        // sample window; the record-based sample must still see semicolons.
        let long_field = format!("\"{}\"", vec!["line"; 30].join("\n"));
        let text = format!("a;b;c\n{long_field};2;3\nx;y;z\n");
        assert_eq!(sniff_delimiter(&text), b';');
    }

    #[test]
    fn utf16_bom_decodes() {
        let mut bytes = vec![0xFF, 0xFE];
        for u in "x,y\ncafé,90\n".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let doc = parse(&bytes).unwrap();
        assert!(!doc.blocks.is_empty());
    }

    /// Encode `text` in a single-byte legacy encoding, refusing unmappable
    /// characters so a sample can't silently lose its letters.
    fn legacy(text: &str, encoding: &'static encoding_rs::Encoding) -> Vec<u8> {
        let (bytes, _, unmappable) = encoding.encode(text);
        assert!(!unmappable, "{text:?} doesn't fit {}", encoding.name());
        bytes.into_owned()
    }

    #[test]
    fn cyrillic_legacy_encodings_decode() {
        let samples = [
            "ФИО;Город\nСмирнова Анна Викторовна;Москва\n",
            "Фамилия,Имя\nЖуравлёв,Пётр\nЩербакова,Юлия\n",
            "Наименование;Количество;Цена\nКарандаш чёрный;12;35,50\n",
        ];
        for text in samples {
            for encoding in [encoding_rs::WINDOWS_1251, encoding_rs::KOI8_R, encoding_rs::IBM866] {
                assert_eq!(decode(&legacy(text, encoding)), text, "{}", encoding.name());
            }
        }
    }

    /// What decoded as Windows-1252 before the detector must still: short
    /// Western European CSVs with only a few accented letters among ASCII.
    #[test]
    fn western_european_stays_windows_1252() {
        let samples = [
            "x,y\ncafé,90\n",
            "col1,col2\nnaïve,café\n",
            "Name,City\nJürgen Müller,München\n",
            "nom;ville\nFrançois Lefèvre;Besançon\n",
            "nombre,ciudad\nIñaki Peña,Logroño\n",
            "nome,cidade\nJoão Gonçalves,São Paulo\n",
            "nome,città\nNiccolò Bianchi,Forlì\n",
            "navn,by\nSøren Ærø,Århus\n",
            "naam,plaats\nGeërt Brouwer,Den Haag\n",
            "item,price\nWidget,€12.50\nGadget,£8\n",
            "id,note\n1,plain ascii\n2,one ° sign\n",
            "Größe;Maß\nÜberprüfung der Prüfgröße;3\nÄußerst wichtige Änderung;4\n",
            "titre,résumé\nL'été à la mer,Un récit où l'on découvre la côte\n",
            // Alone, each of these makes chardetng guess a non-Western
            // encoding (Central European, Baltic, Turkish or Big5).
            "£8",
            "®",
            "±5",
            "µg",
            "¼",
            "¾",
            "¡Hola!",
            "«guillemets»",
            "naïve",
            "Ø",
            "Æ",
            "Œuvre",
            "¥100",
            "¦",
            "¬",
            "¯",
            "³",
            "¶",
            "¹",
            "Ý",
        ];
        for text in samples {
            assert_eq!(decode(&legacy(text, encoding_rs::WINDOWS_1252)), text);
        }
    }

    #[test]
    fn detection_reads_a_prefix_of_a_large_input() {
        let row = "Кудрявцев Олег Петрович;Санкт-Петербург;инженер\n";
        let text = row.repeat(DETECT_PREFIX / row.len() * 2);
        let bytes = legacy(&text, encoding_rs::WINDOWS_1251);
        assert!(bytes.len() > DETECT_PREFIX);
        assert_eq!(decode(&bytes), text);
    }

    #[test]
    fn utf8_and_boms_are_unchanged() {
        let text = "ФИО,Город\nБелова,Тверь\n";
        assert!(matches!(decode(text.as_bytes()), Cow::Borrowed(s) if s == text));
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(text.as_bytes());
        assert_eq!(decode(&bom), text);
    }
}
