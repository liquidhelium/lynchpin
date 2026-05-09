//! Layout of content into a [`TermFrame`] or [`TermFragment`].
//!
//! Terminal equivalent of `lynchpin-layout/src/flow/mod.rs`.
//!
//! The flow pipeline is: **collect → compose → distribute → finalize**.
//! This module orchestrates the pipeline and provides the shared `Work`,
//! `Config`, and `FlowMode` types.

mod block;
mod collect;
mod compose;
mod distribute;

pub(crate) use self::block::unbreakable_pod;

use std::num::NonZeroUsize;
use std::rc::Rc;

use rustc_hash::FxHashSet;

use typst::diag::{SourceDiagnostic, SourceResult};
use typst::engine::Engine;
use typst::foundations::StyleChain;
use typst::introspection::{Locator, SplitLocator, Tag};
use typst::layout::Axes;
use typst::model::FootnoteElem;
use typst::routines::Pair;

use lynchpin_library_ng::{
    Col, TermFrame, TermFragment, TermRegion, TermRegions, TermScalar, TermSize,
};

use self::collect::Child;
use self::collect::MultiSpill;
use self::collect::PlacedChild;
use self::collect::collect;
use self::compose::compose;

// ── Entry points ─────────────────────────────────────────────────────────────

/// Lays out content into a single region, producing a single frame.
pub fn layout_term_frame(
    engine: &mut Engine,
    children: &[Pair<'_>],
    locator: Locator<'_>,
    styles: StyleChain<'_>,
    region: TermRegion,
) -> SourceResult<TermFrame> {
    let fragment = layout_term_fragment(engine, children, locator, styles, region.into())?;
    Ok(fragment.into_iter().next().unwrap_or_else(|| TermFrame::new(TermSize::ZERO)))
}

/// Lays out content into multiple regions.
///
/// When laying out into just one region, prefer [`layout_term_frame`].
pub fn layout_term_fragment(
    engine: &mut Engine,
    children: &[Pair<'_>],
    locator: Locator<'_>,
    styles: StyleChain<'_>,
    regions: TermRegions,
) -> SourceResult<TermFragment> {
    layout_flow(
        engine,
        children,
        &mut locator.split(),
        styles,
        regions,
        NonZeroUsize::new(1).unwrap(),
        TermScalar::ZERO,
        FlowMode::Root,
    )
}

// ── FlowMode ─────────────────────────────────────────────────────────────────

/// The mode a flow can be laid out in.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum FlowMode {
    /// A root flow with block-level elements.
    Root,
    /// A flow whose children are block-level elements.
    Block,
    /// A flow whose children are inline-level elements.
    Inline,
}

// ── Core flow layout ─────────────────────────────────────────────────────────

/// Lays out realized content into regions, potentially with columns.
#[allow(clippy::too_many_arguments)]
pub fn layout_flow<'a>(
    engine: &mut Engine,
    children: &[Pair<'a>],
    locator: &mut SplitLocator<'a>,
    _shared: StyleChain<'a>,
    mut regions: TermRegions,
    columns: NonZeroUsize,
    column_gutter: TermScalar,
    mode: FlowMode,
) -> SourceResult<TermFragment> {
    let _ = (column_gutter, columns, mode); // kept for API compatibility

    // Prepare configuration that is shared across the whole flow.
    let config = Config {
        width: regions.size.cols,
        expand: regions.expand,
    };

    // Collect the elements into pre-processed children.
    let children = collect(
        engine,
        children,
        locator.next(&()),
        regions.base(),
        regions.expand.x,
    )?;

    let mut work = Work::new(&children);
    let mut finished: TermFragment = vec![];

    // This loop runs once per region produced by the flow layout.
    loop {
        let frame = compose(engine, &mut work, &config, locator.next(&()), &regions)?;
        finished.push(frame);

        // Terminate the loop when everything is processed.
        if work.done() && (!regions.expand.y || regions.backlog.is_empty()) {
            break;
        }

        regions.next();
    }

    Ok(finished)
}

// ── Work ─────────────────────────────────────────────────────────────────────

/// The work that is left to do by flow layout.
///
/// The lifetimes 'a and 'b are used across flow layout:
/// - 'a is that of the content coming out of realization
/// - 'b is that of the collected/prepared children
#[derive(Clone)]
struct Work<'a, 'b> {
    /// Children that we haven't processed yet.
    children: &'b [Child<'a>],
    /// Leftovers from a breakable block.
    spill: Option<MultiSpill<'a, 'b>>,
    /// Queued floats that didn't fit in previous regions.
    floats: Vec<&'b PlacedChild<'a>>,
    /// Queued footnotes that didn't fit in previous regions.
    footnotes: Vec<FootnoteElem>,
    /// Spilled frames of a footnote that didn't fully fit.
    footnote_spill: Option<std::vec::IntoIter<TermFrame>>,
    /// Queued tags that will be attached to the next frame.
    tags: Vec<&'a Tag>,
    /// Identifies floats and footnotes that can be skipped.
    skips: Rc<FxHashSet<typst::introspection::Location>>,
}

impl<'a, 'b> Work<'a, 'b> {
    /// Create the initial work state from a list of children.
    fn new(children: &'b [Child<'a>]) -> Self {
        Self {
            children,
            spill: None,
            floats: Vec::new(),
            footnotes: Vec::new(),
            footnote_spill: None,
            tags: Vec::new(),
            skips: Rc::new(FxHashSet::default()),
        }
    }

    /// Get the first unprocessed child, from the start of the slice.
    fn head(&self) -> Option<&'b Child<'a>> {
        self.children.first()
    }

    /// Mark the `head()` child as processed, advancing the slice by one.
    fn advance(&mut self) {
        self.children = &self.children[1..];
    }

    /// Whether all work is done.
    fn done(&self) -> bool {
        self.children.is_empty()
            && self.spill.is_none()
            && self.floats.is_empty()
            && self.footnote_spill.is_none()
            && self.footnotes.is_empty()
    }

    /// Add skipped floats and footnotes to the skip set.
    fn extend_skips(&mut self, skips: &[typst::introspection::Location]) {
        if !skips.is_empty() {
            Rc::make_mut(&mut self.skips).extend(skips.iter().copied());
        }
    }
}

// ── Config ───────────────────────────────────────────────────────────────────

/// Shared configuration for the whole flow.
struct Config {
    /// Available width in columns.
    width: Col,
    /// Whether the region can expand.
    expand: Axes<bool>,
}

// ── FlowResult / Stop ────────────────────────────────────────────────────────

/// The result type for flow layout.
type FlowResult<T> = Result<T, Stop>;

/// A control flow event during flow layout.
enum Stop {
    /// Indicates that the current subregion should be finished.
    Finish(bool),
    /// A fatal error.
    Error(Vec<SourceDiagnostic>),
}

impl From<Vec<SourceDiagnostic>> for Stop {
    fn from(error: Vec<SourceDiagnostic>) -> Self {
        Stop::Error(error)
    }
}
