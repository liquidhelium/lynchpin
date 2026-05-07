//! Shape and decoration helpers.
//!
//! Functions for building stretched vertical delimiter frames and single-char
//! vertical bar frames, used by the math layout subsystem and inline decorators.

use crossterm::style::ContentStyle;

use crate::frame::{Row, TermFrame, TermPoint, TermSize};

// ── Delimiter frames ──────────────────────────────────────────────────────────

/// Build a 1-column-wide [`TermFrame`] from a `Vec<char>` of delimiter
/// characters.
///
/// Each character occupies exactly one row; the frame is `chars.len()` rows
/// tall.  The baseline is set to `chars.len() / 2` (rounded down) so that the
/// delimiter is vertically centred on the math axis.
///
/// This is the terminal equivalent of typst's stretched-delimiter frames built
/// by `config.mode.left_paren(height)` etc.  The caller supplies the character
/// list (produced by [`RenderMode::left_paren`](crate::config::RenderMode::left_paren)
/// and friends); this function turns it into a positioned [`TermFrame`].
///
/// *Single-width characters are assumed.*  If a wide character is passed the
/// visual result will be two columns wide but the frame width will still be 1;
/// callers should use `char_cols` to check and adjust accordingly.
pub fn delimiter_frame(chars: Vec<char>, style: ContentStyle) -> TermFrame {
    let height = chars.len() as Row;
    if height == 0 {
        return TermFrame::new(TermSize::ZERO);
    }

    let mut frame = TermFrame::new(TermSize::new(1, height));
    frame.set_baseline(height / 2);

    for (i, ch) in chars.into_iter().enumerate() {
        // push_text takes impl Into<EcoString>; a single char converts fine.
        frame.push_text(TermPoint::new(0, i as Row), ch.to_string(), style);
    }

    frame
}

// ── Single-char vertical bars ─────────────────────────────────────────────────

/// Build a 1-column-wide [`TermFrame`] by repeating `ch` for `height` rows.
///
/// Useful for plain vertical bars (`|`, `│`), integral signs, etc. that do
/// not need a multi-character stretch.
///
/// The baseline is positioned at `baseline`, clamped to `[0, height - 1]`.
pub fn single_char_vframe(ch: char, height: Row, baseline: Row, style: ContentStyle) -> TermFrame {
    let h = height.max(1);
    let mut frame = TermFrame::new(TermSize::new(1, h));
    frame.set_baseline(baseline.clamp(0, h - 1));

    for r in 0..h {
        frame.push_text(TermPoint::new(0, r), ch.to_string(), style);
    }

    frame
}
