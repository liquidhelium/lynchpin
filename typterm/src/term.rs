pub mod style {

    use crossterm::style::*;
    #[derive(Debug, Clone, Default)]
    pub struct TermStyle {
        style: ContentStyle,
        pub sizing: Option<crate::size_protocol::EncodeParams>,
    }

    impl AsRef<ContentStyle> for TermStyle {
        fn as_ref(&self) -> &ContentStyle {
            &self.style
        }
    }
}
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
    pub fn text(text: impl Into<EcoString>, span: Span) -> TermElement {
        TermElement::Text(TermText {
            text:text.into(),
            span,
            style: Default::default(),
        })
    }
}
