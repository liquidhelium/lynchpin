//! Terminal layout for matrices ([`MatElem`]), vectors ([`VecElem`]), and
//! case distinctions ([`CasesElem`]).
//!
//! All three element types share a common helper [`wrap_with_delimiters`] that
//! places stretched left/right delimiter frames around a body frame.

use crossterm::style::ContentStyle;
use typst::diag::SourceResult;
use typst::foundations::{Packed, StyleChain};
use typst::math::{CasesElem, MatElem, VecElem};
use unicode_math_class::MathClass;

use crate::frame::{Col, Row, TermFrame, TermPoint, TermSize};
use crate::stack::{compose_horizontal, compose_vertical};

use super::run::build_delimiter_frame;
use super::{TermMathContext, TermMathFrameFragment};

// ── layout_vec ────────────────────────────────────────────────────────────────

/// Layout a [`VecElem`]: children stacked vertically, wrapped with delimiters.
pub fn layout_vec(
    elem: &Packed<VecElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    if elem.children.is_empty() {
        let delim = elem.delim.get(styles);
        let empty = TermFrame::new(TermSize::new(1, 1));
        ctx.push(wrap_with_delimiters(ctx, empty, delim.open(), delim.close()));
        return Ok(());
    }

    // Layout each child.
    let mut frames: Vec<TermFrame> = Vec::with_capacity(elem.children.len());
    for child in &elem.children {
        frames.push(ctx.layout_into_frame(child, styles)?);
    }

    // Baseline at the middle child.
    let mid_idx = frames.len() / 2;
    let body = compose_vertical(frames, 0, mid_idx);

    let delim = elem.delim.get(styles);
    ctx.push(wrap_with_delimiters(ctx, body, delim.open(), delim.close()));
    Ok(())
}

// ── layout_mat ────────────────────────────────────────────────────────────────

/// Layout a [`MatElem`]: a 2-D grid of cells wrapped with delimiters.
pub fn layout_mat(
    elem: &Packed<MatElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    if elem.rows.is_empty() {
        let delim = elem.delim.get(styles);
        let empty = TermFrame::new(TermSize::new(1, 1));
        ctx.push(wrap_with_delimiters(ctx, empty, delim.open(), delim.close()));
        return Ok(());
    }

    // Layout all cells.
    let mut cell_grid: Vec<Vec<TermFrame>> = Vec::with_capacity(elem.rows.len());
    for row in &elem.rows {
        let mut row_frames: Vec<TermFrame> = Vec::with_capacity(row.len());
        for cell in row {
            row_frames.push(ctx.layout_into_frame(cell, styles)?);
        }
        cell_grid.push(row_frames);
    }

    let num_rows = cell_grid.len();
    let num_cols = cell_grid.iter().map(|r| r.len()).max().unwrap_or(0);

    if num_cols == 0 {
        let delim = elem.delim.get(styles);
        let empty = TermFrame::new(TermSize::new(1, 1));
        ctx.push(wrap_with_delimiters(ctx, empty, delim.open(), delim.close()));
        return Ok(());
    }

    // Maximum width per column (at least 1).
    let col_widths: Vec<Col> = (0..num_cols)
        .map(|c| {
            cell_grid
                .iter()
                .map(|row| row.get(c).map(|f| f.cols()).unwrap_or(0))
                .max()
                .unwrap_or(0)
                .max(1)
        })
        .collect();

    // Maximum height per row (at least 1).
    let row_heights: Vec<Row> = cell_grid
        .iter()
        .map(|row| row.iter().map(|f| f.rows()).max().unwrap_or(0).max(1))
        .collect();

    let col_gap: Col = 1;
    let row_gap: Row = 1;

    let total_cols: Col = col_widths.iter().sum::<Col>()
        + col_gap * (num_cols.saturating_sub(1)) as Col;
    let total_rows: Row = row_heights.iter().sum::<Row>()
        + row_gap * (num_rows.saturating_sub(1)) as Row;

    // Baseline at the vertical centre of the matrix.
    let baseline: Row = (total_rows / 2).max(0);

    let mut body = TermFrame::new(TermSize::new(total_cols.max(1), total_rows.max(1)));
    body.set_baseline(baseline);

    let mut y: Row = 0;
    for (ri, row) in cell_grid.into_iter().enumerate() {
        let row_height = row_heights[ri];
        let mut x: Col = 0;
        for (ci, cell) in row.into_iter().enumerate() {
            let col_width = col_widths[ci];
            // Centre cell within its column / row slot.
            let cx = x + ((col_width - cell.cols()) / 2).max(0);
            let cy = y + ((row_height - cell.rows()) / 2).max(0);
            body.push_frame(TermPoint::new(cx, cy), cell);
            x += col_width + col_gap;
        }
        y += row_height + row_gap;
    }

    let delim = elem.delim.get(styles);
    ctx.push(wrap_with_delimiters(ctx, body, delim.open(), delim.close()));
    Ok(())
}

// ── layout_cases ─────────────────────────────────────────────────────────────

/// Layout a [`CasesElem`]: children stacked vertically with a single
/// delimiter on the left (or right if `reverse: true`).
pub fn layout_cases(
    elem: &Packed<CasesElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let reverse = elem.reverse.get(styles);
    let delim = elem.delim.get(styles);

    if elem.children.is_empty() {
        let empty = TermFrame::new(TermSize::new(1, 1));
        let (open, close) = cases_delimiters(delim.open(), reverse);
        ctx.push(wrap_with_delimiters(ctx, empty, open, close));
        return Ok(());
    }

    // Layout each branch (left-aligned stacking via compose_vertical).
    let mut frames: Vec<TermFrame> = Vec::with_capacity(elem.children.len());
    for child in &elem.children {
        frames.push(ctx.layout_into_frame(child, styles)?);
    }

    // Baseline at the first child (top-most) for standard alignment.
    let body = compose_vertical(frames, 0, 0);

    // For `cases`, only a single delimiter is shown (left for normal,
    // right for reversed).
    let (open, close) = cases_delimiters(delim.open(), reverse);
    ctx.push(wrap_with_delimiters(ctx, body, open, close));
    Ok(())
}

/// Determine which side of a `cases` block gets the brace delimiter.
///
/// * `reverse: false` — `{` on the left, nothing on the right.
/// * `reverse: true`  — nothing on the left, `{` on the right.
fn cases_delimiters(
    delim_open: Option<char>,
    reverse: bool,
) -> (Option<char>, Option<char>) {
    if reverse {
        (None, delim_open)
    } else {
        (delim_open, None)
    }
}

// ── wrap_with_delimiters ──────────────────────────────────────────────────────

/// Wrap `body` with optional stretched left and right delimiter frames.
///
/// Each delimiter is stretched to the full height of `body`.  The composed
/// fragment's baseline is aligned with `body`'s baseline via
/// [`compose_horizontal`].
pub fn wrap_with_delimiters(
    ctx: &TermMathContext,
    body: TermFrame,
    open: Option<char>,
    close: Option<char>,
) -> TermMathFrameFragment {
    let height = body.rows().max(1);

    let mut parts: Vec<TermFrame> = Vec::with_capacity(3);

    if let Some(ch) = open {
        let chars = stretched_delimiter(ctx, ch, height);
        parts.push(build_delimiter_frame(chars, ContentStyle::default()));
    }

    parts.push(body);

    if let Some(ch) = close {
        let chars = stretched_delimiter(ctx, ch, height);
        parts.push(build_delimiter_frame(chars, ContentStyle::default()));
    }

    let composed = compose_horizontal(parts, 0);
    TermMathFrameFragment::new(composed).with_class(MathClass::Normal)
}

// ── Delimiter stretching ──────────────────────────────────────────────────────

/// Map a delimiter character to a vertically stretched sequence of characters
/// of the given height using the configured render mode.
fn stretched_delimiter(ctx: &TermMathContext, ch: char, height: Row) -> Vec<char> {
    let mode = ctx.config.mode;
    match ch {
        '(' => mode.left_paren(height),
        ')' => mode.right_paren(height),
        '[' => mode.left_bracket(height),
        ']' => mode.right_bracket(height),
        '{' => mode.left_brace(height),
        '}' => mode.right_brace(height),
        '|' => mode.vert_bar(height),
        // Double vertical bar — U+2016 (‖) and U+2225 (∥).
        '‖' | '∥' => mode.double_vert_bar(height),
        '⌊' => mode.floor_left(height),
        '⌋' => mode.floor_right(height),
        '⌈' => mode.ceil_left(height),
        '⌉' => mode.ceil_right(height),
        '⟨' | '〈' => mode.angle_left(height),
        '⟩' | '〉' => mode.angle_right(height),
        // Fallback: repeat the character unchanged.
        _ => vec![ch; height.max(1) as usize],
    }
}
