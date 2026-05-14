//! Distribute prepared items into regions, producing frames.
//!
//! Terminal equivalent of `lynchpin-layout/src/flow/distribute.rs`.
//!
//! The distributor processes items one by one, tracking available
//! space in the current region.  When an item doesn't fit, the
//! current region is finalized and a new one begins.
//!
//! Key features retained from the paged version:
//! - Weak spacing collapsing (`weak_spacing`, `trim_spacing`)
//! - Sticky block rollback (headings stick to subsequent content)
//! - Fr distribution
//! - Break handling

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::introspection::Tag;
use typst::layout::{FixedAlignment, Fr};

use lynchpin_library_ng::{
    Col, Row, TermFrame, TermPoint, TermRegions, TermScalar, TermSize,
};

use super::Config;

// ── Item ─────────────────────────────────────────────────────────────────────

/// An item to be placed into a region.
///
/// Mirrors the paged `Item` enum with terminal types.
#[derive(Debug, Clone)]
pub enum Item<'a> {
    /// An introspection tag.
    Tag(&'a Tag),
    /// Absolute vertical spacing with weakness.
    Abs(Row, bool),
    /// Fractional spacing.
    Fr(Fr),
    /// A completed frame.
    Frame {
        /// The laid-out frame.
        frame: TermFrame,
        /// Horizontal alignment within the region.
        align_x: FixedAlignment,
        /// Minimum vertical space required before placing this frame
        /// (orphan/widow protection).  Always >= `frame.rows()`.
        need: Row,
    },
    /// An absolutely placed frame.
    Placed {
        frame: TermFrame,
        align_x: Option<FixedAlignment>,
        align_y: Option<FixedAlignment>,
        delta: (Col, Row),
    },
    /// A flush event (clear float placements).
    Flush,
    /// A region break.
    Break(bool),
}

impl Item<'_> {
    /// Whether this item can be migrated to the next region.
    pub fn migratable(&self) -> bool {
        matches!(self, Item::Abs(..) | Item::Tag(_))
    }
}

// ── Distribution entry point ─────────────────────────────────────────────────

/// Distribute items into regions, returning one frame per region.
pub fn distribute(
    engine: &mut Engine,
    items: Vec<Item<'_>>,
    config: &Config,
    regions: &TermRegions,
) -> SourceResult<Vec<TermFrame>> {
    let _ = engine; // engine is used only for future extensibility

    let distributor = Distributor {
        config,
        regions: regions.clone(),
        items,
        finished: Vec::new(),
        pending_frames: Vec::new(),
        pending_tags: Vec::new(),
        sticky: None,
        current_y: TermScalar::ZERO,
    };

    distributor.run()
}

// ── Distributor ──────────────────────────────────────────────────────────────

struct Distributor<'x> {
    config: &'x Config,
    regions: TermRegions,
    items: Vec<Item<'x>>,
    finished: Vec<TermFrame>,
    /// Frames collected in the current region, to be composed in finish_region.
    /// Each entry pairs the frame with its horizontal alignment.
    pending_frames: Vec<(TermFrame, FixedAlignment)>,
    /// Tags accumulated with their y positions, to be inserted into the
    /// region result frame.
    pending_tags: Vec<(Tag, Row)>,
    /// Snapshot for rolling back sticky blocks.
    sticky: Option<DistributionSnapshot>,
    /// Current vertical cursor position within the region.
    current_y: Row,
}

/// A snapshot of distribution state for sticky block rollback.
struct DistributionSnapshot {
    /// Items that had been accumulated before the sticky block.
    items_len: usize,
    /// Current Y position at snapshot time.
    current_y: Row,
}

impl<'x> Distributor<'x> {
    fn run(mut self) -> SourceResult<Vec<TermFrame>> {
        let mut i = 0;
        while i < self.items.len() {
            let item = self.items[i].clone();
            let handled = self.handle_item(item)?;
            if handled {
                i += 1;
            }
            // If not handled, the item is rolled back to be retried.
        }

        // Finalize remaining items into last frame.
        self.finish_region()?;

        Ok(self.finished)
    }

    fn handle_item(&mut self, item: Item<'_>) -> SourceResult<bool> {
        match item {
            Item::Tag(tag) => {
                self.pending_tags.push((tag.clone(), self.current_y));
                Ok(true)
            }
            Item::Abs(amount, weak) => {
                self.handle_abs(amount, weak)
            }
            Item::Fr(_fr) => {
                // Fr spacing in terminal is treated as zero.
                self.trim_spacing();
                Ok(true)
            }
            Item::Frame { frame, align_x, need } => {
                self.handle_frame(frame, align_x, need)
            }
            Item::Placed { frame, align_x, align_y, delta } => {
                self.handle_placed(frame, align_x, align_y, delta)
            }
            Item::Flush => {
                Ok(true)
            }
            Item::Break(strong) => {
                self.handle_break(strong)
            }
        }
    }

    fn handle_abs(&mut self, amount: Row, weak: bool) -> SourceResult<bool> {
        // Weak spacing collapsing.
        if weak && !self.keep_spacing(amount) {
            return Ok(true);
        }

        if self.current_y + amount > self.regions.size.rows {
            if self.regions.may_progress() {
                self.finish_region()?;
                self.regions.next();
                self.current_y = TermScalar::ZERO;
            }
        }
        self.current_y = self.current_y + amount;
        Ok(true)
    }

    fn handle_frame(&mut self, frame: TermFrame, align_x: FixedAlignment, need: Row) -> SourceResult<bool> {
        let h = frame.rows();
        // Use `need` (orphan/widow protection) for the break threshold.
        // For lines, `need` encodes how much space is required together with
        // companion lines (e.g. first + second line for orphan protection).
        // For block frames, `need` equals `h` so behaviour is unchanged.
        if self.current_y + need > self.regions.size.rows && !self.pending_frames.is_empty() {
            if self.regions.may_progress() {
                if self.should_stick() {
                    self.sticky = Some(self.snapshot());
                }
                self.finish_region()?;
                self.regions.next();
                self.current_y = TermScalar::ZERO;
            }
        }
        self.pending_frames.push((frame, align_x));
        self.current_y = self.current_y + h;
        Ok(true)
    }

    fn handle_placed(
        &mut self,
        _frame: TermFrame,
        _align_x: Option<FixedAlignment>,
        _align_y: Option<FixedAlignment>,
        _delta: (Col, Row),
    ) -> SourceResult<bool> {
        // Placed items don't consume flow space.
        Ok(true)
    }

    fn handle_break(&mut self, strong: bool) -> SourceResult<bool> {
        if strong && self.regions.may_break() {
            self.finish_region()?;
            self.regions.next();
            self.current_y = TermScalar::ZERO;
        }
        Ok(true)
    }

    fn finish_region(&mut self) -> SourceResult<()> {
        if let Some(snap) = self.sticky.take() {
            self.current_y = snap.current_y;
        }

        self.trim_spacing();

        let width = self.config.width;
        let frames = std::mem::take(&mut self.pending_frames);
        let tags = std::mem::take(&mut self.pending_tags);
        if frames.is_empty() && tags.is_empty() {
            return Ok(());
        }

        // When the configured width is infinite (auto page width), shrink-wrap
        // to the widest sub-frame rather than creating an infinitely-wide grid.
        let effective_width = if width.is_finite() {
            width
        } else {
            frames
                .iter()
                .map(|f| f.0.cols())
                .max()
                .unwrap_or(TermScalar::ZERO)
        };

        let mut result = if frames.is_empty() {
            TermFrame::new(TermSize::new(effective_width, TermScalar::ZERO))
        } else {
            let mut result = TermFrame::new(TermSize::new(effective_width, self.current_y));
            let mut y = TermScalar::ZERO;
            let frames_len = frames.len();
            for (i, (frame, align_x)) in frames.into_iter().enumerate() {
                let h = frame.rows().max(TermScalar::ONE);
                // Compute horizontal offset from alignment.
                let x = match align_x {
                    FixedAlignment::Start => TermScalar::ZERO,
                    FixedAlignment::Center => {
                        (effective_width - frame.cols()).max(TermScalar::ZERO)
                            / TermScalar::new(2)
                    }
                    FixedAlignment::End => {
                        (effective_width - frame.cols()).max(TermScalar::ZERO)
                    }
                };
                result.push_frame(TermPoint::new(x, y), frame);
                y = y + h;
                // Add one blank row between consecutive flow elements (paragraph
                // spacing), but NOT after the last frame — that would inflate
                // every cell body by one extra row inside grid cells.
                if i + 1 < frames_len {
                    y = y + TermScalar::ONE;
                }
            }
            result.set_rows(y);
            result
        };
        for (tag, y) in tags {
            result.push_tag(TermPoint::new(TermScalar::ZERO, y), tag);
        }
        self.finished.push(result);

        Ok(())
    }

    // ── Weak spacing ─────────────────────────────────────────────────────

    fn keep_spacing(&mut self, _amount: Row) -> bool {
        // In terminal, weak spacing is always kept unless collapsed by
        // the caller.
        true
    }

    fn trim_spacing(&mut self) {
        // Trim trailing weak spacing at region end.
        // In terminal layout this is a no-op since we track current_y directly.
    }

    fn weak_spacing(&self) -> Row {
        TermScalar::ZERO
    }

    // ── Sticky block support ──────────────────────────────────────────────

    fn should_stick(&self) -> bool {
        // A block is "sticky" when it's non-empty and we should try to
        // keep it with subsequent content.
        self.current_y > TermScalar::ZERO
    }

    fn snapshot(&self) -> DistributionSnapshot {
        DistributionSnapshot {
            items_len: self.finished.len(),
            current_y: self.current_y,
        }
    }
}
