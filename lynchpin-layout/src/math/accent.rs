//! Terminal layout for [`AccentElem`].
//!
//! Accents are placed in an extra row above (top accents) or below (bottom
//! accents) the base content.  The accent character is output directly from
//! [`Accent::0`] without any special mapping — terminal fonts handle most
//! combining / spacing accent codepoints adequately.

use crossterm::style::ContentStyle;
use lynchpin_library::frame::{Col, Row, TermFrame, TermPoint, TermSize};
use typst::diag::SourceResult;
use typst::foundations::{Packed, StyleChain};
use typst::math::{Accent, AccentElem};

use super::fragment::{make_text_frame, MathClass};
use super::{TermMathContext, TermMathFrameFragment};

// ── layout_accent ─────────────────────────────────────────────────────────────

/// Layout an [`AccentElem`].
///
/// **Top accent** layout (default):
/// ```text
///   ^        (row 0 — accent char, centred over base's accent_attach column)
///   base…    (rows 1 .. 1+base_rows, baseline = 1 + base.baseline())
/// ```
///
/// **Bottom accent** layout ([`Accent::is_bottom`]):
/// ```text
///   base…    (rows 0 .. base_rows-1, baseline = base.baseline())
///   _        (row base_rows — accent char, centred under base)
/// ```
pub fn layout_accent(
    elem: &Packed<AccentElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let base_frame = ctx.layout_into_frame(&elem.base, styles)?;
    let accent = elem.accent;
    let is_bottom = accent.is_bottom();

    let base_cols = base_frame.cols();
    let base_rows = base_frame.rows();
    let base_baseline = base_frame.baseline();

    // Build a one-row frame for the accent character.
    let accent_str: String = display_accent_char(ctx, accent).to_string();
    let accent_text_frame = make_text_frame(accent_str.as_str());
    let accent_cols = accent_text_frame.cols().max(Col::new(1));

    // Horizontal position: centre the accent over the base's accent-attach point.
    // accent_attach is the column midpoint of the base.
    let attach_col = Col::new(base_cols.get() / 2).max(Col::ZERO);
    let accent_x = (attach_col - Col::new(accent_cols.get() / 2)).max(Col::ZERO);

    let total_cols = base_cols.max(accent_cols);

    if is_bottom {
        // ── Bottom accent ──────────────────────────────────────────────────
        let total_rows = base_rows + Row::new(1);
        let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));
        frame.set_baseline(base_baseline);

        frame.push_frame(TermPoint::ZERO, base_frame);
        frame.push_text(
                TermPoint::new(accent_x, base_rows),
                accent_str.into(),
                ContentStyle::default(),
            );

        ctx.push(
            TermMathFrameFragment::new(frame)
                .with_class(MathClass::Normal)
                .with_accent_attach(attach_col),
        );
    } else {
        // ── Top accent ─────────────────────────────────────────────────────
        let total_rows = Row::new(1) + base_rows;
        let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));
        frame.set_baseline(Row::new(1) + base_baseline);

        frame.push_text(
            TermPoint::new(accent_x, Row::ZERO),
            accent_str.into(),
            ContentStyle::default(),
        );
        frame.push_frame(TermPoint::new(Col::ZERO, Row::new(1)), base_frame);

        ctx.push(
            TermMathFrameFragment::new(frame)
                .with_class(MathClass::Normal)
                .with_accent_attach(attach_col),
        );
    }

    Ok(())
}

// ── Accent character selection ────────────────────────────────────────────────

/// Return the character to display for the given accent in the current render
/// mode.
///
/// Combining codepoints (U+0300–U+036F, U+20D0–U+20FF) are fine to output
/// directly — they compose with the preceding base character in most terminals.
/// For the few accents that have a nicer dedicated fallback in ASCII mode we
/// substitute a plain ASCII character.
fn display_accent_char(ctx: &TermMathContext, accent: Accent) -> char {
    let ch = accent.0;
    if ctx.config.mode.is_ascii() {
        // Map common combining accents to ASCII equivalents.
        match ch {
            '\u{0300}' => '`',  // combining grave
            '\u{0301}' => '\'', // combining acute
            '\u{0302}' => '^',  // combining circumflex / hat
            '\u{0303}' => '~',  // combining tilde
            '\u{0304}' | '\u{0305}' => '-', // combining macron / overline
            '\u{0306}' => 'u',  // combining breve
            '\u{0307}' => '.',  // combining dot above
            '\u{0308}' => '"',  // combining diaeresis
            '\u{030A}' => 'o',  // combining ring above
            '\u{030B}' => '"',  // combining double acute
            '\u{030C}' => 'v',  // combining caron
            '\u{20D6}' => '<',  // combining left arrow above
            '\u{20D7}' => '>',  // combining right arrow above
            '\u{20E1}' => '-',  // combining left-right arrow
            '\u{20D0}' => '<',  // combining left harpoon above
            '\u{20D1}' => '>',  // combining right harpoon above
            _ => combining_to_spacing(ch),
        }
    } else {
        // In Unicode mode, combining characters (width=0) are skipped by the
        // terminal grid renderer.  Map them to their spacing equivalents so
        // they actually appear in the accent row.
        combining_to_spacing(ch)
    }
}

/// Convert a combining accent codepoint to a spacing (width-1) equivalent.
///
/// Falls back to the original character for unrecognised codepoints.
fn combining_to_spacing(ch: char) -> char {
    match ch {
        '\u{0300}' => '\u{0060}', // COMBINING GRAVE ACCENT → ` (GRAVE ACCENT)
        '\u{0301}' => '\u{00B4}', // COMBINING ACUTE ACCENT → ´ (ACUTE ACCENT)
        '\u{0302}' => '\u{02C6}', // COMBINING CIRCUMFLEX ACCENT → ˆ
        '\u{0303}' => '\u{02DC}', // COMBINING TILDE → ˜
        '\u{0304}' => '\u{02C9}', // COMBINING MACRON → ˉ
        '\u{0305}' => '\u{00AF}', // COMBINING OVERLINE → ¯ (MACRON)
        '\u{0306}' => '\u{02D8}', // COMBINING BREVE → ˘
        '\u{0307}' => '\u{02D9}', // COMBINING DOT ABOVE → ˙
        '\u{0308}' => '\u{00A8}', // COMBINING DIAERESIS → ¨
        '\u{0309}' => '\u{02C0}', // COMBINING HOOK ABOVE → ˀ
        '\u{030A}' => '\u{02DA}', // COMBINING RING ABOVE → ˚
        '\u{030B}' => '\u{02DD}', // COMBINING DOUBLE ACUTE ACCENT → ˝
        '\u{030C}' => '\u{02C7}', // COMBINING CARON → ˇ
        '\u{0332}' | '\u{0333}' => '_', // COMBINING LOW LINE / DOUBLE LOW LINE
        '\u{20D0}' => '\u{21BC}', // COMBINING LEFT HARPOON ABOVE → ↼
        '\u{20D1}' => '\u{21C0}', // COMBINING RIGHT HARPOON ABOVE → ⇀
        '\u{20D6}' => '\u{2190}', // COMBINING LEFT ARROW ABOVE → ←
        '\u{20D7}' => '\u{2192}', // COMBINING RIGHT ARROW ABOVE → →
        '\u{20E1}' => '\u{2194}', // COMBINING LEFT RIGHT ARROW ABOVE → ↔
        _ => ch,
    }
}
