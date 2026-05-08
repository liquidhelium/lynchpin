//! Terminal left–right delimiter group layout.
//!
//! Handles [`LrElem`] (auto-stretching delimiters), [`MidElem`] (mid
//! delimiters), and [`StretchElem`] (explicitly stretched glyphs).
//!
//! For terminal rendering we cannot sub-pixel scale characters, so stretching
//! is approximated by repeating box-drawing characters to span the tallest
//! inner fragment.

use crossterm::style::ContentStyle;
use typst::diag::SourceResult;
use typst::math::{LrElem, MidElem, StretchElem};
use unicode_math_class::MathClass;

use super::run::build_delimiter_frame;
use super::{TermMathContext, TermMathFragment, TermMathFrameFragment};

use crate::config::RenderMode;
use crate::frame::{Col, Row, TermFrame};
use typst::foundations::{Content, Packed, Resolve, StyleChain, SymbolElem};
use typst::layout::{Length, Rel};

// ── layout_lr ─────────────────────────────────────────────────────────────────

/// Layout an [`LrElem`].
///
/// 1. Layout the body into a fragment list.
/// 2. Identify the first and last fragments as delimiters.
/// 3. Measure the tallest inner fragment.
/// 4. Stretch the delimiter frames to that height.
/// 5. Push the assembled frame fragment.
pub fn layout_lr(
    elem: &Packed<LrElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    // Layout body into raw fragments.
    let mut frags = ctx.layout_into_fragments(&elem.body, styles)?;

    if frags.is_empty() {
        return Ok(());
    }

    // Measure the tallest inner fragment (excluding first / last if they are
    // delimiters – we stretch those to match).
    let (inner_height, inner_baseline) = inner_content_metrics(&frags);

    // Stretch the opening delimiter (first fragment if it looks like one).
    if let Some(first) = frags.first_mut() {
        try_stretch_delimiter(first, inner_height, inner_baseline, ctx, true);
    }

    // Stretch the closing delimiter (last fragment if it looks like one).
    let last_idx = frags.len() - 1;
    if last_idx > 0 {
        if let Some(last) = frags.last_mut() {
            try_stretch_delimiter(last, inner_height, inner_baseline, ctx, false);
        }
    }

    // Compose into a single run frame and push.
    let run_frame = super::run::TermMathRun::new(frags).into_frame();
    ctx.push(TermMathFrameFragment::new(run_frame));
    Ok(())
}

// ── Delimiter detection & stretching ─────────────────────────────────────────

/// Return `true` when `class` indicates that a fragment might be a delimiter.
fn is_delimiter_class(class: MathClass) -> bool {
    matches!(
        class,
        MathClass::Opening | MathClass::Closing | MathClass::Fence
    )
}

/// Measure the tallest non-delimiter fragment inside the lr group.
///
/// We look at all fragments except the first and last (which are typically the
/// opening/closing delimiters).  If the group has ≤ 2 fragments we return 1.
fn inner_content_metrics(frags: &[TermMathFragment]) -> (Row, Row) {
    if frags.len() <= 2 {
        return (1, 0);
    }
    let inner = &frags[1..frags.len() - 1];
    let a = inner.iter().map(|f| f.ascent()).max().unwrap_or(0);
    let d = inner.iter().map(|f| f.descent()).max().unwrap_or(1);
    ((a + d).max(1), a.max(0))
}

/// If `frag` is a single-character delimiter, replace it in-place with a
/// stretched version.  Does nothing if the fragment is not recognisable as a
/// delimiter character.
fn try_stretch_delimiter(
    frag: &mut TermMathFragment,
    height: Row,
    baseline_hint: Row,
    ctx: &TermMathContext,
    _is_opening: bool,
) {
    let class = frag.class();
    if !is_delimiter_class(class) {
        return;
    }
    let ch = match extract_single_char(frag) {
        Some(c) => c,
        None => return,
    };
    let chars = stretch_chars_for(ch, height, ctx);
    if chars.is_empty() {
        return;
    }

    let new_frame = build_delimiter_frame(chars, ContentStyle::default());
    let mut frame_frag = TermMathFrameFragment::new(new_frame).with_class(class);
    frame_frag.frame.set_baseline(baseline_hint);
    *frag = TermMathFragment::Frame(frame_frag);
}

/// Try to extract the single Unicode scalar value from the first text item in
/// a fragment's frame.  Returns `None` if the frame has more than one
/// character or is not text.
fn extract_single_char(frag: &TermMathFragment) -> Option<char> {
    let frame = match frag {
        TermMathFragment::Frame(ff) => &ff.frame,
        _ => return None,
    };

    // Walk items looking for a single text item.
    let mut found: Option<char> = None;
    for (_pt, item) in frame.items() {
        match item {
            crate::frame::TermFrameItem::Text(text, _style) => {
                let mut chars = text.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None; // more than one char
                }
                if found.is_some() {
                    return None; // more than one text item
                }
                found = Some(c);
            }
            crate::frame::TermFrameItem::Frame(_) => return None,
        }
    }
    found
}

/// Return the sequence of characters that form a stretched version of `ch` at
/// the given `height`.  Returns an empty `Vec` if the character is not a known
/// stretchable delimiter.
fn stretch_chars_for(ch: char, height: Row, ctx: &TermMathContext) -> Vec<char> {
    let mode = ctx.config.mode;
    match ch {
        '(' | '⟮' => mode.left_paren(height),
        ')' | '⟯' => mode.right_paren(height),
        '[' => mode.left_bracket(height),
        ']' => mode.right_bracket(height),
        '{' => mode.left_brace(height),
        '}' => mode.right_brace(height),
        '|' => mode.vert_bar(height),
        '‖' => mode.double_vert_bar(height),
        '⌊' => mode.floor_left(height),
        '⌋' => mode.floor_right(height),
        '⌈' => mode.ceil_left(height),
        '⌉' => mode.ceil_right(height),
        '⟨' | '<' => mode.angle_left(height),
        '⟩' | '>' => mode.angle_right(height),
        _ => vec![],
    }
}

// ── layout_mid ────────────────────────────────────────────────────────────────

/// Layout a [`MidElem`].
///
/// Lays out the body and reclassifies it as a `Relation` delimiter.
pub fn layout_mid(
    elem: &Packed<MidElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let mut frag = ctx.layout_into_fragment(&elem.body, styles)?;
    frag.set_class(MathClass::Relation);
    ctx.push(frag);
    Ok(())
}
// ── layout_stretch ────────────────────────────────────────────────────────────

/// Layout a [`StretchElem`].
///
/// Known stretchable characters (arrows, braces, lines) are extended by
/// repeating the glyph body.  Unknown characters pass through as-is.
pub fn layout_stretch(
    elem: &Packed<StretchElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let ch = extract_stretch_char(&elem.body);
    match ch {
        Some(c) => {
            let w = resolve_stretch_width(elem.size.get_cloned(styles), styles).max(1);
            let s = hstretch_char(c, w, ctx.config.mode);
            let frame = TermFrame::text(s, ContentStyle::default());
            ctx.push(TermMathFrameFragment::new(frame));
        }
        None => {
            let frag = ctx.layout_into_fragment(&elem.body, styles)?;
            ctx.push(frag);
        }
    }
    Ok(())
}

fn extract_stretch_char(content: &Content) -> Option<char> {
    content.to_packed::<SymbolElem>().and_then(|sym| {
        let mut cs = sym.text.chars();
        let c = cs.next()?;
        cs.next().is_none().then_some(c)
    })
}

fn resolve_stretch_width(size: Rel<Length>, styles: StyleChain) -> Col {
    use typst::layout::Abs;
    use typst::text::TextElem;
    let font_size = styles.get(TextElem::size).resolve(styles);
    // 1em = 1 terminal column (monospace approximation)
    let col_width = font_size;
    let rel: Rel<Abs> = size.resolve(styles);
    let abs = rel.relative_to(col_width);
    (abs.to_pt() / col_width.to_pt()).ceil().max(1.0) as Col
}

pub fn hstretch_char(ch: char, width: Col, mode: RenderMode) -> String {
    let w = width.max(1) as usize;
    match ch {
        // ── Right arrows ────────────────────────────────────────────────────
        '→' => arrow_r(w, '─', '→', '-', '>', mode),
        '⇒' => arrow_r(w, '═', '⇒', '=', '>', mode),
        // ── Left arrows ─────────────────────────────────────────────────────
        '←' => arrow_l(w, '←', '─', '<', '-', mode),
        '⇐' => arrow_l(w, '<', '=', '<', '=', mode),
        // ── Bidirectional arrows ────────────────────────────────────────────
        '\u{2194}' => arrow_lr(w, '←', '─', '⟶', '<', '-', '>', mode),
        // '\u{21D4}' => arrow_lr(w, '⇐', '═', '⇒','<' ,'=', '>', mode),
        '\u{21D4}' => arrow_lr(w, '<', '=', '>', '<', '=', '>', mode),
        // // ── Horizontal braces ───────────────────────────────────────────────
        // '\u{23DF}' => rep(mode.underbrace_char(), w),
        // '\u{23DE}' => rep(mode.overbrace_char(), w),
        // ── Horizontal lines ────────────────────────────────────────────────
        '\u{2500}' | '\u{2015}' => rep(mode.hbar(), w),
        // ── Fallback ────────────────────────────────────────────────────────
        _ => rep(ch, w),
    }
}

fn arrow_r(
    w: usize,
    uni_body: char,
    uni_head: char,
    ascii_body: char,
    ascii_head: char,
    mode: RenderMode,
) -> String {
    if mode.is_unicode() {
        // ─────→  (body line + arrow head, at least 1 col for head)
        format!("{}{}", rep(uni_body, w.saturating_sub(1)), uni_head)
    } else {
        format!("{}{}", rep(ascii_body, w.saturating_sub(1)), ascii_head)
    }
}

fn arrow_l(
    w: usize,
    uni_head: char,
    uni_body: char,
    ascii_head: char,
    ascii_body: char,
    mode: RenderMode,
) -> String {
    if mode.is_unicode() {
        // ←─────
        format!("{}{}", uni_head, rep(uni_body, w.saturating_sub(1)))
    } else {
        format!("{}{}", ascii_head, rep(ascii_body, w.saturating_sub(1)))
    }
}

fn arrow_lr(
    w: usize,
    uni_l: char,
    uni_body: char,
    uni_r: char,
    ascii_l: char,
    ascii_body: char,
    ascii_r: char,
    mode: RenderMode,
) -> String {
    if mode.is_unicode() {
        format!("{}{}{}", uni_l, rep(uni_body, w.saturating_sub(2)), uni_r)
    } else {
        format!(
            "{}{}{}",
            ascii_l,
            rep(ascii_body, w.saturating_sub(2)),
            ascii_r
        )
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn rep(ch: char, n: usize) -> String {
    std::iter::repeat(ch).take(n).collect()
}
