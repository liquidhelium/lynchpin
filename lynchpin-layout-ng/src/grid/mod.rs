//! Grid / table terminal layout.
//!
//! Mirrors the paged layout in `lynchpin-layout/src/grid/`.
//! Architecture: auto-positioning → measure auto columns → resolve tracks →
//! re-layout cells → measure row heights → assemble with borders.

mod layouter;
mod lines;
mod repeated;
mod rowspans;

pub use self::layouter::GridLayouter;

use typst_library::diag::SourceResult;
use typst_library::engine::Engine;
use typst_library::foundations::{Content, NativeElement, Packed, StyleChain};
use typst_library::introspection::{Location, Locator, SplitLocator, Tag, TagFlags};
use typst_library::layout::grid::resolve::Cell;
use typst_library::layout::{
    GridCell, GridElem, Regions,
};
use typst_library::model::{TableCell, TableElem};

use lynchpin_library_ng::*;

use self::lines::{
    LineSegment, generate_line_segments, hline_stroke_at_column, vline_stroke_at_row,
};
use self::rowspans::{Rowspan, UnbreakableRowGroup};

// ── Shared traits (mirror Abs methods on TermScalar) ────────────────────────

/// `fits` replacement: `a.fits(b)` in paged is `a >= b`.
pub(crate) trait Fits {
    fn fits(self, other: Self) -> bool;
}

impl Fits for TermScalar {
    fn fits(self, other: Self) -> bool {
        self >= other
    }
}

/// Helper: set a value to the max of itself and another.
pub(crate) trait SetMax {
    fn set_max(&mut self, other: Self);
}

impl SetMax for TermScalar {
    fn set_max(&mut self, other: Self) {
        *self = (*self).max(other);
    }
}

/// Layout the cell into the given regions.
///
/// The `disambiguator` indicates which instance of this cell this should be
/// layouted as. For normal cells, it is always `0`, but for headers and
/// footers, it indicates the index of the header/footer among all. See the
/// [`Locator`] docs for more details on the concepts behind this.
pub fn layout_cell(
    cell: &Cell,
    engine: &mut Engine,
    locator: Locator,
    styles: StyleChain,
    regions: TermRegions,
    is_repeated: bool,
) -> SourceResult<TermFragment> {
    // HACK: manually generate tags for table and grid cells. Ideally table and
    // grid cells could just be marked as locatable, but the tags are somehow
    // considered significant for layouting. This hack together with a check in
    // the grid layouter makes the test suite pass.
    let mut locator = locator.split();
    let mut tags = None;
    if let Some(table_cell) = cell.body.to_packed::<TableCell>() {
        let mut table_cell = table_cell.clone();
        table_cell.is_repeated.set(is_repeated);
        tags = Some(generate_tags(table_cell, &mut locator, engine));
    } else if let Some(grid_cell) = cell.body.to_packed::<GridCell>() {
        let mut grid_cell = grid_cell.clone();
        grid_cell.is_repeated.set(is_repeated);
        tags = Some(generate_tags(grid_cell, &mut locator, engine));
    }

    let locator = locator.next(&cell.body.span());

    // Realize the cell content and lay out with terminal flow.
    use typst::routines::{Arenas, RealizationKind};
    let arenas = Arenas::default();
    let pairs = (engine.routines.realize)(
        RealizationKind::LayoutFragment { kind: &mut typst::routines::FragmentKind::Block },
        engine,
        &mut locator,
        &arenas,
        &cell.body,
        styles,
    )?;
    let fragment = crate::flow::layout_term_fragment(engine, &pairs, &mut locator, styles, regions)?;

    // Manually insert tags.
    let mut frames: Vec<TermFrame> = fragment_into_frames(fragment);
    if let Some((elem, loc, key)) = tags
        && let Some((first, remainder)) = frames.split_first_mut()
    {
        let flags = TagFlags {
            introspectable: true,
            tagged: true,
        };
        if remainder.is_empty() {
            first.prepend(TermPoint::ZERO, TermFrameItem::Tag(Tag::Start(elem, flags)));
            first.push(TermPoint::ZERO, TermFrameItem::Tag(Tag::End(loc, key, flags)));
        } else {
            // If there is more than one frame, set the logical parent of all
            // frames to the cell, which converts them to group frames. Then
            // prepend the start and end tags containing no content. The first
            // frame is also a logical child to guarantee correct ordering
            // in the introspector, since logical children are currently
            // inserted immediately after the start tag of the parent element
            // preceding any content within the parent element's tags.
            for frame in frames.iter_mut() {
                frame.set_parent(loc, true);
            }
            frames.first_mut().unwrap().prepend_multiple([
                (TermPoint::ZERO, TermFrameItem::Tag(Tag::Start(elem, flags))),
                (TermPoint::ZERO, TermFrameItem::Tag(Tag::End(loc, key, flags))),
            ]);
        }
    }

    Ok(fragment_from_frames(frames))
}

fn generate_tags<T: NativeElement>(
    mut cell: Packed<T>,
    locator: &mut SplitLocator,
    engine: &mut Engine,
) -> (Content, Location, u128) {
    let key = typst_utils::hash128(&cell);
    let loc = locator.next_location(engine.introspector, key);
    cell.set_location(loc);
    (cell.pack(), loc, key)
}

/// Layout the grid.
#[typst_macros::time(span = elem.span())]
pub fn layout_grid(
    elem: &Packed<GridElem>,
    engine: &mut Engine,
    locator: Locator,
    styles: StyleChain,
    regions: TermRegions,
) -> SourceResult<TermFragment> {
    let grid = elem.grid.as_ref().unwrap();
    GridLayouter::new(grid, regions, locator, styles, elem.span()).layout(engine)
}

/// Layout the table.
#[typst_macros::time(span = elem.span())]
pub fn layout_table(
    elem: &Packed<TableElem>,
    engine: &mut Engine,
    locator: Locator,
    styles: StyleChain,
    regions: TermRegions,
) -> SourceResult<TermFragment> {
    let grid = elem.grid.as_ref().unwrap();
    GridLayouter::new(grid, regions, locator, styles, elem.span()).layout(engine)
}

/// Convert `TermRegions` to paged `Regions` for compatibility with `layout_fragment`.
fn term_regions_to_paged(regions: TermRegions) -> Regions<'static> {
    use typst_library::layout::{Region, Abs, Size, Axes as PagedAxes};
    let size = Size::new(
        Abs::pt(regions.size.cols.get() as f64),
        Abs::pt(regions.size.rows.get() as f64),
    );
    let expand = PagedAxes::new(regions.expand.x, regions.expand.y);
    let mut pod: Regions = Region::new(size, expand).into();
    pod.full = Abs::pt(regions.full.get() as f64);
    let backlog_vec: Vec<typst_library::layout::Abs> = regions.backlog.iter().map(|&r| Abs::pt(r.get() as f64)).collect();
    pod.backlog = Vec::leak(backlog_vec);
    pod.last = regions.last.map(|r| Abs::pt(r.get() as f64));
    pod
}
