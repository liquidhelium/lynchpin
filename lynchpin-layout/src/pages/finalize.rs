//! Piece together the inner page frame and marginals.
//!
//! Terminal equivalent of `lynchpin-layout/src/pages/finalize.rs`.
//!
//! In terminal layout, pages have no margins, headers, or footers.
//! The finalize step simply stamps any remaining tags onto the
//! frame and returns the `TermPage`.

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::introspection::Tag;

use ecow::EcoString;
use crossterm::style::ContentStyle;
use lynchpin_library::{TermPoint, TermScalar};

use super::run::LayoutedPage;
use super::TermPage;

// ── Finalize ─────────────────────────────────────────────────────────────────

/// Finalize a page: assemble inner frame, marginals, and tags.
pub fn finalize(
    engine: &mut Engine,
    tags: &mut Vec<Tag>,
    LayoutedPage {
        inner,
        header,
        footer,
        background,
        foreground,
        ..
    }: LayoutedPage,
) -> SourceResult<TermPage> {
    let _ = engine; // engine used for future counter support

    // Create the full page frame starting from the inner content.
    let mut frame = inner;

    // Add tags.
    for tag in tags.drain(..) {
        frame.push_text(
            TermPoint::ZERO,
            EcoString::new(),
            ContentStyle::default(),
        );
        let _ = tag; // Tags are tracked but not rendered in terminal.
    }

    // Add marginals (simplified: just stack on top or bottom).
    if let Some(background) = background {
        frame.push_frame(TermPoint::ZERO, background);
    }
    if let Some(header) = header {
        frame.push_frame(TermPoint::new(TermScalar::ZERO, frame.rows()), header);
    }
    if let Some(footer) = footer {
        frame.push_frame(TermPoint::new(TermScalar::ZERO, frame.rows()), footer);
    }
    if let Some(foreground) = foreground {
        frame.push_frame(TermPoint::ZERO, foreground);
    }

    Ok(TermPage {
        inner: frame,
        number: 0,
    })
}
