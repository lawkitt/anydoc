//! A DOCX table cell deleted as a tracked change (`w:cellDel`) is left
//! out with its text, as accepting the change leaves the row
//! (lawkitt/mdoc#395); an inserted cell (`w:cellIns`) stays.

use anydoc::{Format, Options, to_markdown_bytes_with};
use std::io::{Cursor, Write};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// A Word package whose body is `body`, stored uncompressed.
fn docx(body: &str) -> Vec<u8> {
    let parts = [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_owned(),
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_owned(),
        ),
        ("word/document.xml", format!(r#"<w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#)),
    ];
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, xml) in parts {
        zip.start_file(name, options).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// A cell holding `text`, with `tcpr` in its properties.
fn cell(text: &str, tcpr: &str) -> String {
    format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/>{tcpr}</w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>"#
    )
}

fn markdown(body: &str) -> String {
    to_markdown_bytes_with(&docx(body), Format::Docx, Options::default()).unwrap().markdown
}

#[test]
fn a_deleted_cell_is_left_out_with_its_text() {
    let deleted = r#"<w:cellDel w:id="1" w:author="Reviewer" w:date="2026-11-27T00:00:00Z"/>"#;
    let body = format!(
        "<w:tbl><w:tblGrid><w:gridCol/><w:gridCol/><w:gridCol/></w:tblGrid>\
         <w:tr>{}{}{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl>",
        cell("Party", ""),
        cell("Guarantor", deleted),
        cell("Role", ""),
        cell("Landlord Ltd", ""),
        cell("Marisol Quenby", deleted),
        cell("Owner", ""),
    );
    let markdown = markdown(&body);
    assert!(!markdown.contains("Quenby"), "{markdown}");
    assert!(!markdown.contains("Guarantor"), "{markdown}");
    for kept in ["Party", "Role", "Landlord Ltd", "Owner"] {
        assert!(markdown.contains(kept), "{kept}: {markdown}");
    }
    // The cells after it move left, as accepting the deletion does.
    assert!(markdown.contains("| Landlord Ltd | Owner |"), "{markdown}");
}

#[test]
fn an_inserted_cell_stays() {
    let inserted = r#"<w:cellIns w:id="2" w:author="Reviewer" w:date="2026-11-27T00:00:00Z"/>"#;
    let body = format!(
        "<w:tbl><w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>",
        cell("Party", ""),
        cell("Landlord Ltd", inserted),
    );
    assert!(markdown(&body).contains("Landlord Ltd"));
}
