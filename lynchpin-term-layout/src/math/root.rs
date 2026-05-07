//! Terminal square-root / nth-root layout.
//!
//! Renders `\sqrt{x}` and `\root[n]{x}` using the terminal character grid.
//!
//! Visual layout (nth-root, multi-row radicand):
//!
//! ```text
//!  n
//!  √ ─────┐        ← row 0: overline + cap
//!    radicand       ← rows 1..
//! ```
//!
//! For a single-row radicand the sqrt symbol and the overline share row 0.

use crossterm::style::ContentStyle;
use typst::diag::SourceResult;
use typst::foundations::{Packed, StyleChain};
use typst::math::RootElem;
use unicode_math_class::MathClass;

use crate::frame::{Col, Row, TermFrame, TermPoint, TermSize};

use super::{TermMathContext, TermMathFrameFragment};

// ── layout_root ───────────────────────────────────────────────────────────────

/// Layout a [`RootElem`] (square root or nth root).
pub fn layout_root(
    elem: &Packed<RootElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    // ── Lay out the radicand ──────────────────────────────────────────────────
    let radicand = ctx.layout_into_frame(&elem.radicand, styles)?;
    let rad_cols = radicand.cols();
    let rad_rows = radicand.rows().max(1);
    let rad_baseline = radicand.baseline(); // save before move

    // ── Lay out the index (optional) ──────────────────────────────────────────
    let index_frame: Option<TermFrame> = match elem.index.get_ref(styles) {
        Some(idx) => Some(ctx.layout_into_frame(idx, styles)?),
        None => None,
    };

    // ── Build the radical frame ───────────────────────────────────────────────

    // Row 0: overline bar + cap
    // Rows 1..=rad_rows: the radicand content (shifted down by 1)
    //
    // The √ symbol occupies 1 column to the left of the radicand.
    // For multi-row radicands a vertical bar fills the left column above the
    // √ glyph so the radical "arm" extends upward.

    let total_rows: Row = 1 + rad_rows; // 1 overline row + radicand rows
    let sqrt_col: Col = 1; // width of the sqrt symbol column
    let total_cols: Col = sqrt_col + rad_cols;

    let mut rad_frame = TermFrame::new(TermSize::new(total_cols, total_rows));
    // Baseline: overline row (row 0) + radicand baseline
    rad_frame.set_baseline(1 + rad_baseline);

    // ── Draw overline (row 0) ─────────────────────────────────────────────────
    let overline_char = ctx.config.mode.sqrt_overline();
    let cap_char = ctx.config.mode.sqrt_cap();

    // The overline spans `rad_cols - 1` columns followed by a 1-column cap.
    if rad_cols > 1 {
        rad_frame.hline(
            TermPoint::new(sqrt_col, 0),
            rad_cols - 1,
            overline_char,
            ContentStyle::default(),
        );
    }
    // Cap at the far-right corner
    if rad_cols >= 1 {
        rad_frame.push_text(
            TermPoint::new(sqrt_col + rad_cols - 1, 0),
            cap_char.to_string(),
            ContentStyle::default(),
        );
    }

    // ── Draw the √ symbol column ──────────────────────────────────────────────
    let sqrt_sym = ctx.config.mode.sqrt_sym();

    if rad_rows == 1 {
        // Single-row radicand: √ sits on the overline row (row 0), but we
        // actually place it on row 1 (the radicand row) and put the overline
        // at row 0.  For a classic look we place √ at row 1.
        rad_frame.push_text(
            TermPoint::new(0, 1),
            sqrt_sym.to_string(),
            ContentStyle::default(),
        );
    } else {
        // Multi-row radicand: √ appears at the bottom row; rows above are
        // left blank (or could use a vertical bar in unicode mode).
        rad_frame.push_text(
            TermPoint::new(0, total_rows - 1),
            sqrt_sym.to_string(),
            ContentStyle::default(),
        );
    }

    // ── Place the radicand (row 1 onwards) ───────────────────────────────────
    rad_frame.push_frame(TermPoint::new(sqrt_col, 1), radicand);

    // ── Combine with index (if present) ──────────────────────────────────────
    let final_frame = if let Some(idx) = index_frame {
        // The index is placed as a small superscript above and to the left of
        // the radical symbol.  We compose:
        //   [idx]
        //   [rad_frame]   (radical + radicand block)
        //
        // using a manual 2-column layout where the index is flush with the
        // left edge of the radical symbol.

        let idx_cols = idx.cols();
        let idx_rows = idx.rows().max(1);

        // Total width: max(idx_cols, sqrt_col) + rad_cols
        // (index is placed above the √ column; it never pushes into radicand)
        let left_cols = idx_cols.max(sqrt_col);
        let combined_cols = left_cols + rad_cols;

        // Vertical layout:
        //   row 0 .. idx_rows-1    : index
        //   row idx_rows .. end    : rad_frame (total_rows high)
        let combined_rows = idx_rows + total_rows;

        let mut combined = TermFrame::new(TermSize::new(combined_cols, combined_rows));
        // Baseline = idx rows + radical baseline
        combined.set_baseline(idx_rows + 1 + rad_baseline);

        // Place index at top-left, centred over the √ column.
        let idx_x = ((left_cols - idx_cols) / 2).max(0);
        combined.push_frame(TermPoint::new(idx_x, 0), idx);

        // Place the radical block below the index, shifted right so the √
        // column aligns with `left_cols - sqrt_col`.
        let rad_x = left_cols - sqrt_col;
        combined.push_frame(TermPoint::new(rad_x, idx_rows), rad_frame);

        combined
    } else {
        rad_frame
    };

    ctx.push(TermMathFrameFragment::new(final_frame).with_class(MathClass::Normal));
    Ok(())
}
