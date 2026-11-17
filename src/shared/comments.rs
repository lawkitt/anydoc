//! Comment extraction shared by the DOCX, ODT and RTF frontends: the bounds,
//! the anchor tracker, the comment-text collector, and the never-failing
//! part reader.
//!
//! Comments are context beside the document, never content: nothing here
//! touches the blocks, and a comments part that is missing, malformed or
//! over a limit is skipped (logged) rather than failing the conversion.
//! Everything is bounded: comments per document, bytes per field, bytes in
//! total, and simultaneously open anchor ranges. Past a bound the text is
//! truncated (or the comment counted instead of read), never an error.

use crate::model::Comment;
use crate::package::Package;
use crate::package::xml::{Element, Node, parse_xml};
use std::collections::{HashMap, HashSet};

/// Comments read per document; the rest are counted
/// (`Document::comments_not_extracted`).
pub const MAX_COMMENTS: usize = 10_000;

/// Bytes kept of one comment's text or anchor text.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;

/// Bytes kept of one comment's id, author or date.
pub const MAX_FIELD_BYTES: usize = 1024;

/// Bytes kept across every comment's text, and again across every anchor
/// text, in one document.
pub const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;

/// Anchor ranges tracked at once; a range opening past this has no anchor
/// text (its comment falls back to its paragraph).
pub const MAX_OPEN_RANGES: usize = 256;

/// A byte allowance shared by many strings.
#[derive(Debug)]
pub struct Budget(usize);

impl Default for Budget {
    fn default() -> Self {
        Budget(MAX_TOTAL_BYTES)
    }
}

impl Budget {
    /// Append as much of `src` as fits both `dst`'s `cap` and what is left
    /// of the allowance, cut at a character boundary.
    pub fn push(&mut self, dst: &mut String, src: &str, cap: usize) {
        let room = cap.saturating_sub(dst.len()).min(self.0);
        let taken = prefix(src, room);
        self.0 -= taken.len();
        dst.push_str(taken);
    }

    /// Give back the bytes of a string that is dropped.
    pub fn refund(&mut self, dropped: &str) {
        self.0 = self.0.saturating_add(dropped.len()).min(MAX_TOTAL_BYTES);
    }
}

/// The longest prefix of `s` at most `max` bytes long, at a char boundary.
pub fn prefix(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// An id, author or date, cut to [`MAX_FIELD_BYTES`].
pub fn field(s: &str) -> String {
    prefix(s, MAX_FIELD_BYTES).to_string()
}

/// What a format's text walker reports: the text it meets, paragraph
/// boundaries, and comment anchors.
pub trait TextSink {
    /// Text, already normalized by the format.
    fn text(&mut self, text: &str);
    /// A paragraph opens (paragraphs may nest, as in text boxes).
    fn paragraph_start(&mut self) {}
    /// The innermost paragraph ends.
    fn paragraph_end(&mut self);
    /// A comment's anchor range opens.
    fn range_start(&mut self, _id: &str) {}
    /// A comment's anchor range closes.
    fn range_end(&mut self, _id: &str) {}
    /// A comment sits here: without a range, it is anchored to this
    /// paragraph.
    fn point(&mut self, _id: &str) {}
}

/// One comment's text: its paragraphs joined by `\n`.
#[derive(Debug, Default)]
pub struct CommentText {
    text: String,
    /// A paragraph has ended since the last text: the next text starts on a
    /// new line.
    pending_break: bool,
    /// The comment has text or a break: a paragraph end adds a line.
    started: bool,
}

impl CommentText {
    /// The text, charged to `budget`, without leading or trailing newlines.
    pub fn finish(self, budget: &mut Budget) -> String {
        let mut out = String::new();
        budget.push(&mut out, self.text.trim_matches('\n'), MAX_TEXT_BYTES);
        out
    }
}

impl TextSink for CommentText {
    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if std::mem::take(&mut self.pending_break) {
            push_capped(&mut self.text, "\n");
        }
        push_capped(&mut self.text, text);
        self.started = true;
    }

    fn paragraph_end(&mut self) {
        // An empty paragraph between two others is an empty line.
        if self.started {
            if self.pending_break {
                push_capped(&mut self.text, "\n");
            }
            self.pending_break = true;
        }
    }
}

fn push_capped(dst: &mut String, src: &str) {
    let room = MAX_TEXT_BYTES.saturating_sub(dst.len());
    dst.push_str(prefix(src, room));
}

/// The anchor text of every comment, gathered while walking the document.
///
/// A range's text runs from its start to its end, paragraphs joined by
/// `\n`; a range that never ends is ignored. A point comment's anchor is the
/// text of the innermost paragraph it sits in. A comment with a finished
/// range uses the range.
#[derive(Debug, Default)]
pub struct Anchors {
    open: Vec<(String, String)>,
    ranges: HashMap<String, String>,
    points: HashMap<String, String>,
    /// Ids seen as a range start or a point, bounded by [`MAX_COMMENTS`]
    /// each so a crafted document cannot grow the maps without limit.
    started: HashSet<String>,
    pointed: HashSet<String>,
    paragraphs: Vec<Paragraph>,
    budget: Budget,
}

#[derive(Debug, Default)]
struct Paragraph {
    text: String,
    points: Vec<String>,
}

impl Anchors {
    fn innermost(&mut self) -> &mut Paragraph {
        if self.paragraphs.is_empty() {
            self.paragraphs.push(Paragraph::default());
        }
        self.paragraphs.last_mut().expect("just pushed")
    }

    /// Close what is still open and hand back the anchors, by comment id.
    pub fn finish(mut self) -> AnchorTexts {
        while !self.paragraphs.is_empty() {
            self.paragraph_end();
        }
        AnchorTexts { ranges: self.ranges, points: self.points }
    }
}

impl TextSink for Anchors {
    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        push_capped(&mut self.innermost().text, text);
        // Open ranges are bounded in number and size; the total budget is
        // charged when one closes, so ranges that never close cost nothing.
        for (_, range) in &mut self.open {
            push_capped(range, text);
        }
    }

    fn paragraph_start(&mut self) {
        self.paragraphs.push(Paragraph::default());
    }

    fn paragraph_end(&mut self) {
        let Some(paragraph) = self.paragraphs.pop() else {
            return;
        };
        let text = paragraph.text.trim();
        for id in paragraph.points {
            // A finished range is the anchor: the paragraph is not needed.
            if self.ranges.contains_key(&id) {
                continue;
            }
            let mut anchor = String::new();
            self.budget.push(&mut anchor, text, MAX_TEXT_BYTES);
            self.points.insert(id, anchor);
        }
        for (_, range) in &mut self.open {
            if !range.is_empty() {
                push_capped(range, "\n");
            }
        }
    }

    fn range_start(&mut self, id: &str) {
        if self.open.len() >= MAX_OPEN_RANGES
            || self.started.len() >= MAX_COMMENTS
            || !self.started.insert(id.to_string())
        {
            return;
        }
        self.open.push((id.to_string(), String::new()));
    }

    fn range_end(&mut self, id: &str) {
        if let Some(at) = self.open.iter().position(|(open, _)| open == id) {
            let (id, text) = self.open.remove(at);
            // The range replaces a paragraph anchor taken before it ended.
            if let Some(point) = self.points.remove(&id) {
                self.budget.refund(&point);
            }
            let mut anchor = String::new();
            self.budget.push(&mut anchor, text.trim(), MAX_TEXT_BYTES);
            self.ranges.insert(id, anchor);
        }
    }

    fn point(&mut self, id: &str) {
        if self.pointed.len() >= MAX_COMMENTS || !self.pointed.insert(id.to_string()) {
            return;
        }
        self.innermost().points.push(id.to_string());
    }
}

/// Anchor texts by comment id, from [`Anchors::finish`].
#[derive(Debug, Default)]
pub struct AnchorTexts {
    ranges: HashMap<String, String>,
    points: HashMap<String, String>,
}

impl AnchorTexts {
    /// The anchor text of the comment with this id: its range's text, else
    /// its paragraph's, else empty.
    pub fn take(&mut self, id: &str) -> String {
        self.ranges.remove(id).or_else(|| self.points.remove(id)).unwrap_or_default()
    }
}

/// One step of [`walk`].
pub enum Step<'a> {
    /// An element opens. Returning `false` skips its subtree (and its
    /// [`Step::Leave`]).
    Enter(&'a Element),
    /// An element whose subtree was walked closes.
    Leave(&'a Element),
    /// A text node.
    Text(&'a str),
}

/// Walk `root`'s descendants in document order, iteratively: a deep tree
/// cannot overflow the stack.
pub fn walk<'a>(root: &'a Element, mut visit: impl FnMut(Step<'a>) -> bool) {
    let mut stack: Vec<(&'a Element, usize)> = vec![(root, 0)];
    while let Some((elem, index)) = stack.last_mut() {
        let elem: &'a Element = elem;
        let Some(child) = elem.children.get(*index) else {
            stack.pop();
            if !stack.is_empty() {
                visit(Step::Leave(elem));
            }
            continue;
        };
        *index += 1;
        match child {
            Node::Text(text) => {
                visit(Step::Text(text));
            }
            Node::Elem(child) => {
                if visit(Step::Enter(child)) {
                    stack.push((child, 0));
                }
            }
        }
    }
}

/// Read and parse an XML part that comments live in, matching its name
/// case-insensitively (OPC part names are). Never fails: an absent part is
/// `None`, and so is one that is unreadable, corrupt or over a limit,
/// skipped with a log, so comments can never fail a conversion.
pub fn read_part(pkg: &mut Package, name: &str) -> Option<Element> {
    let found = pkg.find_part_ignore_case(name)?;
    let bytes = match pkg.part(&found) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return None,
        Err(e) => {
            log::warn!("skipping unreadable comments part {found}: {e}");
            return None;
        }
    };
    match parse_xml(&bytes) {
        Ok(tree) => Some(tree),
        Err(e) => {
            log::warn!("skipping corrupt comments part {found}: {e}");
            None
        }
    }
}

/// True when a part exists under this name, matched case-insensitively.
pub fn has_part(pkg: &Package, name: &str) -> bool {
    pkg.find_part_ignore_case(name).is_some()
}

/// The working id of a comment the source gives no id, the `n`th comment
/// read. It starts with U+0000, which no XML attribute and no cleaned RTF
/// text holds, so it can never meet an id the source wrote;
/// [`name_unnamed`] gives it its reported name.
pub fn unnamed(n: usize) -> String {
    format!("\0{n}")
}

/// Name the comments [`unnamed`] gave working ids: `annotation-N`, or
/// `annotation-N-K` with the first K that no other comment's id takes.
/// Parent ids follow the renaming.
pub fn name_unnamed(comments: &mut [Comment]) {
    let mut taken: HashSet<String> =
        comments.iter().filter(|c| !c.id.starts_with('\0')).map(|c| c.id.clone()).collect();
    let mut renamed: HashMap<String, String> = HashMap::new();
    for comment in comments.iter_mut() {
        let Some(n) = comment.id.strip_prefix('\0') else { continue };
        let base = format!("annotation-{n}");
        let mut name = base.clone();
        let mut k = 2;
        while taken.contains(&name) {
            name = format!("{base}-{k}");
            k += 1;
        }
        taken.insert(name.clone());
        renamed.insert(std::mem::replace(&mut comment.id, name.clone()), name);
    }
    for comment in comments.iter_mut() {
        if let Some(new) = comment.parent_id.as_ref().and_then(|p| renamed.get(p)) {
            comment.parent_id = Some(new.clone());
        }
    }
}

/// Drop parent ids that name no comment in the list, name the comment
/// itself, or close a cycle, so following `parent_id` always ends at a
/// thread's first comment.
pub fn settle_parents(comments: &mut [Comment]) {
    let mut index: HashMap<&str, usize> = HashMap::new();
    for (i, comment) in comments.iter().enumerate() {
        index.entry(comment.id.as_str()).or_insert(i);
    }
    let parents: Vec<Option<usize>> = comments
        .iter()
        .enumerate()
        .map(|(i, c)| {
            c.parent_id.as_deref().and_then(|p| index.get(p).copied()).filter(|&p| p != i)
        })
        .collect();
    let mut parents = parents;
    // 0: unseen, 1: on the chain being followed, 2: settled.
    let mut state = vec![0u8; comments.len()];
    let mut chain = Vec::new();
    for start in 0..comments.len() {
        let mut at = start;
        while state[at] == 0 {
            state[at] = 1;
            chain.push(at);
            match parents[at] {
                Some(parent) if state[parent] == 1 => {
                    parents[at] = None;
                    break;
                }
                Some(parent) => at = parent,
                None => break,
            }
        }
        for settled in chain.drain(..) {
            state[settled] = 2;
        }
    }
    for (comment, parent) in comments.iter_mut().zip(parents) {
        if parent.is_none() {
            comment.parent_id = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(id: &str, parent: Option<&str>) -> Comment {
        Comment { id: id.into(), parent_id: parent.map(Into::into), ..Comment::default() }
    }

    fn parents(comments: &[Comment]) -> Vec<Option<&str>> {
        comments.iter().map(|c| c.parent_id.as_deref()).collect()
    }

    #[test]
    fn parents_that_are_missing_self_or_cyclic_are_dropped() {
        let mut comments = vec![
            comment("a", None),
            comment("b", Some("a")),
            comment("c", Some("missing")),
            comment("d", Some("d")),
            comment("e", Some("f")),
            comment("f", Some("e")),
            comment("g", Some("b")),
        ];
        settle_parents(&mut comments);
        assert_eq!(parents(&comments), [None, Some("a"), None, None, Some("f"), None, Some("b")]);
    }

    #[test]
    fn unnamed_comments_get_names_no_other_comment_has() {
        let mut comments = vec![
            comment("annotation-2", None),
            comment(&unnamed(2), None),
            comment("annotation-2-2", Some(&unnamed(2))),
            comment(&unnamed(3), None),
        ];
        name_unnamed(&mut comments);
        let ids: Vec<_> = comments.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["annotation-2", "annotation-2-3", "annotation-2-2", "annotation-3"]);
        assert_eq!(comments[2].parent_id.as_deref(), Some("annotation-2-3"));
    }

    #[test]
    fn a_finished_range_costs_its_paragraph_nothing() {
        let mut anchors = Anchors { budget: Budget(10), ..Anchors::default() };
        // ODT order: the point (in the first paragraph) before the range
        // ends in the second.
        anchors.paragraph_start();
        anchors.range_start("1");
        anchors.point("1");
        anchors.text("aaaa");
        anchors.paragraph_end();
        anchors.paragraph_start();
        anchors.text("bbbb");
        anchors.range_end("1");
        anchors.paragraph_end();
        // DOCX order: the reference after the range, in its paragraph.
        anchors.paragraph_start();
        anchors.range_start("2");
        anchors.text("cc");
        anchors.range_end("2");
        anchors.point("2");
        anchors.paragraph_end();
        let mut texts = anchors.finish();
        assert_eq!(texts.take("1"), "aaaa\nbbbb");
        assert_eq!(texts.take("2"), "c");
    }

    #[test]
    fn ranges_span_paragraphs_and_points_take_their_paragraph() {
        let mut anchors = Anchors::default();
        anchors.paragraph_start();
        anchors.text("Before ");
        anchors.range_start("1");
        anchors.text("first");
        anchors.paragraph_end();
        anchors.paragraph_start();
        anchors.text("second");
        anchors.range_end("1");
        anchors.text(" after");
        anchors.point("1");
        anchors.point("2");
        anchors.range_start("3");
        anchors.paragraph_end();
        let mut texts = anchors.finish();
        assert_eq!(texts.take("1"), "first\nsecond");
        assert_eq!(texts.take("2"), "second after");
        // A range that never ends is no range, and there was no point.
        assert_eq!(texts.take("3"), "");
    }

    #[test]
    fn comment_text_joins_paragraphs_and_keeps_inner_empty_lines() {
        let mut text = CommentText::default();
        for paragraph in ["", "one", "", "two", ""] {
            text.paragraph_start();
            text.text(paragraph);
            text.paragraph_end();
        }
        assert_eq!(text.finish(&mut Budget::default()), "one\n\ntwo");
    }

    #[test]
    fn budgets_cut_at_char_boundaries() {
        let mut budget = Budget(3);
        let mut out = String::new();
        budget.push(&mut out, "жжж", 100);
        assert_eq!(out, "ж");
        budget.push(&mut out, "abc", 100);
        assert_eq!(out, "жa");
        budget.push(&mut out, "abc", 100);
        assert_eq!(out, "жa");
        assert_eq!(prefix("жж", 3), "ж");
    }

    #[test]
    fn walk_reports_enter_text_and_leave_in_order() {
        let tree = parse_xml(b"<a><b>x</b><c><d/></c>y</a>").unwrap();
        let mut seen = Vec::new();
        walk(&tree, |step| {
            match step {
                Step::Enter(e) => seen.push(format!("<{}", e.local)),
                Step::Leave(e) => seen.push(format!("{}>", e.local)),
                Step::Text(t) => seen.push(t.to_string()),
            }
            !matches!(step, Step::Enter(e) if e.local == "c")
        });
        assert_eq!(seen, ["<a", "<b", "x", "b>", "<c", "y", "a>"]);
    }
}
