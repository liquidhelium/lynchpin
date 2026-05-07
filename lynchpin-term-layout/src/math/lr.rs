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
use typst::foundations::{Packed, StyleChain};
use typst::math::{LrElem, MidElem, StretchElem};
use unicode_math_class::MathClass;

use crate::frame::Row;

use super::{TermMathContext, TermMathFragment, TermMathFrameFragment};
use super::run::build_delimiter_frame;

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
    let inner_height = inner_content_rows(&frags);

    // Stretch the opening delimiter (first fragment if it looks like one).
    if let Some(first) = frags.first_mut() {
        try_stretch_delimiter(first, inner_height, ctx, true);
    }

    // Stretch the closing delimiter (last fragment if it looks like one).
    let last_idx = frags.len() - 1;
    if last_idx > 0 {
        if let Some(last) = frags.last_mut() {
            try_stretch_delimiter(last, inner_height, ctx, false);
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
    matches!(class, MathClass::Opening | MathClass::Closing | MathClass::Fence)
}

/// Measure the tallest non-delimiter fragment inside the lr group.
///
/// We look at all fragments except the first and last (which are typically the
/// opening/closing delimiters).  If the group has ≤ 2 fragments we return 1.
fn inner_content_rows(frags: &[TermMathFragment]) -> Row {
    if frags.len() <= 2 {
        return 1;
    }
    frags[1..frags.len() - 1]
        .iter()
        .map(|f| f.rows())
        .max()
        .unwrap_or(1)
        .max(1)
}

/// If `frag` is a single-character delimiter, replace it in-place with a
/// stretched version.  Does nothing if the fragment is not recognisable as a
/// delimiter character.
fn try_stretch_delimiter(
    frag: &mut TermMathFragment,
    height: Row,
    ctx: &TermMathContext,
    _is_opening: bool,
) {
    // Only stretch Frame fragments.
    let class = frag.class();
    if !is_delimiter_class(class) {
        return;
    }

    // Extract the single character from the frame.
    let ch = match extract_single_char(frag) {
        Some(c) => c,
        None => return,
    };

    let chars = stretch_chars_for(ch, height, ctx);
    if chars.is_empty() {
        return;
    }

    let new_frame = build_delimiter_frame(chars, ContentStyle::default());
    let new_rows = new_frame.rows();
    let baseline = new_rows / 2; // vertically centred baseline
    let mut frame_frag = TermMathFrameFragment::new(new_frame)
        .with_class(class);
    frame_frag.frame.set_baseline(baseline);

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
/// For terminal rendering we cannot perform actual glyph stretching, so we
/// simply lay out the body as-is.
pub fn layout_stretch(
    elem: &Packed<StretchElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let frag = ctx.layout_into_fragment(&elem.body, styles)?;
    ctx.push(frag);
    Ok(())
}
