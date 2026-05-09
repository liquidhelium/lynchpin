//! Layout composition helpers.
//!
//! Functions for composing [`TermFrame`]s horizontally and vertically, with
//! correct baseline alignment and centering.

use crossterm::style::ContentStyle;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, Packed, Resolve, StyleChain};
use typst::layout::{AlignElem, Axes, Axis, Dir, FixedAlignment, Fr, HElem, Spacing, StackChild, StackElem, VElem};

use lynchpin_library::frame::{Col, Row, TermFrame, TermPoint, TermSize};
use lynchpin_library::regions::TermRegions;
use lynchpin_library::units;

use crate::config::TermConfig;
use crate::flow;

pub fn layout_stack(
    elem: &Packed<StackElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let dir = elem.dir.get(styles);
    let axis = dir.axis();
    let spacing = elem.spacing.get(styles);

    // Create a single region using terminal width.
    let size = TermSize::new(config.width.unwrap_or(80) as Col, i32::MAX);
    let expand = Axes::splat(false);
    let regions = TermRegions::one(size, expand);
    let mut layouter = StackLayouter::new(dir, regions, styles);
    let mut deferred = None;

    for child in &elem.children {
        match child {
            StackChild::Spacing(kind) => {
                layouter.layout_spacing(*kind);
                deferred = None;
            }
            StackChild::Block(block) => {
                // Transparent HElem/VElem.
                if axis == Axis::X {
                    if let Some(h) = block.to_packed::<HElem>() {
                        layouter.layout_spacing(h.amount);
                        deferred = None;
                        continue;
                    }
                }
                if axis == Axis::Y {
                    if let Some(v) = block.to_packed::<VElem>() {
                        layouter.layout_spacing(v.amount);
                        deferred = None;
                        continue;
                    }
                }

                if let Some(kind) = deferred {
                    layouter.layout_spacing(kind);
                }
                layouter.layout_block(engine, block, styles, config)?;
                deferred = spacing;
            }
        }
    }

    Ok(layouter.finish())
}

struct StackLayouter<'a> {
    dir: Dir,
    axis: Axis,
    regions: TermRegions,
    styles: StyleChain<'a>,
    initial: TermSize,
    used_main: Row,
    used_cross: Col,
    fr: Fr,
    items: Vec<StackItem>,
}

enum StackItem {
    Absolute(Row),
    Fractional(Fr),
    Frame(TermFrame, Axes<FixedAlignment>),
}

impl<'a> StackLayouter<'a> {
    fn new(dir: Dir, regions: TermRegions, styles: StyleChain<'a>) -> Self {
        let axis = dir.axis();
        let initial = regions.base();
        Self {
            dir,
            axis,
            regions,
            styles,
            initial,
            used_main: 0,
            used_cross: 0,
            fr: Fr::zero(),
            items: Vec::new(),
        }
    }

    fn layout_spacing(&mut self, spacing: Spacing) {
        match spacing {
            Spacing::Rel(v) => {
                let cells = units::rel_to_cols(&v, self.styles) as Row;
                let remaining = match self.axis {
                    Axis::X => &mut self.regions.size.cols,
                    Axis::Y => &mut self.regions.size.rows,
                };
                let limited = cells.min(*remaining);
                *remaining -= limited;
                self.used_main += limited;
                self.items.push(StackItem::Absolute(limited));
            }
            Spacing::Fr(v) => {
                self.fr += v;
                self.items.push(StackItem::Fractional(v));
            }
        }
    }

    fn layout_block(
        &mut self,
        engine: &mut Engine,
        block: &Content,
        styles: StyleChain,
        config: &TermConfig,
    ) -> SourceResult<()> {
        // Resolve alignment.
        let align = block.to_packed::<AlignElem>()
            .map(|a| a.alignment.get(styles))
            .unwrap_or_else(|| styles.get(AlignElem::alignment))
            .resolve(styles);

        let frame = flow::layout_block(engine, block, config, styles)?;
        let sz = frame.size();

        if self.axis == Axis::Y {
            self.regions.size.rows -= sz.rows;
        } else {
            self.regions.size.cols -= sz.cols;
        }
        self.used_main += match self.axis {
            Axis::X => sz.cols,
            Axis::Y => sz.rows,
        };
        self.used_cross = self.used_cross.max(match self.axis {
            Axis::X => sz.rows,
            Axis::Y => sz.cols,
        });
        self.items.push(StackItem::Frame(frame, align));

        Ok(())
    }

    fn finish(self) -> TermFrame {
        let full_main = match self.axis {
            Axis::X => self.initial.cols,
            Axis::Y => self.initial.rows,
        };
        let remaining = (full_main - self.used_main).max(0);
        let total_fr = self.fr;

        // Expand to fill if fr spacings exist.
        let actual_main = if total_fr != Fr::zero() && remaining > 0 {
            full_main
        } else {
            self.used_main
        };

        let total_cols = match self.axis {
            Axis::X => actual_main,
            Axis::Y => self.used_cross.max(1),
        };
        let total_rows = match self.axis {
            Axis::Y => actual_main,
            Axis::X => self.used_cross.max(1),
        };

        let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));
        let mut cursor: Row = 0;

        for item in self.items {
            match item {
                StackItem::Absolute(v) => cursor += v,
                StackItem::Fractional(v) => {
                    if total_fr != Fr::zero() {
                        let share = remaining as f64 * (v.get() as f64 / total_fr.get() as f64);
                        cursor += share.round() as Row;
                    }
                }
                StackItem::Frame(f, align) => {
                    let child_sz = f.size();
                    let used = actual_main - self.used_main;
                    let main_pos = align_main(self.dir, align, self.axis, used) + cursor;
                    let cross_pos = align_cross(align, self.axis, total_cols, total_rows, child_sz.cols, child_sz.rows);
                    let (x, y) = match self.axis {
                        Axis::X => (main_pos, cross_pos),
                        Axis::Y => (cross_pos, main_pos),
                    };
                    frame.push_frame(TermPoint::new(x, y), f);
                    cursor += match self.axis {
                        Axis::X => child_sz.cols,
                        Axis::Y => child_sz.rows,
                    };
                }
            }
        }

        frame
    }
}

fn align_main(_dir: Dir, align: Axes<FixedAlignment>, axis: Axis, used: Row) -> Row {
    let a = match axis { Axis::X => align.x, Axis::Y => align.y };
    match a {
        FixedAlignment::Start => 0,
        FixedAlignment::Center => used / 2,
        FixedAlignment::End => used,
        _ => 0,
    }
}

fn align_cross(
    align: Axes<FixedAlignment>, axis: Axis,
    total_cols: Col, total_rows: Row,
    child_cols: Col, child_rows: Row,
) -> Row {
    let a = match axis { Axis::X => align.y, Axis::Y => align.x };
    let cross = match axis { Axis::X => Axis::Y, Axis::Y => Axis::X };
    let available = match cross { Axis::X => total_cols, Axis::Y => total_rows };
    let child = match cross { Axis::X => child_cols, Axis::Y => child_rows };
    match a {
        FixedAlignment::Start => 0,
        FixedAlignment::Center => (available - child).max(0) / 2,
        FixedAlignment::End => (available - child).max(0),
        _ => 0,
    }
}

// ── Horizontal composition ────────────────────────────────────────────────────

/// Compose frames horizontally, aligning on their baselines.
///
/// All frames are placed so their baseline rows line up.
/// The result frame has:
/// - `width`    = sum of all frame widths + `gap` between each pair
/// - `height`   = `max_ascent + max_descent`
/// - `baseline` = `max_ascent`
///
/// A `gap` of 0 means frames are placed immediately adjacent.
pub fn compose_horizontal(frames: Vec<TermFrame>, gap: Col) -> TermFrame {
    if frames.is_empty() {
        return TermFrame::new(TermSize::ZERO);
    }

    let n = frames.len();

    let max_ascent: Row = frames.iter().map(|f| f.ascent()).max().unwrap_or(0);
    let max_descent: Row = frames.iter().map(|f| f.descent()).max().unwrap_or(1);
    let total_rows = (max_ascent + max_descent).max(1);

    let content_width: Col = frames.iter().map(|f| f.cols()).sum();
    let gap_width: Col = gap * (n.saturating_sub(1)) as Col;
    let total_cols = (content_width + gap_width).max(0);

    let mut out = TermFrame::new(TermSize::new(total_cols, total_rows));
    out.set_baseline(max_ascent);

    let mut x: Col = 0;
    for (i, frame) in frames.into_iter().enumerate() {
        let w = frame.cols();
        // Shift down from the common baseline so that each frame's own
        // baseline aligns with max_ascent.
        let row = max_ascent - frame.ascent();
        out.push_frame(TermPoint::new(x, row), frame);
        x += w;
        if i + 1 < n {
            x += gap;
        }
    }

    out
}

// ── Vertical composition ──────────────────────────────────────────────────────

/// Compose frames vertically, centered horizontally.
///
/// Frames are stacked top-to-bottom with `gap` rows between each pair.
/// The result frame's baseline is determined by `baseline_idx`: it is placed
/// at the start of the frame at that index plus that frame's own baseline.
///
/// If `baseline_idx` is out of range it is clamped to the last frame.
pub fn compose_vertical(frames: Vec<TermFrame>, gap: Row, baseline_idx: usize) -> TermFrame {
    if frames.is_empty() {
        return TermFrame::new(TermSize::ZERO);
    }

    let n = frames.len();
    let max_cols: Col = frames.iter().map(|f| f.cols()).max().unwrap_or(0);

    // Pre-compute row heights (minimum 1 so zero-height frames still advance).
    let heights: Vec<Row> = frames.iter().map(|f| f.rows().max(1)).collect();

    let gap_rows: Row = gap * (n.saturating_sub(1)) as Row;
    let total_rows: Row = heights.iter().sum::<Row>() + gap_rows;

    // Baseline: top of the baseline_idx frame + that frame's baseline.
    let bi = baseline_idx.min(n.saturating_sub(1));
    let bi_frame_baseline = frames[bi].baseline();
    let mut baseline_row: Row = 0;
    for i in 0..bi {
        baseline_row += heights[i] + gap;
    }
    baseline_row += bi_frame_baseline;

    let mut out = TermFrame::new(TermSize::new(max_cols.max(0), total_rows.max(0)));
    out.set_baseline(baseline_row.max(0));

    let mut y: Row = 0;
    for (i, frame) in frames.into_iter().enumerate() {
        // Left-align within the widest frame.
        let cx: Col = 0; // left-aligned
        out.push_frame(TermPoint::new(cx, y), frame);
        y += heights[i];
        if i + 1 < n {
            y += gap;
        }
    }

    out
}

// ── Centering ─────────────────────────────────────────────────────────────────

/// Return the column offset to center `content_cols` within `total_cols`.
///
/// Returns 0 when `content_cols >= total_cols`.
#[inline]
pub fn center_col(content_cols: Col, total_cols: Col) -> Col {
    ((total_cols - content_cols) / 2).max(0)
}

// ── Horizontal rule frame ─────────────────────────────────────────────────────

/// Build a 1-row [`TermFrame`] filled with `ch` repeated to exactly `width`
/// terminal columns.
///
/// Uses [`TermFrame::hline`] internally, which handles wide characters.
/// The baseline is row 0 (the single row).
pub fn hline_frame(width: Col, ch: char, style: ContentStyle) -> TermFrame {
    let mut frame = TermFrame::new(TermSize::new(width.max(0), 1));
    if width > 0 {
        frame.hline(TermPoint::ZERO, width, ch, style);
    }
    frame
}
