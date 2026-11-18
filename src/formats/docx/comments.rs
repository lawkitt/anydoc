//! DOCX comments as structured data: `word/comments.xml` for the comments,
//! `commentsExtended.xml` for replies (`w15:paraIdParent`) and the resolved
//! state (`w15:done`), and the document's `w:commentRangeStart`/`End` and
//! `w:commentReference` markers for the anchor text. Both parts are found
//! through the main document's relationships (conventional names as the
//! fallback), matched case-insensitively. Neither ever fails a conversion.

use super::typed_part_path;
use crate::model::Comment;
use crate::package::Package;
use crate::package::relationships::Relationships;
use crate::package::xml::{Element, ns};
use crate::shared::comments::{
    self, AnchorTexts, Budget, CommentText, MAX_COMMENTS, Step, TextSink, field,
};
use crate::shared::text::clean_text;
use std::cell::RefCell;
use std::collections::HashMap;

const COMMENTS_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const COMMENTS_EXTENDED_REL: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";

/// The comment parts of a document that has a comments part.
pub(super) struct Parts {
    comments: String,
    extended: String,
}

/// The comment parts, when the comments part exists. Reads nothing, so the
/// anchors can be gathered as the document parses and the parts read last.
pub(super) fn parts(pkg: &RefCell<Package>, rels: &Relationships, main: &str) -> Option<Parts> {
    let comments = typed_part_path(rels, main, COMMENTS_REL, "comments.xml");
    if !comments::has_part(&pkg.borrow(), &comments) {
        return None;
    }
    let extended = typed_part_path(rels, main, COMMENTS_EXTENDED_REL, "commentsExtended.xml");
    Some(Parts { comments, extended })
}

/// Walk WordprocessingML content, reporting its text, paragraphs and
/// comment markers. Deleted revisions, field codes and property elements
/// carry no text; `mc:Fallback` repeats its `mc:Choice`.
pub(super) fn walk_text(root: &Element, sink: &mut impl TextSink) {
    comments::walk(root, |step| match step {
        Step::Enter(elem) => enter(elem, sink),
        Step::Leave(elem) => {
            if elem.is(ns::W, "p") {
                sink.paragraph_end();
            }
            true
        }
        // Text lives in w:t, read whole when it opens; other text nodes
        // are layout whitespace.
        Step::Text(_) => true,
    });
}

fn enter(elem: &Element, sink: &mut impl TextSink) -> bool {
    if elem.is(ns::MC, "Fallback") {
        return false;
    }
    if elem.ns.as_deref() != Some(ns::W) {
        return true;
    }
    let id = || elem.attr(ns::W, "id").unwrap_or("");
    match elem.local.as_str() {
        "p" => {
            sink.paragraph_start();
            true
        }
        "t" => {
            sink.text(&clean_text(&elem.text()));
            false
        }
        "tab" | "ptab" => {
            sink.text("\t");
            false
        }
        "br" | "cr" => {
            sink.text("\n");
            false
        }
        "noBreakHyphen" => {
            sink.text("-");
            false
        }
        "sym" => {
            if let Some(glyph) = super::symbols::glyph(elem) {
                sink.text(&glyph);
            }
            false
        }
        "commentRangeStart" => {
            sink.range_start(id());
            false
        }
        "commentRangeEnd" => {
            sink.range_end(id());
            false
        }
        "commentReference" => {
            sink.point(id());
            false
        }
        "del" | "moveFrom" | "delText" | "delInstrText" | "instrText" | "pPr" | "rPr"
        | "sectPr" | "tblPr" | "tblGrid" | "trPr" | "tcPr" => false,
        _ => true,
    }
}

/// Read the comments, with their anchors from `anchors`. Returns the
/// comments and how many more the part holds past [`MAX_COMMENTS`] (or
/// without an id).
pub(super) fn read(
    pkg: &RefCell<Package>,
    parts: &Parts,
    mut anchors: AnchorTexts,
) -> (Vec<Comment>, u32) {
    let Some(tree) = comments::read_part(&mut pkg.borrow_mut(), &parts.comments) else {
        return (Vec::new(), 0);
    };
    let Some(root) = tree.find(ns::W, "comments") else {
        log::warn!("skipping comments part {} without w:comments", parts.comments);
        return (Vec::new(), 0);
    };
    let mut budget = Budget::default();
    let mut out = Vec::new();
    // The w14:paraId of each comment's last paragraph, which
    // commentsExtended.xml keys its entries by.
    let mut para_ids: Vec<Option<String>> = Vec::new();
    let mut not_extracted = 0u32;
    for elem in root.find_all(ns::W, "comment") {
        let Some(id) = elem.attr(ns::W, "id").filter(|_| out.len() < MAX_COMMENTS) else {
            not_extracted = not_extracted.saturating_add(1);
            continue;
        };
        let mut text = CommentText::default();
        walk_text(elem, &mut text);
        para_ids.push(
            elem.descendants(ns::W, "p")
                .last()
                .and_then(|p| p.attr_qualified(W14, "paraId"))
                .map(str::to_ascii_uppercase),
        );
        out.push(Comment {
            id: field(id),
            author: field(elem.attr(ns::W, "author").unwrap_or("")),
            date: field(elem.attr(ns::W, "date").unwrap_or("")),
            text: text.finish(&mut budget),
            anchor_text: anchors.take(id),
            parent_id: None,
            resolved: false,
        });
    }
    let extended = comments::read_part(&mut pkg.borrow_mut(), &parts.extended);
    if let Some(extended) = extended {
        apply_extended(&extended, &para_ids, &mut out);
    }
    comments::settle_parents(&mut out);
    (out, not_extracted)
}

/// Replies and resolved state from `w15:commentEx` entries, matched to the
/// comments by paragraph id.
fn apply_extended(tree: &Element, para_ids: &[Option<String>], comments: &mut [Comment]) {
    let mut by_para: HashMap<&str, usize> = HashMap::new();
    for (i, para) in para_ids.iter().enumerate() {
        if let Some(para) = para {
            by_para.entry(para.as_str()).or_insert(i);
        }
    }
    // Applied as read, by index: duplicate entries cost nothing more, and
    // the last one for a comment wins.
    let mut parents: Vec<Option<usize>> = vec![None; comments.len()];
    for ex in tree.descendants(W15, "commentEx") {
        let Some(&at) =
            ex.attr(W15, "paraId").and_then(|p| by_para.get(p.to_ascii_uppercase().as_str()))
        else {
            continue;
        };
        comments[at].resolved = matches!(ex.attr(W15, "done"), Some("1" | "true" | "on"));
        parents[at] = ex
            .attr(W15, "paraIdParent")
            .and_then(|p| by_para.get(p.to_ascii_uppercase().as_str()))
            .copied();
    }
    for (at, parent) in parents.into_iter().enumerate() {
        if let Some(parent) = parent {
            comments[at].parent_id = Some(comments[parent].id.clone());
        }
    }
}
