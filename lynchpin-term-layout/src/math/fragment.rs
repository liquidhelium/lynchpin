//! Terminal math fragments.
//!
//! Analogous to `typst-layout`'s `MathFragment` / `FrameFragment` but
//! simplified for terminal grid rendering.

use unicode_math_class::MathClass;

use crate::frame::{Col, Row, TermFrame, TermSize};

// ── TermLimits ────────────────────────────────────────────────────────────────

/// Controls whether a large operator (∑, ∫, lim, …) shows its upper/lower
/// attachments as *limits* (stacked above/below) or as *scripts*
/// (smaller, to the top-right / bottom-right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermLimits {
    /// Never use limits layout; always use script positions.
    Never,
    /// Use limits layout only when the surrounding equation is in *Display*
    /// (block) size.
    Display,
    /// Always use limits layout, even inside inline equations.
    Always,
}

impl TermLimits {
    /// Returns `true` when limits layout should be active.
    ///
    /// `is_display` should be `true` when the enclosing equation is in
    /// display / block mode.
    #[inline]
    pub fn active(self, is_display: bool) -> bool {
        match self {
            Self::Never => false,
            Self::Display => is_display,
            Self::Always => true,
        }
    }
}

// ── TermMathFragment ──────────────────────────────────────────────────────────

/// A single item in the terminal math layout stream.
///
/// Analogous to `typst-layout`'s `MathFragment`.
#[derive(Debug, Clone)]
pub enum TermMathFragment {
    /// A rendered box with associated math metadata.
    Frame(TermMathFrameFragment),
    /// Horizontal whitespace (columns, is_weak).
    ///
    /// Weak spacing may be suppressed when an explicit non-weak spacing
    /// appears on the same side.
    Spacing(Col, bool),
    /// Alignment point used by aligned equations (`&`).
    Align,
    /// Explicit line-break inside a multi-line equation (`\\`).
    Linebreak,
}

impl TermMathFragment {
    /// Display width in terminal columns.
    #[inline]
    pub fn width(&self) -> Col {
        match self {
            Self::Frame(f) => f.width(),
            Self::Spacing(cols, _) => *cols,
            Self::Align | Self::Linebreak => 0,
        }
    }

    /// Total height in terminal rows.
    #[inline]
    pub fn rows(&self) -> Row {
        match self {
            Self::Frame(f) => f.rows(),
            _ => 0,
        }
    }

    /// Rows strictly above the baseline.
    #[inline]
    pub fn ascent(&self) -> Row {
        match self {
            Self::Frame(f) => f.ascent(),
            _ => 0,
        }
    }

    /// Rows at or below the baseline.
    #[inline]
    pub fn descent(&self) -> Row {
        match self {
            Self::Frame(f) => f.descent(),
            _ => 0,
        }
    }

    /// Baseline row index (0 = top row).
    #[inline]
    pub fn baseline(&self) -> Row {
        match self {
            Self::Frame(f) => f.baseline(),
            _ => 0,
        }
    }

    /// Unicode math class of this fragment.
    #[inline]
    pub fn class(&self) -> MathClass {
        match self {
            Self::Frame(f) => f.class,
            _ => MathClass::Normal,
        }
    }

    /// Limits mode for large operators.
    #[inline]
    pub fn limits(&self) -> TermLimits {
        match self {
            Self::Frame(f) => f.limits,
            _ => TermLimits::Never,
        }
    }

    /// Whether this fragment should be invisible for automatic inter-fragment
    /// spacing purposes (Align, Linebreak).
    #[inline]
    pub fn is_ignorant(&self) -> bool {
        matches!(self, Self::Align | Self::Linebreak)
    }

    /// Whether this fragment has explicit surrounding space.
    #[inline]
    pub fn is_spaced(&self) -> bool {
        match self {
            Self::Frame(f) => f.spaced,
            _ => false,
        }
    }

    /// Whether this fragment is "text-like" (alphabetics, operator names).
    #[inline]
    pub fn is_text_like(&self) -> bool {
        match self {
            Self::Frame(f) => f.text_like,
            _ => false,
        }
    }

    /// Extra columns after italic content (usually 0 or 1).
    #[inline]
    pub fn italics_correction(&self) -> Col {
        match self {
            Self::Frame(f) => f.italics_correction,
            _ => 0,
        }
    }

    /// Column of the accent attachment point (mid-point of the base).
    #[inline]
    pub fn accent_attach(&self) -> Col {
        match self {
            Self::Frame(f) => f.accent_attach,
            _ => 0,
        }
    }

    /// Set the `MathClass` (no-op for non-Frame variants).
    pub fn set_class(&mut self, class: MathClass) {
        if let Self::Frame(f) = self {
            f.class = class;
        }
    }

    /// Set the `TermLimits` (no-op for non-Frame variants).
    pub fn set_limits(&mut self, limits: TermLimits) {
        if let Self::Frame(f) = self {
            f.limits = limits;
        }
    }

    /// Consume this fragment and return the underlying [`TermFrame`].
    ///
    /// Non-frame variants produce a minimal zero-height frame of the
    /// appropriate width.
    pub fn into_frame(self) -> TermFrame {
        match self {
            Self::Frame(f) => f.frame,
            Self::Spacing(cols, _) => TermFrame::new(TermSize::new(cols.max(0), 0)),
            _ => TermFrame::new(TermSize::ZERO),
        }
    }
}

impl From<TermMathFrameFragment> for TermMathFragment {
    fn from(f: TermMathFrameFragment) -> Self {
        Self::Frame(f)
    }
}

// ── TermMathFrameFragment ─────────────────────────────────────────────────────

/// A [`TermFrame`] together with its math-layout metadata.
///
/// Analogous to `typst-layout`'s `FrameFragment`.
#[derive(Debug, Clone)]
pub struct TermMathFrameFragment {
    /// The rendered content.
    pub frame: TermFrame,
    /// Unicode math class — determines automatic inter-fragment spacing.
    pub class: MathClass,
    /// Whether limits layout is used for large operators.
    pub limits: TermLimits,
    /// Whether this fragment has explicit surrounding space.
    pub spaced: bool,
    /// Whether this fragment is "text-like".
    pub text_like: bool,
    /// Extra columns after italic content (0 or 1).
    pub italics_correction: Col,
    /// Column of the accent attachment point.
    pub accent_attach: Col,
}

impl TermMathFrameFragment {
    /// Build a fragment from `frame` with sensible defaults.
    pub fn new(frame: TermFrame) -> Self {
        let attach = (frame.cols() / 2).max(0);
        Self {
            frame,
            class: MathClass::Normal,
            limits: TermLimits::Never,
            spaced: false,
            text_like: false,
            italics_correction: 0,
            accent_attach: attach,
        }
    }

    // ── Builder methods ───────────────────────────────────────────────────────

    #[inline]
    pub fn with_class(mut self, class: MathClass) -> Self {
        self.class = class;
        self
    }
    #[inline]
    pub fn with_limits(mut self, limits: TermLimits) -> Self {
        self.limits = limits;
        self
    }
    #[inline]
    pub fn with_spaced(mut self, spaced: bool) -> Self {
        self.spaced = spaced;
        self
    }
    #[inline]
    pub fn with_text_like(mut self, text_like: bool) -> Self {
        self.text_like = text_like;
        self
    }
    #[inline]
    pub fn with_italics_correction(mut self, ic: Col) -> Self {
        self.italics_correction = ic;
        self
    }
    #[inline]
    pub fn with_accent_attach(mut self, aa: Col) -> Self {
        self.accent_attach = aa;
        self
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    #[inline]
    pub fn width(&self) -> Col {
        self.frame.cols()
    }
    #[inline]
    pub fn rows(&self) -> Row {
        self.frame.rows()
    }
    #[inline]
    pub fn ascent(&self) -> Row {
        self.frame.ascent()
    }
    #[inline]
    pub fn descent(&self) -> Row {
        self.frame.descent()
    }
    #[inline]
    pub fn baseline(&self) -> Row {
        self.frame.baseline()
    }
}
