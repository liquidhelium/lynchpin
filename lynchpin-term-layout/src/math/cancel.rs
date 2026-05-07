//! Terminal layout for [`CancelElem`].
//!
//! In a terminal grid we cannot draw diagonal lines, so we approximate the
//! cancellation visual by overlaying a horizontal strikethrough rule at the
//! baseline row of the body.  When `cross: true` we also draw a second rule
//! one row above, giving a rough "×" impression within the body's height.

use crossterm::style::ContentStyle;
use typst::diag::SourceResult;
use typst::foundations::{Packed, StyleChain};
use typst::math::CancelElem;

use crate::frame::{TermFrame, TermPoint, TermSize};

use super::{TermMathContext, TermMathFrameFragment};

// ── layout_cancel ─────────────────────────────────────────────────────────────

/// Layout a [`CancelElem`].
///
/// Terminal approximation:
/// - Layout the body normally.
/// - Draw a `─` strikethrough rule across the full width at the baseline row
///   (the visual mid-point of the body).
/// - If `cross: true`, also draw a second rule one row offset from the first,
///   giving a rough double-cross appearance.
pub fn layout_cancel(
    elem: &Packed<CancelElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(&elem.body, styles)?;
    let cross = elem.cross.get(styles);

    let body_cols = body.cols().max(1);
    let body_rows = body.rows().max(1);
    let body_baseline = body.baseline();

    let mut frame = TermFrame::new(TermSize::new(body_cols, body_rows));
    frame.set_baseline(body_baseline);

    // Place the body first so the rule is drawn on top.
    frame.push_frame(TermPoint::ZERO, body);

    // Primary strikethrough at the baseline row.
    let bar_char = ctx.config.mode.hbar();
    frame.hline(
        TermPoint::new(0, body_baseline),
        body_cols,
        bar_char,
        ContentStyle::default(),
    );

    // For `cross: true`, add a second rule.  We pick the row that is
    // symmetric with the baseline relative to the vertical midpoint.
    // If the body is a single row, the second line coincides — that's fine.
    if cross {
        let mid = body_rows / 2;
        // Mirror: mid + (mid - body_baseline), clamped to valid range.
        let mirror_row = (2 * mid - body_baseline).clamp(0, body_rows - 1);
        if mirror_row != body_baseline {
            frame.hline(
                TermPoint::new(0, mirror_row),
                body_cols,
                bar_char,
                ContentStyle::default(),
            );
        }
    }

    ctx.push(TermMathFrameFragment::new(frame));
    Ok(())
}
