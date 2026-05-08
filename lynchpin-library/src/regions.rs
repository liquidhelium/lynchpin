//! Region / container model for terminal layout.
//!
//! Mirrors `typst-library/src/layout/regions.rs` with terminal units.

use typst::layout::Axes;

use crate::frame::{Row, TermSize};

// ── Single region ────────────────────────────────────────────────────────────

/// A single rectangular region to layout into.
#[derive(Debug, Copy, Clone)]
pub struct TermRegion {
    pub size: TermSize,
    pub expand: Axes<bool>,
}

impl TermRegion {
    pub fn new(size: TermSize, expand: Axes<bool>) -> Self {
        Self { size, expand }
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

// ── Region sequence (page-aware) ─────────────────────────────────────────────

/// A sequence of same-width regions for multi-page layout.
#[derive(Debug, Clone)]
pub struct TermRegions {
    pub size: TermSize,
    pub full: Row,
    pub expand: Axes<bool>,
    pub backlog: Vec<Row>,
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

    /// Whether the current region is full AND we cannot progress further.
    pub fn is_full(&self) -> bool {
        self.size.rows <= 0 && self.may_progress()
    }

    /// Whether a region break is permitted.
    pub fn may_break(&self) -> bool {
        !self.backlog.is_empty() || self.last.is_some()
    }

    /// Whether calling `next()` may improve a situation where there is a
    /// lack of space.
    pub fn may_progress(&self) -> bool {
        !self.backlog.is_empty()
            || self.last.is_some_and(|h| self.size.rows != h)
    }

    /// Advance to the next region if there is any.
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
    pub fn map<F>(&self, mut f: F) -> Self
    where
        F: FnMut(TermSize) -> TermSize,
    {
        let x = self.size.cols;
        Self {
            size: f(self.size),
            full: f(TermSize::new(x, self.full)).rows,
            expand: self.expand,
            backlog: self.backlog.iter().map(|&y| f(TermSize::new(x, y)).rows).collect(),
            last: self.last.map(|y| f(TermSize::new(x, y)).rows),
        }
    }

    /// Iterate over the sizes of all regions.
    pub fn iter(&self) -> impl Iterator<Item = TermSize> + '_ {
        let x = self.size.cols;
        std::iter::once(self.size)
            .chain(self.backlog.iter().map(move |&y| TermSize::new(x, y)))
            .chain(self.last.into_iter().map(move |y| TermSize::new(x, y)))
    }
}
