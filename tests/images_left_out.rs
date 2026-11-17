//! `Conversion::images_left_out`: the images whose content the Markdown
//! cannot carry.

use anydoc::{Options, to_markdown_with};
use std::path::Path;

fn images_left_out(name: &str) -> u32 {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx").join(name);
    to_markdown_with(&path, Options::default()).unwrap().images_left_out
}

#[test]
fn counts_embedded_pictures_and_objects_but_not_text_or_charts() {
    // A picture; its text box and VML shape render as text.
    assert_eq!(images_left_out("text.docx"), 1);
    // A picture and an embedded workbook; the chart renders as a table.
    assert_eq!(images_left_out("handmade-rich.docx"), 2);
    // An embedded workbook behind its VML preview: one object.
    assert_eq!(images_left_out("handmade-ole.docx"), 1);
    // Every placement of one image part counts.
    assert_eq!(images_left_out("handmade-manyrefs.docx"), 70);
    assert_eq!(images_left_out("handmade-tables.docx"), 0);
}

/// A package from `(name, bytes)` parts, stored uncompressed.
fn package(parts: &[(String, Vec<u8>)]) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in parts {
        zip.start_file(name.as_str(), options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn docx_images_left_out(bytes: &[u8]) -> u32 {
    anydoc::to_markdown_bytes_with(bytes, anydoc::Format::Docx, Options::default())
        .unwrap()
        .images_left_out
}

#[test]
fn a_picture_whose_part_is_missing_still_counts() {
    // `handmade-manyrefs.docx` without its one image part: the 70
    // placements have no bytes and no alt text.
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx/handmade-manyrefs.docx");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let name = entry.name().unwrap().into_owned();
        if name.starts_with("word/media/") {
            continue;
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
        parts.push((name, bytes));
    }
    assert_eq!(docx_images_left_out(&package(&parts)), 70);
}

#[test]
fn a_picture_in_a_code_styled_paragraph_still_counts() {
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
    let part = |name: &str, xml: String| (name.to_owned(), xml.into_bytes());
    let parts = [
        part(
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.into(),
        ),
        part(
            "_rels/.rels",
            format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#),
        ),
        part(
            "word/_rels/document.xml.rels",
            format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/styles" Target="styles.xml"/><Relationship Id="rId2" Type="{R}/image" Target="media/a.png"/></Relationships>"#),
        ),
        part(
            "word/styles.xml",
            format!(r#"<w:styles xmlns:w="{W}"><w:style w:type="paragraph" w:styleId="SourceCode"><w:name w:val="Source Code"/></w:style></w:styles>"#),
        ),
        part(
            "word/document.xml",
            format!(r#"<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body><w:p><w:pPr><w:pStyle w:val="SourceCode"/></w:pPr><w:r><w:t>let x = 1;</w:t></w:r><w:r><w:drawing><a:blip xmlns:a="{A}" r:embed="rId2"/></w:drawing></w:r></w:p></w:body></w:document>"#),
        ),
        ("word/media/a.png".to_owned(), b"\x89PNG\r\n\x1a\n".to_vec()),
    ];
    let bytes = package(&parts);
    let converted =
        anydoc::to_markdown_bytes_with(&bytes, anydoc::Format::Docx, Options::default()).unwrap();
    assert!(converted.markdown.contains("```"), "{}", converted.markdown);
    assert_eq!(docx_images_left_out(&bytes), 1);
}
