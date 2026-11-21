//! Information-preserving document model.
//!
//! Only fully resolved content lives here: format frontends resolve style
//! cascades, numbering, and references before constructing these types.
//! A [`Document`] is self-contained - embedded assets carry their bytes, so it
//! stays usable after the source archive is gone.

mod asset;
mod block;
mod comment;
mod inline;
mod link;
mod list;
mod style;
mod table;

pub use asset::{Asset, AssetId};
pub use block::Block;
pub use comment::Comment;
pub use inline::{Inline, checkbox_text, inlines_are_empty, inlines_to_plain_text};
pub use link::{AnchorId, ImageSource, LinkTarget};
pub use list::{List, ListItem, MarkerKind};
pub use style::Style;
pub use table::{Cell, CellSlot, Table, TableKind};

/// Frontends build grids; consumers read them off [`Table::grid`].
pub(crate) use table::GridBuilder;

/// A parsed document: its body, its notes, and the bytes of everything it
/// embedded.
#[derive(Debug, Clone, Default)]
pub struct Document {
    /// Body content in reading order.
    pub blocks: Vec<Block>,
    /// Note bodies, in the order the document defines them. Text refers to
    /// them by id through [`Inline::NoteRef`].
    pub notes: Vec<Note>,
    /// Every embedded asset, indexed by [`AssetId`].
    pub assets: Vec<Asset>,
    /// The pictures the source shows whose content is not text (embedded
    /// pictures and objects, and pictures whose bytes are missing), when the
    /// frontend counts them as it parses: DOCX does, so a picture with no
    /// bytes and no alt text, or one in preformatted text (which holds text
    /// only), still counts. `None` from the other frontends.
    pub source_images: Option<u32>,
    /// Stretches of hidden-formatted text the frontend left out: DOCX
    /// `w:vanish`, RTF `\v` and binary DOC `fVanish` text never reaches the
    /// blocks. A stretch is hidden text running on, within one paragraph,
    /// with no visible text in between, counted once however many runs it
    /// spans; hidden whitespace alone is not counted. 0 from the formats
    /// that have no hidden text.
    pub hidden_text_left_out: u32,
    /// Spreadsheet content the source hides and the frontend left out:
    /// hidden worksheets, and hidden rows and columns holding a value. 0
    /// from the formats that are not spreadsheets.
    pub hidden_sheet_content: HiddenSheetContent,
    /// The comments the frontend read as structured data, in document order
    /// of the comments part (DOCX `word/comments.xml`), of the annotations
    /// (ODT) or of the annotation groups (RTF). They never reach the blocks.
    /// Empty from the formats whose comments are only counted.
    pub comments: Vec<Comment>,
    /// Comments the source carries that [`Document::comments`] does not
    /// hold: those of the formats that only count them (binary DOC, PPTX,
    /// XLSX, ODS, ODP), and those past the per-document cap of the formats
    /// that read them. 0 when every comment was read, or there are none.
    pub comments_not_extracted: u32,
}

/// Spreadsheet content left out because the source hides it (XLSX, XLSB,
/// XLS, ODS). None of it reaches the blocks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HiddenSheetContent {
    /// Hidden worksheets: hidden and very hidden ones alike, counted whether
    /// or not they hold anything, since telling would mean reading them.
    /// Hidden chart and macro sheets are not counted.
    pub sheets: u32,
    /// Hidden rows of the visible worksheets that hold a value (a cell with
    /// text or a checkbox), in any column. Empty hidden rows, often whole
    /// runs of them, are not counted.
    pub rows: u32,
    /// Hidden columns of the visible worksheets that hold a value, in any
    /// row. Empty hidden columns are not counted.
    pub columns: u32,
}

/// Footnote or endnote body, referenced from text by [`Inline::NoteRef`].
#[derive(Debug, Clone)]
pub struct Note {
    /// Document-scoped id the referencing [`Inline::NoteRef`] carries.
    pub id: String,
    /// Whether the source placed this note on the page or at the end.
    pub kind: NoteKind,
    /// The note's own content.
    pub blocks: Vec<Block>,
}

/// Where the source document places a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// Placed at the foot of the page that references it.
    Footnote,
    /// Collected at the end of the document or section.
    Endnote,
}
