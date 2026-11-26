//! A DOCX table cell deleted as a tracked change (`w:cellDel`) is left
//! empty in its place, its text gone (lawkitt/mdoc#395); an inserted cell
//! (`w:cellIns`) stays. A note referenced only from deleted content (a
//! deleted cell, `w:del`, `w:moveFrom`) goes with it.

use anydoc::{Format, Options, to_markdown_bytes_with};
use std::io::{Cursor, Write};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const DELETED: &str = r#"<w:cellDel w:id="1" w:author="Reviewer" w:date="2026-11-27T00:00:00Z"/>"#;
const NOTE: &str =
    r#"<w:footnote w:id="1"><w:p><w:r><w:t>Ask Marisol Quenby.</w:t></w:r></w:p></w:footnote>"#;

/// A Word package whose body is `body`, with `footnotes` when not empty,
/// stored uncompressed.
fn docx(body: &str, footnotes: &str) -> Vec<u8> {
    let mut parts = vec![
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
    if !footnotes.is_empty() {
        parts.push((
            "word/_rels/document.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/></Relationships>"#.to_owned(),
        ));
        parts.push((
            "word/footnotes.xml",
            format!(r#"<w:footnotes xmlns:w="{W}">{footnotes}</w:footnotes>"#),
        ));
    }
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, xml) in parts {
        zip.start_file(name, options).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// A cell holding `content` (paragraph content), with `tcpr` in its properties.
fn cell_with(content: &str, tcpr: &str) -> String {
    format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/>{tcpr}</w:tcPr><w:p>{content}</w:p></w:tc>"#
    )
}

/// A cell holding `text`, with `tcpr` in its properties.
fn cell(text: &str, tcpr: &str) -> String {
    cell_with(&format!("<w:r><w:t>{text}</w:t></w:r>"), tcpr)
}

fn markdown(body: &str, footnotes: &str) -> String {
    to_markdown_bytes_with(&docx(body, footnotes), Format::Docx, Options::default())
        .unwrap()
        .markdown
}

#[test]
fn a_deleted_cell_keeps_its_place_without_its_text() {
    let body = format!(
        "<w:tbl><w:tblGrid><w:gridCol/><w:gridCol/><w:gridCol/></w:tblGrid>\
         <w:tr>{}{}{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl>",
        cell("Party", ""),
        cell("Guarantor", DELETED),
        cell("Role", ""),
        cell("Landlord Ltd", ""),
        cell("Marisol Quenby", DELETED),
        cell("Owner", ""),
    );
    let markdown = markdown(&body, "");
    assert!(!markdown.contains("Quenby"), "{markdown}");
    assert!(!markdown.contains("Guarantor"), "{markdown}");
    // Empty in its place: the cells after it keep their columns.
    assert!(markdown.contains("| Party |  | Role |"), "{markdown}");
    assert!(markdown.contains("| Landlord Ltd |  | Owner |"), "{markdown}");
}

#[test]
fn a_deleted_horizontal_merge_origin_takes_its_continuation_with_it() {
    let body = format!(
        "<w:tbl><w:tblGrid><w:gridCol/><w:gridCol/><w:gridCol/><w:gridCol/></w:tblGrid><w:tr>{}{}{}{}</w:tr></w:tbl>",
        cell("Plain", ""),
        cell("Marisol", &format!(r#"<w:hMerge w:val="restart"/>{DELETED}"#)),
        cell("Quenby", "<w:hMerge/>"),
        cell("Last", ""),
    );
    let markdown = markdown(&body, "");
    assert!(!markdown.contains("Marisol") && !markdown.contains("Quenby"), "{markdown}");
    assert!(
        markdown.contains("| Plain |"),
        "the continuation stays out of its neighbour: {markdown}"
    );
    assert!(markdown.contains("Last"), "{markdown}");
}

#[test]
fn an_inserted_cell_stays() {
    let inserted = r#"<w:cellIns w:id="2" w:author="Reviewer" w:date="2026-11-27T00:00:00Z"/>"#;
    let body = format!(
        "<w:tbl><w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>",
        cell("Party", ""),
        cell("Landlord Ltd", inserted),
    );
    assert!(markdown(&body, "").contains("Landlord Ltd"));
}

#[test]
fn a_note_referenced_only_from_deleted_content_goes_with_it() {
    let reference = r#"<w:r><w:footnoteReference w:id="1"/></w:r>"#;
    let in_cell = format!(
        "<w:tbl><w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>",
        cell("Party", ""),
        cell_with(&format!("<w:r><w:t>Guarantor</w:t></w:r>{reference}"), DELETED),
    );
    let deleted = |tag: &str| {
        format!(
            r#"<w:p><w:r><w:t>Kept.</w:t></w:r><w:{tag} w:id="9" w:author="Reviewer">{reference}</w:{tag}></w:p>"#
        )
    };
    for body in [in_cell, deleted("del"), deleted("moveFrom")] {
        let markdown = markdown(&body, NOTE);
        assert!(!markdown.contains("Quenby"), "{body}: {markdown}");
    }
    // Referenced from kept content too, the note stays.
    let both = format!("{}<w:p><w:r><w:t>See</w:t></w:r>{reference}</w:p>", deleted("del"));
    assert!(markdown(&both, NOTE).contains("Ask Marisol Quenby."));
}
