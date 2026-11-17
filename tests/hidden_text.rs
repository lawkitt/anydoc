//! Hidden-formatted text (DOCX `w:vanish`, RTF `\v`, binary DOC
//! `sprmCFVanish`) never reaches the Markdown, and
//! `Conversion::hidden_text_left_out` counts the stretches left out.

use anydoc::{Conversion, Format, Options, to_markdown_bytes_with, to_markdown_with};
use std::io::{Cursor, Write};
use std::path::Path;

fn fixture(path: &str) -> Conversion {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(path);
    to_markdown_with(&path, Options::default()).unwrap()
}

fn convert(bytes: &[u8], format: Format) -> Conversion {
    to_markdown_bytes_with(bytes, format, Options::default()).unwrap()
}

#[test]
fn fixtures_leave_hidden_text_out_and_count_it() {
    // See `hidden_docx` and `hidden_rtf` in tests/gen_fixtures.py: every
    // hidden word is spelled SECRET-*, and the comments there add up the
    // stretches.
    for (path, stretches) in [
        ("docx/handmade-hidden.docx", 7),
        ("rtf/handmade-hidden.rtf", 6),
        ("doc/handmade-hidden.doc", 3),
    ] {
        let converted = fixture(path);
        assert!(!converted.markdown.contains("SECRET"), "{path}: {}", converted.markdown);
        assert!(converted.markdown.contains("Shown in a hidden paragraph."), "{path}");
        assert_eq!(converted.hidden_text_left_out, stretches, "{path}");
    }
}

#[test]
fn formats_and_documents_without_hidden_text_count_none() {
    for path in ["docx/text.docx", "rtf/text.rtf", "doc/text.doc", "odt/text.odt"] {
        assert_eq!(fixture(path).hidden_text_left_out, 0, "{path}");
    }
}

/// A package from `(name, bytes)` parts, stored uncompressed.
fn package(parts: &[(&str, String)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, xml) in parts {
        zip.start_file(*name, options).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// A Word package with `body`, `styles` and `footnotes` (when not empty).
fn docx(body: &str, styles: &str, footnotes: &str) -> Vec<u8> {
    docx_numbered(body, styles, footnotes, "")
}

/// [`docx`] with a numbering part.
fn docx_numbered(body: &str, styles: &str, footnotes: &str, numbering: &str) -> Vec<u8> {
    let mut parts = vec![
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_owned(),
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_owned(),
        ),
        (
            "word/_rels/document.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/" TargetMode="External"/><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#.to_owned(),
        ),
        (
            "word/document.xml",
            format!(
                r#"<w:document xmlns:w="{W}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><w:body>{body}</w:body></w:document>"#
            ),
        ),
        ("word/styles.xml", format!(r#"<w:styles xmlns:w="{W}">{styles}</w:styles>"#)),
    ];
    if !footnotes.is_empty() {
        parts.push((
            "word/footnotes.xml",
            format!(r#"<w:footnotes xmlns:w="{W}">{footnotes}</w:footnotes>"#),
        ));
    }
    if !numbering.is_empty() {
        parts.push((
            "word/numbering.xml",
            format!(r#"<w:numbering xmlns:w="{W}">{numbering}</w:numbering>"#),
        ));
    }
    package(&parts)
}

fn run(text: &str, rpr: &str) -> String {
    format!(r#"<w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

#[test]
fn docx_doc_defaults_can_hide_everything_but_runs_turned_back_on() {
    let styles =
        "<w:docDefaults><w:rPrDefault><w:rPr><w:vanish/></w:rPr></w:rPrDefault></w:docDefaults>";
    let body = format!(
        "<w:p>{}{}</w:p><w:p>{}</w:p>",
        run("SECRET-ONE ", ""),
        run("shown", r#"<w:vanish w:val="off"/>"#),
        run("SECRET-TWO", ""),
    );
    let converted = convert(&docx(&body, styles, ""), Format::Docx);
    assert_eq!(converted.markdown.trim(), "shown");
    assert_eq!(converted.hidden_text_left_out, 2);
}

#[test]
fn docx_hidden_text_in_links_fields_and_notes_is_left_out() {
    let hidden = "<w:vanish/>";
    let body = format!(
        concat!(
            r#"<w:p><w:hyperlink r:id="rId3">{}{}</w:hyperlink></w:p>"#,
            r#"<w:p><w:fldSimple w:instr=" DATE ">{}</w:fldSimple>{}</w:p>"#,
            r#"<w:p>{}<w:r><w:rPr><w:vanish/></w:rPr><w:footnoteReference w:id="1"/></w:r></w:p>"#,
        ),
        run("link ", ""),
        run("SECRET-ONE", hidden),
        run("SECRET-TWO", hidden),
        run("date", ""),
        run("Noted", ""),
    );
    let footnotes = format!(
        r#"<w:footnote w:id="1"><w:p>{}{}</w:p></w:footnote>"#,
        run("Note ", ""),
        run("SECRET-THREE", hidden)
    );
    let converted = convert(&docx(&body, "", &footnotes), Format::Docx);
    assert!(!converted.markdown.contains("SECRET"), "{}", converted.markdown);
    // A hidden note reference keeps its note: only text is left out.
    assert!(converted.markdown.contains("[^"), "{}", converted.markdown);
    assert!(converted.markdown.contains("Note"), "{}", converted.markdown);
    assert_eq!(converted.hidden_text_left_out, 3);
}

#[test]
fn rtf_hidden_text_is_left_out_and_counted() {
    for (src, expected, stretches) in [
        (r"{\rtf1 Keep {\v SECRET }this.\par}", "Keep this.", 1),
        (r"{\rtf1 Keep \v SECRET\v0 this.\par}", "Keep this.", 1),
        (r"{\rtf1 {\v SECRET }{\b\v SECRET }Keep\par}", "Keep", 1),
        (r"{\rtf1 {\v SECRET }Keep{\v SECRET}\par}", "Keep", 2),
        // A hidden paragraph's text is gone, so is the paragraph.
        (r"{\rtf1 {\v SECRET\par}Keep\par}", "Keep", 1),
        (r"{\rtf1 Keep{\v \tab }\par}", "Keep", 0),
    ] {
        let converted = convert(src.as_bytes(), Format::Rtf);
        assert_eq!(converted.markdown.trim(), expected, "{src}");
        assert_eq!(converted.hidden_text_left_out, stretches, "{src}");
    }
}

/// A Word 97 document of one UTF-16 piece and CHPX run per segment, each
/// segment with its CHPX grpprl and piece `Prm`.
fn doc(runs: &[(&str, &[u8], u16)]) -> Vec<u8> {
    fn u16_at(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn u32_at(bytes: &mut [u8], offset: usize, value: usize) {
        bytes[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes());
    }
    let mut word = vec![0; 2048];
    u16_at(&mut word, 0, 0xA5EC);
    u16_at(&mut word, 2, 0xC1);
    u16_at(&mut word, 6, 0x0409);
    u16_at(&mut word, 0x0A, 4); // fComplex
    u16_at(&mut word, 0x20, 14); // csw
    u16_at(&mut word, 0x3E, 22); // cslw
    u16_at(&mut word, 0x98, 93); // cbRgFcLcb
    let n = runs.len();
    let mut plc = vec![0; 4 + n * 12];
    let mut fkp = vec![0; 512];
    let mut blob = ((n + 1) * 4 + n + 1) & !1;
    let mut cp = 0;
    let mut fc = 1024;
    for (i, (text, chpx, prm)) in runs.iter().enumerate() {
        u32_at(&mut plc, i * 4, cp);
        u32_at(&mut plc, (n + 1) * 4 + i * 8 + 2, fc);
        u16_at(&mut plc, (n + 1) * 4 + i * 8 + 6, *prm);
        u32_at(&mut fkp, i * 4, fc);
        if !chpx.is_empty() {
            fkp[(n + 1) * 4 + i] = (blob / 2) as u8;
            fkp[blob] = chpx.len() as u8;
            fkp[blob + 1..blob + 1 + chpx.len()].copy_from_slice(chpx);
            blob = (blob + chpx.len() + 2) & !1;
        }
        for unit in text.encode_utf16() {
            u16_at(&mut word, fc, unit);
            fc += 2;
            cp += 1;
        }
    }
    assert!(fc <= 1536 && blob < 511);
    u32_at(&mut plc, n * 4, cp);
    u32_at(&mut fkp, n * 4, fc);
    fkp[511] = n as u8;
    word[1536..2048].copy_from_slice(&fkp);
    u32_at(&mut word, 0x18, 1024);
    u32_at(&mut word, 0x1C, fc);
    u32_at(&mut word, 0x4C, cp);
    let mut table = vec![2];
    table.extend_from_slice(&(plc.len() as u32).to_le_bytes());
    table.extend_from_slice(&plc);
    u32_at(&mut word, 0x1A2, 0);
    u32_at(&mut word, 0x1A6, table.len());
    u32_at(&mut word, 0xFA, table.len());
    u32_at(&mut word, 0xFE, 12);
    for value in [1024u32, fc as u32, 3] {
        table.extend_from_slice(&value.to_le_bytes());
    }
    let mut ole = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    ole.create_stream("WordDocument").unwrap().write_all(&word).unwrap();
    ole.create_stream("0Table").unwrap().write_all(&table).unwrap();
    ole.into_inner().into_inner()
}

/// sprmCFVanish (0x083C) with a toggle operand.
const VANISH: [u8; 3] = [0x3C, 0x08, 0x01];
const NOT_VANISH: [u8; 3] = [0x3C, 0x08, 0x00];
/// Operand 0x81: the style's value inverted (no style hides, so hidden).
const VANISH_INVERTED: [u8; 3] = [0x3C, 0x08, 0x81];
/// A compressed piece `Prm0`: isprm 0x5C (sprmCFVanish), operand 1.
const PRM0_VANISH: u16 = (0x5C << 1) | (1 << 8);

#[test]
fn doc_hidden_text_is_left_out_and_counted() {
    for (runs, expected, stretches) in [
        (
            vec![("Keep ", &[][..], 0), ("SECRET ", &VANISH[..], 0), ("this.\r", &[][..], 0)],
            "Keep this.",
            1,
        ),
        (
            vec![
                ("Keep ", &[][..], 0),
                ("SECRET ", &VANISH_INVERTED[..], 0),
                ("this.\r", &[][..], 0),
            ],
            "Keep this.",
            1,
        ),
        // The piece Prm applies over the CHPX.
        (
            vec![("Keep ", &[][..], 0), ("SECRET ", &[][..], PRM0_VANISH), ("this.\r", &[][..], 0)],
            "Keep this.",
            1,
        ),
        (vec![("Keep", &NOT_VANISH[..], 0), ("\r", &[][..], 0)], "Keep", 0),
        // Two stretches around visible text; hidden whitespace is not counted.
        (
            vec![
                ("SECRET", &VANISH[..], 0),
                ("Keep", &[][..], 0),
                (" ", &VANISH[..], 0),
                ("it", &[][..], 0),
                ("SECRET\r", &VANISH[..], 0),
            ],
            "Keepit",
            2,
        ),
    ] {
        let converted = convert(&doc(&runs), Format::Doc);
        assert_eq!(converted.markdown.trim(), expected, "{runs:?}");
        assert_eq!(converted.hidden_text_left_out, stretches, "{runs:?}");
    }
}

/// A hyperlink whose whole label is hidden is hidden too: with an empty
/// label the renderer would show its URL.
#[test]
fn docx_a_wholly_hidden_hyperlink_leaves_no_url() {
    let body = format!(
        r#"<w:p>{}<w:hyperlink r:id="rId3">{}</w:hyperlink>{}</w:p>"#,
        run("Before ", ""),
        run("SECRET", "<w:vanish/>"),
        run("after.", ""),
    );
    let converted = convert(&docx(&body, "", ""), Format::Docx);
    assert_eq!(converted.markdown.trim(), "Before after.");
    assert_eq!(converted.hidden_text_left_out, 1);
}

/// Math runs take `w:vanish` like text runs.
#[test]
fn docx_hidden_math_runs_are_left_out() {
    let math_run =
        |text: &str, rpr: &str| format!(r#"<m:r><w:rPr>{rpr}</w:rPr><m:t>{text}</m:t></m:r>"#);
    let body = format!(
        concat!(
            "<w:p>{}<m:oMath>{}</m:oMath>{}</w:p>",
            "<w:p>{}<m:oMath>{}{}</m:oMath></w:p>",
            "<w:p><m:oMathPara><m:oMath>{}</m:oMath></m:oMathPara></w:p>",
        ),
        run("Hidden ", ""),
        math_run("y", "<w:vanish/>"),
        run("equation.", ""),
        run("Mixed ", ""),
        math_run("x", ""),
        math_run("z", "<w:vanish/>"),
        math_run("w", "<w:vanish/>"),
    );
    let converted = convert(&docx(&body, "", ""), Format::Docx);
    assert_eq!(converted.markdown.trim(), "Hidden equation.\n\nMixed $x$");
    assert_eq!(converted.hidden_text_left_out, 3);
}

/// A paragraph that names no style takes the default paragraph style,
/// which can hide its runs.
#[test]
fn docx_the_default_paragraph_style_can_hide_text() {
    let styles = concat!(
        r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:vanish/></w:rPr></w:style>"#,
        r#"<w:style w:type="paragraph" w:styleId="Shown"><w:name w:val="Shown"/></w:style>"#,
    );
    let body = format!(
        r#"<w:p>{}</w:p><w:p><w:pPr><w:pStyle w:val="Shown"/></w:pPr>{}</w:p>"#,
        run("SECRET", ""),
        run("Shown.", ""),
    );
    let converted = convert(&docx(&body, styles, ""), Format::Docx);
    assert_eq!(converted.markdown.trim(), "Shown.");
    assert_eq!(converted.hidden_text_left_out, 1);
}

#[test]
fn rtf_hidden_math_runs_are_left_out() {
    for (src, expected, stretches) in [
        (r"{\rtf1 Hidden {\v{\mmath{\*\moMath{\mr y}}}}equation.\par}", "Hidden equation.", 1),
        (r"{\rtf1 Mixed {\mmath{\*\moMath{\mr x}{\mr\v z}}}\par}", "Mixed $x$", 1),
    ] {
        let converted = convert(src.as_bytes(), Format::Rtf);
        assert_eq!(converted.markdown.trim(), expected, "{src}");
        assert_eq!(converted.hidden_text_left_out, stretches, "{src}");
    }
}

/// A hidden field instruction is still read, so a visible result keeps its
/// link: instructions are never displayed.
#[test]
fn doc_a_hidden_field_instruction_keeps_its_visible_link() {
    let runs = vec![
        ("\u{13}", &[][..], 0),
        (" HYPERLINK \"https://example.com/\" ", &VANISH[..], 0),
        ("\u{14}", &[][..], 0),
        ("Example", &[][..], 0),
        ("\u{15}\r", &[][..], 0),
    ];
    let converted = convert(&doc(&runs), Format::Doc);
    assert_eq!(converted.markdown.trim(), "[Example](https://example.com/)");
    assert_eq!(converted.hidden_text_left_out, 0);
}

/// An equation can sit directly in the body, outside any paragraph.
#[test]
fn docx_hidden_runs_in_body_level_equations_are_left_out() {
    let math_run =
        |text: &str, rpr: &str| format!(r#"<m:r><w:rPr>{rpr}</w:rPr><m:t>{text}</m:t></m:r>"#);
    let body = format!(
        "{}<m:oMath>{}{}</m:oMath><m:oMathPara><m:oMath>{}</m:oMath></m:oMathPara>",
        format_args!("<w:p>{}</w:p>", run("Text.", "")),
        math_run("x", ""),
        math_run("SECRET", "<w:vanish/>"),
        math_run("SECRET", "<w:vanish/>"),
    );
    let converted = convert(&docx(&body, "", ""), Format::Docx);
    assert!(!converted.markdown.contains("SECRET"), "{}", converted.markdown);
    assert!(converted.markdown.contains('x'), "{}", converted.markdown);
    assert_eq!(converted.hidden_text_left_out, 2);
}

/// A list item whose text is all hidden disappears with its marker.
#[test]
fn docx_a_wholly_hidden_list_item_leaves_no_marker() {
    let numbering = concat!(
        r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum>"#,
        r#"<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#,
    );
    let item = |text: &str, rpr: &str| {
        format!(
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>{}</w:p>"#,
            run(text, rpr)
        )
    };
    let body = [item("One", ""), item("SECRET", "<w:vanish/>"), item("Three", "")].concat();
    let converted = convert(&docx_numbered(&body, "", "", numbering), Format::Docx);
    assert!(!converted.markdown.contains("SECRET"), "{}", converted.markdown);
    let items: Vec<&str> =
        converted.markdown.lines().filter(|line| !line.trim().is_empty()).collect();
    assert_eq!(items.len(), 2, "{}", converted.markdown);
    assert!(items[0].ends_with("One") && items[1].ends_with("Three"), "{items:?}");
    assert_eq!(converted.hidden_text_left_out, 1);
}

#[test]
fn rtf_math_runs_follow_their_own_deleted_and_hidden_state() {
    for (src, expected, stretches) in [
        // A deleted run inside an equation is dropped, not counted.
        (r"{\rtf1 {\mmath{\*\moMath{\mr x}{\mr\deleted SECRET}}}\par}", "$x$", 0),
        // A run shown again with \v0 in a hidden zone stays.
        (r"{\rtf1 {\v{\mmath{\*\moMath{\mr SECRET}{\mr\v0 x}}}}\par}", "$x$", 1),
    ] {
        let converted = convert(src.as_bytes(), Format::Rtf);
        assert_eq!(converted.markdown.trim(), expected, "{src}");
        assert_eq!(converted.hidden_text_left_out, stretches, "{src}");
    }
}
