//! Terminal layout for under/over line and brace/bracket/paren/shell decorators.
//!
//! Handles [`UnderlineElem`], [`OverlineElem`], and the family of annotated
//! under/over decorators: brace, bracket, paren, shell.

use crossterm::style::ContentStyle;
use typst::diag::SourceResult;
use typst::foundations::{Content, Packed, StyleChain};
use typst::math::{
    OverbraceElem, OverbracketElem, OverlineElem, OverparenElem, OvershellElem,
    UnderbraceElem, UnderbracketElem, UnderlineElem, UnderparenElem, UndershellElem,
};

use crate::frame::{TermFrame, TermPoint, TermSize};
use crate::stack::compose_vertical;

use super::{TermMathContext, TermMathFrameFragment};

// ── Underline / overline ──────────────────────────────────────────────────────

/// Layout an [`UnderlineElem`]: body with a horizontal rule below.
///
/// Layout:
/// ```text
///   body           (rows 0 .. body_rows-1, baseline = body.baseline())
///   ─────────      (row body_rows)
/// ```
/// The fragment's baseline stays at the body's baseline.
pub fn layout_underline(
    elem: &Packed<UnderlineElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(&elem.body, styles)?;
    ctx.push(build_underline(ctx, body));
    Ok(())
}

/// Layout an [`OverlineElem`]: body with a horizontal rule above.
///
/// Layout:
/// ```text
///   ─────────      (row 0)
///   body           (rows 1 .. body_rows, baseline = 1 + body.baseline())
/// ```
pub fn layout_overline(
    elem: &Packed<OverlineElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(&elem.body, styles)?;
    ctx.push(build_overline(ctx, body));
    Ok(())
}

fn build_underline(ctx: &TermMathContext, body: TermFrame) -> TermMathFrameFragment {
    let body_cols = body.cols().max(1);
    let body_rows = body.rows();
    let body_baseline = body.baseline();

    let total_cols = body_cols;
    let total_rows = body_rows + 1;

    let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));
    frame.set_baseline(body_baseline);
    frame.push_frame(TermPoint::ZERO, body);
    frame.hline(
        TermPoint::new(0, body_rows),
        total_cols,
        ctx.config.mode.hbar(),
        ContentStyle::default(),
    );
    TermMathFrameFragment::new(frame)
}

fn build_overline(ctx: &TermMathContext, body: TermFrame) -> TermMathFrameFragment {
    let body_cols = body.cols().max(1);
    let body_rows = body.rows();
    let body_baseline = body.baseline();

    let total_cols = body_cols;
    let total_rows = 1 + body_rows;

    let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));
    frame.set_baseline(1 + body_baseline);
    frame.hline(TermPoint::ZERO, total_cols, ctx.config.mode.hbar(), ContentStyle::default());
    frame.push_frame(TermPoint::new(0, 1), body);
    TermMathFrameFragment::new(frame)
}

// ── Underbrace / overbrace ────────────────────────────────────────────────────

/// Layout an [`UnderbraceElem`]: body / brace-row / optional-annotation.
pub fn layout_underbrace(
    elem: &Packed<UnderbraceElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    layout_under_deco(
        &elem.body,
        elem.annotation.get_ref(styles).as_ref(),
        ctx.config.mode.underbrace_char(),
        ctx,
        styles,
    )
}

/// Layout an [`OverbraceElem`]: optional-annotation / brace-row / body.
pub fn layout_overbrace(
    elem: &Packed<OverbraceElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    layout_over_deco(
        &elem.body,
        elem.annotation.get_ref(styles).as_ref(),
        ctx.config.mode.overbrace_char(),
        ctx,
        styles,
    )
}

// ── Underbracket / overbracket ────────────────────────────────────────────────

/// Layout an [`UnderbracketElem`].
pub fn layout_underbracket(
    elem: &Packed<UnderbracketElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    layout_under_deco(
        &elem.body,
        elem.annotation.get_ref(styles).as_ref(),
        ctx.config.mode.underbracket_char(),
        ctx,
        styles,
    )
}

/// Layout an [`OverbracketElem`].
pub fn layout_overbracket(
    elem: &Packed<OverbracketElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    layout_over_deco(
        &elem.body,
        elem.annotation.get_ref(styles).as_ref(),
        ctx.config.mode.overbracket_char(),
        ctx,
        styles,
    )
}

// ── Underparen / overparen ────────────────────────────────────────────────────

/// Layout an [`UnderparenElem`].
pub fn layout_underparen(
    elem: &Packed<UnderparenElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    layout_under_deco(
        &elem.body,
        elem.annotation.get_ref(styles).as_ref(),
        ctx.config.mode.underparen_char(),
        ctx,
        styles,
    )
}

/// Layout an [`OverparenElem`].
pub fn layout_overparen(
    elem: &Packed<OverparenElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    layout_over_deco(
        &elem.body,
        elem.annotation.get_ref(styles).as_ref(),
        ctx.config.mode.overparen_char(),
        ctx,
        styles,
    )
}

// ── Undershell / overshell ────────────────────────────────────────────────────

/// Layout an [`UndershellElem`].
pub fn layout_undershell(
    elem: &Packed<UndershellElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    layout_under_deco(
        &elem.body,
        elem.annotation.get_ref(styles).as_ref(),
        ctx.config.mode.undershell_char(),
        ctx,
        styles,
    )
}

/// Layout an [`OvershellElem`].
pub fn layout_overshell(
    elem: &Packed<OvershellElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    layout_over_deco(
        &elem.body,
        elem.annotation.get_ref(styles).as_ref(),
        ctx.config.mode.overshell_char(),
        ctx,
        styles,
    )
}

// ── Generic helpers ───────────────────────────────────────────────────────────

/// Build an under-decoration: `body / deco-row / annotation?`
///
/// The fragment's baseline matches the body's baseline, so the body sits on
/// the math axis and the decoration hangs below.
fn layout_under_deco(
    body_content: &Content,
    annotation: Option<&Content>,
    deco_char: char,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(body_content, styles)?;
    let body_cols = body.cols().max(1);

    // Decoration row: fill body width with the decoration character.
    let mut deco_frame = TermFrame::new(TermSize::new(body_cols, 1));
    deco_frame.hline(TermPoint::ZERO, body_cols, deco_char, ContentStyle::default());

    // Stack: [body, deco, annotation?].
    // baseline_idx = 0 → overall baseline = body.baseline().
    let mut frames = vec![body, deco_frame];
    if let Some(ann_content) = annotation {
        let ann = ctx.layout_into_frame(ann_content, styles)?;
        frames.push(ann);
    }

    let composed = compose_vertical(frames, 0, 0);
    ctx.push(TermMathFrameFragment::new(composed));
    Ok(())
}

/// Build an over-decoration: `annotation? / deco-row / body`
///
/// The fragment's baseline sits at `offset + body.baseline()`, where `offset`
/// is the total height of the annotation (if any) plus the one-row decoration.
fn layout_over_deco(
    body_content: &Content,
    annotation: Option<&Content>,
    deco_char: char,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(body_content, styles)?;
    let body_cols = body.cols().max(1);

    // Decoration row.
    let mut deco_frame = TermFrame::new(TermSize::new(body_cols, 1));
    deco_frame.hline(TermPoint::ZERO, body_cols, deco_char, ContentStyle::default());

    // Stack order (top → bottom): annotation? / deco / body.
    // baseline_idx = index of body = frames.len() - 1.
    let mut frames: Vec<TermFrame> = Vec::new();
    if let Some(ann_content) = annotation {
        let ann = ctx.layout_into_frame(ann_content, styles)?;
        frames.push(ann);
    }
    frames.push(deco_frame);
    let body_idx = frames.len(); // index body will occupy after push
    frames.push(body);

    let composed = compose_vertical(frames, 0, body_idx);
    ctx.push(TermMathFrameFragment::new(composed));
    Ok(())
}
