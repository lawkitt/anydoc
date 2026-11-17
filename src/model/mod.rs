//! Information-preserving document model.
//!
//! Only fully resolved content lives here: format frontends resolve style
//! cascades, numbering, and references before constructing these types.
//! A [`Document`] is self-contained - embedded assets carry their bytes, so it
//! stays usable after the source archive is gone.

mod asset;
mod block;
mod inline;
mod link;
mod list;
mod style;
mod table;

pub use asset::{Asset, AssetId};
pub use block::Block;
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
