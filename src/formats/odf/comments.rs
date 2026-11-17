//! ODF comments: `office:annotation` in text documents as structured data,
//! anchored from the annotation to its `office:annotation-end` (same
//! `office:name`), or to its paragraph without one. Replies and the
//! resolved state come from LibreOffice's extension attributes
//! (`loext:parent-name`, `loext:resolved`) where it writes them.
//! Spreadsheets and presentations only count their annotations.

use crate::model::Comment;
use crate::package::xml::{Element, ns};
use crate::shared::comments::{
    self, Anchors, Budget, CommentText, MAX_COMMENTS, Step, TextSink, field,
};
use crate::shared::text::{clean_text, collapse_ws};

const DC: &str = "http://purl.org/dc/elements/1.1/";
const LOEXT: &str = "urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0";
/// Impress writes slide comments in this namespace.
const OFFICEOOO: &str = "http://openoffice.org/2009/office";

/// The comments of a text document's `office:text`, and how many more it
/// holds past [`MAX_COMMENTS`].
pub(super) fn read(text: &Element) -> (Vec<Comment>, u32) {
    let mut anchors = Anchors::default();
    let mut budget = Budget::default();
    let mut out = Vec::new();
    let mut seen = 0usize;
    let mut not_extracted = 0u32;
    walk(text, &mut anchors, |annotation, anchors| {
        seen += 1;
        if out.len() >= MAX_COMMENTS {
            not_extracted = not_extracted.saturating_add(1);
            return;
        }
        let name = annotation.attr(ns::OFFICE, "name").filter(|n| !n.is_empty());
        let id = match name {
            Some(name) => field(name),
            None => format!("annotation-{seen}"),
        };
        if name.is_some() {
            anchors.range_start(&id);
        }
        anchors.point(&id);
        let mut text = CommentText::default();
        walk(annotation, &mut text, |_, _| {});
        out.push(Comment {
            author: field(&child_text(annotation, "creator")),
            date: field(&child_text(annotation, "date")),
            text: text.finish(&mut budget),
            anchor_text: String::new(),
            parent_id: annotation
                .attr_qualified(LOEXT, "parent-name")
                .filter(|p| !p.is_empty())
                .map(field),
            resolved: annotation.attr_qualified(LOEXT, "resolved") == Some("true"),
            id,
        });
    });
    let mut anchors = anchors.finish();
    for comment in &mut out {
        comment.anchor_text = anchors.take(&comment.id);
    }
    comments::settle_parents(&mut out);
    (out, not_extracted)
}

/// Annotations in a spreadsheet or presentation body, counted only.
pub(super) fn count(body: &Element) -> u32 {
    let found = body
        .descendant_elems()
        .filter(|e| e.is(ns::OFFICE, "annotation") || e.is(OFFICEOOO, "annotation"))
        .count();
    u32::try_from(found).unwrap_or(u32::MAX)
}

fn child_text(annotation: &Element, local: &str) -> String {
    annotation.find(DC, local).map(|e| e.text().trim().to_string()).unwrap_or_default()
}

/// Walk ODF text content: paragraph text (whitespace collapsed as ODF
/// does), spaces, tabs and line breaks, paragraphs, and annotation ends.
/// Each annotation goes to `annotation` and its own content is skipped;
/// notes, tracked deletions and frame titles carry no anchor text.
fn walk<S: TextSink>(root: &Element, sink: &mut S, mut annotation: impl FnMut(&Element, &mut S)) {
    let mut paragraphs = 0usize;
    comments::walk(root, |step| match step {
        Step::Enter(elem) => {
            if elem.is(ns::OFFICE, "annotation") {
                annotation(elem, sink);
                return false;
            }
            if elem.is(ns::OFFICE, "annotation-end") {
                if let Some(name) = elem.attr(ns::OFFICE, "name") {
                    sink.range_end(&field(name));
                }
                return false;
            }
            if elem.ns.as_deref() == Some(ns::SVG_COMPAT) {
                return false;
            }
            if elem.ns.as_deref() != Some(ns::TEXT) {
                return true;
            }
            match elem.local.as_str() {
                "p" | "h" => {
                    paragraphs += 1;
                    sink.paragraph_start();
                    true
                }
                "s" => {
                    let n = elem.attr(ns::TEXT, "c").and_then(|c| c.parse::<usize>().ok());
                    sink.text(&" ".repeat(n.unwrap_or(1).clamp(1, 64)));
                    false
                }
                "tab" => {
                    sink.text("\t");
                    false
                }
                "line-break" => {
                    sink.text("\n");
                    false
                }
                "note" | "tracked-changes" | "soft-page-break" => false,
                _ => true,
            }
        }
        Step::Leave(elem) => {
            if elem.is(ns::TEXT, "p") || elem.is(ns::TEXT, "h") {
                paragraphs -= 1;
                sink.paragraph_end();
            }
            true
        }
        Step::Text(text) => {
            if paragraphs > 0 {
                sink.text(&collapse_ws(&clean_text(text)));
            }
            true
        }
    });
}
