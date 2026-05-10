//! Terminal layout for matrices ([`MatElem`]), vectors ([`VecElem`]), and
//! case distinctions ([`CasesElem`]).
//!
//! All three element types share a common helper [`wrap_with_delimiters`] that
//! places stretched left/right delimiter frames around a body frame.

use crossterm::style::ContentStyle;
use lynchpin_library_ng::frame::{Col, Row, TermFrame, TermPoint, TermSize};
use tracing::debug;
use typst::diag::SourceResult;
use typst::foundations::{Packed, StyleChain};
use typst::math::{CasesElem, MatElem, VecElem};

use super::fragment::MathClass;
use super::run::{build_delimiter_frame, compose_horizontal, compose_vertical};
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
        let empty = TermFrame::new(TermSize::new(Col::new(1), Row::new(1)));
        ctx.push(wrap_with_delimiters(
            ctx,
            empty,
            delim.open(),
            delim.close(),
        ));
        return Ok(());
    }

    // Layout each child.
    let mut frames: Vec<TermFrame> = Vec::with_capacity(elem.children.len());
    for child in &elem.children {
        frames.push(ctx.layout_into_frame(child, styles)?);
    }

    // Baseline at the middle child.
    let mid_idx = frames.len() / 2;
    let body = compose_vertical(frames, Row::ZERO, mid_idx);

    let body_cols = body.cols();
    let body_rows = body.rows();
    let delim = elem.delim.get(styles);
    let result = wrap_with_delimiters(ctx, body, delim.open(), delim.close());
    debug!("layout_vec: body cols={:?} rows={:?}, composed cols={:?} rows={:?}",
        body_cols, body_rows, result.frame.cols(), result.frame.rows());
    ctx.push(result);
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
        let empty = TermFrame::new(TermSize::new(Col::new(1), Row::new(1)));
        ctx.push(wrap_with_delimiters(
            ctx,
            empty,
            delim.open(),
            delim.close(),
        ));
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
        let empty = TermFrame::new(TermSize::new(Col::new(1), Row::new(1)));
        ctx.push(wrap_with_delimiters(
            ctx,
            empty,
            delim.open(),
            delim.close(),
        ));
        return Ok(());
    }

    // Maximum width per column (at least 1).
    let col_widths: Vec<Col> = (0..num_cols)
        .map(|c| {
            cell_grid
                .iter()
                .map(|row| row.get(c).map(|f| f.cols()).unwrap_or(Col::ZERO))
                .max()
                .unwrap_or(Col::ZERO)
                .max(Col::new(1))
        })
        .collect();

    // Maximum height per row (at least 1).
    let row_heights: Vec<Row> = cell_grid
        .iter()
        .map(|row| {
            row.iter()
                .map(|f| f.rows())
                .max()
                .unwrap_or(Row::ZERO)
                .max(Row::new(1))
        })
        .collect();

    let col_gap: Col = Col::new(1);
    let row_gap: Row = Row::new(1);

    let total_cols: Col =
        col_widths.iter().copied().sum::<Col>() + col_gap * (num_cols.saturating_sub(1) as f64);
    let total_rows: Row =
        row_heights.iter().copied().sum::<Row>() + row_gap * (num_rows.saturating_sub(1) as f64);

    // Baseline at the vertical centre of the matrix.
    let baseline: Row = (total_rows / Col::new(2)).max(Row::ZERO);

    let mut body = TermFrame::new(TermSize::new(total_cols.max(Col::new(1)), total_rows.max(Row::new(1))));
    body.set_baseline(baseline);

    let mut y: Row = Row::ZERO;
    for (ri, row) in cell_grid.into_iter().enumerate() {
        let row_height = row_heights[ri];
        let mut x: Col = Col::ZERO;
        for (ci, cell) in row.into_iter().enumerate() {
            let col_width = col_widths[ci];
            // Centre cell within its column / row slot.
            let cx = x + ((col_width - cell.cols()) / Col::new(2)).max(Col::ZERO);
            let cy = y + ((row_height - cell.rows()) / Col::new(2)).max(Row::ZERO);
            body.push_frame(TermPoint::new(cx, cy), cell);
            x = x + col_width + col_gap;
        }
        y = y + row_height + row_gap;
    }

    let body_cols = body.cols();
    let body_rows = body.rows();
    let body_baseline = body.baseline();
    let body_positions: Vec<_> = body.items().iter().map(|(p, _)| (p.col, p.row)).collect();
    let delim = elem.delim.get(styles);
    let result = wrap_with_delimiters(ctx, body, delim.open(), delim.close());
    debug!(
        "layout_mat: body cols={:?} rows={:?} baseline={:?}, cell_positions={:?}, composed cols={:?} rows={:?}",
        body_cols, body_rows, body_baseline, body_positions,
        result.frame.cols(), result.frame.rows(),
    );
    ctx.push(result);
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
        let empty = TermFrame::new(TermSize::new(Col::new(1), Row::new(1)));
        let (open, close) = cases_delimiters(delim.open(), reverse);
        ctx.push(wrap_with_delimiters(ctx, empty, open, close));
        return Ok(());
    }

    // Layout each branch (left-aligned stacking via compose_vertical).
    let mut frames: Vec<TermFrame> = Vec::with_capacity(elem.children.len());
    for child in &elem.children {
        frames.push(ctx.layout_into_frame(child, styles)?);
    }

    let mut body = compose_vertical(frames, Row::ZERO, 0);
    body.set_baseline(body.rows() / Col::new(2));

    let (open, close) = cases_delimiters(delim.open(), reverse);
    ctx.push(wrap_with_delimiters(ctx, body, open, close));
    Ok(())
}

/// Determine which side of a `cases` block gets the brace delimiter.
fn cases_delimiters(delim_open: Option<char>, reverse: bool) -> (Option<char>, Option<char>) {
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
    let height = body.rows().max(Row::new(1));

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

    let composed = compose_horizontal(parts, Col::ZERO);
    TermMathFrameFragment::new(composed).with_class(MathClass::Normal)
}

// ── Delimiter stretching ──────────────────────────────────────────────────────

/// Map a delimiter character to a vertically stretched sequence of characters
/// of the given height using the configured render mode.
fn stretched_delimiter(ctx: &TermMathContext, ch: char, height: Row) -> Vec<char> {
    let mode = ctx.config.mode;
    let h = height.max(Row::new(1)).get() as usize;
    match ch {
        '(' => {
            if mode.is_unicode() {
                match h {
                    1 => vec!['('],
                    2 => vec!['⎛', '⎝'],
                    _ => {
                        let mut v = Vec::with_capacity(h);
                        v.push('⎛');
                        for _ in 0..h - 2 { v.push('⎜'); }
                        v.push('⎝');
                        v
                    }
                }
            } else { vec!['('; h] }
        }
        ')' => {
            if mode.is_unicode() {
                match h {
                    1 => vec![')'],
                    2 => vec!['⎞', '⎠'],
                    _ => {
                        let mut v = Vec::with_capacity(h);
                        v.push('⎞');
                        for _ in 0..h - 2 { v.push('⎟'); }
                        v.push('⎠');
                        v
                    }
                }
            } else { vec![')'; h] }
        }
        '[' => {
            if mode.is_unicode() {
                match h {
                    1 => vec!['['],
                    2 => vec!['⎡', '⎣'],
                    _ => {
                        let mut v = Vec::with_capacity(h);
                        v.push('⎡');
                        for _ in 0..h - 2 { v.push('⎢'); }
                        v.push('⎣');
                        v
                    }
                }
            } else {
                match h {
                    1 => vec!['['],
                    2 => vec!['[', '['],
                    _ => {
                        let mut v = Vec::with_capacity(h);
                        v.push('[');
                        for _ in 0..h - 2 { v.push('|'); }
                        v.push('[');
                        v
                    }
                }
            }
        }
        ']' => {
            if mode.is_unicode() {
                match h {
                    1 => vec![']'],
                    2 => vec!['⎤', '⎦'],
                    _ => {
                        let mut v = Vec::with_capacity(h);
                        v.push('⎤');
                        for _ in 0..h - 2 { v.push('⎥'); }
                        v.push('⎦');
                        v
                    }
                }
            } else {
                match h {
                    1 => vec![']'],
                    2 => vec![']', ']'],
                    _ => {
                        let mut v = Vec::with_capacity(h);
                        v.push(']');
                        for _ in 0..h - 2 { v.push('|'); }
                        v.push(']');
                        v
                    }
                }
            }
        }
        '{' => {
            if mode.is_unicode() {
                match h {
                    1 => vec!['{'],
                    2 => vec!['⎧', '⎩'],
                    3 => vec!['⎧', '⎨', '⎩'],
                    _ => {
                        let mid_rows = h - 3;
                        let top_mids = mid_rows / 2;
                        let bot_mids = mid_rows - top_mids;
                        let mut v = Vec::with_capacity(h);
                        v.push('⎧');
                        v.extend(std::iter::repeat('⎪').take(top_mids));
                        v.push('⎨');
                        v.extend(std::iter::repeat('⎪').take(bot_mids));
                        v.push('⎩');
                        v
                    }
                }
            } else {
                match h {
                    1 => vec!['{'],
                    2 => vec!['/', '\\'],
                    _ => {
                        let mut v = Vec::with_capacity(h);
                        v.push('/');
                        for _ in 0..h - 2 { v.push('|'); }
                        v.push('\\');
                        v
                    }
                }
            }
        }
        '}' => {
            if mode.is_unicode() {
                match h {
                    1 => vec!['}'],
                    2 => vec!['⎫', '⎭'],
                    3 => vec!['⎫', '⎬', '⎭'],
                    _ => {
                        let mid_rows = h - 3;
                        let top_mids = mid_rows / 2;
                        let bot_mids = mid_rows - top_mids;
                        let mut v = Vec::with_capacity(h);
                        v.push('⎫');
                        v.extend(std::iter::repeat('⎪').take(top_mids));
                        v.push('⎬');
                        v.extend(std::iter::repeat('⎪').take(bot_mids));
                        v.push('⎭');
                        v
                    }
                }
            } else {
                match h {
                    1 => vec!['}'],
                    2 => vec!['\\', '/'],
                    _ => {
                        let mut v = Vec::with_capacity(h);
                        v.push('\\');
                        for _ in 0..h - 2 { v.push('|'); }
                        v.push('/');
                        v
                    }
                }
            }
        }
        '|' => vec![mode.vbar(); h],
        '‖' | '∥' => vec![if mode.is_unicode() { '║' } else { '|' }; h],
        '⌊' => {
            if mode.is_unicode() {
                let mut v = Vec::with_capacity(h);
                for _ in 0..h - 1 { v.push('⎢'); }
                v.push('⌊');
                v
            } else {
                let mut v = Vec::with_capacity(h);
                for _ in 0..h - 1 { v.push('|'); }
                v.push('[');
                v
            }
        }
        '⌋' => {
            if mode.is_unicode() {
                let mut v = Vec::with_capacity(h);
                for _ in 0..h - 1 { v.push('⎥'); }
                v.push('⌋');
                v
            } else {
                let mut v = Vec::with_capacity(h);
                for _ in 0..h - 1 { v.push('|'); }
                v.push(']');
                v
            }
        }
        '⌈' => {
            if mode.is_unicode() {
                let mut v = Vec::with_capacity(h);
                v.push('⌈');
                for _ in 0..h - 1 { v.push('⎢'); }
                v
            } else {
                let mut v = Vec::with_capacity(h);
                v.push('[');
                for _ in 0..h - 1 { v.push('|'); }
                v
            }
        }
        '⌉' => {
            if mode.is_unicode() {
                let mut v = Vec::with_capacity(h);
                v.push('⌉');
                for _ in 0..h - 1 { v.push('⎥'); }
                v
            } else {
                let mut v = Vec::with_capacity(h);
                v.push(']');
                for _ in 0..h - 1 { v.push('|'); }
                v
            }
        }
        '⟨' | '〈' => vec![if mode.is_unicode() { '⟨' } else { '<' }; h],
        '⟩' | '〉' => vec![if mode.is_unicode() { '⟩' } else { '>' }; h],
        _ => vec![ch; h],
    }
}
