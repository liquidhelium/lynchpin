//! Terminal rendering configuration.
//!
//! [`RenderMode`] selects between Unicode and ASCII rendering of math
//! constructs and shapes.
//!
//! [`TermConfig`] bundles all render-time options — the terminal equivalent
//! of page setup in paged layout.

use crate::frame::Col;
use crate::scalar::TermScalar;

// ── RenderMode ────────────────────────────────────────────────────────────────

/// Whether to use Unicode drawing characters.
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

    // ── Rule characters ───────────────────────────────────────────────────

    pub fn hbar(self) -> char {
        if self.is_unicode() { '─' } else { '-' }
    }

    pub fn vbar(self) -> char {
        if self.is_unicode() { '│' } else { '|' }
    }

    // ── Square-root ───────────────────────────────────────────────────────

    pub fn sqrt_sym(self) -> char {
        if self.is_unicode() { '√' } else { 'V' }
    }

    pub fn sqrt_overline(self) -> char {
        if self.is_unicode() { '─' } else { '_' }
    }

    // ── Delimiters ────────────────────────────────────────────────────────

    pub fn left_paren(self) -> char { '(' }
    pub fn right_paren(self) -> char { ')' }
    pub fn left_bracket(self) -> char { '[' }
    pub fn right_bracket(self) -> char { ']' }
    pub fn left_brace(self) -> char { '{' }
    pub fn right_brace(self) -> char { '}' }
}

// ── TermConfig ────────────────────────────────────────────────────────────────

/// Top-level configuration for terminal layout and rendering.
///
/// The terminal equivalent of `PageElem` settings in paged layout.
#[derive(Debug, Clone)]
pub struct TermConfig {
    /// ASCII or Unicode rendering mode.
    pub mode: RenderMode,
    /// Maximum output width in columns.  `None` = unbounded.
    pub width: Option<Col>,
    /// Indent step used for nested blocks (list items, block quotes, etc.).
    pub indent: Col,
    /// Page height in rows.  `None` = unbounded (content grows freely).
    pub height: Option<Col>,
    /// Gap between blocks in rows.
    pub block_gap: Col,
    /// Gap between paragraphs in rows.
    pub par_gap: Col,
}

impl Default for TermConfig {
    fn default() -> Self {
        Self {
            mode: RenderMode::Unicode,
            width: Some(TermScalar::new(80)),
            indent: TermScalar::new(2),
            height: None,
            block_gap: TermScalar::new(1),
            par_gap: TermScalar::new(1),
        }
    }
}

impl TermConfig {
    /// Return the effective width, falling back to a generous default.
    pub fn effective_width(&self) -> Col {
        self.width.unwrap_or(TermScalar::new(80))
    }

    /// Return the effective height, or infinity if unbounded.
    pub fn effective_height(&self) -> Col {
        self.height.unwrap_or(TermScalar::INFINITY)
    }
}
