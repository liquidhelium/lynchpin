//! Distribute prepared [`Child`]ren into regions, producing [`Item`]s.
//!
//! Mirrors `lynchpin-layout/src/flow/distribute.rs`.

use typst::diag::SourceResult;
use typst::engine::Engine;

use lynchpin_library::frame::Row;
use lynchpin_library::regions::TermRegions;

use crate::config::TermConfig;
use super::collect;
use super::Item;

/// A snapshot of distribution state for sticky block rollback.
struct Snapshot {
    items_len: usize,
    region_rows: Row,
}

/// Distribute children into regions, returning one frame per region.
pub fn distribute(
    engine: &mut Engine,
    children: Vec<collect::Child<'_>>,
    config: &TermConfig,
    mut regions: TermRegions,
) -> SourceResult<Vec<super::TermFrame>> {
    let mut distributor = Distributor {
        engine,
        config,
        regions: &mut regions,
        items: Vec::new(),
        finished: Vec::new(),
        sticky: None,
    };

    for child in children {
        distributor.child(child)?;
        // After each frame, update sticky snapshot.
        if distributor.should_stick() {
            distributor.sticky = Some(distributor.snapshot());
        }
    }

    // Finalize remaining items.
    distributor.finish_region()?;

    Ok(distributor.finished)
}

struct Distributor<'x, 'y> {
    engine: &'x mut Engine<'y>,
    config: &'x TermConfig,
    regions: &'x mut TermRegions,
    items: Vec<Item>,
    finished: Vec<super::TermFrame>,
    /// Snapshot for rolling back sticky blocks.
    sticky: Option<Snapshot>,
}

impl Distributor<'_, '_> {
    fn child(&mut self, child: collect::Child<'_>) -> SourceResult<()> {
        match child {
            collect::Child::Rel(r, weak) => self.rel(r, weak),
            collect::Child::Fr(fr) => self.fr(fr),
            collect::Child::Line(line) => self.line(line)?,
            collect::Child::Single(single) => self.single(single)?,
            collect::Child::Multi(multi) => self.multi(multi)?,
            collect::Child::Placed(placed) => self.placed(placed)?,
            collect::Child::Break(_) => self.break_(),
            collect::Child::Flush => {}
            collect::Child::Tag(_) => {}
        }
        Ok(())
    }

    fn rel(&mut self, amount: lynchpin_library::frame::Row, weak: bool) {
        // Implement weak spacing collapsing (mirrors paged keep_spacing/trim_spacing).
        if weak && !self.keep_spacing(amount) {
            return;
        }
        self.regions.size.rows -= amount;
        self.items.push(Item::Abs(amount, weak));
    }

    fn fr(&mut self, fr: typst::layout::Fr) {
        self.trim_spacing();
        self.items.push(Item::Fr(fr));
    }

    fn line(&mut self, line: collect::LineChild) -> SourceResult<()> {
        let h = line.frame.rows();
        // If the line doesn't fit and we can advance, finish region.
        if self.regions.size.rows < h && self.regions.may_progress() {
            self.finish_region()?;
        }
        self.regions.size.rows -= h;
        if !line.frame.size().is_empty() {
            self.items.push(Item::Frame(line.frame));
        }
        Ok(())
    }

    fn single(&mut self, single: collect::SingleChild<'_>) -> SourceResult<()> {
        let frame = single.layout(self.engine, self.config)?;
        let h = frame.rows();
        if self.regions.size.rows < h && self.regions.may_progress() {
            self.finish_region()?;
        }
        self.regions.size.rows -= h;
        if !frame.size().is_empty() {
            self.items.push(Item::Frame(frame));
        }
        Ok(())
    }

    fn multi(&mut self, multi: collect::MultiChild<'_>) -> SourceResult<()> {
        let frame = multi.layout(self.engine, self.config)?;
        let h = frame.rows();
        if self.regions.size.rows < h && self.regions.may_progress() {
            self.finish_region()?;
        }
        self.regions.size.rows -= h;
        if !frame.size().is_empty() {
            self.items.push(Item::Frame(frame));
        }
        Ok(())
    }

    fn placed(&mut self, placed: collect::PlacedChild<'_>) -> SourceResult<()> {
        let frame = placed.layout(self.engine, self.config)?;
        // Placed items don't consume vertical space.
        self.items.push(Item::Placed {
            frame,
            align_x: placed.align_x,
            align_y: placed.align_y,
            delta: (0, 0),
        });
        Ok(())
    }

    fn break_(&mut self) {
        // Column break: finish current region if we can advance.
        if self.regions.may_break() {
            if let Ok(()) = self.finish_region() {
                self.regions.next();
            }
        }
    }

    fn finish_region(&mut self) -> SourceResult<()> {
        if self.items.is_empty() {
            return Ok(());
        }
        // If all items are migratable, restore to move everything to next region.
        if self.items.iter().all(Item::migratable) && self.regions.may_progress() {
            return Ok(());
        }
        // If we have a sticky snapshot, roll back to move sticky suffix.
        if let Some(snap) = self.sticky.take() {
            self.restore(snap);
        }
        self.trim_spacing();
        let frame = super::finalize(std::mem::take(&mut self.items), self.config)?;
        self.finished.push(frame);
        Ok(())
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            items_len: self.items.len(),
            region_rows: self.regions.size.rows,
        }
    }

    fn restore(&mut self, snap: Snapshot) {
        self.items.truncate(snap.items_len);
        self.regions.size.rows = snap.region_rows;
    }

    fn should_stick(&self) -> bool {
        // A frame is "sticky" if the last item is a frame (not spacing).
        // Headings etc. should stick to subsequent content.
        self.items.last().is_some_and(|i| matches!(i, Item::Frame(_)))
    }

    // ── Weak spacing (mirrors paged) ────────────────────────────────────

    fn keep_spacing(&mut self, amount: lynchpin_library::frame::Row) -> bool {
        for item in self.items.iter_mut().rev() {
            match item {
                Item::Abs(prev_amount, _prev_weak @ true) => {
                    // Collapse: keep the larger spacing.
                    if amount <= *prev_amount {
                        // New spacing is weaker or equal, skip.
                    } else {
                        self.regions.size.rows -= amount - *prev_amount;
                        *item = Item::Abs(amount, true);
                    }
                    return false;
                }
                Item::Frame(_) | Item::Fr(_) => return true,
                _ => {}
            }
        }
        false
    }

    fn trim_spacing(&mut self) {
        for (i, item) in self.items.iter().enumerate().rev() {
            match item {
                Item::Abs(amount, true) => {
                    self.regions.size.rows += amount;
                    self.items.remove(i);
                    break;
                }
                Item::Frame(_) | Item::Fr(_) => break,
                _ => {}
            }
        }
    }

    fn weak_spacing(&self) -> lynchpin_library::frame::Row {
        for item in self.items.iter().rev() {
            match item {
                Item::Abs(amount, true) => return *amount,
                Item::Frame(_) | Item::Fr(_) => break,
                _ => {}
            }
        }
        0
    }
}
