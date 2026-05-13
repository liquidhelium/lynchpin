//! Terminal left–right delimiter group layout.
//!
//! Handles [`LrElem`] (auto-stretching delimiters), [`MidElem`] (mid
//! delimiters), and [`StretchElem`] (explicitly stretched glyphs).
//!
//! For terminal rendering we cannot sub-pixel scale characters, so stretching
//! is approximated by repeating box-drawing characters to span the tallest
//! inner fragment.

use crossterm::style::ContentStyle;
use lynchpin_library_ng::config::RenderMode;
use lynchpin_library_ng::frame::{Col, Row};
use typst::diag::SourceResult;
use typst::foundations::{Content, Packed, Resolve, StyleChain, SymbolElem};
use typst::layout::{Abs, Length, Rel};
use typst::math::{LrElem, MidElem, StretchElem};
use typst::text::TextElem;

use super::fragment::{make_text_frame, MathClass};
use super::run::build_delimiter_frame;
use super::{TermMathContext, TermMathFragment, TermMathFrameFragment};

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
        return (Row::new(1), Row::ZERO);
    }
    let inner = &frags[1..frags.len() - 1];
    let a = inner.iter().map(|f| f.ascent()).max().unwrap_or(Row::ZERO);
    let d = inner.iter().map(|f| f.descent()).max().unwrap_or(Row::new(1));
    ((a + d).max(Row::new(1)), a.max(Row::ZERO))
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

    let mut found: Option<char> = None;
    for (_pt, item) in frame.items() {
        match item {
            lynchpin_library_ng::frame::TermFrameItem::Text(text, _style) => {
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
            lynchpin_library_ng::frame::TermFrameItem::Frame(_) => return None,
            _ => {}
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
        '(' | '⟮' => left_paren(mode, height),
        ')' | '⟯' => right_paren(mode, height),
        '[' => left_bracket(mode, height),
        ']' => right_bracket(mode, height),
        '{' => left_brace(mode, height),
        '}' => right_brace(mode, height),
        '|' => vec![mode.vbar(); height.max(Row::new(1)).get() as usize],
        '‖' => vec![if mode.is_unicode() { '║' } else { '|' }; height.max(Row::new(1)).get() as usize],
        '⌊' => floor_left(mode, height),
        '⌋' => floor_right(mode, height),
        '⌈' => ceil_left(mode, height),
        '⌉' => ceil_right(mode, height),
        '⟨' | '<' => vec![if mode.is_unicode() { '⟨' } else { '<' }; height.max(Row::new(1)).get() as usize],
        '⟩' | '>' => vec![if mode.is_unicode() { '⟩' } else { '>' }; height.max(Row::new(1)).get() as usize],
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
            let w = resolve_stretch_width(elem.size.get_cloned(styles), styles).max(Col::new(1));
            let s = hstretch_char(c, w, ctx.config.mode);
            let frame = make_text_frame(&s);
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
    let font_size: Abs = styles.get(TextElem::size).resolve(styles);
    let col_width = font_size;
    let rel: Rel<Abs> = size.resolve(styles);
    let abs = rel.relative_to(col_width);
    Col::from_f64((abs.to_pt() / col_width.to_pt()).ceil().max(1.0))
}

/// Horizontally stretch a character to `width` columns.
pub fn hstretch_char(ch: char, width: Col, mode: RenderMode) -> String {
    let w = width.max(Col::new(1)).get() as usize;
    match ch {
        // ── Right arrows ────────────────────────────────────────────────────
        '→' => arrow_r(w, '─', '→', '-', '>', mode),
        '⇒' => arrow_r(w, '═', '⇒', '=', '>', mode),
        // ── Left arrows ─────────────────────────────────────────────────────
        '←' => arrow_l(w, '←', '─', '<', '-', mode),
        '⇐' => arrow_l(w, '<', '=', '<', '=', mode),
        // ── Bidirectional arrows ────────────────────────────────────────────
        '\u{2194}' => arrow_lr(w, '←', '─', '⟶', '<', '-', '>', mode),
        '\u{21D4}' => arrow_lr(w, '<', '=', '>', '<', '=', '>', mode),
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

// ── Delimiter stretching helpers ──────────────────────────────────────────────

fn left_paren(mode: RenderMode, height: Row) -> Vec<char> {
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

fn right_paren(mode: RenderMode, height: Row) -> Vec<char> {
    let h = height.max(Row::new(1)).get() as usize;
    if mode.is_unicode() {
        match h {
            1 => vec![')'],
            2 => vec!['⎞', '⎠'],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push('⎞');
                for _ in 0..h - 2 {
                    v.push('⎟');
                }
                v.push('⎠');
                v
            }
        }
    } else {
        vec![')'; h]
    }
}

fn left_bracket(mode: RenderMode, height: Row) -> Vec<char> {
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

fn right_bracket(mode: RenderMode, height: Row) -> Vec<char> {
    let h = height.max(Row::new(1)).get() as usize;
    if mode.is_unicode() {
        match h {
            1 => vec![']'],
            2 => vec!['⎤', '⎦'],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push('⎤');
                for _ in 0..h - 2 {
                    v.push('⎥');
                }
                v.push('⎦');
                v
            }
        }
    } else {
        match h {
            1 => vec![']'],
            2 => vec![']', ']'],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push(']');
                for _ in 0..h - 2 {
                    v.push('|');
                }
                v.push(']');
                v
            }
        }
    }
}

fn left_brace(mode: RenderMode, height: Row) -> Vec<char> {
    let h = height.max(Row::new(1)).get() as usize;
    if mode.is_unicode() {
        unicode_brace(h, true)
    } else {
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
    }
}

fn right_brace(mode: RenderMode, height: Row) -> Vec<char> {
    let h = height.max(Row::new(1)).get() as usize;
    if mode.is_unicode() {
        unicode_brace(h, false)
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

fn floor_left(mode: RenderMode, height: Row) -> Vec<char> {
    let h = height.max(Row::new(1)).get() as usize;
    if mode.is_unicode() {
        match h {
            1 => vec!['⌊'],
            2 => vec!['│', '└'],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push('╷');
                for _ in 1..h - 1 {
                    v.push('⎢');
                }
                v.push('└');
                v
            }
        }
    } else {
        match h {
            1 => vec!['['],
            _ => {
                let mut v = Vec::with_capacity(h);
                for _ in 0..h - 1 {
                    v.push('|');
                }
                v.push('[');
                v
            }
        }
    }
}

fn floor_right(mode: RenderMode, height: Row) -> Vec<char> {
    let h = height.max(Row::new(1)).get() as usize;
    if mode.is_unicode() {
        match h {
            1 => vec!['⌋'],
            2 => vec!['│', '┘'],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push('╷');
                for _ in 0..h - 1 {
                    v.push('│');
                }
                v.push('┘');
                v
            }
        }
    } else {
        match h {
            1 => vec![']'],
            _ => {
                let mut v = Vec::with_capacity(h);
                for _ in 0..h - 1 {
                    v.push('|');
                }
                v.push(']');
                v
            }
        }
    }
}

fn ceil_left(mode: RenderMode, height: Row) -> Vec<char> {
    let h = height.max(Row::new(1)).get() as usize;
    if mode.is_unicode() {
        match h {
            1 => vec!['⌈'],
            2 => vec!['┌', '│'],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push('┌');
                for _ in 0..h - 2 {
                    v.push('│');
                }
                v.push('╵');
                v
            }
        }
    } else {
        match h {
            1 => vec!['['],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push('[');
                for _ in 0..h - 1 {
                    v.push('|');
                }
                v
            }
        }
    }
}

fn ceil_right(mode: RenderMode, height: Row) -> Vec<char> {
    let h = height.max(Row::new(1)).get() as usize;
    if mode.is_unicode() {
        match h {
            1 => vec!['⌉'],
            2 => vec!['┐', '│'],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push('┐');
                for _ in 0..h - 2 {
                    v.push('│');
                }
                v.push('╵');
                v
            }
        }
    } else {
        match h {
            1 => vec![']'],
            _ => {
                let mut v = Vec::with_capacity(h);
                v.push(']');
                for _ in 0..h - 1 {
                    v.push('|');
                }
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

// ── Helpers ───────────────────────────────────────────────────────────────────

fn rep(ch: char, n: usize) -> String {
    std::iter::repeat(ch).take(n).collect()
}
