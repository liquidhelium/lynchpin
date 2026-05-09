//! Layout of content into a [`TermDocument`].
//!
//! Terminal equivalent of `lynchpin-layout/src/pages/mod.rs`.
//!
//! Terminal rendering has no hard page breaks, but the page structure
//! is retained for future multi-page support.  The entry point is
//! [`layout_term_document`].

mod collect;
mod finalize;
mod run;

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::StyleChain;
use typst::introspection::{SplitLocator, Tag, TagElem};
use typst::routines::Pair;

use ecow::EcoString;
use crossterm::style::ContentStyle;
use lynchpin_library_ng::{TermFrame, TermPoint, TermScalar};

use self::collect::{Item, collect};
use self::finalize::finalize;
use self::run::{layout_blank_page, layout_page_run};

// ── TermDocument ─────────────────────────────────────────────────────────────

/// A terminal document: one or more [`TermPage`]s.
///
/// Terminal equivalent of `PagedDocument`.  In single-page mode this
/// contains exactly one `TermPage`.
pub type TermDocument = Vec<TermPage>;

// ── TermPage ─────────────────────────────────────────────────────────────────

/// A single page of terminal output.
///
/// Wraps a [`TermFrame`] with optional page-level metadata.
#[derive(Debug, Clone)]
pub struct TermPage {
    /// The page's frame.
    pub frame: TermFrame,
    /// Page number (0-based in terminal).
    pub number: usize,
}

// ── Entry point ──────────────────────────────────────────────────────────────

/// Layout content into a terminal document.
///
/// This is the top-level entry point for terminal document layout.
/// It performs root-level realization (via the caller) and then lays
/// out the resulting elements using the flow pipeline.
pub fn layout_term_document<'a>(
    engine: &mut Engine,
    children: &'a mut [Pair<'a>],
    locator: &mut SplitLocator<'a>,
    styles: StyleChain<'a>,
) -> SourceResult<TermDocument> {
    layout_pages(engine, children, locator, styles)
}

// ── Page layout ──────────────────────────────────────────────────────────────

/// Lays out the document's pages.
fn layout_pages<'a>(
    engine: &mut Engine,
    children: &'a mut [Pair<'a>],
    locator: &mut SplitLocator<'a>,
    styles: StyleChain<'a>,
) -> SourceResult<TermDocument> {
    // Slice up the children into logical parts.
    let items = collect(children, locator, styles);

    // Layout the page runs.  In terminal mode, we process sequentially.
    let mut pages: Vec<TermPage> = vec![];
    let mut tags: Vec<Tag> = vec![];

    for item in &items {
        match item {
            Item::Run(children, initial, locator) => {
                let layouted = layout_page_run(engine, children, locator.relayout(), *initial)?;
                for layouted in layouted {
                    let page = finalize(engine, &mut tags, layouted)?;
                    pages.push(page);
                }
            }
            Item::Parity(parity, initial, locator) => {
                if !parity.matches(pages.len()) {
                    continue;
                }
                let layouted = layout_blank_page(engine, locator.relayout(), *initial)?;
                let page = finalize(engine, &mut tags, layouted)?;
                pages.push(page);
            }
            Item::Tags(items) => {
                tags.extend(
                    items
                        .iter()
                        .filter_map(|(c, _)| c.to_packed::<TagElem>())
                        .map(|elem| elem.tag.clone()),
                );
            }
        }
    }

    // Add the remaining tags to the very end of the last page.
    if !tags.is_empty() {
        if let Some(last) = pages.last_mut() {
            let pos = TermPoint::new(TermScalar::ZERO, last.frame.rows());
            for _tag in tags.drain(..) {
                last.frame.push_text(pos, EcoString::new(), ContentStyle::default());
            }
        }
    }

    // Number the pages.
    for (i, page) in pages.iter_mut().enumerate() {
        page.number = i;
    }

    Ok(pages)
}
