//! A CSV in a legacy encoding without a BOM converts to the same Markdown as
//! its UTF-8 twin (fixtures from `gen_fixtures.py`'s `csvs`).

use std::path::Path;

fn convert(name: &str) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/csv");
    anydoc::to_markdown(dir.join(name)).unwrap()
}

#[test]
fn russian_legacy_encodings_match_utf8() {
    let utf8 = convert("russian-utf8.csv");
    assert!(utf8.contains("Белозёрова Анна Викторовна"), "{utf8}");
    for name in ["russian-utf8-bom.csv", "russian-windows-1251.csv", "russian-koi8-r.csv"] {
        assert_eq!(convert(name), utf8, "{name}");
    }
}

#[test]
fn western_windows_1252_matches_utf8() {
    let utf8 = convert("western-utf8.csv");
    assert!(utf8.contains("Jürgen Weißmüller"), "{utf8}");
    assert_eq!(convert("western-windows-1252.csv"), utf8);
}
