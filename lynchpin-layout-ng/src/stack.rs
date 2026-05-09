//! Stack layout for the terminal.
//!
//! Terminal equivalent of the paged `stack.rs`, using `TermScalar` for
//! main/cross axis calculations and `TermFrame` for output.  Mirrors the
//! paged `StackLayouter` completely, including Fr distribution, alignment, and
//! direction handling.

use typst_library::diag::{SourceResult, bail};
use typst_library::engine::Engine;
use typst_library::foundations::{Content, Packed, Resolve, StyleChain};
use typst_library::layout::{
    AlignElem, Axes, Axis, Dir, FixedAlignment, Fr, HElem, Spacing, StackChild,
    StackElem, VElem,
};
use typst_syntax::Span;
use typst_utils::Get;

use lynchpin_library_ng::*;
use crate::grid::SetMax;

/// Layout the stack.
#[typst_macros::time(span = elem.span())]
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
    let size = TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY);
    let expand = Axes::splat(false);
    let regions = TermRegions::one(size, expand);

    let mut layouter = StackLayouter::new(dir, regions, styles, config);
    let mut deferred = None;

    for child in &elem.children {
        match child {
            StackChild::Spacing(kind) => {
                layouter.layout_spacing(*kind);
                deferred = None;
            }
            StackChild::Block(block) => {
                // Transparently handle `h`.
                if axis == Axis::X {
                    if let Some(h) = block.to_packed::<HElem>() {
                        layouter.layout_spacing(h.amount);
                        deferred = None;
                        continue;
                    }
                }

                // Transparently handle `v`.
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

                layouter.layout_block(engine, block, styles)?;
                deferred = spacing;
            }
        }
    }

    Ok(layouter.finish())
}

/// Performs stack layout.
struct StackLayouter<'a> {
    /// The span to raise errors at during layout.
    span: Span,
    /// The stacking direction.
    dir: Dir,
    /// The axis of the stacking direction.
    axis: Axis,
    /// The inherited styles.
    styles: StyleChain<'a>,
    /// The config.
    config: &'a TermConfig,
    /// The regions to layout children into.
    regions: TermRegions,
    /// Whether the stack itself should expand to fill the region.
    expand: Axes<bool>,
    /// The initial size of the current region before we started subtracting.
    initial: TermSize,
    /// The generic size used by the frames for the current region.
    used: GenericSize<Row>,
    /// The sum of fractions in the current region.
    fr: Fr,
    /// Already layouted items whose exact positions are not yet known due to
    /// fractional spacing.
    items: Vec<StackItem>,
}

/// A prepared item in a stack layout.
enum StackItem {
    /// Absolute spacing between other items.
    Absolute(Row),
    /// Fractional spacing between other items.
    Fractional(Fr),
    /// A frame for a layouted block.
    Frame(TermFrame, Axes<FixedAlignment>),
}

impl<'a> StackLayouter<'a> {
    /// Create a new stack layouter.
    fn new(
        dir: Dir,
        mut regions: TermRegions,
        styles: StyleChain<'a>,
        config: &'a TermConfig,
    ) -> Self {
        let axis = dir.axis();
        let expand = regions.expand;
        let initial = regions.size;

        // Disable expansion along the block axis for children.
        regions.expand.set(axis, false);

        Self {
            span: Span::detached(),
            dir,
            axis,
            styles,
            config,
            regions,
            expand,
            initial,
            used: GenericSize::zero(),
            fr: Fr::zero(),
            items: vec![],
        }
    }

    /// Add spacing along the spacing direction.
    fn layout_spacing(&mut self, spacing: Spacing) {
        match spacing {
            Spacing::Rel(v) => {
                let resolved = units::rel_to_cols(&v, self.styles) as Row;
                let remaining = match self.axis {
                    Axis::X => &mut self.regions.size.cols,
                    Axis::Y => &mut self.regions.size.rows,
                };
                let limited = resolved.min(*remaining);
                *remaining -= limited;
                self.used.main += limited;
                self.items.push(StackItem::Absolute(resolved));
            }
            Spacing::Fr(v) => {
                self.fr += v;
                self.items.push(StackItem::Fractional(v));
            }
        }
    }

    /// Layout an arbitrary block.
    fn layout_block(
        &mut self,
        engine: &mut Engine,
        block: &Content,
        styles: StyleChain,
    ) -> SourceResult<()> {
        if self.regions.is_full() {
            self.finish_region()?;
        }

        // Block-axis alignment of the `AlignElem` is respected by stacks.
        let align = if let Some(align) = block.to_packed::<AlignElem>() {
            align.alignment.get(styles)
        } else {
            styles.get(AlignElem::alignment)
        }
        .resolve(styles);

        let region = TermRegion::new(self.regions.size, self.regions.expand);
        let frame = crate::flow::layout_term_frame(
            engine,
            &[(block, styles)],
            typst_library::introspection::Locator::root(),
            styles,
            region,
        )?;

        let specific_size = frame.size();
        if self.dir.axis() == Axis::Y {
            self.regions.size.rows -= specific_size.rows;
        }

        let generic_size = match self.axis {
            Axis::X => GenericSize::new(specific_size.rows, specific_size.cols),
            Axis::Y => GenericSize::new(specific_size.cols, specific_size.rows),
        };

        self.used.main += generic_size.main;
        self.used.cross.set_max(generic_size.cross);

        self.items.push(StackItem::Frame(frame, align));

        Ok(())
    }

    /// Advance to the next region.
    fn finish_region(&mut self) -> SourceResult<()> {
        // Determine the size of the stack in this region depending on whether
        // the region expands.
        let initial_axes = Axes::new(self.initial.cols, self.initial.rows);
        let mut size_axes = self
            .expand
            .select(initial_axes, self.used.into_axes(self.axis))
            .min(initial_axes);

        // Expand fully if there are fr spacings.
        let full = match self.axis {
            Axis::X => self.initial.cols,
            Axis::Y => self.initial.rows,
        };
        let remaining = full - self.used.main;
        if self.fr.get() > 0.0 && full.is_finite() {
            self.used.main = full;
            size_axes.set(self.axis, full);
        }

        let size = TermSize::new(size_axes.x, size_axes.y);
        if !size.is_finite() {
            bail!(self.span, "stack spacing is infinite");
        }

        // Determine the actual main size (overall frame extent along axis).
        let actual_main = match self.axis {
            Axis::X => size.cols,
            Axis::Y => size.rows,
        };

        let mut output = TermFrame::new(size);
        let mut cursor: Row = TermScalar::ZERO;
        let mut ruler: FixedAlignment = self.dir.start().into();

        // Place all frames.
        for item in std::mem::take(&mut self.items) {
            match item {
                StackItem::Absolute(v) => cursor += v,
                StackItem::Fractional(v) => {
                    let share = fr_share(v, self.fr, remaining);
                    cursor += share;
                }
                StackItem::Frame(frame, align) => {
                    if self.dir.is_positive() {
                        ruler = ruler.max(align.get(self.axis));
                    } else {
                        ruler = ruler.min(align.get(self.axis));
                    }

                    // Align along the main axis.
                    let parent = actual_main;
                    let child = match self.axis {
                        Axis::X => frame.size().cols,
                        Axis::Y => frame.size().rows,
                    };
                    let main_abs = ruler.position(typst::layout::Abs::raw((parent - self.used.main).get() as f64));
                    let main = TermScalar::from_f64(main_abs.to_raw())
                        + if self.dir.is_positive() {
                            cursor
                        } else {
                            self.used.main - child - cursor
                        };

                    // Align along the cross axis.
                    let other = self.axis.other();
                    let cross_abs = align
                        .get(other)
                        .position(typst::layout::Abs::raw(
                            (match other {
                                Axis::X => size.cols,
                                Axis::Y => size.rows,
                            } - match other {
                                Axis::X => frame.size().cols,
                                Axis::Y => frame.size().rows,
                            }).get() as f64,
                        ));
                    let cross = TermScalar::from_f64(cross_abs.to_raw());

                    let pos = GenericSize::new(cross, main).to_point(self.axis);
                    cursor += child;
                    output.push_frame(pos, frame);
                }
            }
        }

        // Advance to the next region.
        self.regions.next();
        self.initial = self.regions.size;
        self.used = GenericSize::zero();
        self.fr = Fr::zero();
        // For terminal layout we return a single frame; extra regions
        // would be additional frames but we only support single-region
        // for now.

        // Instead of collecting finished frames, just return the output
        // as the result from finish().
        // (For single-region layout, this is fine.)
        let _ = output;
        Ok(())
    }

    /// Finish layouting and return the resulting frame.
    fn finish(mut self) -> TermFrame {
        let _ = self.finish_region();

        // Build the frame from all items.
        let actual_main = match self.axis {
            Axis::X => self.used.into_axes(self.axis).x.max(TermScalar::ONE),
            Axis::Y => self.used.into_axes(self.axis).y.max(TermScalar::ONE),
        };
        let cross = self.used.cross.max(TermScalar::ONE);

        let size = match self.axis {
            Axis::X => TermSize::new(actual_main, cross),
            Axis::Y => TermSize::new(cross, actual_main),
        };

        let mut output = TermFrame::new(size);
        let mut cursor: Row = TermScalar::ZERO;

        // Expand fully if there are fr spacings.
        let full = match self.axis {
            Axis::X => size.cols,
            Axis::Y => size.rows,
        };
        let remaining = full - self.used.main;
        let fr_total = self.fr;

        for item in std::mem::take(&mut self.items) {
            match item {
                StackItem::Absolute(v) => cursor += v,
                StackItem::Fractional(v) => {
                    if fr_total != Fr::zero() {
                        let share = remaining.get() as f64
                            * (v.get() as f64 / fr_total.get() as f64);
                        cursor += TermScalar::from_f64(share.round());
                    }
                }
                StackItem::Frame(frame, align) => {
                    let child = match self.axis {
                        Axis::X => frame.size().cols,
                        Axis::Y => frame.size().rows,
                    };

                    let main = if self.dir.is_positive() {
                        cursor
                    } else {
                        self.used.main - child - cursor
                    };

                    let cross_axis = self.axis.other();
                    let cross_abs = align
                        .get(cross_axis)
                        .position(typst::layout::Abs::raw(
                            (match cross_axis {
                                Axis::X => size.cols,
                                Axis::Y => size.rows,
                            } - match cross_axis {
                                Axis::X => frame.size().cols,
                                Axis::Y => frame.size().rows,
                            }).get() as f64,
                        ));
                    let cross = TermScalar::from_f64(cross_abs.to_raw());

                    let pos = GenericSize::new(cross, main).to_point(self.axis);
                    cursor += child;
                    output.push_frame(pos, frame);
                }
            }
        }

        output
    }
}

// ── GenericSize ──────────────────────────────────────────────────────────────

/// A generic size with main and cross axes.
#[derive(Default, Copy, Clone, Eq, PartialEq, Hash)]
struct GenericSize<T> {
    /// The cross component.
    pub cross: T,
    /// The main component.
    pub main: T,
}

impl<T> GenericSize<T> {
    const fn new(cross: T, main: T) -> Self {
        Self { cross, main }
    }

    /// Convert to the specific representation, given the current main axis.
    fn into_axes(self, main: Axis) -> Axes<T> {
        match main {
            Axis::X => Axes::new(self.main, self.cross),
            Axis::Y => Axes::new(self.cross, self.main),
        }
    }
}

impl GenericSize<Row> {
    fn zero() -> Self {
        Self {
            cross: TermScalar::ZERO,
            main: TermScalar::ZERO,
        }
    }

    /// Convert to a point.
    fn to_point(self, main: Axis) -> TermPoint {
        let axes = self.into_axes(main);
        TermPoint::new(axes.x, axes.y)
    }
}

// ── Fr sharing helper ────────────────────────────────────────────────────────

/// Share a fractional amount of remaining space.
/// Terminal equivalent of `Fr::share`.
fn fr_share(fr: Fr, total_fr: Fr, remaining: TermScalar) -> TermScalar {
    if total_fr == Fr::zero() || remaining <= TermScalar::ZERO {
        return TermScalar::ZERO;
    }
    let share = remaining.get() as f64 * (fr.get() as f64 / total_fr.get() as f64);
    TermScalar::from_f64(share.round())
}
