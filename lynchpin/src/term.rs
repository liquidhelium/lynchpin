pub mod style {
    use crossterm::style::ContentStyle;

    #[derive(Debug, Clone, Default)]
    pub struct TermStyle {
        pub style: ContentStyle,
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

use crate::term::style::TermStyle;

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
                    let styled = StyledContent::new(text.style.style, text.text.as_str());
                    write!(f, "{styled}")?;
                }
                // Frames are skipped in terminal output.
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
            style: TermStyle { style },
        })
    }
}
