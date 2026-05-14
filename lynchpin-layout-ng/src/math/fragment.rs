//! Terminal math fragments.
//!
//! Analogous to `typst-layout`'s `MathFragment` / `FrameFragment` but
//! simplified for terminal grid rendering.

use crossterm::style::ContentStyle;
use lynchpin_library_ng::frame::{text_cols, Col, Row, TermFrame, TermSize};

// ── MathClass ────────────────────────────────────────────────────────────────

/// Unicode math class for a character or fragment.
///
/// Mirrors `unicode_math_class::MathClass`.  We define this locally to avoid
/// pulling in an extra dependency that isn't available in `lynchpin-layout-ng`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathClass {
    Normal,
    Alphabetic,
    Binary,
    Closing,
    Diacritic,
    Fence,
    GlyphPart,
    Large,
    Opening,
    Punctuation,
    Relation,
    Space,
    Special,
    Unclassified,
    Varying,
}

/// Classify a Unicode character into its math class.
///
/// Returns `None` for characters not in our lookup table (callers should
/// default to `MathClass::Normal`).
///
/// This is a minimal implementation covering only the most common math
/// symbols.  A full implementation would use the `unicode-math-class` crate.
pub fn math_class(ch: char) -> Option<MathClass> {
    match ch {
        // ── Opening delimiters ─────────────────────────────────────────────
        '(' | '[' | '{' | '⟮' | '⟨' | '〈' | '⌈' | '⌊' => Some(MathClass::Opening),

        // ── Closing delimiters ─────────────────────────────────────────────
        ')' | ']' | '}' | '⟯' | '⟩' | '〉' | '⌉' | '⌋' => Some(MathClass::Closing),

        // ── Fence (unicode) ────────────────────────────────────────────────
        '|' => Some(MathClass::Fence),
        '‖' | '∥' => Some(MathClass::Fence),

        // ── Binary operators ───────────────────────────────────────────────
        '+' | '−' | '±' | '∓' | '×' | '÷' | '∗' | '∘' | '∙' | '∩' | '∪'
        | '∧' | '∨' | '⊂' | '⊃' | '⊄' | '⊅' | '⊆' | '⊇' | '⊈' | '⊉'
        | '⊊' | '⊋' | '⊌' | '⊍' | '⊎' | '⊏' | '⊐' | '⊑' | '⊒' | '⊓'
        | '⊔' | '⊕' | '⊖' | '⊗' | '⊘' | '⊙' | '⊚' | '⊛' | '⊜' | '⊝'
        | '⋅' | '⋆' | '⋇' | '⋈' | '⋉' | '⋊' | '⋋' | '⋌' | '⋍' | '⋎'
        | '⋏' | '⋐' | '⋑' | '⋒' | '⋓' | '⋔' | '⋕' | '⋖' | '⋗' | '⋘'
        | '⋙' | '⋚' | '⋛' | '⋜' | '⋝' | '⋞' | '⋟' | '⋠' | '⋡' | '⋢'
        | '⋣' | '⋤' | '⋥' | '⋦' | '⋧' | '⋨' | '⋩' | '⋪' | '⋫' | '⋬'
        | '⋭' | '⋮' | '⋯' | '⋰' | '⋱' | '⋲' | '⋳' | '⋴' | '⋵' | '⋶'
        | '⋷' | '⋸' | '⋹' | '⋺' | '⋻' | '⋼' | '⋽' | '⋾' | '⋿'
        | '\\' | '¬' => Some(MathClass::Binary),

        // ── Relations ──────────────────────────────────────────────────────
        '=' | '≠' | '≡' | '≢' | '<' | '>' | '≤' | '≥' | '≪' | '≫' | '≮'
        | '≯' | '≰' | '≱' | '≲' | '≳' | '≴' | '≵' | '≶' | '≷' | '≸'
        | '≹' | '≺' | '≻' | '≼' | '≽' | '≾' | '≿' | '⊀' | '⊁' | '∼'
        | '∽' | '∿' | '≁' | '≂' | '≃' | '≄' | '≅' | '≆' | '≇' | '≈'
        | '≉' | '≊' | '≋' | '≌' | '≍' | '≎' | '≏' | '≐' | '≑' | '≒'
        | '≓' | '≔' | '≕' | '≖' | '≗' | '≘' | '≙' | '≚' | '≛' | '≜'
        | '≝' | '≞' | '≟' | '≣' | '∈' | '∉' | '∊'
        | '∋' | '∌' | '∍' | '→' | '←' | '↔'
        | '⇒' | '⇐' | '⇔' | '↦' | '⟹' | '⟸' | '⟺' | '∶' | '∷'
        | '∝' | '∞' | '∟' => Some(MathClass::Relation),

        // ── Large operators ────────────────────────────────────────────────
        '∑' | '∫' | '∬' | '∭' | '∮' | '∯' | '∰' | '∱' | '∲' | '∳'
        | '∏' | '∐' | '⋀' | '⋁' | '⋂' | '⋃' | '⨀' | '⨁' | '⨂' | '⨃'
        | '⨄' | '⨅' | '⨆' | '⨇' | '⨈' | '⨉' | '⨊' | '⨋' | '⨌' | '⨍'
        | '⨎' | '⨏' | '⨐' | '⨑' | '⨒' | '⨓' | '⨔' | '⨕' | '⨖' | '⨗'
        | '⨘' | '⨙' | '⨚' | '⨛' | '⨜' | '⨝' | '⨞' | '⨟' | '⨠' | '⨡'
        | '⨢' | '⨣' | '⨤' | '⨥' | '⨦' | '⨧' | '⨨' | '⨩' | '⨪' | '⨫'
        | '⨬' | '⨭' | '⨮' | '⨯' | '⨰' | '⨱' | '⨲' | '⨳' | '⨴' | '⨵'
        | '⨶' | '⨷' | '⨸' | '⨹' | '⨺' | '⨻' | '⨼' | '⨽' | '⨾' | '⨿'
        | '⩀' | '⩁' | '⩂' => Some(MathClass::Large),

        // ── Punctuation ────────────────────────────────────────────────────
        ',' | ';' | ':' | '.' | '!' | '?' => Some(MathClass::Punctuation),

        // ── Diacritic ──────────────────────────────────────────────────────
        '\u{0300}'..='\u{036F}' | '\u{20D0}'..='\u{20FF}' => Some(MathClass::Diacritic),

        // ── Space ──────────────────────────────────────────────────────────
        ' ' | '\u{00A0}' | '\u{2000}'..='\u{200A}' => Some(MathClass::Space),

        // ── Alphabetic ─────────────────────────────────────────────────────
        'a'..='z' | 'A'..='Z' | 'α'..='ω' | 'Α'..='Ω' | '0'..='9' => {
            Some(MathClass::Alphabetic)
        }

        _ => None,
    }
}

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
    /// Soft space originating from a `SpaceElem` in the math source.
    ///
    /// Unlike `Spacing`, this variant is **never pushed** into the resolved
    /// fragment list directly.  Instead it is held as a pending soft-space and
    /// only materialised into a `Spacing(1, false)` when the adjacent fragments
    /// have `is_spaced() == true` (e.g. multi-letter text operators).
    /// This mirrors the upstream `MathFragment::Space` handling in
    /// `typst-layout`'s `MathRun::new`.
    Space,
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
            Self::Space | Self::Align | Self::Linebreak => Col::ZERO,
        }
    }

    /// Total height in terminal rows.
    #[inline]
    pub fn rows(&self) -> Row {
        match self {
            Self::Frame(f) => f.rows(),
            Self::Space | Self::Spacing(_, _) | Self::Align | Self::Linebreak => Row::ZERO,
        }
    }

    /// Rows strictly above the baseline.
    #[inline]
    pub fn ascent(&self) -> Row {
        match self {
            Self::Frame(f) => f.ascent(),
            Self::Space | Self::Spacing(_, _) | Self::Align | Self::Linebreak => Row::ZERO,
        }
    }

    /// Rows at or below the baseline.
    #[inline]
    pub fn descent(&self) -> Row {
        match self {
            Self::Frame(f) => f.descent(),
            Self::Space | Self::Spacing(_, _) | Self::Align | Self::Linebreak => Row::ZERO,
        }
    }

    /// Baseline row index (0 = top row).
    #[inline]
    pub fn baseline(&self) -> Row {
        match self {
            Self::Frame(f) => f.baseline(),
            Self::Space | Self::Spacing(_, _) | Self::Align | Self::Linebreak => Row::ZERO,
        }
    }

    /// Unicode math class of this fragment.
    #[inline]
    pub fn class(&self) -> MathClass {
        match self {
            Self::Frame(f) => f.class,
            Self::Space | Self::Spacing(_, _) | Self::Align | Self::Linebreak => MathClass::Normal,
        }
    }

    /// Limits mode for large operators.
    #[inline]
    pub fn limits(&self) -> TermLimits {
        match self {
            Self::Frame(f) => f.limits,
            Self::Space | Self::Spacing(_, _) | Self::Align | Self::Linebreak => TermLimits::Never,
        }
    }

    /// Whether this fragment should be invisible for automatic inter-fragment
    /// spacing purposes (Align, Linebreak).
    ///
    /// Note: `Space` is handled *before* the ignorant check in `TermMathRun::new`
    /// and is never pushed to the resolved list, so it does not need to be
    /// listed here.
    #[inline]
    pub fn is_ignorant(&self) -> bool {
        matches!(self, Self::Align | Self::Linebreak)
    }

    /// Whether this fragment has explicit surrounding space.
    ///
    /// When `true`, a pending soft-space (from `SpaceElem`) will be
    /// materialised as a `Spacing(1, false)` between this fragment and its
    /// neighbour.  Set on multi-letter text operators and inline-box frames.
    #[inline]
    pub fn is_spaced(&self) -> bool {
        match self {
            Self::Frame(f) => f.spaced,
            Self::Space | Self::Spacing(_, _) | Self::Align | Self::Linebreak => false,
        }
    }

    /// Whether this fragment is "text-like" (alphabetics, operator names).
    #[inline]
    pub fn is_text_like(&self) -> bool {
        match self {
            Self::Frame(f) => f.text_like,
            Self::Space | Self::Spacing(_, _) | Self::Align | Self::Linebreak => false,
        }
    }

    /// Extra columns after italic content (usually 0 or 1).
    #[inline]
    pub fn italics_correction(&self) -> Col {
        match self {
            Self::Frame(f) => f.italics_correction,
            Self::Space | Self::Spacing(_, _) | Self::Align | Self::Linebreak => Col::ZERO,
        }
    }

    /// Column of the accent attachment point (mid-point of the base).
    #[inline]
    pub fn accent_attach(&self) -> Col {
        match self {
            Self::Frame(f) => f.accent_attach,
            Self::Space | Self::Spacing(_, _) | Self::Align | Self::Linebreak => Col::ZERO,
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
            Self::Spacing(cols, _) => TermFrame::new(TermSize::new(cols.max(Col::ZERO), Row::ZERO)),
            Self::Space | Self::Align | Self::Linebreak => TermFrame::new(TermSize::ZERO),
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

// ── Conversion from unicode_math_class ──────────────────────────────────────

impl From<unicode_math_class::MathClass> for MathClass {
    fn from(c: unicode_math_class::MathClass) -> Self {
        match c {
            unicode_math_class::MathClass::Normal => MathClass::Normal,
            unicode_math_class::MathClass::Alphabetic => MathClass::Alphabetic,
            unicode_math_class::MathClass::Binary => MathClass::Binary,
            unicode_math_class::MathClass::Closing => MathClass::Closing,
            unicode_math_class::MathClass::Diacritic => MathClass::Diacritic,
            unicode_math_class::MathClass::Fence => MathClass::Fence,
            unicode_math_class::MathClass::GlyphPart => MathClass::GlyphPart,
            unicode_math_class::MathClass::Large => MathClass::Large,
            unicode_math_class::MathClass::Opening => MathClass::Opening,
            unicode_math_class::MathClass::Punctuation => MathClass::Punctuation,
            unicode_math_class::MathClass::Relation => MathClass::Relation,
            unicode_math_class::MathClass::Space => MathClass::Space,
            unicode_math_class::MathClass::Unary => MathClass::Normal,
            unicode_math_class::MathClass::Vary => MathClass::Varying,
            unicode_math_class::MathClass::Special => MathClass::Special,
        }
    }
}

impl TermMathFrameFragment {
    /// Build a fragment from `frame` with sensible defaults.
    pub fn new(frame: TermFrame) -> Self {
        let attach = Col::new(frame.cols().get() / 2).max(Col::ZERO);
        Self {
            frame,
            class: MathClass::Normal,
            limits: TermLimits::Never,
            spaced: false,
            text_like: false,
            italics_correction: Col::ZERO,
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

// ── Text frame helper ────────────────────────────────────────────────────────

/// Create a single-row [`TermFrame`] containing the given text.
///
/// Wraps `TermFrame::text()` with the correct column width computed from the
/// text length and a default row height of 1.
pub fn make_text_frame(text: &str) -> TermFrame {
    let cols = text_cols(text);
    TermFrame::text(text, ContentStyle::default(), cols, Row::new(1))
}
