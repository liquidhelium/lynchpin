//! Terminal fraction and binomial layout.

use crossterm::style::ContentStyle;
use typst::foundations::{Content, Packed, StyleChain, SymbolElem};
use typst::math::{BinomElem, FracElem, FracStyle};

use crate::frame::{Row, TermFrame, TermPoint, TermSize};

use super::TermMathContext;
use super::fragment::TermMathFrameFragment;

// ── layout_frac ───────────────────────────────────────────────────────────────

/// Layout a [`FracElem`].
pub fn layout_frac(
    elem: &Packed<FracElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> typst::diag::SourceResult<()> {
    match elem.style.get(styles) {
        FracStyle::Vertical => {
            let num = ctx.layout_into_frame(&elem.num, styles)?;
            let denom = ctx.layout_into_frame(&elem.denom, styles)?;
            ctx.push(build_vertical_frac(ctx, num, denom, false));
        }
        FracStyle::Skewed | FracStyle::Horizontal => {
            // Inline: num / denom
            let num = ctx.layout_into_fragment(&elem.num, styles)?;
            ctx.push(num);
            ctx.push(super::text::text_frag(
                "/",
                unicode_math_class::MathClass::Normal,
            ));
            let denom = ctx.layout_into_fragment(&elem.denom, styles)?;
            ctx.push(denom);
        }
    }
    Ok(())
}

// ── layout_binom ──────────────────────────────────────────────────────────────

/// Layout a [`BinomElem`].
pub fn layout_binom(
    elem: &Packed<BinomElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> typst::diag::SourceResult<()> {
    let upper = ctx.layout_into_frame(&elem.upper, styles)?;

    // Join multiple lower elements with ", " separator.
    let lower_frame = if elem.lower.len() == 1 {
        ctx.layout_into_frame(&elem.lower[0], styles)?
    } else {
        let items: Vec<Content> = elem
            .lower
            .iter()
            .enumerate()
            .flat_map(|(i, c)| {
                if i == 0 {
                    vec![c.clone()]
                } else {
                    vec![
                        SymbolElem::packed(',').spanned(typst::syntax::Span::detached()),
                        c.clone(),
                    ]
                }
            })
            .collect();
        ctx.layout_into_frame(&Content::sequence(items), styles)?
    };

    let frac = build_vertical_frac(ctx, upper, lower_frame, true);
    ctx.push(frac);
    Ok(())
}

// ── Vertical fraction / binomial construction ─────────────────────────────────

/// Build a vertical stacked fraction frame.
///
/// Layout:
/// ```text
///  numerator    (centered, rows 0..num_rows-1)
///  ─────────    (fraction bar OR absent for binom, row num_rows)
///  denominator  (centered, rows num_rows+1..)
/// ```
///
/// * Width    = max(num_cols, denom_cols) + 2  (1 col padding each side)
/// * Baseline = num_rows                       (bar row / mid row for binom)
fn build_vertical_frac(
    ctx: &TermMathContext,
    num: TermFrame,
    denom: TermFrame,
    binom: bool,
) -> TermMathFrameFragment {
    let num_cols = num.cols();
    let denom_cols = denom.cols();
    let num_rows = num.rows().max(1);
    let denom_rows = denom.rows().max(1);

    let inner_cols = num_cols.max(denom_cols);
    let total_cols = inner_cols + 2; // 1 col padding each side

    // For binom there's no bar row; we still reserve 1 row for spacing.
    let bar_rows: Row = 1;
    let total_rows = num_rows + bar_rows + denom_rows;
    let baseline = num_rows; // bar row

    let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));
    frame.set_baseline(baseline);

    // Place numerator (centered).
    let num_x = 1 + (inner_cols - num_cols) / 2;
    frame.push_frame(TermPoint::new(num_x, 0), num);

    // Draw fraction bar (or nothing for binom).
    if !binom {
        let bar_char = ctx.config.mode.hbar();
        frame.hline(
            TermPoint::new(1, num_rows),
            inner_cols,
            bar_char,
            ContentStyle::default(),
        );
    }

    // Place denominator (centered).
    let denom_x = 1 + (inner_cols - denom_cols) / 2;
    frame.push_frame(TermPoint::new(denom_x, num_rows + bar_rows), denom);

    if binom {
        // Wrap with ( and ) brackets.
        let height = total_rows;
        let left_chars = ctx.config.mode.left_paren(height);
        let right_chars = ctx.config.mode.right_paren(height);
        let left_frame = super::run::build_delimiter_frame(left_chars, ContentStyle::default());
        let right_frame = super::run::build_delimiter_frame(right_chars, ContentStyle::default());

        let lw = left_frame.cols();
        let rw = right_frame.cols();
        let wrapped_cols = lw + total_cols + rw;
        let mut wrapped = TermFrame::new(TermSize::new(wrapped_cols, total_rows));
        wrapped.set_baseline(baseline);
        wrapped.push_frame(TermPoint::ZERO, left_frame);
        wrapped.push_frame(TermPoint::new(lw, 0), frame);
        wrapped.push_frame(TermPoint::new(lw + total_cols, 0), right_frame);
        TermMathFrameFragment::new(wrapped)
    } else {
        TermMathFrameFragment::new(frame)
    }
}
