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
