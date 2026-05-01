use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Kitty Text Sizing Encoder - Encodes text sizing commands for Kitty Text Sizing Protocol
///
/// Reference: doc/kitty/docs/text-sizing-protocol.rst
#[derive(Debug, Clone, Copy, Default)]
pub struct KittyTextSizingEncoder;

/// Vertical alignment for fractional scaling
#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum KittyVerticalAlignment {
    /// Top alignment (default)
    Top = 0,
    /// Bottom alignment
    Bottom = 1,
    /// Centered alignment
    Centered = 2,
}

impl KittyVerticalAlignment {
    pub const fn value(self) -> u8 {
        self as u8
    }
}

/// Horizontal alignment for fractional scaling
#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum KittyHorizontalAlignment {
    /// Left alignment (default)
    Left = 0,
    /// Right alignment
    Right = 1,
    /// Centered alignment
    Centered = 2,
}

impl KittyHorizontalAlignment {
    pub const fn value(self) -> u8 {
        self as u8
    }
}

/// Parameters used to build a text sizing escape sequence.
#[derive(Debug, Clone, Default)]
pub struct EncodeParams {
    pub scale: Option<u8>,
    pub width: Option<u8>,
    pub numerator: Option<u8>,
    pub denominator: Option<u8>,
    pub vertical_alignment: Option<KittyVerticalAlignment>,
    pub horizontal_alignment: Option<KittyHorizontalAlignment>,
    pub use_bel_terminator: bool,
}

impl EncodeParams {
    pub fn new() -> Self {
        Self {
            use_bel_terminator: true,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(s) = self.scale {
            if !(1..=7).contains(&s) {
                return Err(ProtocolError::InvalidScale(s));
            }
        }
        if let Some(w) = self.width {
            if w > 7 {
                return Err(ProtocolError::InvalidWidth(w));
            }
        }
        if let Some(n) = self.numerator {
            if n > 15 {
                return Err(ProtocolError::InvalidNumerator(n));
            }
        }
        if let Some(d) = self.denominator {
            if d > 15 {
                return Err(ProtocolError::InvalidDenominatorOutOfRange(d));
            }
            let n = self.numerator.unwrap_or(0);
            if d != 0 && d <= n {
                return Err(ProtocolError::InvalidFraction {
                    numerator: n,
                    denominator: d,
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ProtocolError {
    InvalidScale(u8),
    InvalidWidth(u8),
    InvalidNumerator(u8),
    InvalidDenominatorOutOfRange(u8),
    InvalidFraction { numerator: u8, denominator: u8 },
    TextTooLong { len: usize, max: usize },
}

impl core::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProtocolError::InvalidScale(v) => {
                write!(f, "scale must be in 1..=7, got {v}")
            }
            ProtocolError::InvalidWidth(v) => {
                write!(f, "width must be in 0..=7, got {v}")
            }
            ProtocolError::InvalidNumerator(v) => {
                write!(f, "numerator must be in 0..=15, got {v}")
            }
            ProtocolError::InvalidDenominatorOutOfRange(v) => {
                write!(f, "denominator must be in 0..=15, got {v}")
            }
            ProtocolError::InvalidFraction {
                numerator,
                denominator,
            } => write!(
                f,
                "denominator must be > numerator when non-zero, got n={numerator}, d={denominator}"
            ),
            ProtocolError::TextTooLong { len, max } => {
                write!(f, "text exceeds MAX_TEXT_LENGTH ({max} bytes), got {len} bytes")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Grapheme + width information for robust client-side splitting/measurement.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GraphemeMeasure {
    pub grapheme: String,
    pub width: usize,
}

/// Terminal cell footprint occupied by one encoded text sizing sequence.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CellRect {
    pub width_cells: usize,
    pub height_cells: usize,
}

/// Builder for constructing Kitty text sizing escape sequences.
///
/// This is a fluent API over `EncodeParams`.
#[derive(Debug, Clone)]
pub struct KittyTextSizingBuilder {
    text: String,
    params: EncodeParams,
}

impl KittyTextSizingBuilder {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            params: EncodeParams::new(),
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    pub fn scale(mut self, value: u8) -> Self {
        self.params.scale = Some(value);
        self
    }

    pub fn width(mut self, value: u8) -> Self {
        self.params.width = Some(value);
        self
    }

    pub fn numerator(mut self, value: u8) -> Self {
        self.params.numerator = Some(value);
        self
    }

    pub fn denominator(mut self, value: u8) -> Self {
        self.params.denominator = Some(value);
        self
    }

    pub fn fraction(mut self, numerator: u8, denominator: u8) -> Self {
        self.params.numerator = Some(numerator);
        self.params.denominator = Some(denominator);
        self
    }

    pub fn vertical_alignment(mut self, value: KittyVerticalAlignment) -> Self {
        self.params.vertical_alignment = Some(value);
        self
    }

    pub fn horizontal_alignment(mut self, value: KittyHorizontalAlignment) -> Self {
        self.params.horizontal_alignment = Some(value);
        self
    }

    pub fn use_bel_terminator(mut self, value: bool) -> Self {
        self.params.use_bel_terminator = value;
        self
    }

    pub fn params(mut self, params: EncodeParams) -> Self {
        self.params = params;
        self
    }

    pub fn build(self) -> Result<(String, EncodeParams), ProtocolError> {
        self.params.validate()?;
        Ok((self.text, self.params))
    }

    pub fn encode(self) -> Result<String, ProtocolError> {
        let encoder = KittyTextSizingEncoder::new();
        let (text, params) = self.build()?;
        encoder.encode_checked(&text, params)
    }

    pub fn encode_with_rect(self) -> Result<(String, CellRect), ProtocolError> {
        let encoder = KittyTextSizingEncoder::new();
        let (text, params) = self.build()?;
        encoder.encode_with_rect_checked(&text, params)
    }
}

impl KittyTextSizingEncoder {
    /// Text size code identifier
    pub const TEXT_SIZE_CODE: &'static str = "_text_size_code";

    /// BEL terminator
    pub const BEL_TERMINATOR: &'static str = "\u{0007}";

    /// ESC ST terminator
    pub const ST_TERMINATOR: &'static str = "\u{001b}\\";

    /// Maximum text length in bytes (safe_utf8 payload)
    pub const MAX_TEXT_LENGTH: usize = 4096;

    pub const fn new() -> Self {
        Self
    }

    /// Start a fluent builder.
    pub fn builder(&self, text: impl Into<String>) -> KittyTextSizingBuilder {
        KittyTextSizingBuilder::new(text)
    }

    /// Build a text sizing escape sequence (unchecked parameter ranges).
    ///
    /// Format: <OSC> _text_size_code ; metadata ; text <terminator>
    /// Example: \x1b]_text_size_code;s=2;Double sized text\x07
    pub fn encode(&self, text: &str, params: EncodeParams) -> String {
        let metadata = self.build_metadata(
            params.scale,
            params.width,
            params.numerator,
            params.denominator,
            params.vertical_alignment,
            params.horizontal_alignment,
        );

        let terminator = if params.use_bel_terminator {
            Self::BEL_TERMINATOR
        } else {
            Self::ST_TERMINATOR
        };

        format!(
            "\u{001b}]{};{};{}{}",
            Self::TEXT_SIZE_CODE,
            metadata,
            text,
            terminator
        )
    }

    /// Checked encoder variant with protocol validation.
    pub fn encode_checked(&self, text: &str, params: EncodeParams) -> Result<String, ProtocolError> {
        params.validate()?;
        if text.len() > Self::MAX_TEXT_LENGTH {
            return Err(ProtocolError::TextTooLong {
                len: text.len(),
                max: Self::MAX_TEXT_LENGTH,
            });
        }
        Ok(self.encode(text, params))
    }

    /// Build metadata string from parameters.
    ///
    /// Invalid values are skipped (for strict behavior, use `encode_checked`).
    pub fn build_metadata(
        &self,
        scale: Option<u8>,
        width: Option<u8>,
        numerator: Option<u8>,
        denominator: Option<u8>,
        vertical_alignment: Option<KittyVerticalAlignment>,
        horizontal_alignment: Option<KittyHorizontalAlignment>,
    ) -> String {
        let mut parts: Vec<String> = Vec::new();

        if let Some(scale) = scale {
            if (1..=7).contains(&scale) {
                parts.push(format!("s={scale}"));
            }
        }
        if let Some(width) = width {
            if width <= 7 {
                parts.push(format!("w={width}"));
            }
        }
        if let Some(numerator) = numerator {
            if numerator <= 15 {
                parts.push(format!("n={numerator}"));
            }
        }
        if let Some(denominator) = denominator {
            let num = numerator.unwrap_or(0);
            if denominator <= 15 && (denominator == 0 || denominator > num) {
                parts.push(format!("d={denominator}"));
            }
        }
        if let Some(vertical_alignment) = vertical_alignment {
            parts.push(format!("v={}", vertical_alignment.value()));
        }
        if let Some(horizontal_alignment) = horizontal_alignment {
            parts.push(format!("h={}", horizontal_alignment.value()));
        }

        parts.join(":")
    }

    /// Compute terminal footprint for an encoded payload according to protocol semantics.
    ///
    /// Height is always `s` (default 1). Width is:
    /// - `s * w` when `w > 0`
    /// - `s * measured_text_cells` when `w == 0` or absent
    pub fn measure_rect(&self, text: &str, params: &EncodeParams) -> CellRect {
        let s = params.scale.unwrap_or(1) as usize;
        let w = params.width.unwrap_or(0) as usize;

        let width_cells = if w > 0 {
            s * w
        } else {
            s * self.measure_cells(text)
        };

        CellRect {
            width_cells,
            height_cells: s,
        }
    }

    pub fn encode_with_rect(&self, text: &str, params: EncodeParams) -> (String, CellRect) {
        let rect = self.measure_rect(text, &params);
        let encoded = self.encode(text, params);
        (encoded, rect)
    }

    pub fn encode_with_rect_checked(
        &self,
        text: &str,
        params: EncodeParams,
    ) -> Result<(String, CellRect), ProtocolError> {
        params.validate()?;
        if text.len() > Self::MAX_TEXT_LENGTH {
            return Err(ProtocolError::TextTooLong {
                len: text.len(),
                max: Self::MAX_TEXT_LENGTH,
            });
        }
        let rect = self.measure_rect(text, &params);
        let encoded = self.encode(text, params);
        Ok((encoded, rect))
    }

    /// Measure terminal display width in cells.
    ///
    /// Uses grapheme clusters and Unicode width to better match terminal behavior.
    pub fn measure_cells(&self, text: &str) -> usize {
        UnicodeSegmentation::graphemes(text, true)
            .map(UnicodeWidthStr::width)
            .sum()
    }

    /// Per-grapheme measurement details.
    pub fn measure_graphemes(&self, text: &str) -> Vec<GraphemeMeasure> {
        UnicodeSegmentation::graphemes(text, true)
            .map(|g| GraphemeMeasure {
                grapheme: g.to_string(),
                width: UnicodeWidthStr::width(g),
            })
            .collect()
    }

    /// Split text into chunks whose measured width is <= `max_cells`.
    ///
    /// This is robust for non-ASCII content because splitting is by grapheme.
    pub fn split_by_cells(&self, text: &str, max_cells: usize) -> Vec<String> {
        if max_cells == 0 {
            return Vec::new();
        }

        let mut chunks: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut current_w = 0usize;

        for g in UnicodeSegmentation::graphemes(text, true) {
            let gw = UnicodeWidthStr::width(g);

            if current_w + gw <= max_cells {
                current.push_str(g);
                current_w += gw;
                continue;
            }

            if !current.is_empty() {
                chunks.push(current);
                current = String::new();
            }

            // Handle over-wide single grapheme (e.g. max_cells=1, grapheme width=2):
            // keep it as its own chunk so callers can decide w policy.
            current.push_str(g);
            current_w = gw;
        }

        if !current.is_empty() {
            chunks.push(current);
        }

        chunks
    }

    /// Backward-compatible alias.
    pub fn estimate_width(&self, text: &str) -> usize {
        self.measure_cells(text)
    }

    /// Backward-compatible alias.
    pub fn chunk_by_width(&self, text: &str, width_in_cells: usize) -> Vec<String> {
        self.split_by_cells(text, width_in_cells)
    }
}

pub fn sized(t: &str) -> KittyTextSizingBuilder {
    KittyTextSizingBuilder::new(t)
}
