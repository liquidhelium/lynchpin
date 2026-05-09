//! Terminal layout for under/over line and brace/bracket/paren/shell decorators.
//!
//! Handles [`UnderlineElem`], [`OverlineElem`], and the family of annotated
//! under/over decorators: brace, bracket, paren, shell.

use crossterm::style::ContentStyle;
use ecow::EcoString;
use lynchpin_library_ng::config::RenderMode;
use lynchpin_library_ng::frame::{Col, Row, TermFrame, TermPoint, TermSize};
use typst::diag::SourceResult;
use typst::foundations::{Content, Packed, StyleChain};
use typst::math::{
    OverbraceElem, OverbracketElem, OverlineElem, OverparenElem, OvershellElem, UnderbraceElem,
    UnderbracketElem, UnderlineElem, UnderparenElem, UndershellElem,
};

use super::fragment::TermMathFrameFragment;
use super::run::compose_vertical;
use super::TermMathContext;

// ── Underline / overline ──────────────────────────────────────────────────────

/// Layout an [`UnderlineElem`]: body with a horizontal rule below.
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
    let body_cols = body.cols().max(Col::new(1));
    let body_rows = body.rows();
    let body_baseline = body.baseline();

    let total_cols = body_cols;
    let total_rows = body_rows + Row::new(1);

    let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));
    frame.set_baseline(body_baseline);
    frame.push_frame(TermPoint::ZERO, body);
    frame.hline(
        TermPoint::new(Col::ZERO, body_rows),
        total_cols,
        ctx.config.mode.hbar(),
        ContentStyle::default(),
    );
    TermMathFrameFragment::new(frame)
}

fn build_overline(ctx: &TermMathContext, body: TermFrame) -> TermMathFrameFragment {
    let body_cols = body.cols().max(Col::new(1));
    let body_rows = body.rows();
    let body_baseline = body.baseline();

    let total_cols = body_cols;
    let total_rows = Row::new(1) + body_rows;

    let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));
    frame.set_baseline(Row::new(1) + body_baseline);
    frame.hline(
        TermPoint::ZERO,
        total_cols,
        ctx.config.mode.hbar(),
        ContentStyle::default(),
    );
    frame.push_frame(TermPoint::new(Col::ZERO, Row::new(1)), body);
    TermMathFrameFragment::new(frame)
}

// ── Underbrace / overbrace ────────────────────────────────────────────────────

pub fn layout_underbrace(
    elem: &Packed<UnderbraceElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(&elem.body, styles)?;
    let w = body.cols().max(Col::new(1));
    let (l, m, r, c) = underbrace_parts(ctx.config.mode);
    let deco = build_hdeco_center(w, l, m, r, Some(c));
    stack_deco_below(
        ctx,
        body,
        deco,
        elem.annotation.get_ref(styles).as_ref(),
        styles,
    )
}

pub fn layout_overbrace(
    elem: &Packed<OverbraceElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(&elem.body, styles)?;
    let w = body.cols().max(Col::new(1));
    let (l, m, r, c) = overbrace_parts(ctx.config.mode);
    let deco = build_hdeco_center(w, l, m, r, Some(c));
    stack_deco_above(
        ctx,
        body,
        deco,
        elem.annotation.get_ref(styles).as_ref(),
        styles,
    )
}

pub fn layout_underbracket(
    elem: &Packed<UnderbracketElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(&elem.body, styles)?;
    let w = body.cols().max(Col::new(1));
    let (l, m, r) = underbracket_parts(ctx.config.mode);
    let deco = build_hdeco(w, l, m, r);
    stack_deco_below(
        ctx,
        body,
        deco,
        elem.annotation.get_ref(styles).as_ref(),
        styles,
    )
}

pub fn layout_overbracket(
    elem: &Packed<OverbracketElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(&elem.body, styles)?;
    let w = body.cols().max(Col::new(1));
    let (l, m, r) = overbracket_parts(ctx.config.mode);
    let deco = build_hdeco(w, l, m, r);
    stack_deco_above(
        ctx,
        body,
        deco,
        elem.annotation.get_ref(styles).as_ref(),
        styles,
    )
}

pub fn layout_underparen(
    elem: &Packed<UnderparenElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(&elem.body, styles)?;
    let w = body.cols().max(Col::new(1));
    let (l, m, r) = underparen_parts(ctx.config.mode);
    let deco = build_hdeco(w, l, m, r);
    stack_deco_below(
        ctx,
        body,
        deco,
        elem.annotation.get_ref(styles).as_ref(),
        styles,
    )
}

pub fn layout_overparen(
    elem: &Packed<OverparenElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(&elem.body, styles)?;
    let w = body.cols().max(Col::new(1));
    let (l, m, r) = overparen_parts(ctx.config.mode);
    let deco = build_hdeco(w, l, m, r);
    stack_deco_above(
        ctx,
        body,
        deco,
        elem.annotation.get_ref(styles).as_ref(),
        styles,
    )
}

pub fn layout_undershell(
    elem: &Packed<UndershellElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(&elem.body, styles)?;
    let w = body.cols().max(Col::new(1));
    let (l, m, r) = undershell_parts(ctx.config.mode);
    let deco = build_hdeco(w, l, m, r);
    stack_deco_below(
        ctx,
        body,
        deco,
        elem.annotation.get_ref(styles).as_ref(),
        styles,
    )
}

pub fn layout_overshell(
    elem: &Packed<OvershellElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let body = ctx.layout_into_frame(&elem.body, styles)?;
    let w = body.cols().max(Col::new(1));
    let (l, m, r) = overshell_parts(ctx.config.mode);
    let deco = build_hdeco(w, l, m, r);
    stack_deco_above(
        ctx,
        body,
        deco,
        elem.annotation.get_ref(styles).as_ref(),
        styles,
    )
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_hdeco(width: Col, left: char, mid: char, right: char) -> TermFrame {
    build_hdeco_center(width, left, mid, right, None)
}

fn build_hdeco_center(
    width: Col,
    left: char,
    mid: char,
    right: char,
    center: Option<char>,
) -> TermFrame {
    let w = width.max(Col::new(1)).get() as usize;
    let mut f = TermFrame::new(TermSize::new(Col::new(w as i32), Row::new(1)));
    if w == 1 {
        f.push_text(
            TermPoint::new(Col::ZERO, Row::ZERO),
            EcoString::from(mid),
            ContentStyle::default(),
        );
    } else {
        f.push_text(
            TermPoint::new(Col::ZERO, Row::ZERO),
            EcoString::from(left),
            ContentStyle::default(),
        );
        if w >= 3 {
            if let Some(center) = center {
                for x in 1..w / 2 {
                    f.push_text(
                        TermPoint::new(Col::new(x as i32), Row::ZERO),
                        EcoString::from(mid),
                        ContentStyle::default(),
                    );
                }
                f.push_text(
                    TermPoint::new(Col::new((w / 2) as i32), Row::ZERO),
                    EcoString::from(center),
                    ContentStyle::default(),
                );
                for x in (w / 2 + 1)..w - 1 {
                    f.push_text(
                        TermPoint::new(Col::new(x as i32), Row::ZERO),
                        EcoString::from(mid),
                        ContentStyle::default(),
                    );
                }
            } else {
                for x in 1..w - 1 {
                    f.push_text(
                        TermPoint::new(Col::new(x as i32), Row::ZERO),
                        EcoString::from(mid),
                        ContentStyle::default(),
                    );
                }
            }
        } else {
            // w == 2: just left + right
        }
        f.push_text(
            TermPoint::new(Col::new((w - 1) as i32), Row::ZERO),
            EcoString::from(right),
            ContentStyle::default(),
        );
    }
    f
}

fn stack_deco_below(
    ctx: &mut TermMathContext,
    body: TermFrame,
    deco: TermFrame,
    ann: Option<&Content>,
    styles: StyleChain,
) -> SourceResult<()> {
    let mut frames = vec![body, deco];
    if let Some(a) = ann {
        frames.push(ctx.layout_into_frame(a, styles)?);
    }
    ctx.push(TermMathFrameFragment::new(compose_vertical(
        frames,
        Row::ZERO,
        0,
    )));
    Ok(())
}

fn stack_deco_above(
    ctx: &mut TermMathContext,
    body: TermFrame,
    deco: TermFrame,
    ann: Option<&Content>,
    styles: StyleChain,
) -> SourceResult<()> {
    let mut frames: Vec<TermFrame> = Vec::new();
    if let Some(a) = ann {
        frames.push(ctx.layout_into_frame(a, styles)?);
    }
    frames.push(deco);
    let idx = frames.len();
    frames.push(body);
    ctx.push(TermMathFrameFragment::new(compose_vertical(
        frames,
        Row::ZERO,
        idx,
    )));
    Ok(())
}

// ── Character maps ────────────────────────────────────────────────────────────

fn u(mode: RenderMode) -> bool {
    mode.is_unicode()
}

fn underbrace_parts(m: RenderMode) -> (char, char, char, char) {
    if u(m) {
        ('╰', '─', '╯', '🮦')
    } else {
        ('\\', '-', '/', 'v')
    }
}
fn overbrace_parts(m: RenderMode) -> (char, char, char, char) {
    if u(m) {
        ('╭', '─', '╮', '🮧')
    } else {
        ('/', '-', '\\', '^')
    }
}
fn underbracket_parts(m: RenderMode) -> (char, char, char) {
    if u(m) {
        ('└', '─', '┘')
    } else {
        ('[', '-', ']')
    }
}
fn overbracket_parts(m: RenderMode) -> (char, char, char) {
    if u(m) {
        ('┌', '\u{2500}', '┐')
    } else {
        ('[', '-', ']')
    }
}
fn underparen_parts(m: RenderMode) -> (char, char, char) {
    if u(m) {
        ('╰', '─', '╯')
    } else {
        ('(', '-', ')')
    }
}
fn overparen_parts(m: RenderMode) -> (char, char, char) {
    if u(m) {
        ('╭', '─', '╮')
    } else {
        ('(', '-', ')')
    }
}
fn undershell_parts(m: RenderMode) -> (char, char, char) {
    if u(m) {
        ('🮡', '─', '🮠')
    } else {
        ('\\', '_', '/')
    }
}
fn overshell_parts(m: RenderMode) -> (char, char, char) {
    if u(m) {
        ('🮣', '─', '🮢')
    } else {
        ('/', '_', '\\')
    }
}
