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

use lynchpin_library::{
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
        rollback_frames: Vec::new(),
        pending_tags: Vec::new(),
        sticky: None,
        current_y: TermScalar::ZERO,
        trailing_weak: TermScalar::ZERO,
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
    /// Frames that were rolled back from the previous region during sticky
    /// block processing.  They are prepended to the next region's
    /// `pending_frames` before any new frames are added.
    rollback_frames: Vec<(TermFrame, FixedAlignment)>,
    /// Tags accumulated with their y positions, to be inserted into the
    /// region result frame.
    pending_tags: Vec<(Tag, Row)>,
    /// Snapshot for rolling back sticky blocks.
    sticky: Option<DistributionSnapshot>,
    /// Current vertical cursor position within the region.
    current_y: Row,
    /// Cumulative trailing *weak* spacing added after the last real frame.
    /// Reset to zero whenever a real frame (or strong spacing) is placed.
    /// `trim_spacing()` subtracts this from `current_y` so that the region
    /// result frame does not gain empty rows at the bottom.
    trailing_weak: Row,
}

/// A snapshot of distribution state for sticky block rollback.
struct DistributionSnapshot {
    /// Number of pending frames at snapshot time.  Used to truncate
    /// `pending_frames` when rolling back, so sticky frames are moved
    /// to the next region rather than left at the bottom of the current one.
    pending_frames_len: usize,
    /// Current Y position at snapshot time.
    current_y: Row,
    /// Trailing weak spacing amount at snapshot time.
    trailing_weak: Row,
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
                self.trailing_weak = TermScalar::ZERO;
            }
        }
        self.current_y = self.current_y + amount;
        if weak {
            // Accumulate trailing weak spacing so trim_spacing() can remove it.
            self.trailing_weak = self.trailing_weak + amount;
        } else {
            // Strong spacing anchors position; previous trailing weak is consumed.
            self.trailing_weak = TermScalar::ZERO;
        }
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
                // Restore any rolled-back sticky frames into the new region.
                if !self.rollback_frames.is_empty() {
                    let frames = std::mem::take(&mut self.rollback_frames);
                    let frames_height: Row = frames.iter().map(|(f, _)| f.rows()).fold(TermScalar::ZERO, |a, b| a + b);
                    self.current_y = self.current_y + frames_height;
                    // Insert at the front of pending_frames (pending_frames is empty here).
                    debug_assert!(self.pending_frames.is_empty(), "pending_frames should be empty after finish_region");
                    self.pending_frames = frames;
                }
            }
        }
        self.pending_frames.push((frame, align_x));
        self.current_y = self.current_y + h;
        // A real frame anchors position: trailing weak spacing before it
        // is consumed (it was the gap between the previous frame and this one).
        self.trailing_weak = TermScalar::ZERO;
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
        // When `strong` is false this is a *weak* break: it is intentionally a
        // no-op here.  Weak breaks only force a region change when the layout
        // engine itself decides the current region is full.  Forcing the break
        // unconditionally would produce spurious empty leading regions for
        // documents that begin with a weak break (e.g. the implicit break
        // before the first paragraph).
        Ok(true)
    }

    fn finish_region(&mut self) -> SourceResult<()> {
        if let Some(snap) = self.sticky.take() {
            // Roll back pending_frames to the snapshot length so that sticky
            // frames (e.g. headings) are removed from the current region and
            // re-placed at the top of the next one.
            let rolled_back = self.pending_frames.drain(snap.pending_frames_len..).collect::<Vec<_>>();
            // Prepend rolled-back frames so the next region picks them up first.
            // Any previously pending rollback frames come before the new ones.
            let mut new_rollback = std::mem::take(&mut self.rollback_frames);
            new_rollback.extend(rolled_back);
            self.rollback_frames = new_rollback;
            self.current_y = snap.current_y;
            self.trailing_weak = snap.trailing_weak;
        }

        self.trim_spacing();

        let width = self.config.width;
        let frames = std::mem::take(&mut self.pending_frames);
        let tags = std::mem::take(&mut self.pending_tags);
        if frames.is_empty() && tags.is_empty() {
            return Ok(());
        }

        // Determine the effective frame width:
        // - If expand.x is true AND width is finite: use the configured width
        //   (alignment offsets are meaningful — content is positioned within
        //   the full region width).
        // - Otherwise (infinite width OR expand.x = false / measurement mode):
        //   shrink-wrap to the widest sub-frame.  In measurement mode
        //   (expand.x = false, e.g. auto-column sizing) alignment must NOT
        //   inflate the reported content width, so the result frame is sized
        //   to its content and the x offset for every frame becomes zero.
        let effective_width = if self.config.expand.x && width.is_finite() {
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
        // the caller.  Collapsing of consecutive weak spacings is handled
        // by problem #9 (keep_spacing overhaul); for now we always accept.
        true
    }

    fn trim_spacing(&mut self) {
        // Remove trailing weak spacing accumulated since the last real frame.
        // This prevents empty rows at the bottom of a region (or the end of
        // the document) that come purely from weak inter-block gaps.
        if self.trailing_weak > TermScalar::ZERO {
            self.current_y = (self.current_y - self.trailing_weak).max(TermScalar::ZERO);
            self.trailing_weak = TermScalar::ZERO;
        }
    }

    // ── Sticky block support ──────────────────────────────────────────────

    fn should_stick(&self) -> bool {
        // A block is "sticky" when it's non-empty and we should try to
        // keep it with subsequent content.
        self.current_y > TermScalar::ZERO
    }

    fn snapshot(&self) -> DistributionSnapshot {
        DistributionSnapshot {
            pending_frames_len: self.pending_frames.len(),
            current_y: self.current_y,
            trailing_weak: self.trailing_weak,
        }
    }
}
