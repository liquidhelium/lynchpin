//! Compose in-flow and out-of-flow content into pages.
//!
//! Mirrors `lynchpin-layout/src/flow/compose.rs`.
//!
//! Manages floats, footnotes, and page composition across regions.

use typst::diag::SourceResult;
use typst::engine::Engine;

use lynchpin_library::frame::{Row, TermFrame, TermSize};
use lynchpin_library::regions::TermRegions;

use crate::config::TermConfig;
use super::collect::{self, Child};
use super::distribute;

/// Tracks remaining work across regions.
pub struct Work<'a> {
    /// Children yet to be processed.
    pub children: Vec<Child<'a>>,
    /// Leftover from a breakable block.
    pub spill: Option<Box<Child<'a>>>,
    /// Floats that didn't fit in previous regions.
    pub floats: Vec<Child<'a>>,
}

impl<'a> Work<'a> {
    pub fn new(children: Vec<Child<'a>>) -> Self {
        Self {
            children,
            spill: None,
            floats: Vec::new(),
        }
    }

    pub fn done(&self) -> bool {
        self.children.is_empty() && self.spill.is_none()
    }

    pub fn advance(&mut self) {
        if !self.children.is_empty() {
            self.children.remove(0);
        }
    }
}

/// Compose a single page/region.
pub fn compose(
    engine: &mut Engine,
    work: &mut Work<'_>,
    config: &TermConfig,
    mut regions: TermRegions,
) -> SourceResult<TermFrame> {
    // Process floats first, then in-flow content.
    let mut all_children: Vec<Child> = Vec::new();

    // Add floats at the beginning.
    all_children.extend(work.floats.drain(..));

    // Add remaining children and spill.
    if let Some(spill) = work.spill.take() {
        all_children.push(*spill);
    }
    all_children.append(&mut work.children);

    let frames = distribute::distribute(engine, all_children, config, regions)?;

    // For terminal, just return first frame (or stack multiple).
    if frames.is_empty() {
        Ok(TermFrame::new(TermSize::ZERO))
    } else if frames.len() == 1 {
        Ok(frames.into_iter().next().unwrap())
    } else {
        // Stack multiple frames vertically.
        Ok(crate::stack::compose_vertical(frames, 1, 0))
    }
}

/// Handle a float (placed element that floats to top/bottom).
pub fn handle_float(
    engine: &mut Engine,
    child: &collect::PlacedChild<'_>,
    config: &TermConfig,
    _regions: &TermRegions,
) -> SourceResult<TermFrame> {
    child.layout(engine, config)
}

/// Handle a footnote.
pub fn handle_footnote(
    _engine: &mut Engine,
    _entry: &typst::foundations::Content,
    _config: &TermConfig,
) -> SourceResult<Option<TermFrame>> {
    // Terminal: footnotes are rendered inline as [^N] markers.
    // Full footnote rendering would require page-bottom composition.
    Ok(None)
}
