//! Comments a document carries beside its content.

/// One comment (annotation) the source document carries, as structured data.
///
/// Comments never enter the document's blocks or the Markdown: they are
/// context about the document, reported beside it
/// ([`Document::comments`](crate::model::Document::comments),
/// [`Conversion::comments`](crate::Conversion::comments)). Read from DOCX,
/// ODT and RTF; every field holds what the source wrote, so any of them may
/// be empty.
///
/// `#[non_exhaustive]`: read the fields; outside this crate, build one from
/// [`Comment::default`] and set them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Comment {
    /// The comment's id. DOCX: `w:id`. ODT: `office:name`. RTF: the
    /// `\atnref` bookmark. A comment without one is named `annotation-N`
    /// (N counting the document's comments from 1), or `annotation-N-K`
    /// with the first K no other comment's id takes, so a generated name
    /// never repeats one the document wrote.
    pub id: String,
    /// Who wrote it, as the document records it (DOCX `w:author`, ODT
    /// `dc:creator`, RTF `\atnauthor`). Empty when the document names no
    /// one.
    pub author: String,
    /// When it was written, as the document writes it: DOCX `w:date` and ODT
    /// `dc:date` verbatim (usually ISO 8601, `2026-11-18T09:30:00Z`). RTF
    /// stores `\atndate` as a packed Word date-time, given here as
    /// `YYYY-MM-DDTHH:MM` (local time, no zone). Empty when absent or
    /// unreadable.
    pub date: String,
    /// The comment's plain text: its paragraphs joined by `\n`, with tabs as
    /// `\t` and line breaks as `\n`. Formatting, pictures and field codes
    /// are dropped; field results are kept. Empty for an empty comment.
    pub text: String,
    /// The text the comment is attached to: the commented range's text,
    /// paragraphs joined by `\n` (DOCX `w:commentRangeStart`/`End`, ODT
    /// `office:annotation` to `office:annotation-end`, RTF
    /// `\atrfstart`/`\atrfend`). A point comment, one without a range, gets
    /// the text of the paragraph it sits in. Deleted revisions and field
    /// codes are left out; hidden-formatted text is kept, as written. Empty
    /// when the anchor cannot be found.
    pub anchor_text: String,
    /// For a reply, the [`id`](Comment::id) of the comment it answers; that
    /// comment is in the same list. `None` for a comment that starts a
    /// thread, and for a reply whose parent is missing (or would close a
    /// cycle). DOCX: `w15:paraIdParent` in `commentsExtended.xml`. ODT:
    /// LibreOffice's `loext:parent-name`. RTF: `\atnparent` where written,
    /// naming the parent's `\atnref`, or its `\atnid` initials when only
    /// one comment has them.
    pub parent_id: Option<String>,
    /// The comment is marked resolved (done). DOCX: `w15:done` in
    /// `commentsExtended.xml`. ODT: LibreOffice's `loext:resolved`. RTF has
    /// no resolved state: always `false`.
    pub resolved: bool,
}
