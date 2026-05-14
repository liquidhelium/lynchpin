//! Terminal math text, symbol, and operator layout.

use typst::foundations::{Packed, StyleChain, SymbolElem};
use typst::math::{EquationElem, MathSize, OpElem};
use typst::text::TextElem;

use super::fragment::{make_text_frame, MathClass, TermLimits, TermMathFragment, TermMathFrameFragment};
use super::operators;
use super::TermMathContext;

// ── layout_text ───────────────────────────────────────────────────────────────

/// Layout a [`TextElem`] in a math context.
pub fn layout_text(
    elem: &Packed<TextElem>,
    ctx: &mut TermMathContext,
    _styles: StyleChain,
) -> typst::diag::SourceResult<()> {
    let text = &elem.text;
    if text.is_empty() {
        return Ok(());
    }

    // Split on newlines to produce linebreak-separated fragments.
    if text.contains('\n') {
        let lines: Vec<&str> = text.split('\n').collect();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                ctx.push(TermMathFragment::Linebreak);
            }
            if !line.is_empty() {
                ctx.push(single_text_fragment(line));
            }
        }
    } else {
        ctx.push(single_text_fragment(text.as_str()));
    }
    Ok(())
}

fn single_text_fragment(text: &str) -> TermMathFrameFragment {
    let frame = make_text_frame(text);
    // Multi-letter alphabetic text (e.g. "sin", "text") gets `spaced = true`
    // so that a soft-space (from SpaceElem) between it and an adjacent symbol
    // is materialised.  Single characters are not marked spaced; their spacing
    // is handled purely by the math-class auto-spacing rules.
    // Mirrors the upstream `layout_inline_text` which calls `.with_spaced(true)`
    // for non-digit multi-character text.
    let is_multi_letter = text.chars().count() > 1;
    TermMathFrameFragment::new(frame)
        .with_class(MathClass::Alphabetic)
        .with_text_like(true)
        .with_spaced(is_multi_letter)
}

// ── layout_symbol ─────────────────────────────────────────────────────────────

/// Layout a [`SymbolElem`] in a math context.
///
/// Each character is turned into a separate `TermMathFrameFragment`.
/// In display mode, large operators (∑, ∫, ∏, …) are rendered as
/// composed multi-row forms instead of single characters.
pub fn layout_symbol(
    elem: &Packed<SymbolElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> typst::diag::SourceResult<()> {
    let text = &elem.text;
    if text.is_empty() {
        return Ok(());
    }

    let is_display = styles.get(EquationElem::size) == MathSize::Display;

    for ch in text.chars() {
        let class = crate::math::fragment::math_class(ch).unwrap_or(MathClass::Normal);

        let fragment = if class == MathClass::Large && is_display {
            large_op_fragment(ch, ctx)
        } else {
            let s = ch.to_string();
            let frame = make_text_frame(&s);
            TermMathFrameFragment::new(frame).with_class(class)
        };

        ctx.push(fragment);
    }
    Ok(())
}

/// Build a composed large-operator fragment for display mode.
///
/// Returns a `TermMathFrameFragment` with `MathClass::Large` and
/// `TermLimits::Display` so the attach system places limits above/below.
fn large_op_fragment(ch: char, ctx: &TermMathContext) -> TermMathFrameFragment {
    let mode = ctx.config.mode;
    let frame = match ch {
        '\u{2211}' => operators::build_sum_operator(mode, lynchpin_library_ng::frame::Row::new(1)),
        '\u{222B}' => operators::build_integral_operator(mode, lynchpin_library_ng::frame::Row::new(2)),
        '\u{220F}' => operators::build_prod_operator(mode),
        '\u{2210}' => operators::build_prod_operator(mode),
        _ => {
            let s = ch.to_string();
            return TermMathFrameFragment::new(make_text_frame(&s))
                .with_class(MathClass::Large);
        }
    };

    TermMathFrameFragment::new(frame)
        .with_class(MathClass::Large)
        .with_limits(TermLimits::Display)
}

// ── layout_op ─────────────────────────────────────────────────────────────────

/// Layout an [`OpElem`] (named math operator like `lim`, `sin`, `max`, …).
pub fn layout_op(
    elem: &Packed<OpElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> typst::diag::SourceResult<()> {
    // Layout the operator's text content (usually a TextElem "lim", "sin", …).
    let frag = ctx.layout_into_fragment(&elem.text, styles)?;
    let italics = frag.italics_correction();
    let accent = frag.accent_attach();
    let text_like = frag.is_text_like();

    let limits = if elem.limits.get(styles) {
        TermLimits::Display
    } else {
        TermLimits::Never
    };

    ctx.push(
        TermMathFrameFragment::new(frag.into_frame())
            .with_class(MathClass::Large)
            .with_limits(limits)
            .with_italics_correction(italics)
            .with_accent_attach(accent)
            .with_text_like(text_like),
    );
    Ok(())
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Create a plain text fragment for `s` with the given class.
pub(super) fn text_frag(s: &str, class: MathClass) -> TermMathFrameFragment {
    let frame = make_text_frame(s);
    TermMathFrameFragment::new(frame)
        .with_class(class)
        .with_text_like(true)
}
