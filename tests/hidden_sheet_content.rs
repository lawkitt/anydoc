//! A spreadsheet's hidden sheets, rows and columns never reach the Markdown,
//! and `Conversion::hidden_sheets_left_out`, `hidden_rows_left_out` and
//! `hidden_columns_left_out` count what was left out, the same in XLSX,
//! XLSB, XLS and ODS.

use anydoc::{Conversion, Format, Options, to_markdown_bytes_with, to_markdown_with};
use std::io::{Cursor, Write};
use std::path::Path;

fn fixture(path: &str) -> Conversion {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(path);
    to_markdown_with(&path, Options::default()).unwrap()
}

fn counts(converted: &Conversion) -> (u32, u32, u32) {
    (
        converted.hidden_sheets_left_out,
        converted.hidden_rows_left_out,
        converted.hidden_columns_left_out,
    )
}

#[test]
fn fixtures_leave_hidden_content_out_and_count_it() {
    // tests/fixture-src/hidden.fods, through LibreOffice for ODS, XLSX and
    // XLS, and `hidden_xlsb` in tests/gen_fixtures.py: one hidden sheet,
    // one hidden row and one hidden column, every hidden name spelled
    // HIDDEN-*.
    for path in
        ["ods/hidden.ods", "xlsx/hidden.xlsx", "xlsb/handmade-hidden.xlsb", "xls/hidden.xls"]
    {
        let converted = fixture(path);
        let md = &converted.markdown;
        assert!(!md.contains("HIDDEN"), "{path}: {md}");
        for shown in ["Client", "Ada Quillfeather", "Cora Penhallow", "Will update"] {
            assert!(md.contains(shown), "{path}: {shown} missing from {md}");
        }
        assert_eq!(counts(&converted), (1, 1, 1), "{path}");
    }
}

#[test]
fn workbooks_without_hidden_content_count_none() {
    for path in ["ods/sheet.ods", "xlsx/sheet.xlsx", "xls/sheet.xls", "docx/text.docx"] {
        assert_eq!(counts(&fixture(path)), (0, 0, 0), "{path}");
    }
}

fn ods(automatic_styles: &str, tables: &str) -> Conversion {
    let content = format!(
        r#"<?xml version="1.0"?><office:document-content
        xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
        xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
        xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
        xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
        xmlns:dc="http://purl.org/dc/elements/1.1/">
        <office:automatic-styles>{automatic_styles}</office:automatic-styles>
        <office:body><office:spreadsheet>{tables}</office:spreadsheet></office:body>
        </office:document-content>"#
    );
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for (name, body) in [
        ("mimetype", "application/vnd.oasis.opendocument.spreadsheet"),
        ("content.xml", content.as_str()),
    ] {
        zip.start_file(name, opts).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    let bytes = zip.finish().unwrap().into_inner();
    to_markdown_bytes_with(&bytes, Format::Ods, Options::default()).unwrap()
}

fn cell(text: &str) -> String {
    format!(
        r#"<table:table-cell office:value-type="string"><text:p>{text}</text:p></table:table-cell>"#
    )
}

fn row(cells: &[&str]) -> String {
    format!(
        "<table:table-row>{}</table:table-row>",
        cells.iter().map(|c| cell(c)).collect::<String>()
    )
}

#[test]
fn ods_rows_hidden_by_a_filter_are_left_out_and_counted_per_repeat() {
    let table = format!(
        r#"<table:table table:name="S">{}<table:table-row table:visibility="filter" table:number-rows-repeated="3">{}</table:table-row>{}</table:table>"#,
        row(&["Name", "Role"]),
        cell("HIDDEN-Filtered") + &cell("x"),
        row(&["Wren Ashby", "Clerk"]),
    );
    let converted = ods("", &table);
    assert!(!converted.markdown.contains("HIDDEN"), "{}", converted.markdown);
    assert!(converted.markdown.contains("Wren Ashby"), "{}", converted.markdown);
    assert_eq!(counts(&converted), (0, 3, 0));
}

#[test]
fn ods_empty_hidden_rows_and_columns_are_not_counted() {
    // Every column past the data hidden at once, and a hidden empty row.
    let table = format!(
        r#"<table:table table:name="S"><table:table-column table:number-columns-repeated="2"/><table:table-column table:visibility="collapse" table:number-columns-repeated="16382"/>{}<table:table-row table:visibility="collapse"><table:table-cell table:number-columns-repeated="1024"/></table:table-row>{}</table:table>"#,
        row(&["Name", "Role"]),
        row(&["Wren Ashby", "Clerk"]),
    );
    let converted = ods("", &table);
    assert_eq!(counts(&converted), (0, 0, 0));
    assert!(converted.markdown.contains("Wren Ashby"), "{}", converted.markdown);
}

#[test]
fn ods_a_value_in_a_repeated_hidden_column_run_counts_each_column() {
    let table = r#"<table:table table:name="S"><table:table-column/><table:table-column table:visibility="collapse" table:number-columns-repeated="3"/><table:table-column/><table:table-row><table:table-cell office:value-type="string"><text:p>Wren Ashby</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="7" table:number-columns-repeated="2"><text:p>7</text:p></table:table-cell><table:table-cell/><table:table-cell office:value-type="string"><text:p>Clerk</text:p></table:table-cell></table:table-row></table:table>"#;
    let converted = ods("", table);
    assert_eq!(counts(&converted), (0, 0, 2));
    assert!(converted.markdown.contains("| Wren Ashby | Clerk |"), "{}", converted.markdown);
}

#[test]
fn ods_spans_shrink_over_hidden_rows_and_columns() {
    // A1:C3 merged, with column B and row 2 hidden: the merge spans the two
    // rows and two columns that remain.
    let table = r#"<table:table table:name="S"><table:table-column/><table:table-column table:visibility="collapse"/><table:table-column/><table:table-column/><table:table-row><table:table-cell table:number-columns-spanned="3" table:number-rows-spanned="3" office:value-type="string"><text:p>Merged</text:p></table:table-cell><table:covered-table-cell table:number-columns-repeated="2"/><table:table-cell office:value-type="string"><text:p>Edge</text:p></table:table-cell></table:table-row><table:table-row table:visibility="collapse"><table:covered-table-cell table:number-columns-repeated="3"/><table:table-cell/></table:table-row><table:table-row><table:covered-table-cell table:number-columns-repeated="3"/><table:table-cell office:value-type="string"><text:p>Last</text:p></table:table-cell></table:table-row></table:table>"#;
    let converted = ods("", table);
    // Covered cells are not values: nothing hidden is counted.
    assert_eq!(counts(&converted), (0, 0, 0));
    // Two visible rows of three visible columns (A, C, D): the merge covers
    // A and C of both rows.
    let body: Vec<&str> = converted.markdown.lines().skip(2).collect();
    assert_eq!(body, ["| Merged |  | Edge |", "|  |  | Last |"], "{}", converted.markdown);
}

#[test]
fn ods_hidden_cells_are_never_parsed_and_comments_are_not_values() {
    // A hidden row holding only a comment holds no value.
    let table = format!(
        r#"<table:table table:name="S">{}<table:table-row table:visibility="collapse"><table:table-cell><office:annotation><dc:creator>A</dc:creator><text:p>HIDDEN-Note</text:p></office:annotation></table:table-cell></table:table-row>{}</table:table>"#,
        row(&["Name"]),
        row(&["Wren Ashby"]),
    );
    let converted = ods("", &table);
    assert_eq!(counts(&converted), (0, 0, 0));
    assert!(!converted.markdown.contains("HIDDEN"), "{}", converted.markdown);
}

#[test]
fn ods_hidden_sheets_are_counted_whether_or_not_they_hold_anything() {
    let styles = r#"<style:style style:name="ta1" style:family="table"><style:table-properties table:display="false"/></style:style>"#;
    let tables = format!(
        r#"<table:table table:name="Shown">{}</table:table><table:table table:name="Gone" table:style-name="ta1">{}</table:table><table:table table:name="Empty" table:style-name="ta1"/>"#,
        row(&["Wren Ashby"]),
        row(&["HIDDEN-Dorian Vale"]),
    );
    let converted = ods(styles, &tables);
    assert_eq!(counts(&converted), (2, 0, 0));
    // One sheet left visible, so no sheet heading either.
    assert!(!converted.markdown.contains('#'), "{}", converted.markdown);
    assert!(!converted.markdown.contains("HIDDEN"), "{}", converted.markdown);
}
