//! RTF comments (annotations): `{\*\atnid}` and `{\*\atnauthor}` before
//! each `{\*\annotation}` group, which holds `{\*\atnref}`, `{\*\atndate}`,
//! `{\*\atnparent}` where written, and the comment's text. The anchor runs
//! from `{\*\atrfstart N}` to `{\*\atrfend N}`, N being the annotation's
//! `\atnref`; an annotation without one is anchored to its paragraph.
//!
//! The parser feeds this only text it would otherwise drop (suppressed
//! destinations) and copies of body text, so the body never changes.

use crate::model::Comment;
use crate::shared::comments::{
    self, Anchors, Budget, CommentText, MAX_COMMENTS, MAX_FIELD_BYTES, TextSink, field,
};
use std::collections::{HashMap, HashSet};

/// Which part of a comment a group's text belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum AtnDest {
    /// Not inside a comment destination.
    #[default]
    None,
    /// The `\annotation` group: the comment's text.
    Text,
    /// `\atnid`: the author's initials (not reported; a reply's
    /// `\atnparent` may name its parent by them).
    Initials,
    /// `\atnauthor`.
    Author,
    /// `\atnref`: the comment's id and anchor bookmark.
    Ref,
    /// `\atndate`.
    Date,
    /// `\atnparent`: the id of the comment a reply answers.
    Parent,
    /// `\atrfstart`: an anchor range opens.
    RangeStart,
    /// `\atrfend`: an anchor range closes.
    RangeEnd,
    /// An ignorable (`\*`) destination inside a comment: not comment text.
    Skip,
}

impl AtnDest {
    /// Its text is read (the initials and ignorable groups are not).
    pub(super) fn reads_text(self) -> bool {
        !matches!(self, AtnDest::None | AtnDest::Skip)
    }

    /// The destination a control word opens, if it is a comment one.
    pub(super) fn for_word(word: &str) -> Option<AtnDest> {
        Some(match word {
            "annotation" => AtnDest::Text,
            "atnid" => AtnDest::Initials,
            "atnauthor" => AtnDest::Author,
            "atnref" => AtnDest::Ref,
            "atndate" => AtnDest::Date,
            "atnparent" => AtnDest::Parent,
            "atrfstart" => AtnDest::RangeStart,
            "atrfend" => AtnDest::RangeEnd,
            _ => return None,
        })
    }
}

/// The annotation being read.
#[derive(Default)]
struct Open {
    author: String,
    initials: String,
    reference: String,
    date: String,
    parent: String,
    text: CommentText,
    /// Past [`MAX_COMMENTS`]: counted, not read.
    counted_only: bool,
}

/// Comments gathered while the parser runs.
#[derive(Default)]
pub(super) struct RtfComments {
    /// Body text and paragraph ends, for the anchor texts.
    pub(super) anchors: Anchors,
    comments: Vec<Comment>,
    open: Option<Open>,
    /// The last `\atnauthor` and `\atnid`, for the next annotation.
    author: String,
    initials: String,
    /// Each comment's `\atnid`, beside `comments`.
    comment_initials: Vec<String>,
    /// The `\atrfstart`/`\atrfend` bookmark being read.
    range: String,
    seen: usize,
    not_extracted: u32,
    budget: Budget,
}

impl RtfComments {
    /// A control word opened `dest` in the current group.
    pub(super) fn open(&mut self, dest: AtnDest) {
        match dest {
            AtnDest::Text => {
                self.finish_annotation();
                self.seen += 1;
                let counted_only = self.comments.len() >= MAX_COMMENTS;
                self.open = Some(Open {
                    author: std::mem::take(&mut self.author),
                    initials: std::mem::take(&mut self.initials),
                    counted_only,
                    ..Open::default()
                });
            }
            AtnDest::Author => self.author.clear(),
            AtnDest::Initials => self.initials.clear(),
            AtnDest::RangeStart | AtnDest::RangeEnd => self.range.clear(),
            _ => {}
        }
    }

    /// Text the parser would otherwise drop, inside `dest`.
    pub(super) fn text(&mut self, dest: AtnDest, text: &str) {
        let short = |buf: &mut String| {
            let room = MAX_FIELD_BYTES.saturating_sub(buf.len());
            buf.push_str(comments::prefix(text, room));
        };
        match (dest, &mut self.open) {
            (AtnDest::Author, _) => short(&mut self.author),
            (AtnDest::Initials, _) => short(&mut self.initials),
            (AtnDest::RangeStart | AtnDest::RangeEnd, _) => short(&mut self.range),
            (AtnDest::Text, Some(open)) => open.text.text(text),
            (AtnDest::Ref, Some(open)) => short(&mut open.reference),
            (AtnDest::Date, Some(open)) => short(&mut open.date),
            (AtnDest::Parent, Some(open)) => short(&mut open.parent),
            _ => {}
        }
    }

    /// A paragraph mark inside the comment's text.
    pub(super) fn paragraph(&mut self) {
        if let Some(open) = &mut self.open {
            open.text.paragraph_end();
        }
    }

    /// The group that opened `dest` closed.
    pub(super) fn close(&mut self, dest: AtnDest) {
        match dest {
            AtnDest::Text => self.finish_annotation(),
            AtnDest::RangeStart => self.anchors.range_start(self.range.trim()),
            AtnDest::RangeEnd => self.anchors.range_end(self.range.trim()),
            _ => {}
        }
    }

    fn finish_annotation(&mut self) {
        let Some(open) = self.open.take() else {
            return;
        };
        if open.counted_only {
            self.not_extracted = self.not_extracted.saturating_add(1);
            return;
        }
        let reference = open.reference.trim();
        let id = if reference.is_empty() { comments::unnamed(self.seen) } else { field(reference) };
        // The annotation sits where its reference mark is: without a range,
        // that paragraph is its anchor.
        self.anchors.point(&id);
        let parent = open.parent.trim();
        self.comments.push(Comment {
            id,
            author: field(open.author.trim()),
            date: dttm(open.date.trim()).unwrap_or_default(),
            text: open.text.finish(&mut self.budget),
            anchor_text: String::new(),
            parent_id: (!parent.is_empty()).then(|| field(parent)),
            resolved: false,
        });
        self.comment_initials.push(field(open.initials.trim()));
    }

    /// The comments with their anchors, and how many more were counted.
    pub(super) fn finish(mut self) -> (Vec<Comment>, u32) {
        self.finish_annotation();
        let mut anchors = std::mem::take(&mut self.anchors).finish();
        for comment in &mut self.comments {
            comment.anchor_text = anchors.take(&comment.id);
        }
        self.resolve_parents();
        comments::name_unnamed(&mut self.comments);
        comments::settle_parents(&mut self.comments);
        (self.comments, self.not_extracted)
    }
}

impl RtfComments {
    /// `\atnparent` names the parent by its `\atnref` or, as the RTF
    /// specification has it, by its `\atnid`: an id match wins, else the one
    /// comment with those initials; initials several comments share name no
    /// parent.
    fn resolve_parents(&mut self) {
        let ids: HashSet<&str> = self.comments.iter().map(|c| c.id.as_str()).collect();
        let mut by_initials: HashMap<&str, Option<usize>> = HashMap::new();
        for (i, initials) in self.comment_initials.iter().enumerate() {
            if !initials.is_empty() {
                by_initials.entry(initials).and_modify(|found| *found = None).or_insert(Some(i));
            }
        }
        let resolved: Vec<Option<String>> = self
            .comments
            .iter()
            .map(|c| {
                let parent = c.parent_id.as_deref()?;
                if ids.contains(parent) {
                    return Some(parent.to_string());
                }
                let at = (*by_initials.get(parent)?)?;
                Some(self.comments[at].id.clone())
            })
            .collect();
        for (comment, parent) in self.comments.iter_mut().zip(resolved) {
            comment.parent_id = parent;
        }
    }
}

/// A Word DTTM (minutes, hours, day, month, years since 1900, weekday,
/// packed low bit first) as `YYYY-MM-DDTHH:MM`; `None` unless it is a real
/// date.
fn dttm(digits: &str) -> Option<String> {
    let value = digits.parse::<i64>().ok()?;
    // Written signed or unsigned; the bits are what count.
    let bits = u32::try_from(value).or_else(|_| i32::try_from(value).map(|v| v as u32)).ok()?;
    let minute = bits & 0x3F;
    let hour = (bits >> 6) & 0x1F;
    let day = (bits >> 11) & 0x1F;
    let month = (bits >> 16) & 0x0F;
    let year = 1900 + ((bits >> 20) & 0x1FF);
    if !(1..=12).contains(&month) || day == 0 || hour > 23 || minute > 59 {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}"))
}

#[cfg(test)]
mod tests {
    use super::dttm;

    #[test]
    fn dttm_unpacks_word_dates() {
        // 2026-11-18 09:30: minutes 30, hours 9, day 18, month 11, year 126.
        let packed: u32 = 30 | (9 << 6) | (18 << 11) | (11 << 16) | (126 << 20) | (5 << 29);
        assert_eq!(dttm(&packed.to_string()).as_deref(), Some("2026-11-18T09:30"));
        // The same bits written as a negative long.
        let signed = packed as i32;
        assert!(signed < 0);
        assert_eq!(dttm(&signed.to_string()).as_deref(), Some("2026-11-18T09:30"));
        for bad in ["", "abc", "0", "99999999999"] {
            assert_eq!(dttm(bad), None, "{bad}");
        }
    }
}
