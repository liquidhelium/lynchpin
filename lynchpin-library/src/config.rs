//! Terminal rendering configuration.
//!
//! [`RenderMode`] selects between Unicode and ASCII rendering of math
//! constructs (Σ glyphs, stretched brackets, radical signs, etc.).
//! This is analogous to Diagon's `Style` struct.
//!
//! [`TermConfig`] bundles all render-time options.

use crate::frame::{Col, Row};

// ── RenderMode ────────────────────────────────────────────────────────────────

/// Whether to use Unicode drawing characters for math constructs.
///
/// Both modes output regular Unicode text for all alphanumeric and operator
/// symbols (α, β, ∑, ∫, ≤, …) — they differ only for *graphical* constructs
/// that require multi-character or multi-row drawing:
///
/// | Construct        | Unicode mode              | ASCII mode         |
/// |------------------|---------------------------|--------------------|
/// | Fraction bar     | `─`                       | `-`                |
/// | Sqrt overline    | `─` + `√`                 | `_` + `V`          |
/// | Stretched `(`    | `⎛⎜⎝`                     | `(`s               |
/// | Sum (display)    | Σ + `─` diagonals         | Σ + `_` / `\` `/` |
/// | Integral (tall)  | `⌠│⌡`                     | `/\|\/`            |
/// | Underbrace       | `⏟` stretched             | `v---v`            |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Use Unicode drawing / box-drawing characters.  Default.
    #[default]
    Unicode,
    /// Use ASCII-only characters for maximum compatibility.
    Ascii,
}

impl RenderMode {
    #[inline]
    pub fn is_unicode(self) -> bool {
        self == RenderMode::Unicode
    }
    #[inline]
    pub fn is_ascii(self) -> bool {
        self == RenderMode::Ascii
    }

    // ── Fraction / rule characters ────────────────────────────────────────────

    /// Horizontal rule character used for fraction bars, overlines, etc.
    pub fn hbar(self) -> char {
        if self.is_unicode() { '─' } else { '-' }
    }

    // ── Square-root characters ────────────────────────────────────────────────

    /// The radical symbol placed to the left of the radicand.
    pub fn sqrt_sym(self) -> char {
        if self.is_unicode() { '√' } else { 'V' }
    }

    /// Character used for the overline drawn above the radicand.
    pub fn sqrt_overline(self) -> char {
        if self.is_unicode() { '─' } else { '_' }
    }

    /// Top-right corner cap of the radical overline.
    pub fn sqrt_cap(self) -> char {
        // Unicode: '┐' (upper-right corner), ASCII: none (just extend the bar)
        if self.is_unicode() { '┐' } else { '_' }
    }

    // ── Stretched vertical delimiters ─────────────────────────────────────────

    /// Return `height` characters that form a stretched left parenthesis.
    pub fn left_paren(self, height: Row) -> Vec<char> {
        stretch_delim(
            height,
            if self.is_unicode() {
                ('[', '⎛', '⎜', '⎝')
            } else {
                ('(', '(', '(', '(')
            },
        )
    }

    pub fn right_paren(self, height: Row) -> Vec<char> {
        stretch_delim(
            height,
            if self.is_unicode() {
                (']', '⎞', '⎟', '⎠')
            } else {
                (')', ')', ')', ')')
            },
        )
    }

    pub fn left_bracket(self, height: Row) -> Vec<char> {
        stretch_delim(
            height,
            if self.is_unicode() {
                ('[', '⎡', '⎢', '⎣')
            } else {
                ('[', '[', '|', '[')
            },
        )
    }

    pub fn right_bracket(self, height: Row) -> Vec<char> {
        stretch_delim(
            height,
            if self.is_unicode() {
                (']', '⎤', '⎥', '⎦')
            } else {
                (']', ']', '|', ']')
            },
        )
    }

    /// Left curly brace, stretched to `height` rows.
    pub fn left_brace(self, height: Row) -> Vec<char> {
        if self.is_unicode() {
            unicode_brace(height, true)
        } else {
            stretch_delim(height, ('{', '/', '|', '\\'))
        }
    }

    /// Right curly brace, stretched to `height` rows.
    pub fn right_brace(self, height: Row) -> Vec<char> {
        if self.is_unicode() {
            unicode_brace(height, false)
        } else {
            stretch_delim(height, ('}', '\\', '|', '/'))
        }
    }

    /// Vertical bar `|`, stretched to `height` rows.
    pub fn vert_bar(self, height: Row) -> Vec<char> {
        let ch = if self.is_unicode() { '│' } else { '|' };
        vec![ch; height.max(1) as usize]
    }

    /// Double vertical bar `‖`, stretched to `height` rows.
    pub fn double_vert_bar(self, height: Row) -> Vec<char> {
        let ch = if self.is_unicode() { '║' } else { '|' };
        vec![ch; height.max(1) as usize]
    }

    pub fn floor_left(self, height: Row) -> Vec<char> {
        stretch_delim(
            height,
            if self.is_unicode() {
                ('⌊', '⎢', '⎢', '⌊')
            } else {
                ('[', '|', '|', '[')
            },
        )
    }

    pub fn floor_right(self, height: Row) -> Vec<char> {
        stretch_delim(
            height,
            if self.is_unicode() {
                ('⌋', '⎥', '⎥', '⌋')
            } else {
                (']', '|', '|', ']')
            },
        )
    }

    pub fn ceil_left(self, height: Row) -> Vec<char> {
        stretch_delim(
            height,
            if self.is_unicode() {
                ('⌈', '⌈', '⎢', '⎢')
            } else {
                ('[', '[', '|', '|')
            },
        )
    }

    pub fn ceil_right(self, height: Row) -> Vec<char> {
        stretch_delim(
            height,
            if self.is_unicode() {
                ('⌉', '⌉', '⎥', '⎥')
            } else {
                (']', ']', '|', '|')
            },
        )
    }

    /// Angle bracket ⟨, stretched.
    pub fn angle_left(self, height: Row) -> Vec<char> {
        let ch = if self.is_unicode() { '⟨' } else { '<' };
        vec![ch; height.max(1) as usize]
    }

    /// Angle bracket ⟩, stretched.
    pub fn angle_right(self, height: Row) -> Vec<char> {
        let ch = if self.is_unicode() { '⟩' } else { '>' };
        vec![ch; height.max(1) as usize]
    }

    // ── Large operator characters ─────────────────────────────────────────────

    /// Summation operator characters for display mode (top, diagonal-top,
    /// mid/Σ, diagonal-bot, bottom).
    pub fn sum_chars(self) -> SumChars {
        if self.is_unicode() {
            SumChars {
                top: '─',
                diag_top: '╲',
                mid: 'Σ',
                diag_bot: '╱',
                bot: '─',
            }
        } else {
            SumChars {
                top: '_',
                diag_top: '\\',
                mid: 'E',
                diag_bot: '/',
                bot: '_',
            }
        }
    }

    /// Product operator (Π) display-mode characters (top bar, vert sides,
    /// separator char).
    pub fn prod_chars(self) -> ProdChars {
        if self.is_unicode() {
            ProdChars {
                top: '─',
                vert: '│',
                base: '─',
            }
        } else {
            ProdChars {
                top: '_',
                vert: '|',
                base: '_',
            }
        }
    }

    /// Integral sign characters: top, middle (repeated), bottom.
    pub fn integral_chars(self) -> IntegralChars {
        if self.is_unicode() {
            IntegralChars {
                top: '⌠',
                mid: '│',
                bot: '⌡',
            }
        } else {
            IntegralChars {
                top: '/',
                mid: '|',
                bot: '\\',
            }
        }
    }

    // ── Underbrace / overbrace ────────────────────────────────────────────────

    /// Character for underbrace bottom

    // ── Accent attach characters ──────────────────────────────────────────────

    /// Character to use for a hat (^) accent.
    pub fn hat_char(self) -> char {
        if self.is_unicode() { '̂' } else { '^' } // combining circumflex
    }

    pub fn tilde_char(self) -> char {
        if self.is_unicode() { '̃' } else { '~' } // combining tilde
    }

    pub fn dot_char(self) -> char {
        if self.is_unicode() { '̇' } else { '.' } // combining dot above
    }

    pub fn ddot_char(self) -> char {
        if self.is_unicode() { '̈' } else { '"' } // combining diaeresis
    }

    pub fn bar_accent_char(self) -> char {
        if self.is_unicode() { '̄' } else { '-' } // combining macron
    }

    pub fn arrow_over_char(self) -> char {
        // Combining right arrow above: →
        if self.is_unicode() { '⃗' } else { '>' }
    }
}

// ── Helper types returned by RenderMode methods ───────────────────────────────

pub struct SumChars {
    pub top: char,
    pub diag_top: char,
    pub mid: char,
    pub diag_bot: char,
    pub bot: char,
}

pub struct ProdChars {
    pub top: char,
    pub vert: char,
    pub base: char,
}

pub struct IntegralChars {
    pub top: char,
    pub mid: char,
    pub bot: char,
}

// ── Delimiter stretching helpers ──────────────────────────────────────────────

/// Build a vertical delimiter of `height` rows from (single, top, mid, bot).
///
/// * height == 1 → [single]
/// * height == 2 → [top, bot]
/// * height  > 2 → [top, mid×(height-2), bot]
pub fn stretch_delim(height: Row, (single, top, mid, bot): (char, char, char, char)) -> Vec<char> {
    let h = height.max(1) as usize;
    match h {
        1 => vec![single],
        2 => vec![top, bot],
        _ => {
            let mut v = Vec::with_capacity(h);
            v.push(top);
            for _ in 0..h - 2 {
                v.push(mid);
            }
            v.push(bot);
            v
        }
    }
}

/// Build a Unicode curly brace of `height` rows.
///
/// For heights ≥ 3 the brace has a *knot* at the centre.
fn unicode_brace(height: Row, left: bool) -> Vec<char> {
    let h = height.max(1) as usize;
    if left {
        match h {
            1 => vec!['{'],
            2 => vec!['⎧', '⎩'],
            3 => vec!['⎧', '⎨', '⎩'],
            _ => {
                let mid_rows = h - 3;
                let top_mids = mid_rows / 2;
                let bot_mids = mid_rows - top_mids;
                let mut v = Vec::with_capacity(h);
                v.push('⎧');
                v.extend(std::iter::repeat('⎪').take(top_mids));
                v.push('⎨');
                v.extend(std::iter::repeat('⎪').take(bot_mids));
                v.push('⎩');
                v
            }
        }
    } else {
        match h {
            1 => vec!['}'],
            2 => vec!['⎫', '⎭'],
            3 => vec!['⎫', '⎬', '⎭'],
            _ => {
                let mid_rows = h - 3;
                let top_mids = mid_rows / 2;
                let bot_mids = mid_rows - top_mids;
                let mut v = Vec::with_capacity(h);
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

// ── TermConfig ────────────────────────────────────────────────────────────────

/// Top-level configuration for terminal layout and rendering.
#[derive(Debug, Clone)]
pub struct TermConfig {
    /// ASCII or Unicode rendering mode.
    pub mode: RenderMode,
    /// Maximum output width in columns.  `None` = unlimited.
    pub width: Option<Col>,
    /// Indent step used for nested blocks (list items, block quotes, etc.).
    pub indent: Col,
}

impl Default for TermConfig {
    fn default() -> Self {
        Self {
            mode: RenderMode::Unicode,
            width: Some(80),
            indent: 2,
        }
    }
}
