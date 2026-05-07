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

use typst::{introspection::Introspector, model::DocumentInfo};

use lynchpin_term_layout::config::TermConfig;
use lynchpin_term_layout::flow::TermPage;

/// A compiled terminal document ready for display.
#[derive(Debug, Clone)]
pub struct TermDocument {
    /// One page of terminal output (terminal rendering always produces exactly
    /// one page; the field is `Vec` to leave room for future multi-page support).
    pub pages: Vec<TermPage>,
    pub info: DocumentInfo,
    pub introspector: Introspector,
    pub config: TermConfig,
}

impl fmt::Display for TermDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, page) in self.pages.iter().enumerate() {
            if i > 0 {
                // Blank separator line between pages.
                writeln!(f)?;
            }
            let grid = page.frame.render();
            write!(f, "{}", grid.to_ansi())?;
        }
        Ok(())
    }
}
