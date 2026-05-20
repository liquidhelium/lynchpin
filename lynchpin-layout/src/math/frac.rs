//! Terminal fraction and binomial layout.

use crossterm::style::ContentStyle;
use lynchpin_library::config::RenderMode;
use lynchpin_library::frame::{Col, Row, TermFrame, TermPoint, TermSize};
use typst::foundations::{Content, Packed, StyleChain, SymbolElem};
use typst::math::{BinomElem, FracElem, FracStyle};

use super::TermMathContext;
use super::fragment::{MathClass, TermMathFrameFragment};

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
            ctx.push(super::text::text_frag("/", MathClass::Normal));
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
    let num_rows = num.rows().max(Row::new(1));
    let denom_rows = denom.rows().max(Row::new(1));

    let inner_cols = num_cols.max(denom_cols);
    let total_cols = inner_cols + Col::new(2); // 1 col padding each side

    // For binom there's no bar row; we still reserve 1 row for spacing.
    let bar_rows: Row = Row::new(1);
    let total_rows = num_rows + bar_rows + denom_rows;
    let baseline = num_rows; // bar row

    let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));
    frame.set_baseline(baseline);

    // Place numerator (centered).
    let num_x = Col::new(1) + Col::new((inner_cols - num_cols).get() / 2);
    frame.push_frame(TermPoint::new(num_x, Row::ZERO), num);

    // Draw fraction bar (or nothing for binom).
    if !binom {
        let bar_char = ctx.config.mode.hbar();
        frame.hline(
            TermPoint::new(Col::new(1), num_rows),
            inner_cols,
            bar_char,
            ContentStyle::default(),
        );
    }

    // Place denominator (centered).
    let denom_x = Col::new(1) + Col::new((inner_cols - denom_cols).get() / 2);
    frame.push_frame(TermPoint::new(denom_x, num_rows + bar_rows), denom);

    if binom {
        // Wrap with ( and ) brackets.
        let height = total_rows;
        let left_chars = stretch_delim(ctx, '(', height);
        let right_chars = stretch_delim(ctx, ')', height);
        let left_frame = super::run::build_delimiter_frame(left_chars, ContentStyle::default());
        let right_frame = super::run::build_delimiter_frame(right_chars, ContentStyle::default());

        let lw = left_frame.cols();
        let rw = right_frame.cols();
        let wrapped_cols = lw + total_cols + rw;
        let mut wrapped = TermFrame::new(TermSize::new(wrapped_cols, total_rows));
        wrapped.set_baseline(baseline);
        wrapped.push_frame(TermPoint::ZERO, left_frame);
        wrapped.push_frame(TermPoint::new(lw, Row::ZERO), frame);
        wrapped.push_frame(TermPoint::new(lw + total_cols, Row::ZERO), right_frame);
        TermMathFrameFragment::new(wrapped)
    } else {
        TermMathFrameFragment::new(frame)
    }
}

// ── Delimiter stretching (local) ──────────────────────────────────────────────

/// Map a delimiter character to a vertically stretched sequence of characters.
fn stretch_delim(ctx: &TermMathContext, ch: char, height: Row) -> Vec<char> {
    let mode = ctx.config.mode;
    match ch {
        '(' => stretch_paren(mode, height, true),
        ')' => stretch_paren(mode, height, false),
        '[' => stretch_bracket(mode, height, true),
        ']' => stretch_bracket(mode, height, false),
        '{' => stretch_brace(mode, height, true),
        '}' => stretch_brace(mode, height, false),
        '|' => vec![mode.vbar(); height.max(Row::new(1)).get() as usize],
        _ => vec![ch; height.max(Row::new(1)).get() as usize],
    }
}

fn stretch_paren(mode: RenderMode, height: Row, _left: bool) -> Vec<char> {
    let h = height.max(Row::new(1)).get() as usize;
    if mode.is_unicode() {
        match h {
            1 => vec!['('],
            2 => vec!['⎛', '⎝'],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push('⎛');
                for _ in 0..h - 2 {
                    v.push('⎜');
                }
                v.push('⎝');
                v
            }
        }
    } else {
        vec!['('; h]
    }
}

fn stretch_bracket(mode: RenderMode, height: Row, _left: bool) -> Vec<char> {
    let h = height.max(Row::new(1)).get() as usize;
    if mode.is_unicode() {
        match h {
            1 => vec!['['],
            2 => vec!['⎡', '⎣'],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push('⎡');
                for _ in 0..h - 2 {
                    v.push('⎢');
                }
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
                for _ in 0..h - 2 {
                    v.push('|');
                }
                v.push('[');
                v
            }
        }
    }
}

fn stretch_brace(mode: RenderMode, height: Row, left: bool) -> Vec<char> {
    let h = height.max(Row::new(1)).get() as usize;
    if mode.is_unicode() {
        unicode_brace(h, left)
    } else if left {
        match h {
            1 => vec!['{'],
            2 => vec!['/', '\\'],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push('/');
                for _ in 0..h - 2 {
                    v.push('|');
                }
                v.push('\\');
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
                for _ in 0..h - 2 {
                    v.push('|');
                }
                v.push('/');
                v
            }
        }
    }
}

fn unicode_brace(height: usize, left: bool) -> Vec<char> {
    if left {
        match height {
            1 => vec!['{'],
            2 => vec!['⎧', '⎩'],
            3 => vec!['⎧', '⎨', '⎩'],
            _ => {
                let mid_rows = height - 3;
                let top_mids = mid_rows / 2;
                let bot_mids = mid_rows - top_mids;
                let mut v = Vec::with_capacity(height);
                v.push('⎧');
                v.extend(std::iter::repeat('⎪').take(top_mids));
                v.push('⎨');
                v.extend(std::iter::repeat('⎪').take(bot_mids));
                v.push('⎩');
                v
            }
        }
    } else {
        match height {
            1 => vec!['}'],
            2 => vec!['⎫', '⎭'],
            3 => vec!['⎫', '⎬', '⎭'],
            _ => {
                let mid_rows = height - 3;
                let top_mids = mid_rows / 2;
                let bot_mids = mid_rows - top_mids;
                let mut v = Vec::with_capacity(height);
                v.push('⎫');
                v.extend(std::iter::repeat('⎪').take(top_mids));
                v.push('⎬');
                v.extend(std::iter::repeat('⎪').take(bot_mids));
                v.push('⎭');
                v
            }
        }
    }
}
