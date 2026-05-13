//! Unicode superscript / subscript character conversion.
//!
//! Used by [`crate::math::attach`] to inline single-row scripts when every
//! character in the script has a corresponding Unicode combining form.
//!
//! # Tables
//!
//! Two macros (`superscript_map!` / `subscript_map!`) generate the `match`
//! arms from a compact source table.  Each arm maps one base character to its
//! Unicode equivalent.  If a character has no equivalent the macro arm is
//! simply absent and the catch-all `_ => None` fires.
//!
//! # Usage
//!
//! ```ignore
//! use crate::math::unicode_scripts::{frame_to_superscript, frame_to_subscript};
//!
//! if let Some(s) = frame_to_superscript(frame) {
//!     // all characters converted — place `s` inline
//! }
//! ```

use lynchpin_library_ng::TermFrame;
use lynchpin_library_ng::TermFrameItem;
use lynchpin_library_ng::frame::{Col, char_cols};
use lynchpin_library_ng::scalar::TermScalar;

// ── Public single-character converters ───────────────────────────────────────

/// Convert a single character to its Unicode superscript equivalent, if one
/// exists.
#[inline]
pub fn to_superscript(ch: char) -> Option<char> {
    match ch {
        '0' => Some('⁰'),
        '1' => Some('¹'),
        '2' => Some('²'),
        '3' => Some('³'),
        '4' => Some('⁴'),
        '5' => Some('⁵'),
        '6' => Some('⁶'),
        '7' => Some('⁷'),
        '8' => Some('⁸'),
        '9' => Some('⁹'),
        '+' => Some('⁺'),
        '-'|'−' => Some('⁻'),
        '=' => Some('⁼'),
        '(' => Some('⁽'),
        ')' => Some('⁾'),
        'a' => Some('ᵃ'),
        'b' => Some('ᵇ'),
        'c' => Some('ᶜ'),
        'd' => Some('ᵈ'),
        'e' => Some('ᵉ'),
        'f' => Some('ᶠ'),
        'g' => Some('ᵍ'),
        'h' => Some('ʰ'),
        'i' => Some('ⁱ'),
        'j' => Some('ʲ'),
        'k' => Some('ᵏ'),
        'l' => Some('ˡ'),
        'm' => Some('ᵐ'),
        'n' => Some('ⁿ'),
        'o' => Some('ᵒ'),
        'p' => Some('ᵖ'),
        'r' => Some('ʳ'),
        's' => Some('ˢ'),
        't' => Some('ᵗ'),
        'u' => Some('ᵘ'),
        'v' => Some('ᵛ'),
        'w' => Some('ʷ'),
        'x' => Some('ˣ'),
        'y' => Some('ʸ'),
        'z' => Some('ᶻ'),
        'A' => Some('ᴬ'),
        'B' => Some('ᴮ'),
        'C' => Some('ᶜ'),
        'D' => Some('ᴰ'),
        'E' => Some('ᴱ'),
        'F' => Some('ꟳ'),
        'G' => Some('ᴳ'),
        'H' => Some('ᴴ'),
        'I' => Some('ᴵ'),
        'J' => Some('ᴶ'),
        'K' => Some('ᴷ'),
        'L' => Some('ᴸ'),
        'M' => Some('ᴹ'),
        'N' => Some('ᴺ'),
        'O' => Some('ᴼ'),
        'P' => Some('ᴾ'),
        'Q' => Some('ꟴ'),
        'R' => Some('ᴿ'),
        'S' => Some('꟱'),
        'T' => Some('ᵀ'),
        'U' => Some('ᵁ'),
        'W' => Some('ᵂ'),
        'β' => Some('ᵝ'),
        'γ' => Some('ᵞ'),
        'δ' => Some('ᵟ'),
        'φ' => Some('ᵠ'),
        'χ' => Some('ᵡ'),
        'θ' => Some('ᶿ'),
        _ => None, }
}

/// Convert a single character to its Unicode subscript equivalent, if one
/// exists.
#[inline]
pub fn to_subscript(ch: char) -> Option<char> {
    match ch {
        '0' => Some('₀'),
        '1' => Some('₁'),
        '2' => Some('₂'),
        '3' => Some('₃'),
        '4' => Some('₄'),
        '5' => Some('₅'),
        '6' => Some('₆'),
        '7' => Some('₇'),
        '8' => Some('₈'),
        '9' => Some('₉'),
        '+' => Some('₊'),
        '-'|'−' => Some('₋'),
        '=' => Some('₌'),
        '(' => Some('₍'),
        ')' => Some('₎'),
        'a' => Some('ₐ'),
        'e' => Some('ₑ'),
        'h' => Some('ₕ'),
        'i' => Some('ᵢ'),
        'j' => Some('ⱼ'),
        'k' => Some('ₖ'),
        'l' => Some('ₗ'),
        'm' => Some('ₘ'),
        'n' => Some('ₙ'),
        'o' => Some('ₒ'),
        'p' => Some('ₚ'),
        'r' => Some('ᵣ'),
        's' => Some('ₛ'),
        't' => Some('ₜ'),
        'u' => Some('ᵤ'),
        'v' => Some('ᵥ'),
        'x' => Some('ₓ'),
        'β' => Some('ᵦ'),
        'γ' => Some('ᵧ'),
        'ρ' => Some('ᵨ'),
        'φ' => Some('ᵩ'),
        'χ' => Some('ᵪ'),
        _ => None,

    }
}

// ── Frame-level converters ────────────────────────────────────────────────────

/// Try to convert every character in `frame` to its Unicode superscript form.
///
/// Returns `Some(converted_string)` only if:
/// - The frame contains nothing other than `Text` and nested `Frame` items
///   (rules, shapes, and images cannot be converted)
/// - Every individual character has a Unicode superscript equivalent
///
/// Characters are collected in column order.
pub fn frame_to_superscript(frame: &TermFrame) -> Option<String> {
    let chars = collect_chars(frame, TermScalar::ZERO)?;
    // it is too ugly to preserve spacing between items with spaces, and it is not worth the effort to do so, so we just ignore spacing between items and concatenate all characters together
     chars.into_iter().map(|(_, ch)| to_superscript(ch)).collect()
    // let chars_and_blanks_between = chars.windows(2).map(|w| (w[0].0, w[0].1, w[1].0 - w[0].0)).chain(
    //     chars.last().map(|(col, ch)| (*col, *ch, TermScalar::ZERO))
    // );
    // chars_and_blanks_between
    //     .map(|(_, ch, blank)| {
    //         let mut s = String::new();
    //         for _ in 0..blank.get() {
    //             s.push(' '); // preserve spacing between items with spaces
    //         }
    //         to_superscript(ch).map(|sup| {
    //             s.push(sup);
    //             s
    //         })
    //     })
    //     .collect()
}

/// Try to convert every character in `frame` to its Unicode subscript form.
///
/// Same constraints as [`frame_to_superscript`].
pub fn frame_to_subscript(frame: &TermFrame) -> Option<String> {
    let chars = collect_chars(frame, TermScalar::ZERO)?;
    chars.into_iter().map(|(_, ch)| to_subscript(ch)).collect()
    // let chars_and_blanks_between = chars.windows(2).map(|w| (w[0].0, w[0].1, w[1].0 - w[0].0)).chain(
    //     chars.last().map(|(col, ch)| (*col, *ch, TermScalar::ZERO))
    // );
    // chars_and_blanks_between
    //     .map(|(_, ch, blank)| {
    //         let mut s = String::new();
    //         for _ in 0..blank.get() {
    //             s.push(' '); // preserve spacing between items with spaces
    //         }
    //         to_subscript(ch).map(|sub| {
    //             s.push(sub);
    //             s
    //         })
    //     })
    //     .collect()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Recursively collect `(col, char)` pairs from a frame, sorted by column.
///
/// Returns `None` if the frame contains any item type that cannot be expressed
/// as characters (rules, shapes, images).
fn collect_chars(frame: &TermFrame, col_offset: Col) -> Option<Vec<(Col, char)>> {
    let mut out: Vec<(Col, char)> = Vec::new();

    for (pos, item) in frame.items() {
        let abs_col = pos.col + col_offset;
        match item {
            TermFrameItem::Text(s, _style) => {
                let mut c = abs_col;
                for ch in s.chars() {
                    out.push((c, ch));
                    c = c + char_cols(ch);
                }
            }
            TermFrameItem::Frame(sub) => {
                let sub_chars = collect_chars(sub, abs_col)?;
                out.extend(sub_chars);
            }
            // Tags carry no visible glyphs — skip them.
            TermFrameItem::Tag(_) => {}
            // Rules / shapes / images are not expressible as Unicode chars.
            _ => return None,
        }
    }

    // Sort by column position so multi-item frames come out in reading order.
    out.sort_by_key(|(col, _)| *col);
    Some(out)
}
