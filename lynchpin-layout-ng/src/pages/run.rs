//! Layout a page run with uniform properties.
//!
//! Terminal equivalent of `lynchpin-layout/src/pages/run.rs`.
//!
//! In terminal layout, page runs are simplified: no real margins,
//! headers, footers, or page numbering.  The run produces one or
//! more `LayoutedPage` values that are later finalized.

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::StyleChain;
use typst::introspection::Locator;
use typst::layout::Axes;
use typst::routines::Pair;

use lynchpin_library_ng::{
    Col, Row, TermFrame, TermRegions, TermScalar, TermSize,
};

use crate::flow::{FlowMode, layout_flow};

// ── LayoutedPage ─────────────────────────────────────────────────────────────

/// A mostly finished layout for one page.
#[derive(Clone)]
pub struct LayoutedPage {
    /// The inner content frame.
    pub inner: TermFrame,
    /// Optional header frame.
    pub header: Option<TermFrame>,
    /// Optional footer frame.
    pub footer: Option<TermFrame>,
    /// Optional background frame.
    pub background: Option<TermFrame>,
    /// Optional foreground frame.
    pub foreground: Option<TermFrame>,
}

// ── Blank page ───────────────────────────────────────────────────────────────

/// Layout a single blank page suitable for parity adjustment.
pub fn layout_blank_page(
    engine: &mut Engine,
    locator: Locator<'_>,
    initial: StyleChain<'_>,
) -> SourceResult<LayoutedPage> {
    let layouted = layout_page_run(engine, &[], locator, initial)?;
    Ok(layouted.into_iter().next().unwrap_or_else(|| LayoutedPage {
        inner: TermFrame::new(TermSize::ZERO),
        header: None,
        footer: None,
        background: None,
        foreground: None,
    }))
}

// ── Page run ─────────────────────────────────────────────────────────────────

/// Layout a page run with uniform properties.
pub fn layout_page_run(
    engine: &mut Engine,
    children: &[Pair<'_>],
    locator: Locator<'_>,
    initial: StyleChain<'_>,
) -> SourceResult<Vec<LayoutedPage>> {
    // Determine the page dimensions from the style chain or use defaults.
    // In terminal layout, we use a generous width and auto-height.
    let width: Col = TermScalar::new(80);
    let height: Row = TermScalar::INFINITY;
    let size = TermSize::new(width, height);

    // Layout the children using the flow pipeline.
    let fragment = layout_flow(
        engine,
        children,
        &mut locator.split(),
        initial,
        TermRegions::repeat(size, Axes::new(false, false)),
        std::num::NonZeroUsize::new(1).unwrap(),
        TermScalar::ZERO,
        FlowMode::Root,
    )?;

    // Convert each frame into a LayoutedPage.
    let mut layouted: Vec<LayoutedPage> = Vec::with_capacity(fragment.len());
    for inner in fragment {
        layouted.push(LayoutedPage {
            inner,
            header: None,
            footer: None,
            background: None,
            foreground: None,
        });
    }

    Ok(layouted)
}
