//! ODF tables: canonical grid construction with covered-cell consumption,
//! repeats honored in every container, a hard expansion budget for
//! repeat expansion, and typed value-attribute fallback for cells without
//! display text.
//!
//! Empty filler runs *inside* the used range materialize fully so every
//! later cell keeps its source coordinates; only trailing filler (empty rows
//! at the end, empty cells at a row's end) is elided. All materialization is
//! charged against the fixed expansion budget.
//!
//! A spreadsheet's hidden rows and columns (`table:visibility` `collapse` or
//! `filter`) are dropped before their cells are parsed, spans shrink to the
//! rows and columns that remain, and the hidden rows and columns holding a
//! value are counted. Hidden sheets (a table style with
//! `table:display="false"`) are skipped whole and counted.

use crate::error::ConvertError;
use crate::formats::odf::text::{Ctx, parse_container};
use crate::model::{Block, Cell, GridBuilder, HiddenSheetContent, Inline, TableKind};
use crate::package::limits;
use crate::package::xml::{Element, Node, ns};
use crate::shared::header::resolve_header_rows;
use crate::shared::text::clean_text;
use std::collections::HashMap;

pub fn parse_table(elem: &Element, ctx: &Ctx) -> Result<Vec<Block>, ConvertError> {
    parse_table_in(elem, ctx, None)
}

/// [`parse_table`], with a spreadsheet's hidden rows and columns left out
/// when `sheet` holds them.
fn parse_table_in(
    elem: &Element,
    ctx: &Ctx,
    sheet: Option<&mut SheetHidden>,
) -> Result<Vec<Block>, ConvertError> {
    let mut state = TableState {
        builder: GridBuilder::new(),
        expansion: 0,
        expansion_bytes: 0,
        pending_rows: 0,
        header_rows: 0,
        rows_emitted: 0,
        checkboxes: read_checkboxes(elem),
        sheet,
        next_row: 0,
    };
    walk_rows(elem, ctx, &mut state, true)?;
    let mut table = state.builder.finish(TableKind::Data);
    if table.grid.is_empty() {
        return Ok(Vec::new());
    }
    table.header_rows = resolve_header_rows(&table, state.header_rows);
    Ok(vec![Block::Table(table)])
}

struct TableState<'s> {
    builder: GridBuilder,
    /// Grid slots produced by expansion, checked against the fixed budget.
    expansion: u64,
    /// Text bytes duplicated by expansion, checked against their own budget.
    expansion_bytes: u64,
    pending_rows: u64,
    header_rows: usize,
    rows_emitted: usize,
    /// The sheet's form checkboxes by control id, as the inlines a
    /// `draw:control` in a cell expands to.
    checkboxes: HashMap<String, Vec<Inline>>,
    /// A spreadsheet table's hidden rows and columns, and what they hold;
    /// `None` for the tables of text documents and presentations.
    sheet: Option<&'s mut SheetHidden>,
    /// Source position of the next `table:table-row`, hidden rows included.
    next_row: u64,
}

/// Sorted, disjoint, half-open ranges of source positions, with the length
/// before each range, so the positions they hold in any span count in
/// logarithmic time.
#[derive(Default)]
struct Ranges {
    ranges: Vec<(u64, u64)>,
    /// `before[i]`: total length of `ranges[..i]`.
    before: Vec<u64>,
}

impl Ranges {
    /// Append `[start, start + len)`, which must not start before the end
    /// of the last range.
    fn push(&mut self, start: u64, len: u64) {
        let end = start.saturating_add(len);
        if let Some(last) = self.ranges.last_mut()
            && last.1 == start
        {
            last.1 = end;
            return;
        }
        let total = match (self.ranges.last(), self.before.last()) {
            (Some(&(a, b)), Some(&before)) => before.saturating_add(b - a),
            _ => 0,
        };
        self.ranges.push((start, end));
        self.before.push(total);
    }

    /// Positions in the ranges below `x`.
    fn below(&self, x: u64) -> u64 {
        let i = self.ranges.partition_point(|&(start, _)| start < x);
        if i == 0 {
            return 0;
        }
        let (start, end) = self.ranges[i - 1];
        self.before[i - 1].saturating_add(end.min(x) - start)
    }

    /// Positions in the ranges within `[lo, lo + len)`.
    fn within(&self, lo: u64, len: u64) -> u64 {
        self.below(lo.saturating_add(len)) - self.below(lo)
    }
}

/// One spreadsheet table's hidden rows and columns, and the hidden ones
/// found holding a value.
struct SheetHidden {
    rows: Ranges,
    columns: Ranges,
    /// Hidden rows holding a value.
    rows_left_out: u64,
    /// Column spans of the cells holding a value that cover a hidden
    /// column, in no order and possibly overlapping.
    values_in_columns: Vec<(u64, u64)>,
}

impl SheetHidden {
    fn read(table: &Element) -> SheetHidden {
        let mut rows = Ranges::default();
        hidden_rows(table, &mut 0, &mut rows);
        let mut columns = Ranges::default();
        hidden_columns(table, &mut 0, &mut columns);
        SheetHidden { rows, columns, rows_left_out: 0, values_in_columns: Vec::new() }
    }

    /// Note that a cell holding a value spans columns `[at, at + len)`.
    fn value_at(&mut self, at: u64, len: u64) {
        if self.columns.within(at, len) > 0 {
            self.values_in_columns.push((at, at.saturating_add(len)));
        }
    }

    /// Hidden columns holding a value: the hidden positions in the union
    /// of the noted spans.
    fn columns_left_out(&mut self) -> u64 {
        self.values_in_columns.sort_unstable();
        let mut count = 0u64;
        let mut current: Option<(u64, u64)> = None;
        for &(a, b) in &self.values_in_columns {
            current = match current {
                Some((ca, cb)) if a <= cb => Some((ca, cb.max(b))),
                Some((ca, cb)) => {
                    count = count.saturating_add(self.columns.within(ca, cb - ca));
                    Some((a, b))
                }
                None => Some((a, b)),
            };
        }
        if let Some((ca, cb)) = current {
            count = count.saturating_add(self.columns.within(ca, cb - ca));
        }
        count
    }
}

/// Whether a row or column element is hidden: collapsed by hand or
/// filtered out.
fn is_hidden(elem: &Element) -> bool {
    matches!(elem.attr(ns::TABLE, "visibility"), Some("collapse" | "filter"))
}

fn repeat_of(elem: &Element, attr: &str) -> u64 {
    elem.attr(ns::TABLE, attr).and_then(|v| v.parse().ok()).unwrap_or(1).max(1)
}

/// The hidden rows, by source position, in the row containers
/// [`walk_rows`] reads.
fn hidden_rows(container: &Element, at: &mut u64, out: &mut Ranges) {
    for child in container.child_elems() {
        if child.ns.as_deref().is_none_or(|n| n != ns::TABLE) {
            continue;
        }
        match child.local.as_str() {
            "table-header-rows" | "table-rows" | "table-row-group" => {
                hidden_rows(child, at, out);
            }
            "table-row" => {
                let repeat = repeat_of(child, "number-rows-repeated");
                if is_hidden(child) {
                    out.push(*at, repeat);
                }
                *at = at.saturating_add(repeat);
            }
            _ => {}
        }
    }
}

/// The hidden columns, by source position, from the table's column
/// declarations.
fn hidden_columns(container: &Element, at: &mut u64, out: &mut Ranges) {
    for child in container.child_elems() {
        if child.ns.as_deref().is_none_or(|n| n != ns::TABLE) {
            continue;
        }
        match child.local.as_str() {
            "table-header-columns" | "table-columns" | "table-column-group" => {
                hidden_columns(child, at, out);
            }
            "table-column" => {
                let repeat = repeat_of(child, "number-columns-repeated");
                if is_hidden(child) {
                    out.push(*at, repeat);
                }
                *at = at.saturating_add(repeat);
            }
            _ => {}
        }
    }
}

/// Whether a cell holds a value: a typed value, text, or a drawing (an
/// image, a control). Comments are not values. Read from the markup alone,
/// so a hidden cell is never parsed.
fn holds_value(cell: &Element) -> bool {
    if !cell.is(ns::TABLE, "table-cell") {
        return false;
    }
    if cell.attr(ns::OFFICE, "value-type").is_some() {
        return true;
    }
    let mut stack: Vec<&Element> = vec![cell];
    while let Some(elem) = stack.pop() {
        for node in &elem.children {
            match node {
                Node::Text(text) if !text.trim().is_empty() => return true,
                Node::Text(_) => {}
                Node::Elem(child) => {
                    if child.is(ns::OFFICE, "annotation") {
                        continue;
                    }
                    if child.ns.as_deref() == Some(ns::DRAW) {
                        return true;
                    }
                    stack.push(child);
                }
            }
        }
    }
    false
}

/// Checkbox controls declared in the table's `office:forms`. The mixed
/// ("unknown") state has no token and is left out.
fn read_checkboxes(table: &Element) -> HashMap<String, Vec<Inline>> {
    let mut out = HashMap::new();
    for cb in table.find_all(ns::OFFICE, "forms").flat_map(|f| f.descendants(ns::FORM, "checkbox"))
    {
        let Some(id) = cb.attr_qualified(ns::XML, "id").or_else(|| cb.attr(ns::FORM, "id")) else {
            continue;
        };
        let state = cb.attr(ns::FORM, "current-state").or_else(|| cb.attr(ns::FORM, "state"));
        let checked = match state {
            Some("checked") => true,
            Some("unchecked") | None => false,
            Some(_) => continue,
        };
        let mut inlines = vec![Inline::Checkbox(checked)];
        let label = cb.attr(ns::FORM, "label").map(clean_text).unwrap_or_default();
        if !label.is_empty() {
            inlines.push(Inline::plain(format!(" {label}")));
        }
        out.insert(id.to_string(), inlines);
    }
    out
}

/// Append the checkboxes anchored in a cell after its content, on the same
/// line. Controls under `table:shapes` are anchored to the page by
/// coordinates alone, so they are not placed.
fn append_cell_checkboxes(
    cell: &Element,
    checkboxes: &HashMap<String, Vec<Inline>>,
    blocks: &mut Vec<Block>,
) {
    for control in cell.find_all(ns::DRAW, "control") {
        let Some(found) = control.attr(ns::DRAW, "control").and_then(|id| checkboxes.get(id))
        else {
            continue;
        };
        match blocks.last_mut() {
            Some(Block::Paragraph(inlines)) => {
                inlines.push(Inline::plain(" "));
                inlines.extend(found.iter().cloned());
            }
            _ => blocks.push(Block::Paragraph(found.clone())),
        }
    }
}

impl TableState<'_> {
    fn charge(&mut self, cells: u64) -> Result<(), ConvertError> {
        self.expansion = self.expansion.saturating_add(cells);
        if self.expansion > limits::MAX_EXPANSION {
            return Err(ConvertError::ResourceLimit {
                limit: "max_expansion",
                detail: "table repeat expansion exceeds the content budget".into(),
            });
        }
        Ok(())
    }

    /// Charge text bytes that repeat expansion duplicates: the slot budget
    /// bounds positions, this bounds the amplified content itself.
    fn charge_bytes(&mut self, bytes: u64) -> Result<(), ConvertError> {
        self.expansion_bytes = self.expansion_bytes.saturating_add(bytes);
        if self.expansion_bytes > limits::MAX_EXPANSION_TEXT_BYTES {
            return Err(ConvertError::ResourceLimit {
                limit: "max_expansion_text_bytes",
                detail: "table repeat expansion duplicates more text than the budget".into(),
            });
        }
        Ok(())
    }
}

/// One parsed cell of a row template, reused across row repeats.
enum RowCell {
    Covered { repeat: u64 },
    Cell { repeat: u64, col_span: u32, row_span: u32, blocks: Vec<Block>, bytes: u64 },
}

/// Approximate retained text bytes of parsed cell content.
fn block_bytes(blocks: &[Block]) -> u64 {
    fn inline_bytes(inlines: &[crate::model::Inline]) -> u64 {
        use crate::model::Inline;
        inlines
            .iter()
            .map(|i| match i {
                Inline::Text { text, .. } => text.len() as u64,
                Inline::Link { content, target } => {
                    let target_len = match target {
                        crate::model::LinkTarget::External(s)
                        | crate::model::LinkTarget::Relative(s)
                        | crate::model::LinkTarget::Anchor(s) => s.len() as u64,
                    };
                    inline_bytes(content) + target_len
                }
                Inline::Image { alt, .. } => alt.len() as u64,
                Inline::Math(tex) => tex.len() as u64,
                Inline::Anchor(id) | Inline::NoteRef(id) => id.len() as u64,
                Inline::Checkbox(_) => 3,
                Inline::LineBreak => 1,
            })
            .sum()
    }
    blocks
        .iter()
        .map(|b| match b {
            Block::Paragraph(i) | Block::Heading { content: i, .. } => inline_bytes(i),
            Block::Math(tex) => tex.len() as u64,
            Block::List(list) => {
                list.items.iter().map(|item| block_bytes(&item.blocks)).sum::<u64>()
            }
            Block::Table(t) => t
                .grid
                .iter()
                .flatten()
                .map(|slot| match slot {
                    crate::model::CellSlot::Origin(c) => block_bytes(&c.blocks),
                    crate::model::CellSlot::Covered { .. } => 0,
                })
                .sum(),
            Block::BlockQuote(blocks) => block_bytes(blocks),
            Block::CodeBlock { text, .. } => text.len() as u64,
            Block::Rule => 0,
        })
        .sum()
}

fn walk_rows(
    container: &Element,
    ctx: &Ctx,
    state: &mut TableState,
    top: bool,
) -> Result<(), ConvertError> {
    for child in container.child_elems() {
        if child.ns.as_deref().is_none_or(|n| n != ns::TABLE) {
            continue;
        }
        match child.local.as_str() {
            "table-header-rows" => {
                let before = state.rows_emitted;
                walk_rows(child, ctx, state, false)?;
                if top && state.header_rows == 0 {
                    state.header_rows = state.rows_emitted - before;
                }
            }
            "table-rows" | "table-row-group" => walk_rows(child, ctx, state, false)?,
            "table-row" => {
                let repeat = repeat_of(child, "number-rows-repeated");
                let at = state.next_row;
                state.next_row = at.saturating_add(repeat);
                if let Some(sheet) = state.sheet.as_deref_mut()
                    && is_hidden(child)
                {
                    leave_out_row(child, sheet, repeat);
                    continue;
                }
                emit_row(child, ctx, state, repeat, at)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Leave a hidden row out unparsed, counting it (each repeat) when it holds
/// a value, and noting the hidden columns its values sit in.
fn leave_out_row(row: &Element, sheet: &mut SheetHidden, repeat: u64) {
    let mut at = 0u64;
    let mut holds = false;
    for cell in row.child_elems() {
        if !(cell.is(ns::TABLE, "table-cell") || cell.is(ns::TABLE, "covered-table-cell")) {
            continue;
        }
        let len = repeat_of(cell, "number-columns-repeated");
        if holds_value(cell) {
            holds = true;
            sheet.value_at(at, len);
        }
        at = at.saturating_add(len);
    }
    if holds {
        sheet.rows_left_out = sheet.rows_left_out.saturating_add(repeat);
    }
}

fn emit_row(
    row: &Element,
    ctx: &Ctx,
    state: &mut TableState,
    repeat: u64,
    at: u64,
) -> Result<(), ConvertError> {
    if row_is_empty(row) {
        state.pending_rows = state.pending_rows.saturating_add(repeat);
        return Ok(());
    }
    // Materialize any buffered empty gap first, in full: rows after the gap
    // keep their source coordinates. The rows are charged so a pathological
    // gap hits the budget instead of memory.
    if state.pending_rows > 0 {
        state.charge(state.pending_rows)?;
        for _ in 0..state.pending_rows {
            state.builder.next_row();
            state.rows_emitted += 1;
        }
        state.pending_rows = 0;
    }
    // Parse the row template exactly once: repeated rows clone the parsed
    // cells instead of reparsing, so per-parse side effects (notes, assets)
    // happen once and the duplicated text bytes are charged up front.
    let cells = parse_row_cells(row, ctx, &state.checkboxes, state.sheet.as_deref_mut())?;
    state.charge(repeat.saturating_sub(1))?;
    for cell in &cells {
        if let RowCell::Cell { repeat: cell_repeat, bytes, .. } = cell {
            let copies = repeat.saturating_mul(*cell_repeat).saturating_sub(1);
            state.charge_bytes(bytes.saturating_mul(copies))?;
        }
    }
    for copy in 0..repeat {
        state.builder.next_row();
        state.rows_emitted += 1;
        emit_parsed_cells(&cells, state, at.saturating_add(copy))?;
    }
    Ok(())
}

/// Parse one row's cells into the reusable template. In a spreadsheet the
/// cells of hidden columns are dropped unparsed (their values noted), and a
/// span shrinks by the hidden columns it covers.
fn parse_row_cells(
    row: &Element,
    ctx: &Ctx,
    checkboxes: &HashMap<String, Vec<Inline>>,
    mut sheet: Option<&mut SheetHidden>,
) -> Result<Vec<RowCell>, ConvertError> {
    let mut out = Vec::new();
    let mut at = 0u64;
    for cell in row.child_elems() {
        if !(cell.is(ns::TABLE, "table-cell") || cell.is(ns::TABLE, "covered-table-cell")) {
            continue;
        }
        let source_repeat = repeat_of(cell, "number-columns-repeated");
        let cell_at = at;
        at = at.saturating_add(source_repeat);
        let mut repeat = source_repeat;
        if let Some(sheet) = sheet.as_deref_mut() {
            let hidden = sheet.columns.within(cell_at, source_repeat);
            if hidden > 0 {
                if holds_value(cell) {
                    sheet.value_at(cell_at, source_repeat);
                }
                repeat = source_repeat - hidden;
                if repeat == 0 {
                    continue;
                }
            }
        }
        if cell.is(ns::TABLE, "covered-table-cell") {
            out.push(RowCell::Covered { repeat });
            continue;
        }
        let mut col_span: u32 = cell
            .attr(ns::TABLE, "number-columns-spanned")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
            .max(1);
        if let Some(sheet) = sheet.as_deref()
            && col_span > 1
        {
            let hidden = sheet.columns.within(cell_at, u64::from(col_span));
            col_span = col_span.saturating_sub(u32::try_from(hidden).unwrap_or(u32::MAX)).max(1);
        }
        let row_span: u32 = cell
            .attr(ns::TABLE, "number-rows-spanned")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
            .max(1);
        let mut blocks = cell_blocks(cell, ctx)?;
        append_cell_checkboxes(cell, checkboxes, &mut blocks);
        let bytes = block_bytes(&blocks);
        out.push(RowCell::Cell { repeat, col_span, row_span, blocks, bytes });
    }
    Ok(out)
}

/// Place a parsed row at source row `row`; in a spreadsheet a row span
/// shrinks by the hidden rows it covers.
fn emit_parsed_cells(
    cells: &[RowCell],
    state: &mut TableState,
    row: u64,
) -> Result<(), ConvertError> {
    let mut pending_cells: u64 = 0;
    for cell in cells {
        match cell {
            RowCell::Covered { repeat } => {
                flush_gap(state, &mut pending_cells)?;
                // One explicitly written covered position each; a stray one
                // (no span accounts for it) becomes an empty cell inside
                // covered().
                state.charge(*repeat)?;
                for _ in 0..*repeat {
                    if !state.builder.covered() {
                        log::debug!("covered table cell without a spanning origin");
                    }
                }
            }
            RowCell::Cell { repeat, col_span, row_span, blocks, .. } => {
                if blocks.is_empty() && *col_span == 1 && *row_span == 1 {
                    pending_cells = pending_cells.saturating_add(*repeat);
                    continue;
                }
                flush_gap(state, &mut pending_cells)?;
                state.charge(repeat.saturating_mul(*col_span as u64))?;
                let mut row_span = *row_span;
                if let Some(sheet) = state.sheet.as_deref()
                    && row_span > 1
                {
                    let hidden = sheet.rows.within(row, u64::from(row_span));
                    row_span =
                        row_span.saturating_sub(u32::try_from(hidden).unwrap_or(u32::MAX)).max(1);
                }
                for _ in 0..*repeat {
                    state.builder.place(Cell::spanning(blocks.clone(), *col_span, row_span))?;
                }
            }
        }
    }
    Ok(())
}

/// Materialize a buffered empty-cell run in full so the next cell lands on
/// its source column. Trailing runs are never flushed and stay elided.
fn flush_gap(state: &mut TableState, pending: &mut u64) -> Result<(), ConvertError> {
    if *pending == 0 {
        return Ok(());
    }
    state.charge(*pending)?;
    for _ in 0..*pending {
        state.builder.place(Cell::default())?;
    }
    *pending = 0;
    Ok(())
}

/// A cell's blocks: its text content, or a typed value-attribute fallback
/// when the producer wrote no display text.
fn cell_blocks(cell: &Element, ctx: &Ctx) -> Result<Vec<Block>, ConvertError> {
    let blocks = parse_container(cell, ctx)?;
    let has_content = blocks.iter().any(|b| match b {
        Block::Paragraph(inlines) => !crate::model::inlines_are_empty(inlines),
        _ => true,
    });
    if has_content {
        return Ok(blocks);
    }
    match value_text(cell) {
        Some(text) => Ok(vec![Block::Paragraph(vec![Inline::plain(text)])]),
        None => Ok(Vec::new()),
    }
}

fn value_text(cell: &Element) -> Option<String> {
    let value_type = cell.attr(ns::OFFICE, "value-type")?;
    match value_type {
        "percentage" => {
            let v: f64 = cell.attr(ns::OFFICE, "value")?.parse().ok()?;
            Some(format!("{}%", trim_float(v * 100.0)))
        }
        "currency" => {
            let v = cell.attr(ns::OFFICE, "value")?;
            let cur = cell.attr(ns::OFFICE, "currency").unwrap_or("");
            Some(if cur.is_empty() {
                trim_float(v.parse().ok()?)
            } else {
                format!("{} {cur}", trim_float(v.parse().ok()?))
            })
        }
        "float" => Some(trim_float(cell.attr(ns::OFFICE, "value")?.parse().ok()?)),
        "date" => cell.attr(ns::OFFICE, "date-value").map(str::to_string),
        "time" => cell.attr(ns::OFFICE, "time-value").map(duration_text),
        "boolean" => cell
            .attr(ns::OFFICE, "boolean-value")
            .map(|b| if b == "true" { "TRUE" } else { "FALSE" }.to_string()),
        "string" => cell.attr(ns::OFFICE, "string-value").map(str::to_string),
        _ => None,
    }
}

/// Shortest round-trip float formatting - no fixed-precision rounding.
fn trim_float(v: f64) -> String {
    format!("{v}")
}

/// Full ISO 8601 duration -> clock text: `P1DT2H` -> `26:00:00`,
/// `PT26H30M15S` -> `26:30:15`, weeks are 7 days, and fractional values are
/// valid on any component (`PT1.5H` -> `1:30:00`). Year/month components
/// have no fixed length, and unrepresentably large values overflow the
/// clock format, so both keep their raw ISO text.
fn duration_text(iso: &str) -> String {
    let mut total = 0.0f64; // seconds
    let mut number = String::new();
    let mut in_time = false;
    let mut negative = false;
    for c in iso.chars() {
        match c {
            '-' if total == 0.0 && number.is_empty() => negative = true,
            'P' => {}
            'T' => {
                in_time = true;
                number.clear();
            }
            'Y' | 'M' if !in_time => {
                if number.parse::<f64>().is_ok_and(|n| n != 0.0) {
                    return iso.to_string();
                }
                number.clear();
            }
            'W' | 'D' if !in_time => {
                let unit = if c == 'W' { 604_800.0 } else { 86_400.0 };
                total += number.parse::<f64>().unwrap_or(0.0) * unit;
                number.clear();
            }
            'H' | 'M' | 'S' if in_time => {
                let unit = match c {
                    'H' => 3600.0,
                    'M' => 60.0,
                    _ => 1.0,
                };
                total += number.parse::<f64>().unwrap_or(0.0) * unit;
                number.clear();
            }
            c if c.is_ascii_digit() || c == '.' => number.push(c),
            _ => number.clear(),
        }
    }
    // Round to milliseconds; a value the clock format cannot represent
    // exactly keeps its source text instead of silently degrading.
    let total_ms = (total * 1000.0).round();
    if !(0.0..1e18).contains(&total_ms) {
        return iso.to_string();
    }
    let total_ms = total_ms as u64;
    let sign = if negative { "-" } else { "" };
    let (hours, rest) = (total_ms / 3_600_000, total_ms % 3_600_000);
    let (minutes, ms) = (rest / 60_000, rest % 60_000);
    if ms % 1000 != 0 {
        format!("{sign}{hours}:{minutes:02}:{:06.3}", ms as f64 / 1000.0)
    } else {
        format!("{sign}{hours}:{minutes:02}:{:02}", ms / 1000)
    }
}

fn row_is_empty(row: &Element) -> bool {
    row.child_elems().all(|cell| {
        if cell.is(ns::TABLE, "covered-table-cell") {
            return false; // structural: belongs to a span
        }
        if !cell.is(ns::TABLE, "table-cell") {
            return true;
        }
        cell.attr(ns::TABLE, "number-columns-spanned").is_none()
            && cell.attr(ns::TABLE, "number-rows-spanned").is_none()
            && cell.attr(ns::OFFICE, "value-type").is_none()
            && cell.children.is_empty()
    })
}

/// A spreadsheet body's visible sheets, and the hidden content left out:
/// hidden sheets whole, and the hidden rows and columns of the visible ones
/// that hold a value.
pub fn parse_spreadsheet(
    sheet: &Element,
    ctx: &Ctx,
) -> Result<(Vec<Block>, HiddenSheetContent), ConvertError> {
    let mut hidden = HiddenSheetContent::default();
    let mut tables: Vec<&Element> = Vec::new();
    for table in sheet.child_elems().filter(|e| e.is(ns::TABLE, "table")) {
        if table.attr(ns::TABLE, "style-name").is_some_and(|name| ctx.styles.hides_table(name)) {
            hidden.sheets = hidden.sheets.saturating_add(1);
        } else {
            tables.push(table);
        }
    }
    let multi_sheet = tables.len() > 1;
    let mut blocks = Vec::new();
    let saturate = |n: u64| u32::try_from(n).unwrap_or(u32::MAX);
    for table in tables {
        let name = table.attr(ns::TABLE, "name").unwrap_or("");
        let mut sheet_hidden = SheetHidden::read(table);
        let content = parse_table_in(table, ctx, Some(&mut sheet_hidden))?;
        hidden.rows = hidden.rows.saturating_add(saturate(sheet_hidden.rows_left_out));
        hidden.columns = hidden.columns.saturating_add(saturate(sheet_hidden.columns_left_out()));
        if content.is_empty() {
            continue;
        }
        if multi_sheet {
            blocks.push(Block::heading(2, vec![Inline::plain(name)]));
        }
        blocks.extend(content);
    }
    Ok((blocks, hidden))
}
