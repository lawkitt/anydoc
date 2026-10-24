//! GitHub-Flavored Markdown serializer for the document model.

mod anchors;
mod escape;
mod inline;
mod table;

#[cfg(test)]
mod tests;

use crate::model::{Block, Document, Inline, List, MarkerKind, Note, TableKind, inlines_are_empty};
use anchors::{AnchorMap, resolve_anchors};
use escape::{EscapeOpts, InlineContext, backtick_fence, escape_text};
use inline::render_inlines;
use std::collections::{HashMap, HashSet};

/// Escape a source-derived composite marker label for literal use: control
/// characters collapse to spaces and Markdown syntax is neutralized so a
/// crafted label cannot alter document structure.
pub(crate) fn escape_marker_label(label: &str, ctx: InlineContext) -> String {
    let cleaned: String = label.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let opts = EscapeOpts {
        // List-item content re-opens block syntax after the `- ` marker.
        at_line_start: ctx == InlineContext::Block,
        trailing_active: true,
        ..Default::default()
    };
    escape_text(&cleaned, ctx, opts)
}

/// Footnote id -> rendered number, shared by all render functions.
type NoteNumbers = HashMap<String, usize>;

/// Immutable render context threaded through every render function.
pub(crate) struct Ctx {
    nums: NoteNumbers,
    anchors: AnchorMap,
}

pub fn document_to_markdown(doc: &Document) -> String {
    let rc = Ctx { nums: number_notes(doc), anchors: resolve_anchors(doc) };
    let body = render_blocks(&doc.blocks, &rc);
    let mut parts: Vec<String> = Vec::new();
    if !body.is_empty() {
        parts.push(body);
    }
    let mut rendered_defs: HashSet<usize> = HashSet::new();
    let mut ordered: Vec<(&Note, usize)> =
        doc.notes.iter().filter_map(|n| rc.nums.get(&n.id).map(|&num| (n, num))).collect();
    ordered.sort_by_key(|(_, num)| *num);
    for (note, num) in ordered {
        let body = render_blocks(&note.blocks, &rc);
        if body.is_empty() {
            continue;
        }
        // The first non-empty definition for a duplicate id wins.
        if !rendered_defs.insert(num) {
            log::debug!("duplicate note id {:?} dropped from output", note.id);
            continue;
        }
        let mut lines = body.lines();
        let first = lines.next().unwrap_or("");
        let mut s = format!("[^{num}]: {first}");
        for line in lines {
            s.push('\n');
            if !line.is_empty() {
                s.push_str("    ");
                s.push_str(line);
            }
        }
        parts.push(s);
    }
    let mut out = normalize_blank_lines(&parts.join("\n\n"));
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Blank lines in finished Markdown: whitespace-only lines become empty,
/// runs of them collapse to one, and none lead or trail. Fenced code and
/// `$$` math keep their lines as written.
fn normalize_blank_lines(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    // The open fence (its backtick run) or `$$`, while inside one.
    let mut verbatim: Option<&str> = None;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if let Some(open) = verbatim {
            // Inside a quote every line carries its markers.
            let mut closer = trimmed;
            while let Some(rest) = closer.strip_prefix('>') {
                closer = rest.trim_start();
            }
            let closes = if open == "$$" {
                closer == "$$"
            } else {
                closer.len() >= open.len() && closer.bytes().all(|b| b == b'`')
            };
            if closes {
                verbatim = None;
            }
            out.push(line);
            continue;
        }
        if trimmed.is_empty() {
            if out.last().is_some_and(|last| !last.is_empty()) {
                out.push("");
            }
            continue;
        }
        let opener = strip_container_markers(trimmed);
        if opener == "$$" {
            verbatim = Some("$$");
        } else if opener.starts_with("```") {
            let ticks = opener.bytes().take_while(|&b| b == b'`').count();
            verbatim = Some(&opener[..ticks]);
        }
        out.push(line);
    }
    while out.last().is_some_and(|last| last.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// A line with the list markers and quote markers that open it removed, so
/// a fence opening a list item (`- ```, `1. $$`) is still seen.
fn strip_container_markers(mut line: &str) -> &str {
    loop {
        let digits = line.bytes().take_while(u8::is_ascii_digit).count();
        let rest = if let Some(rest) = line.strip_prefix(['-', '*', '+', '>']) {
            rest
        } else if (1..=9).contains(&digits) {
            match line[digits..].strip_prefix(['.', ')']) {
                Some(rest) => rest,
                None => return line,
            }
        } else {
            return line;
        };
        if !rest.starts_with([' ', '\t']) && !(line.starts_with('>') || rest.is_empty()) {
            return line;
        }
        line = rest.trim_start();
    }
}

/// Number notes in first-reference order; unreferenced notes follow at the
/// end. The first note wins a duplicated id.
fn number_notes(doc: &Document) -> NoteNumbers {
    let mut valid: HashMap<&str, &Note> = HashMap::new();
    for note in &doc.notes {
        if !note.blocks.iter().all(block_is_blank) {
            valid.entry(note.id.as_str()).or_insert(note);
        }
    }
    let mut order: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    collect_note_refs(&doc.blocks, &valid, &mut order, &mut seen);
    for note in &doc.notes {
        if valid.contains_key(note.id.as_str()) && seen.insert(note.id.clone()) {
            order.push(note.id.clone());
        }
    }
    order.into_iter().enumerate().map(|(i, id)| (id, i + 1)).collect()
}

fn block_is_blank(block: &Block) -> bool {
    match block {
        Block::Paragraph(inlines) => inlines_are_empty(inlines),
        _ => false,
    }
}

fn collect_note_refs(
    blocks: &[Block],
    valid: &HashMap<&str, &Note>,
    order: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    fn walk_inlines(
        inlines: &[Inline],
        valid: &HashMap<&str, &Note>,
        order: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) {
        for inline in inlines {
            match inline {
                Inline::NoteRef(id) => {
                    if let Some(note) = valid.get(id.as_str())
                        && seen.insert(id.clone())
                    {
                        order.push(id.clone());
                        collect_note_refs(&note.blocks, valid, order, seen);
                    }
                }
                Inline::Link { content, .. } => walk_inlines(content, valid, order, seen),
                _ => {}
            }
        }
    }
    for block in blocks {
        match block {
            Block::Paragraph(i) | Block::Heading { content: i, .. } => {
                walk_inlines(i, valid, order, seen)
            }
            Block::List(list) => {
                for item in &list.items {
                    collect_note_refs(&item.blocks, valid, order, seen);
                }
            }
            Block::Table(t) => {
                for row in &t.grid {
                    for slot in row {
                        if let crate::model::CellSlot::Origin(cell) = slot {
                            collect_note_refs(&cell.blocks, valid, order, seen);
                        }
                    }
                }
            }
            Block::BlockQuote(blocks) => collect_note_refs(blocks, valid, order, seen),
            Block::CodeBlock { .. } | Block::Rule | Block::Math(_) => {}
        }
    }
}

/// Blocks separated by one blank line. Adjacent lists that CommonMark
/// reads as one list (both bulleted, or both numbered) are joined without
/// it when both are tight, so the merged list stays tight.
fn render_blocks(blocks: &[Block], rc: &Ctx) -> String {
    let mut out = String::new();
    // The previous list's family and tightness, when the last block was one.
    let mut prev_list: Option<(ListFamily, bool)> = None;
    for block in blocks {
        let (text, list) = match block {
            Block::List(list) => match render_list(list, rc) {
                Some((text, loose)) => (
                    text,
                    Some((list_family(list, 0), list_family(list, list.items.len() - 1), loose)),
                ),
                None => continue,
            },
            _ => match render_block(block, rc) {
                Some(text) => (text, None),
                None => continue,
            },
        };
        if !out.is_empty() {
            let tight_merge = matches!(
                (prev_list, list),
                (Some((last, false)), Some((first, _, false))) if last == first
            );
            out.push_str(if tight_merge { "\n" } else { "\n\n" });
        }
        out.push_str(&text);
        prev_list = list.map(|(_, last, loose)| (last, loose));
    }
    out
}

/// Which CommonMark list an item's marker belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ListFamily {
    /// `- `, also for labels Markdown cannot number (`- iv. `).
    Bullet,
    /// `3. `
    Ordered,
}

fn list_family(list: &List, item: usize) -> ListFamily {
    let labelled = list.items.get(item).is_some_and(|item| item.marker_label.is_some());
    if list.marker == MarkerKind::Decimal && !labelled {
        ListFamily::Ordered
    } else {
        ListFamily::Bullet
    }
}

/// Whether a single-cell table's lone cell holds a nested table.
fn wraps_table(t: &crate::model::Table) -> bool {
    let crate::model::CellSlot::Origin(cell) = &t.grid[0][0] else { return false };
    cell.blocks.iter().any(|b| matches!(b, Block::Table(_)))
}

fn render_block(block: &Block, rc: &Ctx) -> Option<String> {
    match block {
        Block::Heading { level, content, .. } => {
            let text = render_inlines(content, InlineContext::Heading, rc);
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            let level = (*level).clamp(1, 6) as usize;
            Some(format!("{} {}", "#".repeat(level), text))
        }
        Block::Paragraph(inlines) => {
            let text = render_inlines(inlines, InlineContext::Block, rc);
            let trimmed = trim_paragraph(&text);
            if trimmed.is_empty() { None } else { Some(trimmed) }
        }
        Block::List(list) => render_list(list, rc).map(|(text, _)| text),
        // Trivial layout tables are scaffolding; render their content directly.
        // A single-cell table wrapping a nested table is the same scaffolding
        // shape (Word forms often wrap their whole body in one): flattening
        // would collapse the inner grid into `a / b` runs, so unwrap the
        // wrapper and let the nested table render as a real one.
        Block::Table(t)
            if t.is_single_cell() && (t.kind == TableKind::Layout || wraps_table(t)) =>
        {
            let crate::model::CellSlot::Origin(cell) = &t.grid[0][0] else { unreachable!() };
            let inner = render_blocks(&cell.blocks, rc);
            if inner.is_empty() { None } else { Some(inner) }
        }
        Block::Table(t) => table::render_table(t, rc),
        Block::BlockQuote(blocks) => {
            let inner = render_blocks(blocks, rc);
            if inner.is_empty() {
                return None;
            }
            let quoted: Vec<String> = inner
                .lines()
                .map(|l| if l.is_empty() { ">".to_string() } else { format!("> {l}") })
                .collect();
            Some(quoted.join("\n"))
        }
        Block::CodeBlock { lang, text } => {
            let fence = backtick_fence(text, 3);
            let lang = lang.as_deref().unwrap_or("");
            let body = text.trim_end_matches('\n');
            Some(format!("{fence}{lang}\n{body}\n{fence}"))
        }
        Block::Rule => Some("---".to_string()),
        Block::Math(tex) => {
            let tex = tex.trim();
            if tex.is_empty() {
                return None;
            }
            // A bare `$` is never valid inside math; escaped, it cannot
            // close the block early.
            let mut source = String::with_capacity(tex.len());
            let mut backslashes = 0;
            for c in tex.chars() {
                if c == '$' && backslashes % 2 == 0 {
                    source.push('\\');
                }
                source.push(c);
                backslashes = if c == '\\' { backslashes + 1 } else { 0 };
            }
            Some(format!("$$\n{source}\n$$"))
        }
    }
}

/// A list and whether it is loose. Items are tight unless one holds blocks
/// that need a blank line between them; nested lines are indented by the
/// width of the Markdown marker (`- `, `3. `), not of a label written after
/// it, so every level of a list indents the same way.
fn render_list(list: &List, rc: &Ctx) -> Option<(String, bool)> {
    if list.items.is_empty() {
        return None;
    }
    let mut rendered_items: Vec<String> = Vec::new();
    let mut loose = false;
    for (i, item) in list.items.iter().enumerate() {
        // GFM has decimal ordered lists only, so Roman/alphabetic levels
        // render as bullets carrying the source marker as literal text
        // (`- iv. …`) — the source marker semantics stay visible. Items with
        // an explicit label (composite number text) render it the same way.
        let (marker, label) = match (&item.marker_label, list.marker) {
            (Some(label), _) => {
                ("- ".to_string(), format!("{} ", escape_marker_label(label, InlineContext::Block)))
            }
            (None, MarkerKind::Bullet) => ("- ".to_string(), String::new()),
            (None, MarkerKind::Decimal) => {
                (format!("{}. ", list.start.saturating_add(i as u64)), String::new())
            }
            (None, kind) => {
                ("- ".to_string(), format!("{} ", kind.label(list.start.saturating_add(i as u64))))
            }
        };
        let (body, item_loose) = render_item_blocks(&item.blocks, rc);
        loose |= item_loose;
        let indent = " ".repeat(marker.len());
        let mut lines = body.lines();
        let first = lines.next().unwrap_or("");
        let mut s = format!("{marker}{label}{first}");
        for line in lines {
            s.push('\n');
            if !line.is_empty() {
                s.push_str(&indent);
                s.push_str(line);
            }
        }
        rendered_items.push(s);
    }
    let sep = if loose { "\n\n" } else { "\n" };
    Some((rendered_items.join(sep), loose))
}

/// An item's blocks, and whether they make its list loose. Blocks are
/// separated by a blank line, which makes the list loose, except a nested
/// list straight after a paragraph that CommonMark lets interrupt it, and a
/// tight nested list after one CommonMark merges it with.
fn render_item_blocks(blocks: &[Block], rc: &Ctx) -> (String, bool) {
    let mut out = String::new();
    let mut loose = false;
    let mut prev: Option<&Block> = None;
    // The previous nested list's last family and tightness.
    let mut prev_list: Option<(ListFamily, bool)> = None;
    for block in blocks {
        let (text, list) = match block {
            Block::List(list) => match render_list(list, rc) {
                Some((text, list_loose)) => (
                    text,
                    Some((
                        list_family(list, 0),
                        list_family(list, list.items.len() - 1),
                        list_loose,
                    )),
                ),
                None => continue,
            },
            _ => match render_block(block, rc) {
                Some(text) => (text, None),
                None => continue,
            },
        };
        if let Some(prev) = prev {
            let after_paragraph = matches!(prev, Block::Paragraph(_))
                && matches!(block, Block::List(list) if interrupts_paragraph(list, rc));
            let merges = matches!(
                (prev_list, list),
                (Some((last, false)), Some((first, _, false))) if last == first
            );
            let tight = after_paragraph || merges;
            out.push_str(if tight { "\n" } else { "\n\n" });
            loose |= !tight;
        }
        out.push_str(&text);
        prev = Some(block);
        prev_list = list.map(|(_, last, list_loose)| (last, list_loose));
    }
    (out, loose)
}

/// Whether a nested list may start on the line after a paragraph: its first
/// item has content, and a numbered one starts at 1 (CommonMark reads
/// `3. inner` there as paragraph text).
fn interrupts_paragraph(list: &List, rc: &Ctx) -> bool {
    let Some(first) = list.items.first() else { return false };
    let has_content = first.blocks.iter().any(|block| render_block(block, rc).is_some());
    has_content && (list_family(list, 0) == ListFamily::Bullet || list.start == 1)
}

/// Trim paragraph lines, keeping hard-break backslashes intact.
fn trim_paragraph(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| {
            let t = l.trim_start();
            let t = if ends_with_hard_break(t) { t } else { t.trim_end() };
            if t.trim_end_matches('\\').trim().is_empty() { "" } else { t }
        })
        .collect();
    let start = lines.iter().position(|l| !l.is_empty());
    let end = lines.iter().rposition(|l| !l.is_empty());
    match (start, end) {
        (Some(s), Some(e)) => {
            let mut out = lines[s..=e].join("\n");
            if ends_with_hard_break(&out) {
                out.pop();
                out.truncate(out.trim_end().len());
            }
            out
        }
        _ => String::new(),
    }
}

fn ends_with_hard_break(line: &str) -> bool {
    line.chars().rev().take_while(|&c| c == '\\').count() % 2 == 1
}
