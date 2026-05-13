//! Terminal rendering configuration.
//!
//! [`RenderMode`] selects between Unicode and ASCII rendering of math
//! constructs and shapes.
//!
//! [`TermConfig`] bundles render-time options — the terminal equivalent
//! of page setup in paged layout.  Unlike paged, page **dimensions** are
//! NOT stored here; they are resolved from `PageElem::width`/`PageElem::height`
//! in the style chain (see `resolve_page_size` in `lynchpin-layout-ng`).

use typst::foundations::{Resolve, StyleChain};
use typst::layout::PageElem;
use typst::text::TextElem;

use crate::frame::{Col, TermSize};
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

    pub fn hbar(self) -> char {
        if self.is_unicode() { '─' } else { '-' }
    }

    pub fn vbar(self) -> char {
        if self.is_unicode() { '│' } else { '|' }
    }

    pub fn sqrt_sym(self) -> char {
        if self.is_unicode() { '√' } else { 'V' }
    }

    pub fn sqrt_overline(self) -> char {
        if self.is_unicode() { '─' } else { '_' }
    }

    pub fn left_paren(self) -> char { '(' }
    pub fn right_paren(self) -> char { ')' }
    pub fn left_bracket(self) -> char { '[' }
    pub fn right_bracket(self) -> char { ']' }
    pub fn left_brace(self) -> char { '{' }
    pub fn right_brace(self) -> char { '}' }
}

// ── TermConfig ────────────────────────────────────────────────────────────────

/// Terminal rendering configuration — the terminal equivalent of
/// `PageElem` settings in paged layout.
///
/// Page **dimensions** are NOT stored here.  They are resolved from
/// `PageElem::width` / `PageElem::height` in the style chain via
/// [`resolve_page_size`].
#[derive(Debug, Clone)]
pub struct TermConfig {
    /// ASCII or Unicode rendering mode.
    pub mode: RenderMode,
    /// Indent step used for nested blocks (list items, block quotes, etc.).
    pub indent: Col,
    /// Gap between blocks in rows.
    pub block_gap: Col,
    /// Gap between paragraphs in rows.
    pub par_gap: Col,
}

impl Default for TermConfig {
    fn default() -> Self {
        Self {
            mode: RenderMode::Unicode,
            indent: TermScalar::new(2),
            block_gap: TermScalar::new(1),
            par_gap: TermScalar::new(1),
        }
    }
}

// ── Page size resolution ─────────────────────────────────────────────────────

/// Resolve the terminal page size from the style chain, just as paged
/// reads `PageElem::width` / `PageElem::height`.
///
/// Returns `(width_in_cols, height_in_rows)`.
pub fn resolve_page_size(styles: StyleChain) -> TermSize {
    use typst::layout::Abs;

    // Read page dimensions from styles (same as paged `pages/run.rs:102-103`).
    let page_width = styles.resolve(PageElem::width).unwrap_or(Abs::inf());
    let page_height = styles.resolve(PageElem::height).unwrap_or(Abs::inf());

    // Convert to terminal columns/rows using the font size.
    let font_size = styles.get(TextElem::size).0.resolve(styles);

    let cols = if page_width != Abs::inf() {
        TermScalar::from_f64((page_width / font_size).round().max(1.0))
    } else {
        // Page width is auto: use unbounded so layout shrink-wraps to content.
        TermScalar::INFINITY
    };

    let rows = if page_height != Abs::inf() {
        TermScalar::from_f64((page_height / font_size).round().max(1.0))
    } else {
        // When page height is auto, use unbounded.
        TermScalar::INFINITY
    };

    TermSize::new(cols, rows)
}
