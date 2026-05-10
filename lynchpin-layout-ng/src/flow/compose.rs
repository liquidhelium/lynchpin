//! Compose in-flow and out-of-flow content into regions.
//!
//! Terminal equivalent of `lynchpin-layout/src/flow/compose.rs`.
//!
//! The composer processes children from the `Work` queue, handling
//! floats, footnotes, and page/column content assembly.  In terminal
//! layout this is simplified: no multi-column, no line numbers, and
//! footnotes are rendered inline.

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::introspection::Location;

use lynchpin_library_ng::{
    Row, TermConfig, TermFrame, TermFragment, TermRegion, TermRegions,
};

use super::collect::Child;
use super::distribute;
use super::{Config, Work};

// ── Compose entry point ──────────────────────────────────────────────────────

/// Compose a single region from the work queue.
pub fn compose(
    engine: &mut Engine,
    work: &mut Work<'_, '_>,
    config: &Config,
    locator: typst::introspection::Locator<'_>,
    regions: &TermRegions,
) -> SourceResult<TermFragment> {
    let _ = locator; // kept for API compatibility

    let mut composer = Composer {
        engine,
        work,
        config,
        regions: regions.clone(),
        insertions: Insertions::default(),
    };

    composer.run()
}

// ── Composer ─────────────────────────────────────────────────────────────────

struct Composer<'a, 'b, 'x, 'y> {
    engine: &'x mut Engine<'y>,
    work: &'x mut Work<'a, 'b>,
    config: &'x Config,
    regions: TermRegions,
    insertions: Insertions<'a, 'b>,
}

impl<'a, 'b> Composer<'a, 'b, '_, '_> {
    fn run(&mut self) -> SourceResult<TermFragment> {
        let mut items: Vec<super::distribute::Item> = Vec::new();
        self.handle_floats(&mut items)?;
        self.process_children(&mut items)?;
        distribute::distribute(self.engine, items, self.config, &self.regions)
    }



    fn handle_floats(
        &mut self,
        items: &mut Vec<super::distribute::Item<'a>>,
    ) -> SourceResult<()> {
        // Process floats from previous regions.
        let floats: Vec<_> = self.work.floats.drain(..).collect();
        for placed in floats {
            if self.work.skips.contains(&placed.location()) {
                continue;
            }
            let frame = placed.layout(self.engine, &TermConfig::default())?;
            items.push(distribute::Item::Placed {
                frame,
                align_x: placed.align_x,
                align_y: placed.align_y,
                delta: placed.delta,
            });
        }
        Ok(())
    }

    fn process_children(
        &mut self,
        items: &mut Vec<super::distribute::Item<'a>>,
    ) -> SourceResult<()> {
        // Process children from the work queue.
        // In terminal layout, we handle the spill first, then remaining children.
        let region = TermRegion::new(self.regions.size, self.regions.expand);
        if let Some(spill) = self.work.spill.take() {
            let frames = spill.layout(self.engine, region)?;
            for frame in frames {
                if !frame.size().is_empty() {
                    items.push(distribute::Item::Frame(frame));
                }
            }
        }

        while let Some(child) = self.work.head().cloned() {
            match child {
                Child::Tag(tag) => {
                    items.push(distribute::Item::Tag(tag));
                }
                Child::Rel(amount, weak) => {
                    items.push(distribute::Item::Abs(amount, weak));
                }
                Child::Fr(fr) => {
                    items.push(distribute::Item::Fr(fr));
                }
                Child::Line(line) => {
                    if !line.frame.size().is_empty() {
                        items.push(distribute::Item::Frame(line.frame.clone()));
                    }
                }
                Child::Single(single) => {
                    let frame = single.layout(self.engine, region)?;
                    if !frame.size().is_empty() {
                        items.push(distribute::Item::Frame(frame));
                    }
                }
                Child::Multi(multi) => {
                    let frame = multi.layout(self.engine, region)?;
                    if !frame.size().is_empty() {
                        items.push(distribute::Item::Frame(frame));
                    }
                }
                Child::Placed(placed) => {
                    let frame = placed.layout(self.engine, &TermConfig::default())?;
                    items.push(distribute::Item::Placed {
                        frame,
                        align_x: placed.align_x,
                        align_y: placed.align_y,
                        delta: placed.delta,
                    });
                }
                Child::Flush => {
                    items.push(distribute::Item::Flush);
                }
                Child::Break(strong) => {
                    items.push(distribute::Item::Break(strong));
                }
            }
            self.work.advance();
        }

        Ok(())
    }
}

// ── Insertions (floats and footnotes) ────────────────────────────────────────

/// Tracks floats and footnotes built up during column/page composition.
#[derive(Default)]
struct Insertions<'a, 'b> {
    /// Top-of-region floats.
    top_floats: Vec<TermFrame>,
    /// Bottom-of-region floats.
    bottom_floats: Vec<TermFrame>,
    /// Footnotes for this region.
    footnotes: Vec<TermFrame>,
    /// Footnote separator frame.
    footnote_separator: Option<TermFrame>,
    /// Total height consumed by top insertions.
    top_size: Row,
    /// Total height consumed by bottom insertions.
    bottom_size: Row,
    /// Available width for insertions.
    width: lynchpin_library_ng::Col,
    /// Locations to skip.
    skips: Vec<Location>,
    _phantom: std::marker::PhantomData<(&'a (), &'b ())>,
}

impl<'a, 'b> Insertions<'a, 'b> {
    fn push_float(&mut self, frame: TermFrame, _top: bool) {
        if _top {
            self.top_size = self.top_size + frame.rows();
            self.top_floats.push(frame);
        } else {
            self.bottom_size = self.bottom_size + frame.rows();
            self.bottom_floats.push(frame);
        }
    }

    fn push_footnote(&mut self, frame: TermFrame) {
        self.footnotes.push(frame);
    }

    fn push_footnote_separator(&mut self, frame: TermFrame) {
        self.footnote_separator = Some(frame);
    }

    fn height(&self) -> Row {
        self.top_size + self.bottom_size
    }

    fn finalize(&mut self, base: &mut TermFrame) {
        // Stamp top floats.
        for float in self.top_floats.drain(..) {
            base.push_frame(lynchpin_library_ng::TermPoint::ZERO, float);
        }

        // Stamp bottom floats above footnotes.
        let footnotes_start = base.rows() - self.bottom_size;
        for float in self.bottom_floats.drain(..) {
            base.push_frame(
                lynchpin_library_ng::TermPoint::new(
                    lynchpin_library_ng::TermScalar::ZERO,
                    footnotes_start,
                ),
                float,
            );
        }
    }
}
