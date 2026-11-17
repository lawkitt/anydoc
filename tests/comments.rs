//! Comments are reported beside the Markdown as structured data
//! (`Conversion::comments`) and never enter it; formats that only count
//! them report `Conversion::comments_left_out`.

mod common;

use anydoc::model::Comment;
use anydoc::{Conversion, Format, Options, to_markdown_bytes_with, to_markdown_with};
use common::{fixture_root, walk};
use std::io::{Cursor, Write};

fn fixture(path: &str) -> Conversion {
    to_markdown_with(fixture_root().join(path), Options::default()).unwrap()
}

fn convert(bytes: &[u8], format: Format) -> Conversion {
    to_markdown_bytes_with(bytes, format, Options::default()).unwrap()
}

/// A comment as `(id, author, date, text, anchor_text, parent_id, resolved)`.
type Row<'a> = (&'a str, &'a str, &'a str, &'a str, &'a str, Option<&'a str>, bool);

fn rows(comments: &[Comment]) -> Vec<Row<'_>> {
    comments
        .iter()
        .map(|c| {
            (
                c.id.as_str(),
                c.author.as_str(),
                c.date.as_str(),
                c.text.as_str(),
                c.anchor_text.as_str(),
                c.parent_id.as_deref(),
                c.resolved,
            )
        })
        .collect()
}

/// Text that only the comments of the handmade-comments fixtures carry.
const COMMENT_ONLY: &[&str] = &[
    "Check this",
    "Agreed",
    "First line",
    "A point comment.",
    "Fixed, resolving",
    "Проверьте",
    "Alice",
    "Иван",
    "SECRET",
];

fn assert_no_comment_text(path: &str, markdown: &str) {
    for text in COMMENT_ONLY {
        assert!(!markdown.contains(text), "{path}: {text:?} leaked into\n{markdown}");
    }
}

#[test]
fn docx_fixture_reports_its_comments() {
    let converted = fixture("docx/handmade-comments.docx");
    assert_no_comment_text("docx", &converted.markdown);
    let date = "2026-11-18T09:30:00Z";
    assert_eq!(
        rows(&converted.comments),
        [
            ("0", "Alice Reviewer", date, "Check this range.", "commented words", None, false),
            (
                "1",
                "Alice Reviewer",
                date,
                "First line.\nSecond\tline.",
                "spans two\nparagraphs",
                None,
                false
            ),
            ("2", "Carol", date, "A point comment.", "A point comment sits here.", None, false),
            (
                "3",
                "Bob Builder",
                "2026-11-18T10:15:00Z",
                "Agreed.",
                "commented words",
                Some("0"),
                false
            ),
            ("4", "Bob Builder", date, "Fixed, resolving.", "resolved words", None, true),
            ("5", "Alice Reviewer", date, "", "empty note", None, false),
            ("6", "Иван Петров", date, "Проверьте формулировку.", "договор поставки", None, false),
        ]
    );
    assert_eq!(converted.comments_left_out, 7);
}

#[test]
fn odt_fixture_reports_its_comments() {
    let converted = fixture("odt/handmade-comments.odt");
    assert_no_comment_text("odt", &converted.markdown);
    let date = "2026-11-18T09:30:00";
    assert_eq!(
        rows(&converted.comments),
        [
            (
                "__Annotation__1",
                "Alice Reviewer",
                date,
                "Check this range.",
                "commented words",
                None,
                false
            ),
            (
                "__Annotation__4",
                "Bob Builder",
                "2026-11-18T10:15:00",
                "Agreed.",
                "commented words",
                Some("__Annotation__1"),
                false
            ),
            (
                "__Annotation__2",
                "Alice Reviewer",
                date,
                "First line.\nSecond\tline.",
                "spans two\nparagraphs",
                None,
                false
            ),
            (
                "annotation-4",
                "Carol",
                date,
                "A point comment.",
                "A point comment sits here.",
                None,
                false
            ),
            (
                "__Annotation__5",
                "Bob Builder",
                date,
                "Fixed, resolving.",
                "resolved words",
                None,
                true
            ),
            ("__Annotation__6", "Alice Reviewer", "", "", "empty note", None, false),
            (
                "__Annotation__7",
                "Иван Петров",
                date,
                "Проверьте формулировку.",
                "договор поставки",
                None,
                false
            ),
        ]
    );
    assert_eq!(converted.comments_left_out, 7);
}

#[test]
fn rtf_fixture_reports_its_comments() {
    let converted = fixture("rtf/handmade-comments.rtf");
    assert_no_comment_text("rtf", &converted.markdown);
    let date = "2026-11-18T09:30";
    assert_eq!(
        rows(&converted.comments),
        [
            ("1", "Alice Reviewer", date, "Check this range.", "commented words", None, false),
            (
                "4",
                "Bob Builder",
                "2026-11-18T10:15",
                "Agreed.",
                "commented words",
                Some("1"),
                false
            ),
            (
                "2",
                "Alice Reviewer",
                date,
                "First line.\nSecond\tline.",
                "spans two\nparagraphs",
                None,
                false
            ),
            (
                "annotation-4",
                "Carol",
                date,
                "A point comment.",
                "A point comment sits here.",
                None,
                false
            ),
            // RTF has no resolved state.
            ("5", "Bob Builder", date, "Fixed, resolving.", "resolved words", None, false),
            ("6", "Alice Reviewer", "", "", "empty note", None, false),
            ("7", "Иван Петров", date, "Проверьте формулировку.", "договор поставки", None, false),
        ]
    );
    assert_eq!(converted.comments_left_out, 7);
}

#[test]
fn counted_formats_report_how_many_comments_they_left_out() {
    for (path, count) in [("pptx/handmade-comments.pptx", 4), ("xlsx/handmade-comments.xlsx", 3)] {
        let converted = fixture(path);
        assert_no_comment_text(path, &converted.markdown);
        assert!(converted.comments.is_empty(), "{path}");
        assert_eq!(converted.comments_left_out, count, "{path}");
    }
}

/// Every other fixture carries no comments, and its Markdown is the one the
/// plain API returns (the snapshots pin that Markdown byte for byte).
#[test]
fn other_fixtures_report_no_comments() {
    let root = fixture_root();
    let mut files = Vec::new();
    walk(&root, &mut files);
    for path in files {
        let rel = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        let stem = path.file_stem().unwrap().to_string_lossy();
        if rel.starts_with("abuse") || stem == "handmade-comments" {
            continue;
        }
        if cfg!(not(feature = "pdf")) && rel.starts_with("pdf") {
            continue;
        }
        let Ok(converted) = to_markdown_with(&path, Options::default()) else { continue };
        assert!(converted.comments.is_empty(), "{rel}: {:?}", converted.comments);
        assert_eq!(converted.comments_left_out, 0, "{rel}");
        assert_eq!(anydoc::to_markdown(&path).unwrap(), converted.markdown, "{rel}");
    }
}

// ---------------------------------------------------------------------------
// DOCX edge cases, in-test packages.

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
const W14: &str = r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#;
const W15: &str = r#"xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml""#;
const COMMENTS_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const EXTENDED_REL: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";

fn package(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, xml) in parts {
        zip.start_file(*name, options).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn rels(entries: &[(&str, &str)]) -> String {
    let mut out = String::from(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    for (i, (kind, target)) in entries.iter().enumerate() {
        out.push_str(&format!(r#"<Relationship Id="rId{i}" Type="{kind}" Target="{target}"/>"#));
    }
    out + "</Relationships>"
}

const ROOT_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

fn document(body: &str) -> String {
    format!("<w:document {W}><w:body>{body}</w:body></w:document>")
}

/// A ranged comment on "anchored" with id `id`.
fn ranged(id: &str) -> String {
    format!(
        r#"<w:p><w:r><w:t xml:space="preserve">Body </w:t></w:r><w:commentRangeStart w:id="{id}"/><w:r><w:t>anchored</w:t></w:r><w:commentRangeEnd w:id="{id}"/><w:r><w:commentReference w:id="{id}"/></w:r></w:p>"#
    )
}

fn comment(id: &str, para_id: &str, text: &str) -> String {
    format!(
        r#"<w:comment w:id="{id}" w:author="A"><w:p w14:paraId="{para_id}"><w:r><w:t>{text}</w:t></w:r></w:p></w:comment>"#
    )
}

fn comments_part(comments: &str) -> String {
    format!("<w:comments {W} {W14}>{comments}</w:comments>")
}

#[test]
fn docx_comment_parts_match_case_insensitively_and_fall_back_to_their_names() {
    let body = document(&ranged("1"));
    let comments = comments_part(&comment("1", "AA", "Note"));
    // The relationship names the part in another case.
    let by_rel = package(&[
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", &body),
        ("word/_rels/document.xml.rels", &rels(&[(COMMENTS_REL, "COMMENTS.XML")])),
        ("word/comments.xml", &comments),
    ]);
    // No relationship: the conventional name, in another case.
    let by_name = package(&[
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", &body),
        ("word/Comments.xml", &comments),
    ]);
    for bytes in [by_rel, by_name] {
        let converted = convert(&bytes, Format::Docx);
        assert_eq!(converted.markdown, "Body anchored\n");
        assert_eq!(rows(&converted.comments), [("1", "A", "", "Note", "anchored", None, false)]);
    }
}

#[test]
fn malformed_comment_parts_are_skipped_and_never_fail_the_conversion() {
    let body = document(&ranged("1"));
    let doc_rels = rels(&[(COMMENTS_REL, "comments.xml"), (EXTENDED_REL, "commentsExtended.xml")]);
    let good = comments_part(&comment("1", "AA", "Note"));
    let plain = convert(
        &package(&[("_rels/.rels", ROOT_RELS), ("word/document.xml", &body)]),
        Format::Docx,
    );
    assert!(plain.comments.is_empty());
    for (comments, extended, expected) in [
        // A broken comments part: no comments, same Markdown.
        ("<w:comments <<< broken", "", 0),
        (r#"<w:comments xmlns:w="urn:wrong"><w:comment w:id="1"/></w:comments>"#, "", 0),
        // A broken extended part: the comments without replies or state.
        (good.as_str(), "<w15:commentsEx <<< broken", 1),
        (good.as_str(), "not xml at all", 1),
    ] {
        let bytes = package(&[
            ("_rels/.rels", ROOT_RELS),
            ("word/document.xml", &body),
            ("word/_rels/document.xml.rels", &doc_rels),
            ("word/comments.xml", comments),
            ("word/commentsExtended.xml", extended),
        ]);
        let converted = convert(&bytes, Format::Docx);
        assert_eq!(converted.markdown, plain.markdown, "{comments}");
        assert_eq!(converted.comments.len(), expected, "{comments}");
        assert!(converted.comments.iter().all(|c| c.parent_id.is_none() && !c.resolved));
    }
}

#[test]
fn reply_cycles_and_dangling_parents_are_cut() {
    let body = document(&format!("{}{}{}", ranged("1"), ranged("2"), ranged("3")));
    let comments = comments_part(&format!(
        "{}{}{}",
        comment("1", "AA", "one"),
        comment("2", "BB", "two"),
        comment("3", "CC", "three")
    ));
    let extended = format!(
        r#"<w15:commentsEx {W15}><w15:commentEx w15:paraId="AA" w15:paraIdParent="BB"/><w15:commentEx w15:paraId="BB" w15:paraIdParent="AA"/><w15:commentEx w15:paraId="CC" w15:paraIdParent="FF"/></w15:commentsEx>"#
    );
    let bytes = package(&[
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", &body),
        (
            "word/_rels/document.xml.rels",
            &rels(&[(COMMENTS_REL, "comments.xml"), (EXTENDED_REL, "commentsExtended.xml")]),
        ),
        ("word/comments.xml", &comments),
        ("word/commentsExtended.xml", &extended),
    ]);
    let converted = convert(&bytes, Format::Docx);
    let parents: Vec<_> = converted.comments.iter().map(|c| c.parent_id.as_deref()).collect();
    // 1 -> 2 -> 1 closes a cycle at 2; 3's parent does not exist.
    assert_eq!(parents, [Some("2"), None, None]);
}

#[test]
fn docx_comments_are_bounded() {
    // Past the cap comments are counted, not read.
    let many: String = (0..10_005).map(|i| comment(&i.to_string(), "00", "n")).collect();
    let bytes = package(&[
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", &document(&ranged("0"))),
        ("word/comments.xml", &comments_part(&many)),
    ]);
    let converted = convert(&bytes, Format::Docx);
    assert_eq!(converted.comments.len(), 10_000);
    assert_eq!(converted.comments_left_out, 10_005);
    assert_eq!(converted.comments[0].anchor_text, "anchored");

    // A long range and long text are cut; ranges that never close, and
    // more than can be open at once, cost nothing more.
    let long = "x".repeat(100_000);
    let unclosed: String =
        (0..2_000).map(|i| format!(r#"<w:commentRangeStart w:id="u{i}"/>"#)).collect();
    let body = document(&format!(
        r#"<w:p>{unclosed}<w:commentRangeStart w:id="1"/><w:r><w:t>{long}</w:t></w:r><w:commentRangeEnd w:id="1"/><w:r><w:commentReference w:id="1"/></w:r></w:p>"#
    ));
    let bytes = package(&[
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", &body),
        ("word/comments.xml", &comments_part(&comment("1", "00", &long))),
    ]);
    let converted = convert(&bytes, Format::Docx);
    let [comment] = converted.comments.as_slice() else { panic!("{:?}", converted.comments) };
    assert_eq!(comment.text.len(), 64 * 1024);
    // Its range opened past the open-range bound: the paragraph, cut.
    assert_eq!(comment.anchor_text.len(), 64 * 1024);
}

// ---------------------------------------------------------------------------
// Other formats, in-test documents.

fn odf(mimetype: &str, body: &str) -> Vec<u8> {
    let content = format!(
        r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:officeooo="http://openoffice.org/2009/office" xmlns:dc="http://purl.org/dc/elements/1.1/"><office:body>{body}</office:body></office:document-content>"#
    );
    package(&[("mimetype", mimetype), ("content.xml", &content)])
}

#[test]
fn ods_and_odp_count_their_annotations() {
    let note =
        "<office:annotation><dc:creator>A</dc:creator><text:p>SECRET</text:p></office:annotation>";
    let ods = odf(
        "application/vnd.oasis.opendocument.spreadsheet",
        &format!(
            r#"<office:spreadsheet><table:table table:name="S"><table:table-row><table:table-cell>{note}<text:p>cell</text:p></table:table-cell><table:table-cell>{note}<text:p>more</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet>"#
        ),
    );
    let odp = odf(
        "application/vnd.oasis.opendocument.presentation",
        r#"<office:presentation><draw:page><officeooo:annotation><dc:creator>A</dc:creator><text:p>SECRET</text:p></officeooo:annotation></draw:page></office:presentation>"#,
    );
    for (bytes, format, count) in [(ods, Format::Ods, 2), (odp, Format::Odp, 1)] {
        let converted = convert(&bytes, format);
        assert!(!converted.markdown.contains("SECRET"), "{}", converted.markdown);
        assert!(converted.comments.is_empty());
        assert_eq!(converted.comments_left_out, count, "{format:?}");
    }
}

#[test]
fn rtf_comment_text_never_disturbs_the_body() {
    // Unicode escapes and code-page bytes inside a comment use their own
    // decoder: a fallback skip armed in the comment never eats body text,
    // and one armed in the body is not spent inside the comment.
    for (rtf, body, text) in [
        (r"{\rtf1 A{\*\atnid X}{\*\annotation \u1078}B\par}", "AB", "ж"),
        (r"{\rtf1\uc1 A\u1078{\*\annotation x}?B\par}", "AжB", "x"),
        (r"{\rtf1 A{\*\annotation {\*\atnref 9}\'e9t\'e9}B\par}", "AB", "été"),
    ] {
        let converted = convert(rtf.as_bytes(), Format::Rtf);
        assert_eq!(converted.markdown.trim(), body, "{rtf}");
        let [comment] = converted.comments.as_slice() else { panic!("{rtf}") };
        assert_eq!(comment.text, text, "{rtf}");
    }
    // Nor is a skip left over from one comment spent in the next.
    let rtf = r"{\rtf1 A{\*\annotation \u1078}{\*\annotation x}B\par}";
    let texts: Vec<_> =
        convert(rtf.as_bytes(), Format::Rtf).comments.into_iter().map(|c| c.text).collect();
    assert_eq!(texts, ["ж", "x"]);
}

/// A Word 97 document whose one piece holds `text` (8-bit), with an
/// annotation reference table of `refs` entries and `ccp_atn` annotation
/// characters after the main text.
fn doc(main: &str, annotations: &str, refs: usize) -> Vec<u8> {
    fn u32_at(bytes: &mut [u8], offset: usize, value: usize) {
        bytes[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes());
    }
    let mut word = vec![0u8; 0x400];
    word[0..2].copy_from_slice(&0xA5ECu16.to_le_bytes());
    word[2..4].copy_from_slice(&0xC1u16.to_le_bytes());
    word[6..8].copy_from_slice(&0x0409u16.to_le_bytes());
    let text = format!("{main}{annotations}");
    u32_at(&mut word, 0x18, 0x400);
    u32_at(&mut word, 0x1C, 0x400 + text.len());
    u32_at(&mut word, 0x4C, main.len());
    u32_at(&mut word, 0x5C, annotations.len());
    word.extend_from_slice(text.as_bytes());
    // PlcfandRef: refs + 1 CPs, then refs 30-byte ATRDPre10.
    let mut table = Vec::new();
    for i in 0..=refs {
        table.extend_from_slice(&(i as u32).to_le_bytes());
    }
    table.extend(std::iter::repeat_n(0u8, refs * 30));
    u32_at(&mut word, 0xBA, 0);
    u32_at(&mut word, 0xBE, table.len());
    let mut ole = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    ole.create_stream("WordDocument").unwrap().write_all(&word).unwrap();
    ole.create_stream("0Table").unwrap().write_all(&table).unwrap();
    ole.into_inner().into_inner()
}

#[test]
fn doc_counts_its_annotations() {
    let converted = convert(&doc("Body.\r", "\u{5}SECRET one\r\u{5}SECRET two\r", 2), Format::Doc);
    assert_eq!(converted.markdown.trim(), "Body.");
    assert!(converted.comments.is_empty());
    assert_eq!(converted.comments_left_out, 2);
    // No annotation text, or a table that does not fit: nothing counted.
    assert_eq!(convert(&doc("Body.\r", "", 2), Format::Doc).comments_left_out, 0);
}

// ---------------------------------------------------------------------------
// Review regressions.

fn rtf_comments(rtf: &str) -> Conversion {
    convert(rtf.as_bytes(), Format::Rtf)
}

#[test]
fn rtf_destinations_inside_a_comment_never_reach_the_body() {
    for inner in [
        r"{\object\objemb{\*\objclass Package}{\*\objdata 00}{\result SECRET}}",
        r"{\*\shpinst{\sp{\sn x}{\sv y}}{\*\shptxt SECRET}}",
        r"{\footnote SECRET}",
        r"{\*\bkmkstart SECRET}",
        r"{\listtext SECRET}",
        r"{\*\shppict{\pict\pngblip 89504e470d0a1a0a}}",
        r"\intbl SECRET\cell\row",
    ] {
        let rtf = format!(r"{{\rtf1 Body{{\*\annotation {inner}}}.\par}}");
        let converted = rtf_comments(&rtf);
        assert_eq!(converted.markdown, "Body.\n", "{inner}");
        assert_eq!(converted.images_left_out, 0, "{inner}");
        assert_eq!(converted.comments.len(), 1, "{inner}");
    }
    // A paragraph mark in a comment inside a footnote stays in the comment.
    let converted =
        rtf_comments(r"{\rtf1 Body{\footnote Note{\*\annotation one\par two} end}.\par}");
    assert!(!converted.markdown.contains("one") && !converted.markdown.contains("two"));
    assert_eq!(converted.comments[0].text, "one\ntwo");
}

#[test]
fn rtf_references_parents_and_line_breaks() {
    // \atnref written as a parameter still names the range.
    let converted = rtf_comments(
        r"{\rtf1 A {\*\atrfstart 42}range{\*\atrfend 42}{\*\annotation{\*\atnref42}x} B\par}",
    );
    assert_eq!(rows(&converted.comments), [("42", "", "", "x", "range", None, false)]);

    // \atnparent naming the parent's \atnid, as the specification has it.
    let converted = rtf_comments(concat!(
        r"{\rtf1 Text{\*\atnid JD}{\*\annotation{\*\atnref 100}root}",
        r"{\*\atnid RS}{\*\annotation{\*\atnref 101}{\*\atnparent JD}reply}\par}"
    ));
    let parents: Vec<_> = converted.comments.iter().map(|c| c.parent_id.as_deref()).collect();
    assert_eq!(parents, [None, Some("100")]);

    // Initials two comments share name no parent.
    let converted = rtf_comments(concat!(
        r"{\rtf1 Text{\*\atnid JD}{\*\annotation{\*\atnref 1}a}{\*\atnid JD}{\*\annotation{\*\atnref 2}b}",
        r"{\*\atnid RS}{\*\annotation{\*\atnref 3}{\*\atnparent JD}c}\par}"
    ));
    assert_eq!(converted.comments[2].parent_id, None);

    // Line breaks in the body reach the anchors.
    let converted = rtf_comments(
        r"{\rtf1 {\*\atrfstart 1}one\line two{\*\atrfend 1}{\*\annotation{\*\atnref 1}x}\par}",
    );
    assert_eq!(converted.comments[0].anchor_text, "one\ntwo");
}

#[test]
fn generated_ids_never_collide_with_written_ones() {
    let odt = odf(
        "application/vnd.oasis.opendocument.text",
        concat!(
            r#"<office:text><text:p>A<office:annotation office:name="annotation-2"><text:p>named</text:p></office:annotation>x<office:annotation-end office:name="annotation-2"/></text:p>"#,
            r#"<text:p>B<office:annotation><text:p>unnamed</text:p></office:annotation></text:p></office:text>"#
        ),
    );
    let converted = convert(&odt, Format::Odt);
    let ids: Vec<_> =
        converted.comments.iter().map(|c| (c.id.as_str(), c.anchor_text.as_str())).collect();
    assert_eq!(ids, [("annotation-2", "x"), ("annotation-2-2", "B")]);

    let rtf = rtf_comments(concat!(
        r"{\rtf1 A{\*\atrfstart annotation-2}x{\*\atrfend annotation-2}{\*\annotation{\*\atnref annotation-2}n}\par ",
        r"B{\*\annotation u}\par}"
    ));
    let ids: Vec<_> =
        rtf.comments.iter().map(|c| (c.id.as_str(), c.anchor_text.as_str())).collect();
    assert_eq!(ids, [("annotation-2", "x"), ("annotation-2-2", "B")]);
}

#[test]
fn every_range_within_the_total_budget_keeps_its_anchor() {
    // 100 ranges of 64 KiB: 6.25 MiB of anchors, under the 8 MiB budget.
    let text = "y".repeat(64 * 1024);
    let body: String =
        (0..100).map(|i| ranged(&i.to_string()).replace("anchored", &text)).collect();
    let comments: String = (0..100).map(|i| comment(&i.to_string(), "00", "n")).collect();
    let bytes = package(&[
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", &document(&body)),
        ("word/comments.xml", &comments_part(&comments)),
    ]);
    let converted = convert(&bytes, Format::Docx);
    assert_eq!(converted.comments.len(), 100);
    assert!(converted.comments.iter().all(|c| c.anchor_text.len() == text.len()));
}
