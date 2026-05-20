//! Terminal square-root / nth-root layout.
//!
//! Uses the Diagon/Math.cpp diagonal construction:
//!
//! ```text
//!              ____
//!  3    3 __
//!  ╲╱x+1    ╲╱  x+1
//! ```
//!
//! The radical symbol grows diagonally with radicand height.

use crossterm::style::ContentStyle;
use ecow::EcoString;
use lynchpin_library::frame::{Col, Row, TermFrame, TermPoint, TermSize};
use typst::diag::SourceResult;
use typst::foundations::{Packed, StyleChain};
use typst::math::RootElem;

use super::fragment::MathClass;
use super::{TermMathContext, TermMathFrameFragment};

// ── Character access helpers ──────────────────────────────────────────────────

/// Characters for the radical construction.
struct SqrtChars {
    /// Top-left diagonal (╲ in Unicode, \\ in ASCII).
    diag_tl: char,
    /// Connecting diagonal (╱ in Unicode, / in ASCII).
    diag_tr: char,
    /// Overline bar (_ or -).
    overline: char,
}

fn sqrt_chars(ctx: &TermMathContext) -> SqrtChars {
    if ctx.config.mode.is_unicode() {
        SqrtChars {
            diag_tl: '╲',
            diag_tr: '╱',
            overline: '_',
        }
    } else {
        SqrtChars {
            diag_tl: '\\',
            diag_tr: '/',
            overline: '_',
        }
    }
}

// ── layout_root ───────────────────────────────────────────────────────────────

/// Layout a [`RootElem`].
///
/// The radical is constructed from diagonal slashes that grow with the
/// radicand height, terminated by an overline across the top of the
/// radicand.
pub fn layout_root(
    elem: &Packed<RootElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let sc = sqrt_chars(ctx);

    // ── Lay out the radicand ──────────────────────────────────────────────────
    let radicand = ctx.layout_into_frame(&elem.radicand, styles)?;
    let rad_cols = radicand.cols();
    let rad_rows = radicand.rows().max(Row::new(1));
    let rad_baseline = radicand.baseline();

    // ── Lay out the index (optional) ──────────────────────────────────────────
    let index_frame: Option<TermFrame> = match elem.index.get_ref(styles) {
        Some(idx) => Some(ctx.layout_into_frame(idx, styles)?),
        None => None,
    };

    // ── Compute radical dimensions ────────────────────────────────────────────
    let overline_row: Row = Row::new(1);
    let total_rows: Row = rad_rows + overline_row;
    let diag_cols: Col = rad_rows + Col::new(1);
    let total_cols: Col = diag_cols + rad_cols;

    let mut rad_frame = TermFrame::new(TermSize::new(total_cols, total_rows));
    rad_frame.set_baseline(overline_row + rad_baseline);

    // ── Draw overline (row 0, above content) ──────────────────────────────────
    let mut x = diag_cols;
    while x < total_cols {
        rad_frame.push_text(
            TermPoint::new(x, Row::ZERO),
            EcoString::from(sc.overline),
            ContentStyle::default(),
        );
        x = x + Col::new(1);
    }

    // ── Draw diagonal body ─────────────────────────────────────────────────────
    // Bottom-left corner: diag_tl (╲ or \)
    let bot_row = total_rows - Row::new(1);
    rad_frame.push_text(
        TermPoint::new(Col::ZERO, bot_row),
        EcoString::from(sc.diag_tl),
        ContentStyle::default(),
    );

    // For each diagonal step upward: diag_tr (╱ or /), starting on the same
    // row as the bottom corner and moving up-right.
    for y in 0..rad_rows.get() {
        let row = total_rows - Row::new(1) - Row::new(y);
        let col = Col::new(1) + Col::new(y);
        rad_frame.push_text(
            TermPoint::new(col, row),
            EcoString::from(sc.diag_tr),
            ContentStyle::default(),
        );
    }

    // ── Place the radicand ───────────────────────────────────────────────────
    rad_frame.push_frame(TermPoint::new(diag_cols, overline_row), radicand);

    // ── Combine with index ────────────────────────────────────────────────────
    let final_frame = if let Some(idx) = index_frame {
        let idx_cols = idx.cols();
        let idx_rows = idx.rows().max(Row::new(1));
        let combined_cols = idx_cols.max(total_cols);
        let combined_rows = idx_rows + total_rows;

        let mut combined = TermFrame::new(TermSize::new(combined_cols, combined_rows));
        combined.set_baseline(idx_rows + overline_row + rad_baseline);

        // Place index in the overline row, to the left of the diagonal.
        let idx_x: Col = (diag_cols - idx_cols).max(Col::ZERO);
        combined.push_frame(TermPoint::new(idx_x, Row::ZERO), idx);

        // Place radical below.
        let rad_offset = if total_cols < combined_cols {
            combined_cols - total_cols
        } else {
            Col::ZERO
        };
        combined.push_frame(TermPoint::new(rad_offset, idx_rows), rad_frame);

        combined
    } else {
        rad_frame
    };

    ctx.push(TermMathFrameFragment::new(final_frame).with_class(MathClass::Normal));
    Ok(())
}
