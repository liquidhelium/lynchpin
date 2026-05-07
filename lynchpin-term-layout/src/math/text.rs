//! Terminal math text, symbol, and operator layout.

use crossterm::style::ContentStyle;
use ecow::EcoString;
use typst::foundations::{Packed, StyleChain, SymbolElem};
use typst::math::{EquationElem, MathSize, OpElem};
use typst::text::TextElem;
use unicode_math_class::MathClass;

use crate::frame::TermFrame;

use super::fragment::{TermLimits, TermMathFragment, TermMathFrameFragment};
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
    let frame = TermFrame::text(EcoString::from(text), ContentStyle::default());
    TermMathFrameFragment::new(frame)
        .with_class(MathClass::Alphabetic)
        .with_text_like(true)
}

// ── layout_symbol ─────────────────────────────────────────────────────────────

/// Layout a [`SymbolElem`] in a math context.
///
/// Each grapheme cluster (character) is turned into a separate
/// `TermMathFrameFragment` so that class-based spacing can operate per-glyph.
pub fn layout_symbol(
    elem: &Packed<SymbolElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> typst::diag::SourceResult<()> {
    let text = &elem.text;
    if text.is_empty() {
        return Ok(());
    }

    // Retrieve current display size so we can set limits for large operators.
    let is_display = styles
        .get(EquationElem::size) == MathSize::Display;

    for ch in text.chars() {
        let class = unicode_math_class::class(ch).unwrap_or(MathClass::Normal);

        // Single-character frame.
        let s = EcoString::from(ch.to_string());
        let frame = TermFrame::text(s, ContentStyle::default());
        let mut frag = TermMathFrameFragment::new(frame).with_class(class);

        // Large operators in display mode use limits layout by default.
        if class == MathClass::Large && is_display {
            frag = frag.with_limits(TermLimits::Display);
        }

        ctx.push(frag);
    }
    Ok(())
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
    let frame = TermFrame::text(EcoString::from(s), ContentStyle::default());
    TermMathFrameFragment::new(frame).with_class(class).with_text_like(true)
}
