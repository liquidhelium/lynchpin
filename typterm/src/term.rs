pub mod style {

    use crossterm::style::*;
    #[derive(Debug, Clone, Default)]
    pub struct TermStyle {
        pub style: ContentStyle,
        pub sizing: Option<crate::size_protocol::EncodeParams>,
    }

    impl AsRef<ContentStyle> for TermStyle {
        fn as_ref(&self) -> &ContentStyle {
            &self.style
        }
    }
}
use std::fmt;

use crossterm::style::{ContentStyle, StyledContent};
use typst::{
    ecow::{EcoString, EcoVec},
    introspection::Introspector,
    layout::Frame,
    model::DocumentInfo,
    syntax::Span,
};

use crate::{size_protocol::EncodeParams, term::style::TermStyle};

#[derive(Debug, Clone)]
pub struct TermDocument {
    pub flow: EcoVec<TermElement>,
    pub info: DocumentInfo,
    pub introspector: Introspector,
}

impl fmt::Display for TermDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for element in &self.flow {
            match element {
                TermElement::Text(text) => {
                    let raw = text.text.as_str();
                    // Apply Kitty Text Sizing when sizing is set and text has no newlines.
                    // Newlines are always emitted raw to avoid corrupting line advancement.
                    let display: String = if let Some(ref sizing) = text.style.sizing {
                        crate::size_protocol::KittyTextSizingEncoder::new()
                            .encode(raw, sizing.clone())
                    } else {
                        raw.to_owned()
                    };
                    let styled = StyledContent::new(text.style.style, display.as_str());
                    write!(f, "{styled}")?;
                }
                // Frames are skipped as per design
                TermElement::Frame(_) => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum TermElement {
    Text(TermText),
    Frame(TermFrame),
}

#[derive(Debug, Clone)]
pub struct TermFrame {
    pub frame: Frame,
    pub span: Span,
}
#[derive(Debug, Clone)]
pub struct TermText {
    pub text: EcoString,
    pub span: Span,
    pub style: TermStyle,
}

impl TermElement {
    /// Create a plain text element with no styling.
    pub fn text(text: impl Into<EcoString>, span: Span) -> TermElement {
        TermElement::Text(TermText {
            text: text.into(),
            span,
            style: Default::default(),
        })
    }

    /// Create a styled text element.
    pub fn text_with_style(
        text: impl Into<EcoString>,
        span: Span,
        style: ContentStyle,
    ) -> TermElement {
        TermElement::Text(TermText {
            text: text.into(),
            span,
            style: TermStyle {
                style,
                sizing: None,
            },
        })
    }
    /// Create a styled text element.
    pub fn text_with_style_and_size(
        text: impl Into<EcoString>,
        span: Span,
        style: ContentStyle,
        sizing: EncodeParams,
    ) -> TermElement {
        TermElement::Text(TermText {
            text: text.into(),
            span,
            style: TermStyle {
                style,
                sizing: Some(sizing),
            },
        })
    }
}
