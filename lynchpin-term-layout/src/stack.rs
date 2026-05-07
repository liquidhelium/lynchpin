//! Layout composition helpers.
//!
//! Functions for composing [`TermFrame`]s horizontally and vertically, with
//! correct baseline alignment and centering.

use crossterm::style::ContentStyle;

use crate::frame::{Col, Row, TermFrame, TermPoint, TermSize};

// ── Horizontal composition ────────────────────────────────────────────────────

/// Compose frames horizontally, aligning on their baselines.
///
/// All frames are placed so their baseline rows line up.
/// The result frame has:
/// - `width`    = sum of all frame widths + `gap` between each pair
/// - `height`   = `max_ascent + max_descent`
/// - `baseline` = `max_ascent`
///
/// A `gap` of 0 means frames are placed immediately adjacent.
pub fn compose_horizontal(frames: Vec<TermFrame>, gap: Col) -> TermFrame {
    if frames.is_empty() {
        return TermFrame::new(TermSize::ZERO);
    }

    let n = frames.len();

    let max_ascent: Row = frames.iter().map(|f| f.ascent()).max().unwrap_or(0);
    let max_descent: Row = frames.iter().map(|f| f.descent()).max().unwrap_or(1);
    let total_rows = (max_ascent + max_descent).max(1);

    let content_width: Col = frames.iter().map(|f| f.cols()).sum();
    let gap_width: Col = gap * (n.saturating_sub(1)) as Col;
    let total_cols = (content_width + gap_width).max(0);

    let mut out = TermFrame::new(TermSize::new(total_cols, total_rows));
    out.set_baseline(max_ascent);

    let mut x: Col = 0;
    for (i, frame) in frames.into_iter().enumerate() {
        let w = frame.cols();
        // Shift down from the common baseline so that each frame's own
        // baseline aligns with max_ascent.
        let row = max_ascent - frame.ascent();
        out.push_frame(TermPoint::new(x, row), frame);
        x += w;
        if i + 1 < n {
            x += gap;
        }
    }

    out
}

// ── Vertical composition ──────────────────────────────────────────────────────

/// Compose frames vertically, centered horizontally.
///
/// Frames are stacked top-to-bottom with `gap` rows between each pair.
/// The result frame's baseline is determined by `baseline_idx`: it is placed
/// at the start of the frame at that index plus that frame's own baseline.
///
/// If `baseline_idx` is out of range it is clamped to the last frame.
pub fn compose_vertical(frames: Vec<TermFrame>, gap: Row, baseline_idx: usize) -> TermFrame {
    if frames.is_empty() {
        return TermFrame::new(TermSize::ZERO);
    }

    let n = frames.len();
    let max_cols: Col = frames.iter().map(|f| f.cols()).max().unwrap_or(0);

    // Pre-compute row heights (minimum 1 so zero-height frames still advance).
    let heights: Vec<Row> = frames.iter().map(|f| f.rows().max(1)).collect();

    let gap_rows: Row = gap * (n.saturating_sub(1)) as Row;
    let total_rows: Row = heights.iter().sum::<Row>() + gap_rows;

    // Baseline: top of the baseline_idx frame + that frame's baseline.
    let bi = baseline_idx.min(n.saturating_sub(1));
    let bi_frame_baseline = frames[bi].baseline();
    let mut baseline_row: Row = 0;
    for i in 0..bi {
        baseline_row += heights[i] + gap;
    }
    baseline_row += bi_frame_baseline;

    let mut out = TermFrame::new(TermSize::new(max_cols.max(0), total_rows.max(0)));
    out.set_baseline(baseline_row.max(0));

    let mut y: Row = 0;
    for (i, frame) in frames.into_iter().enumerate() {
        // Center the frame horizontally within the widest frame.
        let cx = ((max_cols - frame.cols()) / 2).max(0);
        out.push_frame(TermPoint::new(cx, y), frame);
        y += heights[i];
        if i + 1 < n {
            y += gap;
        }
    }

    out
}

// ── Centering ─────────────────────────────────────────────────────────────────

/// Return the column offset to center `content_cols` within `total_cols`.
///
/// Returns 0 when `content_cols >= total_cols`.
#[inline]
pub fn center_col(content_cols: Col, total_cols: Col) -> Col {
    ((total_cols - content_cols) / 2).max(0)
}

// ── Horizontal rule frame ─────────────────────────────────────────────────────

/// Build a 1-row [`TermFrame`] filled with `ch` repeated to exactly `width`
/// terminal columns.
///
/// Uses [`TermFrame::hline`] internally, which handles wide characters.
/// The baseline is row 0 (the single row).
pub fn hline_frame(width: Col, ch: char, style: ContentStyle) -> TermFrame {
    let mut frame = TermFrame::new(TermSize::new(width.max(0), 1));
    if width > 0 {
        frame.hline(TermPoint::ZERO, width, ch, style);
    }
    frame
}
