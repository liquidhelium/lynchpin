//! Region / container model for terminal layout.
//!
//! Mirrors `typst-library/src/layout/regions.rs` with terminal units.
//! `TermRegion` corresponds to paged `Region`; `TermRegions` to `Regions`.

use typst::layout::Axes;

use crate::frame::{Row, TermSize};
use crate::scalar::TermScalar;

// ── Single region ────────────────────────────────────────────────────────────

/// A single rectangular region to layout into.
///
/// Mirrors paged `Region { size: Size, expand: Axes<bool> }`.
#[derive(Debug, Copy, Clone)]
pub struct TermRegion {
    /// Available space.
    pub size: TermSize,
    /// Whether the region can expand on each axis.
    ///
    /// In terminal layout this is typically `Axes::splat(false)` for
    /// shrink-to-fit, or `Axes::new(true, false)` for fixed-width
    /// auto-height.
    pub expand: Axes<bool>,
}

impl TermRegion {
    pub fn new(size: TermSize, expand: Axes<bool>) -> Self {
        Self { size, expand }
    }

    /// The base size for relative sizing.
    pub fn base(&self) -> TermSize {
        self.size
    }
}

impl From<TermRegion> for TermRegions {
    fn from(r: TermRegion) -> Self {
        TermRegions {
            size: r.size,
            full: r.size.rows,
            expand: r.expand,
            backlog: Vec::new(),
            last: None,
        }
    }
}

// ── Region sequence ──────────────────────────────────────────────────────────

/// A sequence of same-width regions for multi-page layout.
///
/// Mirrors paged `Regions`.
#[derive(Debug, Clone)]
pub struct TermRegions {
    /// Size of the current region.
    pub size: TermSize,
    /// Full height of all regions (used for relative sizing).
    pub full: Row,
    /// Whether regions can expand.
    pub expand: Axes<bool>,
    /// Remaining region heights (after the first).
    pub backlog: Vec<Row>,
    /// Repeatable last region height (`None` = no repeating tail).
    pub last: Option<Row>,
}

impl TermRegions {
    /// Create a single-region setup (no page breaking).
    pub fn one(size: TermSize, expand: Axes<bool>) -> Self {
        TermRegion::new(size, expand).into()
    }

    /// Create repeating regions of the same size.
    pub fn repeat(size: TermSize, expand: Axes<bool>) -> Self {
        Self {
            size,
            full: size.rows,
            expand,
            backlog: Vec::new(),
            last: Some(size.rows),
        }
    }

    /// The base size for relative sizing.
    pub fn base(&self) -> TermSize {
        TermSize::new(self.size.cols, self.full)
    }

    /// Whether the current region is full AND we can progress.
    pub fn is_full(&self) -> bool {
        self.size.rows <= TermScalar::ZERO && self.may_progress()
    }

    /// Whether a region break is permitted.
    pub fn may_break(&self) -> bool {
        !self.backlog.is_empty() || self.last.is_some()
    }

    /// Whether calling `next()` may improve space availability.
    pub fn may_progress(&self) -> bool {
        !self.backlog.is_empty()
            || self.last.is_some_and(|h| self.size.rows != h)
    }

    /// Advance to the next region.
    pub fn next(&mut self) {
        let h = if let Some((&first, tail)) = self.backlog.split_first() {
            self.backlog = tail.to_vec();
            first
        } else if let Some(h) = self.last {
            h
        } else {
            return;
        };
        self.size.rows = h;
        self.full = h;
    }

    /// Map all region sizes through `f`.
    pub fn map<F>(&self, f: F) -> Self
    where
        F: Fn(TermSize) -> TermSize,
    {
        let x = self.size.cols;
        Self {
            size: f(self.size),
            full: f(TermSize::new(x, self.full)).rows,
            expand: self.expand,
            backlog: self
                .backlog
                .iter()
                .map(|&y| f(TermSize::new(x, y)).rows)
                .collect(),
            last: self.last.map(|y| f(TermSize::new(x, y)).rows),
        }
    }

    /// Iterate over the sizes of all regions.
    pub fn iter(&self) -> impl Iterator<Item = TermSize> + '_ {
        let x = self.size.cols;
        std::iter::once(self.size)
            .chain(
                self.backlog
                    .iter()
                    .map(move |&y| TermSize::new(x, y)),
            )
            .chain(self.last.into_iter().map(move |y| TermSize::new(x, y)))
    }
}
