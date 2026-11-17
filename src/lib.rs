//! anydoc converts documents to GitHub-Flavored Markdown.
//!
//! Recovery and skipped-content events are reported through the [`log`]
//! facade (debug/warn level); logging never changes conversion behavior and
//! its messages are not a stable API.

#![warn(missing_docs)]
// The crate parses hostile binary formats entirely in safe Rust. Make that
// property enforced rather than incidental: a parser bug must never be able
// to become a memory-safety bug.
#![forbid(unsafe_code)]

pub mod model;

mod error;
mod formats;
mod package;
mod render;
mod shared;

pub use error::ConvertError;

use render::markdown::document_to_markdown;

use std::path::Path;

/// Input format. Selects the parser; container variants that share a parser
/// (docm, xlsm, ...) map onto these via [`Format::from_bytes`] or
/// [`Format::from_extension`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// Binary Word 97-2003 (`.doc`).
    Doc,
    /// WordprocessingML (`.docx`, `.docm`), both Transitional and Strict.
    Docx,
    /// OpenDocument Text (`.odt`).
    Odt,
    /// Converted with [pdf-inspector], which emits Markdown directly:
    /// [`to_document`] is unsupported for PDFs. Scanned or image-only pages
    /// need OCR, which anydoc does not do: the document errors with
    /// [`ConvertError::NeedsOcr`] naming them, or converts without them
    /// under [`Ocr::Skip`].
    ///
    /// Needs the `pdf` cargo feature (on by default). Without it PDFs are
    /// still detected, and converting one fails with
    /// [`ConvertError::Unsupported`].
    ///
    /// [pdf-inspector]: https://github.com/firecrawl/pdf-inspector
    Pdf,
    /// Binary PowerPoint 97-2003 (`.ppt`, `.pps`, `.pot`).
    Ppt,
    /// PresentationML (`.pptx`, `.pptm`, `.ppsx`, `.ppsm`).
    Pptx,
    /// Rich Text Format (`.rtf`).
    Rtf,
    /// EPUB 2 and 3 (`.epub`).
    Epub,
    /// Excel workbooks: `.xlsx`, `.xlsm`, binary `.xlsb`, and legacy
    /// OLE-based `.xls`.
    Excel,
    /// OpenDocument Spreadsheet (`.ods`).
    Ods,
    /// OpenDocument Presentation (`.odp`).
    Odp,
    /// Delimiter-separated text (`.csv`). Carries no signature, so it has to
    /// be named rather than detected.
    Csv,
}

impl Format {
    /// Detect the format from the content itself: the signature and identity
    /// each container specification designates (PDF header, RTF open group,
    /// OLE stream names, ZIP package mimetype/content types). Plain-text
    /// formats (CSV) carry no signature and return `None`; so does anything
    /// unrecognized.
    pub fn from_bytes(bytes: &[u8]) -> Option<Format> {
        formats::detect::from_bytes(bytes)
    }

    /// The format a bare extension names (no leading dot), matched
    /// case-insensitively. `None` for anything unrecognized.
    pub fn from_extension(ext: &str) -> Option<Format> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "doc" => Format::Doc,
            "docx" | "docm" => Format::Docx,
            "odt" => Format::Odt,
            "pdf" => Format::Pdf,
            "pptx" | "pptm" | "ppsx" | "ppsm" => Format::Pptx,
            "ppt" | "pps" | "pot" => Format::Ppt,
            "rtf" => Format::Rtf,
            "epub" => Format::Epub,
            "xlsx" | "xlsm" | "xlsb" | "xls" => Format::Excel,
            "ods" => Format::Ods,
            "odp" => Format::Odp,
            "csv" => Format::Csv,
            _ => return None,
        })
    }

    /// The format a path's extension names. `None` when the path has no
    /// extension or names nothing recognized.
    pub fn from_path(path: &Path) -> Option<Format> {
        path.extension().and_then(|e| e.to_str()).and_then(Format::from_extension)
    }
}

/// What conversion does with PDF pages that need OCR.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Ocr {
    /// Fail with [`ConvertError::NeedsOcr`] naming the pages. The default:
    /// Markdown missing those pages would otherwise read as complete.
    #[default]
    Reject,
    /// Convert the pages that carry text instead of failing the document,
    /// and name the pages left out on [`Conversion::pages_needing_ocr`].
    ///
    /// The Markdown holds content and nothing else - the pages left out are
    /// reported as data, never written into the text. A document where no
    /// page yielded text still fails with [`ConvertError::NeedsOcr`]: there
    /// is nothing to hand back, and the error is what tells a caller the
    /// document needs OCR rather than better parsing.
    Skip,
}

/// Conversion options.
///
/// `#[non_exhaustive]`, so build from [`Options::default`] and set what you
/// need: `Options::default().ocr(Ocr::Skip)`.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct Options {
    /// What to do with PDF pages that need OCR.
    pub ocr: Ocr,
}

impl Options {
    /// Set what happens to PDF pages that need OCR.
    #[must_use]
    pub fn ocr(mut self, ocr: Ocr) -> Self {
        self.ocr = ocr;
        self
    }
}

/// Markdown from a conversion, and what the conversion left out.
///
/// Returned by [`to_markdown_with`] and [`to_markdown_bytes_with`]. Only PDFs
/// paginate, so every other format reports no pages.
#[derive(Debug, Clone)]
pub struct Conversion {
    /// The Markdown. Content only: conversion never writes notes about
    /// itself into the output.
    pub markdown: String,
    /// 1-indexed PDF pages left unconverted because they need OCR. Only
    /// [`Ocr::Skip`] can leave any: otherwise the document would have
    /// errored. Empty for a document that converted whole.
    pub pages_needing_ocr: Vec<u32>,
    /// Pages in the document, or 0 for the formats that do not paginate.
    pub page_count: u32,
    /// Images, in the body and notes, whose content the Markdown does not
    /// carry: embedded pictures and objects, and images whose bytes are
    /// missing, which render as their alt text at most. Linked images,
    /// which render as `![alt](url)`, and charts and diagrams, which render
    /// as text, are not counted. DOCX counts each picture as it parses
    /// (`Document::source_images`), so one without bytes or alt text, or one
    /// in a preformatted paragraph, still counts; the other formats count
    /// the image inlines their document holds. Always 0 for PDFs, which
    /// convert without the document model.
    pub images_left_out: u32,
    /// Stretches of hidden-formatted text left out of the Markdown: Word's
    /// hidden character property, from DOCX (`w:vanish`, through styles too,
    /// and off again with `w:val="0"`), RTF (`\v`) and binary DOC
    /// (`sprmCFVanish`). A stretch is hidden text running on, within one
    /// paragraph, with no visible text in between: it counts once however
    /// many runs it spans, and hidden whitespace alone does not count. Hidden
    /// runs keep their pictures, note references and paragraph breaks; only
    /// their text and line breaks are left out. Always 0 for the other
    /// formats, PDFs included.
    pub hidden_text_left_out: u32,
}

/// Convert a document file to Markdown. The format is detected from the
/// file content ([`Format::from_bytes`]); the extension is the fallback for
/// signature-less formats (CSV) and unrecognizable containers.
pub fn to_markdown(path: impl AsRef<Path>) -> Result<String, ConvertError> {
    to_markdown_with(path, Options::default()).map(|converted| converted.markdown)
}

/// [`to_markdown`] with [`Options`], reporting what it left out.
pub fn to_markdown_with(
    path: impl AsRef<Path>,
    options: Options,
) -> Result<Conversion, ConvertError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)?;
    let Some(format) = Format::from_bytes(&bytes).or_else(|| Format::from_path(path)) else {
        return Err(ConvertError::Unsupported(format!(
            "unrecognized file content and extension: {}",
            path.display()
        )));
    };
    to_markdown_bytes_with(&bytes, format, options)
}

/// Convert an in-memory document to Markdown. Pass a [`Format`] to select the
/// parser, or `None` to detect it from the content ([`Format::from_bytes`]),
/// which signature-less formats (CSV) have to name explicitly.
pub fn to_markdown_bytes(
    bytes: &[u8],
    format: impl Into<Option<Format>>,
) -> Result<String, ConvertError> {
    to_markdown_bytes_with(bytes, format, Options::default()).map(|converted| converted.markdown)
}

/// [`to_markdown_bytes`] with [`Options`], reporting what it left out.
pub fn to_markdown_bytes_with(
    bytes: &[u8],
    format: impl Into<Option<Format>>,
    options: Options,
) -> Result<Conversion, ConvertError> {
    let format = resolve_format(bytes, format.into())?;
    // PDFs convert to Markdown directly (pdf-inspector) without passing
    // through the document model, and are the only format with pages to
    // leave out.
    if format == Format::Pdf {
        return pdf_to_markdown(bytes, options.ocr);
    }
    let document = to_document(bytes, format)?;
    let markdown = document_to_markdown(&document);
    let images_left_out = document.source_images.unwrap_or_else(|| images_left_out(&document));
    Ok(Conversion {
        markdown,
        pages_needing_ocr: Vec::new(),
        page_count: 0,
        images_left_out,
        hidden_text_left_out: document.hidden_text_left_out,
    })
}

#[cfg(feature = "pdf")]
fn pdf_to_markdown(bytes: &[u8], ocr: Ocr) -> Result<Conversion, ConvertError> {
    formats::pdf::to_markdown(bytes, ocr)
}

/// Without the `pdf` feature a PDF is still detected, but not converted.
#[cfg(not(feature = "pdf"))]
fn pdf_to_markdown(_bytes: &[u8], _ocr: Ocr) -> Result<Conversion, ConvertError> {
    Err(ConvertError::Unsupported(PDF_NOT_BUILT.to_string()))
}

/// The [`ConvertError::Unsupported`] detail for a PDF when anydoc is built
/// without its `pdf` feature.
#[cfg(not(feature = "pdf"))]
const PDF_NOT_BUILT: &str = "PDF conversion is not built in (anydoc's `pdf` feature is off)";

/// [`Conversion::images_left_out`] from a parsed document's image inlines,
/// for the frontends that do not count `Document::source_images`.
fn images_left_out(document: &model::Document) -> u32 {
    use model::{Block, CellSlot, ImageSource, Inline};
    fn walk(inlines: &[Inline], count: &mut u32) {
        for inline in inlines {
            match inline {
                Inline::Image { source: ImageSource::External(_), .. } => {}
                Inline::Image { .. } => *count = count.saturating_add(1),
                Inline::Link { content, .. } => walk(content, count),
                _ => {}
            }
        }
    }
    let mut count = 0;
    let mut stack: Vec<&Block> = document.blocks.iter().collect();
    stack.extend(document.notes.iter().flat_map(|note| &note.blocks));
    while let Some(block) = stack.pop() {
        match block {
            Block::Paragraph(content) | Block::Heading { content, .. } => walk(content, &mut count),
            Block::List(list) => stack.extend(list.items.iter().flat_map(|item| &item.blocks)),
            Block::Table(table) => {
                for slot in table.grid.iter().flatten() {
                    if let CellSlot::Origin(cell) = slot {
                        stack.extend(&cell.blocks);
                    }
                }
            }
            Block::BlockQuote(blocks) => stack.extend(blocks),
            Block::CodeBlock { .. } | Block::Rule | Block::Math(_) => {}
        }
    }
    count
}

/// Parse an in-memory document into the document model. Pass a [`Format`] to
/// select the parser, or `None` to detect it from the content.
///
/// Unsupported for [`Format::Pdf`]: PDF conversion produces Markdown
/// directly and has no document-model form; use [`to_markdown_bytes`].
pub fn to_document(
    bytes: &[u8],
    format: impl Into<Option<Format>>,
) -> Result<model::Document, ConvertError> {
    formats::parse(bytes, resolve_format(bytes, format.into())?)
}

fn resolve_format(bytes: &[u8], format: Option<Format>) -> Result<Format, ConvertError> {
    format.or_else(|| Format::from_bytes(bytes)).ok_or_else(|| {
        ConvertError::Unsupported("unrecognized file content: name the format explicitly".into())
    })
}

#[cfg(test)]
mod images_left_out_tests {
    use super::images_left_out;
    use crate::model::{
        AssetId, Block, Cell, Document, ImageSource, Inline, LinkTarget, List, ListItem,
        MarkerKind, Note, NoteKind, Table, TableKind,
    };

    fn image(source: ImageSource) -> Inline {
        Inline::Image { alt: "alt".into(), source }
    }

    #[test]
    fn counts_images_without_content_wherever_they_sit() {
        let embedded = || image(ImageSource::Asset(AssetId(0)));
        let document = Document {
            blocks: vec![
                Block::Paragraph(vec![
                    embedded(),
                    image(ImageSource::Unavailable),
                    // A linked image renders as `![alt](url)`.
                    image(ImageSource::External("https://example.com/a.png".into())),
                    Inline::Link {
                        content: vec![embedded()],
                        target: LinkTarget::External("https://example.com".into()),
                    },
                ]),
                Block::List(List {
                    marker: MarkerKind::Bullet,
                    start: 1,
                    items: vec![ListItem {
                        blocks: vec![Block::BlockQuote(vec![Block::Paragraph(vec![embedded()])])],
                        marker_label: None,
                    }],
                }),
                Block::Table(Table::from_rows(
                    vec![vec![Cell::from_inlines(vec![embedded()])]],
                    0,
                    TableKind::Data,
                )),
            ],
            notes: vec![Note {
                id: "1".into(),
                kind: NoteKind::Footnote,
                blocks: vec![Block::Heading { level: 1, anchor: None, content: vec![embedded()] }],
            }],
            assets: Vec::new(),
            source_images: None,
            hidden_text_left_out: 0,
        };
        assert_eq!(images_left_out(&document), 6);
        assert_eq!(images_left_out(&Document::default()), 0);
    }
}
