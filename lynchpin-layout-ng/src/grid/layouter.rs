//! Grid layouter: the core grid/table layout engine.
//!
//! Mirrors `lynchpin-layout/src/grid/layouter.rs`.
//! Architecture: measure columns → layout rows → handle regions →
//! resolve rowspans → render grid lines.

use std::fmt::Debug;

use rustc_hash::FxHashMap;
use typst_library::diag::{SourceResult, bail};
use typst_library::engine::Engine;
use typst_library::foundations::{Resolve, StyleChain};
use typst_library::introspection::Locator;
use typst_library::layout::grid::resolve::{Cell, CellGrid, Header, LinePosition, Repeatable};
use typst_library::layout::resolve::Entry;
use typst_library::layout::{Axes, Dir, Fr, Length, Rel, Sizing};
use typst_library::text::TextElem;
use typst_syntax::Span;
use typst_utils::Numeric;

use lynchpin_library_ng::*;

use super::{
    LineSegment, Rowspan, UnbreakableRowGroup, generate_line_segments, hline_stroke_at_column,
    layout_cell, vline_stroke_at_row,
};

// ── TermScalar extensions ────────────────────────────────────────────────────

/// `fits` replacement: `a.fits(b)` in paged is `a >= b`.
trait Fits {
    fn fits(self, other: Self) -> bool;
}

impl Fits for TermScalar {
    fn fits(self, other: Self) -> bool {
        self >= other
    }
}

/// Helper: set a value to the max of itself and another.
trait SetMax {
    fn set_max(&mut self, other: Self);
}

impl SetMax for TermScalar {
    fn set_max(&mut self, other: Self) {
        *self = (*self).max(other);
    }
}

/// Share fractional space — mirrors `Fr::share`.
fn fr_share(fr: Fr, total: Fr, space: TermScalar) -> TermScalar {
    if total == Fr::zero() || space <= TermScalar::ZERO {
        return TermScalar::ZERO;
    }
    let ratio = fr.get() / total.get();
    TermScalar::from_f64(space.get() as f64 * ratio)
}

/// Resolve a `Rel<Length>` to an integer column count.
///
/// Mirrors paged `measure_columns`: the `Rel<Length>` is resolved via
/// `resolve(styles)` → `Rel<Abs>`, then `.relative_to(base_abs)` produces
/// the absolute column size in `Abs`.  We convert between columns and
/// points using the font size (1 column ≈ 1 character width ≈ font size).
/// The result is rounded to an integer to keep grid coordinates exact.
fn resolve_rel(rel: &Rel<Length>, base: TermScalar, styles: StyleChain) -> TermScalar {
    let font_size: typst_library::layout::Abs =
        styles.get(typst_library::text::TextElem::size).0.resolve(styles);
    let base_abs = typst_library::layout::Abs::pt(base.get() as f64 * font_size.to_pt());
    let result_abs: typst_library::layout::Abs = rel.resolve(styles).relative_to(base_abs);
    let cols = result_abs.to_pt() / font_size.to_pt();
    TermScalar::from_f64(cols.round())
}

// ── Main structures ──────────────────────────────────────────────────────────

/// Performs grid layout.
pub struct GridLayouter<'a> {
    /// The grid of cells.
    pub(super) grid: &'a CellGrid,
    /// The regions to layout children into.
    pub(super) regions: TermRegions,
    /// The locators for each cell in the cell grid.
    pub(super) cell_locators: FxHashMap<Axes<usize>, Locator<'a>>,
    /// The inherited styles.
    pub(super) styles: StyleChain<'a>,
    /// Resolved column sizes.
    pub(super) rcols: Vec<TermScalar>,
    /// The sum of `rcols`.
    pub(super) width: TermScalar,
    /// Resolved row sizes, by region.
    pub(super) rrows: Vec<Vec<RowPiece>>,
    /// The amount of unbreakable rows remaining to be laid out in the
    /// current unbreakable row group. While this is positive, no region breaks
    /// should occur.
    pub(super) unbreakable_rows_left: usize,
    /// Rowspans not yet laid out because not all of their spanned rows were
    /// laid out yet.
    pub(super) rowspans: Vec<Rowspan>,
    /// Grid layout state for the current region.
    pub(super) current: Current,
    /// Frames for finished regions.
    pub(super) finished: Vec<TermFrame>,
    /// The amount and height of header rows on each finished region.
    pub(super) finished_header_rows: Vec<FinishedHeaderRowInfo>,
    /// Whether this is an RTL grid.
    pub(super) is_rtl: bool,
    /// Currently repeating headers, one per level. Sorted by increasing
    /// levels.
    ///
    /// Note that some levels may be absent, in particular level 0, which does
    /// not exist (so all levels are >= 1).
    pub(super) repeating_headers: Vec<&'a Header>,
    /// Headers, repeating or not, awaiting their first successful layout.
    /// Sorted by increasing levels.
    pub(super) pending_headers: &'a [Repeatable<Header>],
    /// Next headers to be processed.
    pub(super) upcoming_headers: &'a [Repeatable<Header>],
    /// State of the row being currently laid out.
    ///
    /// This is kept as a field to avoid passing down too many parameters from
    /// `layout_row` into called functions, which would then have to pass them
    /// down to `push_row`, which reads these values.
    pub(super) row_state: RowState,
    /// The span of the grid element.
    pub(super) span: Span,
}

/// Grid layout state for the current region. This should be reset or updated
/// on each region break.
pub(super) struct Current {
    /// The initial size of the current region before we started subtracting.
    pub(super) initial: TermSize,
    /// The height of the region after repeated headers were placed and footers
    /// prepared. This also includes pending repeating headers from the start,
    /// even if they were not repeated yet, since they will be repeated in the
    /// next region anyway (bar orphan prevention).
    ///
    /// This is used to quickly tell if any additional space in the region has
    /// been occupied since then, meaning that additional space will become
    /// available after a region break (see
    /// [`GridLayouter::may_progress_with_repeats`]).
    pub(super) initial_after_repeats: TermScalar,
    /// Whether `layouter.regions.may_progress()` was `true` at the top of the
    /// region.
    pub(super) could_progress_at_top: bool,
    /// Rows in the current region.
    pub(super) lrows: Vec<Row>,
    /// The amount of repeated header rows at the start of the current region.
    /// Thus, excludes rows from pending headers (which were placed for the
    /// first time).
    ///
    /// Note that `repeating_headers` and `pending_headers` can change if we
    /// find a new header inside the region (not at the top), so this field
    /// is required to access information from the top of the region.
    ///
    /// This information is used on finish region to calculate the total height
    /// of resolved header rows at the top of the region, which is used by
    /// multi-page rowspans so they can properly skip the header rows at the
    /// top of each region during layout.
    pub(super) repeated_header_rows: usize,
    /// The end bound of the row range of the last repeating header at the
    /// start of the region.
    ///
    /// The last row might have disappeared from layout due to being empty, so
    /// this is how we can become aware of where the last header ends without
    /// having to check the vector of rows. Line layout uses this to determine
    /// when to prioritize the last lines under a header.
    ///
    /// A value of zero indicates no repeated headers were placed.
    pub(super) last_repeated_header_end: usize,
    /// Stores the length of `lrows` before a sequence of rows equipped with
    /// orphan prevention was laid out. In this case, if no more rows without
    /// orphan prevention are laid out after those rows before the region ends,
    /// the rows will be removed, and there may be an attempt to place them
    /// again in the new region. Effectively, this is the mechanism used for
    /// orphan prevention of rows.
    ///
    /// At the moment, this is only used by repeated headers (they aren't laid
    /// out if alone in the region) and by new headers, which are moved to the
    /// `pending_headers` vector and so will automatically be placed again
    /// until they fit and are not orphans in at least one region (or exactly
    /// one, for non-repeated headers).
    pub(super) lrows_orphan_snapshot: Option<usize>,
    /// The height of effectively repeating headers, that is, ignoring
    /// non-repeating pending headers, in the current region.
    ///
    /// This is used by multi-page auto rows so they can inform cell layout on
    /// how much space should be taken by headers if they break across regions.
    /// In particular, non-repeating headers only occupy the initial region,
    /// but disappear on new regions, so they can be ignored.
    ///
    /// This field is reset on each new region and properly updated by
    /// `layout_auto_row` and `layout_relative_row`, and should not be read
    /// before all header rows are fully laid out. It is usually fine because
    /// header rows themselves are unbreakable, and unbreakable rows do not
    /// need to read this field at all.
    ///
    /// This height is not only computed at the beginning of the region. It is
    /// updated whenever a new header is found, subtracting the height of
    /// headers which stopped repeating and adding the height of all new
    /// headers.
    pub(super) repeating_header_height: TermScalar,
    /// The height for each repeating header that was placed in this region.
    pub(super) repeating_header_heights: Vec<TermScalar>,
    /// The simulated footer height for this region.
    ///
    /// The simulation occurs before any rows are laid out for a region.
    pub(super) footer_height: TermScalar,
}

/// Data about the row being laid out right now.
#[derive(Debug, Default)]
pub(super) struct RowState {
    /// If this is `Some`, this will be updated by the currently laid out row's
    /// height if it is auto or relative. This is used for header height
    /// calculation.
    pub(super) current_row_height: Option<TermScalar>,
    /// This is `true` when laying out non-short lived headers and footers.
    pub(super) in_active_repeatable: bool,
    /// This is `false` if a header row is laid out the first time, and `false`
    /// for any other time. For footers it's the opposite.
    pub(super) is_being_repeated: bool,
}

/// Data about laid out repeated header rows for a specific finished region.
#[derive(Debug, Default)]
pub(super) struct FinishedHeaderRowInfo {
    /// The amount of repeated headers at the top of the region.
    pub(super) repeated_amount: usize,
    /// The end bound of the row range of the last repeated header at the top
    /// of the region.
    pub(super) last_repeated_header_end: usize,
    /// The total height of repeated headers at the top of the region.
    pub(super) repeated_height: TermScalar,
}

/// Details about a resulting row piece.
#[derive(Debug)]
pub struct RowPiece {
    /// The height of the segment.
    pub height: TermScalar,
    /// The index of the row.
    pub y: usize,
}

/// Produced by initial row layout, auto and relative rows are already finished,
/// fractional rows not yet.
pub(super) enum Row {
    /// Finished row frame of auto or relative row with y index.
    /// The last parameter indicates whether or not this is the last region
    /// where this row is laid out, and it can only be false when a row uses
    /// `layout_multi_row`, which in turn is only used by breakable auto rows.
    Frame(TermFrame, usize, bool),
    /// Fractional row with y index and disambiguator.
    Fr(Fr, usize, usize),
}

impl Row {
    /// Returns the `y` index of this row.
    fn index(&self) -> usize {
        match self {
            Self::Frame(_, y, _) => *y,
            Self::Fr(_, y, _) => *y,
        }
    }
}

impl<'a> GridLayouter<'a> {
    /// Create a new grid layouter.
    ///
    /// This prepares grid layout by unifying content and gutter tracks.
    pub fn new(
        grid: &'a CellGrid,
        regions: TermRegions,
        locator: Locator<'a>,
        styles: StyleChain<'a>,
        span: Span,
    ) -> Self {
        // We use these regions for auto row measurement. Since at that moment,
        // columns are already sized, we can enable horizontal expansion.
        let mut regions = regions;
        regions.expand = Axes::new(true, false);

        // Prepare the locators for each cell in the cell grid.
        let mut locator = locator.split();
        let mut cell_locators = FxHashMap::default();
        for y in 0..grid.rows.len() {
            for x in 0..grid.cols.len() {
                let Some(Entry::Cell(cell)) = grid.entry(x, y) else {
                    continue;
                };
                cell_locators.insert(Axes::new(x, y), locator.next(&cell.body.span()));
            }
        }

        let initial_size = regions.size;
        let initial_rows = regions.size.rows;
        let could_progress = regions.may_progress();

        Self {
            grid,
            regions,
            cell_locators,
            styles,
            rcols: vec![TermScalar::ZERO; grid.cols.len()],
            width: TermScalar::ZERO,
            rrows: vec![],
            unbreakable_rows_left: 0,
            rowspans: vec![],
            finished: vec![],
            finished_header_rows: vec![],
            is_rtl: styles.resolve(TextElem::dir) == Dir::RTL,
            repeating_headers: vec![],
            upcoming_headers: &grid.headers,
            pending_headers: Default::default(),
            row_state: RowState::default(),
            current: Current {
                initial: initial_size,
                initial_after_repeats: initial_rows,
                could_progress_at_top: could_progress,
                lrows: vec![],
                repeated_header_rows: 0,
                last_repeated_header_end: 0,
                lrows_orphan_snapshot: None,
                repeating_header_height: TermScalar::ZERO,
                repeating_header_heights: vec![],
                footer_height: TermScalar::ZERO,
            },
            span,
        }
    }

    /// Create a [`Locator`] for use in [`layout_cell`].
    pub(super) fn cell_locator(&self, pos: Axes<usize>, disambiguator: usize) -> Locator<'a> {
        let mut cell_locator = self.cell_locators[&pos].relayout();

        // The disambiguator is used for repeated cells, e.g. in repeated headers.
        if disambiguator > 0 {
            cell_locator = cell_locator.split().next_inner(disambiguator as u128);
        }

        cell_locator
    }

    /// Determines the columns sizes and then layouts the grid row-by-row.
    pub fn layout(mut self, engine: &mut Engine) -> SourceResult<TermFragment> {
        self.measure_columns(engine)?;

        if let Some(footer) = &self.grid.footer
            && footer.repeated
        {
            // Ensure rows in the first region will be aware of the
            // possible presence of the footer.
            self.prepare_footer(footer, engine, 0)?;
            self.regions.size.rows -= self.current.footer_height;
            self.current.initial_after_repeats = self.regions.size.rows;
        }

        let mut y = 0;
        let mut consecutive_header_count = 0;
        while y < self.grid.rows.len() {
            if let Some(next_header) = self.upcoming_headers.get(consecutive_header_count)
                && next_header.range.contains(&y)
            {
                self.place_new_headers(&mut consecutive_header_count, engine)?;
                y = next_header.range.end;

                // Skip header rows during normal layout.
                continue;
            }

            if let Some(footer) = &self.grid.footer
                && footer.repeated
                && y >= footer.start
            {
                if y == footer.start {
                    self.layout_footer(footer, engine, self.finished.len(), false)?;
                    self.flush_orphans();
                }
                y = footer.end;
                continue;
            }

            self.layout_row(y, engine, 0)?;

            // After the first non-header row is placed, pending headers are no
            // longer orphans and can repeat, so we move them to repeating
            // headers.
            self.flush_orphans();

            y += 1;
        }

        self.finish_region(engine, true)?;

        // Layout any missing rowspans.
        for rowspan in std::mem::take(&mut self.rowspans) {
            self.layout_rowspan(rowspan, None, engine)?;
        }

        self.render_fills_strokes()
    }

    /// Layout a row with a certain initial state, returning the final state.
    #[inline]
    pub(super) fn layout_row_with_state(
        &mut self,
        y: usize,
        engine: &mut Engine,
        disambiguator: usize,
        initial_state: RowState,
    ) -> SourceResult<RowState> {
        let previous = std::mem::replace(&mut self.row_state, initial_state);
        self.layout_row_internal(y, engine, disambiguator)?;
        Ok(std::mem::replace(&mut self.row_state, previous))
    }

    /// Layout the given row with the default row state.
    #[inline]
    pub(super) fn layout_row(
        &mut self,
        y: usize,
        engine: &mut Engine,
        disambiguator: usize,
    ) -> SourceResult<()> {
        self.layout_row_with_state(y, engine, disambiguator, RowState::default())?;
        Ok(())
    }

    /// Layout the given row using the current state.
    pub(super) fn layout_row_internal(
        &mut self,
        y: usize,
        engine: &mut Engine,
        disambiguator: usize,
    ) -> SourceResult<()> {
        // Skip to next region if current one is full, but only for content
        // rows, not for gutter rows, and only if we aren't laying out an
        // unbreakable group of rows.
        let is_content_row = !self.grid.is_gutter_track(y);
        if self.unbreakable_rows_left == 0 && self.regions.is_full() && is_content_row {
            self.finish_region(engine, false)?;
        }

        if is_content_row {
            // Gutter rows have no rowspans or possibly unbreakable cells.
            self.check_for_rowspans(disambiguator, y);
            self.check_for_unbreakable_rows(y, engine)?;
        }

        // Don't layout gutter rows at the top of a region.
        if is_content_row || !self.current.lrows.is_empty() {
            match self.grid.rows[y] {
                Sizing::Auto => self.layout_auto_row(engine, disambiguator, y)?,
                Sizing::Rel(v) => self.layout_relative_row(engine, disambiguator, v, y)?,
                Sizing::Fr(v) => {
                    if !self.row_state.in_active_repeatable {
                        self.flush_orphans();
                    }
                    self.current.lrows.push(Row::Fr(v, y, disambiguator))
                }
            }
        }

        self.unbreakable_rows_left = self.unbreakable_rows_left.saturating_sub(1);

        Ok(())
    }

    /// Add lines and backgrounds.
    fn render_fills_strokes(mut self) -> SourceResult<TermFragment> {
        eprintln!(
            "DEBUG grid: cols={} rows={} hlines_len={} vlines_len={} has_gutter={}",
            self.grid.cols.len(), self.grid.rows.len(),
            self.grid.hlines.len(), self.grid.vlines.len(),
            self.grid.has_gutter,
        );
        let mut finished = std::mem::take(&mut self.finished);
        let finished_len = finished.len();
        for ((frame_index, frame), finished_header_rows) in finished.iter_mut().enumerate().zip(
            self.finished_header_rows
                .iter()
                .map(Some)
                .chain(std::iter::repeat(None)),
        ) {
            let rows = &self.rrows[frame_index.min(self.rrows.len().saturating_sub(1))];
            if self.rcols.is_empty() || rows.is_empty() {
                continue;
            }

            // Render grid lines.
            // Which line position to look for in the list of lines for a track.
            let expected_line_position = |index, is_max_index: bool| {
                if self.grid.is_gutter_track(index) && !is_max_index {
                    LinePosition::After
                } else {
                    LinePosition::Before
                }
            };

            // Each content column contributes rcols[x] + 1 (cell + vline).
            // Gutter columns contribute just rcols[x] (no vline).
            let padded: Vec<TermScalar> = self
                .rcols
                .iter()
                .enumerate()
                .map(|(x, c)| {
                    if self.grid.is_gutter_track(x) { *c }
                    else { *c + TermScalar::ONE }
                })
                .collect();
            let frame_width = self.width + TermScalar::new(self.grid.cols.len() as i32 + 1);

            // Render vertical lines.
            for (x, dx) in points(padded.iter().copied()).enumerate() {
                let dx = if self.is_rtl { frame_width - dx } else { dx };
                let is_end_border = x == self.grid.cols.len();
                let expected_vline_position = expected_line_position(x, is_end_border);

                let vlines_at_column = self
                    .grid
                    .vlines
                    .get(if !self.grid.has_gutter {
                        x
                    } else if is_end_border {
                        x / 2 + 1
                    } else {
                        x / 2
                    })
                    .into_iter()
                    .flatten()
                    .filter(|line| line.position == expected_vline_position);

                let tracks = rows.iter().map(|row| (row.y, row.height));

                let segments = generate_line_segments(
                    self.grid,
                    tracks,
                    x,
                    vlines_at_column,
                    vline_stroke_at_row,
                );

                for segment in segments {
                    let LineSegment {
                        stroke: _,
                        offset: dy,
                        length,
                        priority: _,
                    } = segment;
                    // Approximate stroke with vline using │ character.
                    if length > TermScalar::ZERO {
                        let ch = '│';
                        let style = crossterm::style::ContentStyle::default();
                        frame.vline(TermPoint::new(dx, dy), length, ch, style);
                    }
                }
            }

            // Render horizontal lines at row boundaries.
            // Check per-cell strokes: if any cell at this boundary has a
            // top/bottom stroke, draw the hline. Table cells have default
            // strokes; list/enum cells don't.
            let hline_offsets: Vec<TermScalar> =
                points(rows.iter().map(|piece| piece.height)).collect();
            let hline_style = crossterm::style::ContentStyle::default();

            for (i, row_piece) in rows.iter().enumerate() {
                let y = row_piece.y;
                // Check if any cell in this row has a stroke configured.
                let has_stroke = (0..self.grid.cols.len()).any(|x| {
                    if self.grid.is_gutter_track(x) { return false; }
                    self.grid.cell(x, y)
                        .map(|c| c.stroke.top.is_some() || c.stroke.bottom.is_some())
                        .unwrap_or(false)
                });

                if !has_stroke { continue; }

                let dy = hline_offsets[i];
                // Top border: draw at the top of the first row.
                if i == 0 {
                    frame.hline(TermPoint::new(TermScalar::ZERO, dy), frame_width, '─', hline_style);
                }
                // Bottom border: draw below each row.
                let bottom_dy = hline_offsets[i + 1];
                frame.hline(TermPoint::new(TermScalar::ZERO, bottom_dy), frame_width, '─', hline_style);
            }
        }

        Ok(finished)
    }

    /// Determine all column sizes.
    fn measure_columns(&mut self, engine: &mut Engine) -> SourceResult<()> {
        let mut rel = TermScalar::ZERO;
        let mut fr = Fr::zero();
        let mut non_fr_content = 0usize;
        let mut fr_count = 0usize;

        // Resolve the size of all relative columns.  Gutter columns are
        // fixed and do NOT get vlines — they themselves serve as spacing.
        for (x, (&col, rcol)) in self.grid.cols.iter().zip(&mut self.rcols).enumerate() {
            let is_gutter = self.grid.is_gutter_track(x);
            match col {
                Sizing::Auto => {
                    if !is_gutter { non_fr_content += 1; }
                }
                Sizing::Rel(v) => {
                    let resolved = resolve_rel(&v, self.regions.base().cols, self.styles);
                    *rcol = resolved;
                    rel = rel + resolved;
                    if !is_gutter { non_fr_content += 1; }
                }
                Sizing::Fr(v) => {
                    fr += v;
                    fr_count += 1;
                }
            }
        }

        // Vlines: one per content column, plus the right border.
        // Gutter columns don't get vlines.
        let vline_cost = TermScalar::new((non_fr_content + 1) as i32);
        let page_width = self.regions.size.cols;
        let available = page_width - rel - vline_cost;
        if available >= TermScalar::ZERO {
            let (auto, count) = self.measure_auto_columns(engine, available)?;

            let remaining = available - auto;
            if remaining >= TermScalar::ZERO {
                self.grow_fractional_columns(remaining, fr, fr_count);
            } else {
                self.shrink_auto_columns(available, count);
            }
        }

        self.width = self.rcols.iter().sum();
        Ok(())
    }

    /// Total width spanned by the cell (among resolved columns).
    /// Includes spanned gutter columns.
    pub(super) fn cell_spanned_width(&self, cell: &Cell, x: usize) -> TermScalar {
        let colspan = self.grid.effective_colspan_of_cell(cell);
        self.rcols.iter().skip(x).take(colspan).sum()
    }

    /// Measure the size that is available to auto columns.
    fn measure_auto_columns(
        &mut self,
        engine: &mut Engine,
        available: TermScalar,
    ) -> SourceResult<(TermScalar, usize)> {
        let mut auto = TermScalar::ZERO;
        let mut count = 0;
        let all_frac_cols = self
            .grid
            .cols
            .iter()
            .enumerate()
            .filter(|(_, col)| col.is_fractional())
            .map(|(x, _)| x)
            .collect::<Vec<_>>();

        // Determine size of auto columns by laying out all cells in those
        // columns, measuring them and finding the largest one.
        for (x, &col) in self.grid.cols.iter().enumerate() {
            if col != Sizing::Auto {
                continue;
            }

            let mut resolved = TermScalar::ZERO;
            for y in 0..self.grid.rows.len() {
                // We get the parent cell in case this is a merged position.
                let Some(parent) = self.grid.parent_cell_position(x, y) else {
                    continue;
                };
                if parent.y != y {
                    // Don't check the width of rowspans more than once.
                    continue;
                }
                let cell = self.grid.cell(parent.x, parent.y).unwrap();
                let colspan = self.grid.effective_colspan_of_cell(cell);
                if colspan > 1 {
                    let last_spanned_auto_col = self
                        .grid
                        .cols
                        .iter()
                        .enumerate()
                        .skip(parent.x)
                        .take(colspan)
                        .rev()
                        .find(|&(_, &c)| c == Sizing::Auto)
                        .map(|(x, _)| x);

                    if last_spanned_auto_col != Some(x) {
                        // A colspan should only affect the last spanned auto
                        // column's width.
                        continue;
                    }
                }

                // Mirror paged: measure in the full available width, then
                // take the actual content width as the column size.
                let size = TermSize::new(available, TermScalar::INFINITY);
                let pod: TermRegions = TermRegion::new(size, Axes::splat(false)).into();

                let locator = self.cell_locator(parent, 0);
                let frames = layout_cell(cell, engine, locator, self.styles, pod, false)?
                    .into_iter()
                    .map(|frame| frame.content_width())
                    .max()
                    .unwrap_or(TermScalar::ZERO);

                resolved.set_max(frames);
            }

            if !all_frac_cols.contains(&x) {
                auto = auto + resolved;
            }
            self.rcols[x] = resolved;
            count += 1;
        }

        Ok((auto, count))
    }

    /// Distribute remaining space (including vlines) to fractional columns.
    ///
    /// Each fr column receives `cell + 1` proportional to its fraction,
    /// then the vline column is subtracted to get the cell width.
    /// The integer remainder is distributed to columns with the largest
    /// fractional parts so that the total exactly equals `remaining`.
    fn grow_fractional_columns(
        &mut self,
        remaining: TermScalar,
        fr: Fr,
        fr_count: usize,
    ) {
        if fr.is_zero() || fr_count == 0 {
            return;
        }

        let total = remaining.get() as f64;
        let total_fr = fr.get();

        // Compute each fr cell+vline as floating.
        let mut shares: Vec<(usize, f64, f64)> = self
            .grid
            .cols
            .iter()
            .enumerate()
            .filter(|(_, col)| col.is_fractional())
            .map(|(x, col)| {
                let Sizing::Fr(v) = col else { unreachable!() };
                let exact = v.get() / total_fr * total;
                (x, exact, exact.fract())
            })
            .collect();

        // Assign floor as the base cell+vline width.
        let mut assigned: i32 = 0;
        for (x, exact, _) in &shares {
            let cell_vline = exact.floor() as i32;
            self.rcols[*x] = TermScalar::new(cell_vline);
            assigned += cell_vline;
        }

        // Distribute remainder (1 col each to the largest fractional parts).
        let remainder = remaining.get() as i32 - assigned;
        shares.sort_unstable_by(|a, b| b.2.total_cmp(&a.2));
        for (x, _, _) in shares.iter().take(remainder.max(0) as usize) {
            self.rcols[*x] += TermScalar::ONE;
        }

        // Subtract the vline column from each fr cell to get cell width.
        for (x, _, _) in &shares {
            self.rcols[*x] -= TermScalar::ONE;
        }
    }

    /// Redistribute space to auto columns so that each gets a fair share.
    fn shrink_auto_columns(&mut self, available: TermScalar, count: usize) {
        let mut last;
        let mut fair = TermScalar::NEG_INFINITY;
        let mut redistribute = available;
        let mut overlarge = count;
        let mut changed = true;

        // Iteratively remove columns that don't need to be shrunk.
        while changed && overlarge > 0 {
            changed = false;
            last = fair;
            fair = TermScalar::from_f64(redistribute.get() as f64 / overlarge as f64);

            for (&col, &rcol) in self.grid.cols.iter().zip(&self.rcols) {
                // Remove an auto column if it is not overlarge (rcol <= fair),
                // but also hasn't already been removed (rcol > last).
                if col == Sizing::Auto && rcol <= fair && rcol > last {
                    redistribute = redistribute - rcol;
                    overlarge -= 1;
                    changed = true;
                }
            }
        }

        // Redistribute space fairly among overlarge columns.
        for (&col, rcol) in self.grid.cols.iter().zip(&mut self.rcols) {
            if col == Sizing::Auto && *rcol > fair {
                *rcol = fair;
            }
        }
    }

    /// Layout a row with automatic height. Such a row may break across multiple
    /// regions.
    fn layout_auto_row(
        &mut self,
        engine: &mut Engine,
        disambiguator: usize,
        y: usize,
    ) -> SourceResult<()> {
        // Determine the size for each region of the row. If the first region
        // ends up empty for some column, skip the region and remeasure.
        let mut resolved = match self.measure_auto_row(
            engine,
            disambiguator,
            y,
            true,
            self.unbreakable_rows_left,
            None,
        )? {
            Some(resolved) => resolved,
            None => {
                self.finish_region(engine, false)?;
                self.measure_auto_row(
                    engine,
                    disambiguator,
                    y,
                    false,
                    self.unbreakable_rows_left,
                    None,
                )?
                .unwrap()
            }
        };

        // Nothing to layout.
        if resolved.is_empty() {
            return Ok(());
        }

        // Layout into a single region.
        if let &[first] = resolved.as_slice() {
            let frame = self.layout_single_row(engine, disambiguator, first, y)?;
            self.push_row(frame, y, true);

            if let Some(row_height) = &mut self.row_state.current_row_height {
                // Add to header height, as we are in a header row.
                *row_height = *row_height + first;
            }

            return Ok(());
        }

        // Expand all but the last region.
        let len = resolved.len();
        for ((i, region), target) in self
            .regions
            .iter()
            .enumerate()
            .zip(&mut resolved[..len - 1])
            .skip(
                self.current
                    .lrows
                    .iter()
                    .any(|row| matches!(row, Row::Fr(..))) as usize,
            )
        {
            // Subtract header and footer heights from the region height when
            // it's not the first. Ignore non-repeating headers as they only
            // appear on the first region by definition.
            target.set_max(
                region.rows
                    - if i > 0 {
                        self.current.repeating_header_height + self.current.footer_height
                    } else {
                        TermScalar::ZERO
                    },
            );
        }

        // Layout into multiple regions.
        let fragment = self.layout_multi_row(engine, disambiguator, &resolved, y)?;
        let len = fragment.len();
        for (i, frame) in fragment.into_iter().enumerate() {
            self.push_row(frame, y, i + 1 == len);
            if i + 1 < len {
                self.finish_region(engine, false)?;
            }
        }

        Ok(())
    }

    /// Measure the regions sizes of an auto row. The option is always `Some(_)`
    /// if `can_skip` is false.
    pub(super) fn measure_auto_row(
        &self,
        engine: &mut Engine,
        disambiguator: usize,
        y: usize,
        can_skip: bool,
        unbreakable_rows_left: usize,
        row_group_data: Option<&UnbreakableRowGroup>,
    ) -> SourceResult<Option<Vec<TermScalar>>> {
        let breakable = unbreakable_rows_left == 0;
        let mut resolved: Vec<TermScalar> = vec![];
        let mut pending_rowspans: Vec<(usize, usize, Vec<TermScalar>)> = vec![];

        for x in 0..self.rcols.len() {
            // Get the parent cell in case this is a merged position.
            let Some(parent) = self.grid.parent_cell_position(x, y) else {
                // Skip gutter columns.
                continue;
            };
            if parent.x != x {
                // Only check the height of a colspan once.
                continue;
            }
            // The parent cell is never a gutter or merged position.
            let cell = self.grid.cell(parent.x, parent.y).unwrap();
            let rowspan = self.grid.effective_rowspan_of_cell(cell);

            if rowspan > 1 {
                let last_spanned_auto_row = self
                    .grid
                    .rows
                    .iter()
                    .enumerate()
                    .skip(parent.y)
                    .take(rowspan)
                    .rev()
                    .find(|&(_, &row)| row == Sizing::Auto)
                    .map(|(y, _)| y);

                if last_spanned_auto_row != Some(y) {
                    // A rowspan should only affect the height of its last
                    // spanned auto row.
                    continue;
                }
            }

            let measurement_data =
                self.prepare_auto_row_cell_measurement(parent, cell, breakable, row_group_data);
            let size = TermSize::new(measurement_data.width, measurement_data.height);
            let backlog = measurement_data
                .backlog
                .unwrap_or(&measurement_data.custom_backlog);

            let pod = if !breakable {
                // Force cell to fit into a single region when the row is
                // unbreakable, even when it is a breakable rowspan, as a best
                // effort.
                let mut pod: TermRegions = TermRegion::new(size, self.regions.expand).into();
                pod.full = measurement_data.full;

                if measurement_data.frames_in_previous_regions > 0 {
                    // Best effort to conciliate a breakable rowspan which
                    // started at a previous region going through an
                    // unbreakable auto row. Ensure it goes through previously
                    // laid out regions, but stops at this one when measuring.
                    pod.backlog = backlog.to_vec();
                }

                pod
            } else {
                // This row is breakable, so measure the cell normally, with
                // the initial height and backlog determined previously.
                let mut pod = self.regions.clone();
                pod.size = size;
                pod.backlog = backlog.to_vec();
                pod.full = measurement_data.full;
                pod.last = measurement_data.last;

                pod
            };

            let locator = self.cell_locator(parent, disambiguator);
            let frames = layout_cell(
                cell,
                engine,
                locator,
                self.styles,
                pod,
                self.row_state.is_being_repeated,
            )?;

            // HACK: Also consider frames empty if they only contain tags.
            fn is_empty_frame(frame: &TermFrame) -> bool {
                // In terminal, we consider a frame empty if it has no items.
                frame.is_empty_items()
            }

            // Skip the first region if one cell in it is empty. Then,
            // remeasure.
            if let [first, ..] = frames.as_slice()
                && can_skip
                && breakable
                && is_empty_frame(first)
                && frames.iter().skip(1).any(|frame| !is_empty_frame(frame))
            {
                return Ok(None);
            }

            // Skip frames from previous regions if applicable.
            let mut sizes = frames
                .iter()
                .skip(measurement_data.frames_in_previous_regions)
                .map(|frame| frame.height())
                .collect::<Vec<_>>();

            if rowspan > 1 {
                let should_simulate = self.prepare_rowspan_sizes(
                    y,
                    &mut sizes,
                    cell,
                    parent.y,
                    rowspan,
                    unbreakable_rows_left,
                    &measurement_data,
                );

                if should_simulate {
                    pending_rowspans.push((parent.y, rowspan, sizes));
                    continue;
                }
            }

            let mut sizes = sizes.into_iter();

            for (target, size) in resolved.iter_mut().zip(&mut sizes) {
                target.set_max(size);
            }

            resolved.extend(sizes);
        }

        // Simulate the upcoming regions in order to predict how much we need
        // to expand this auto row for rowspans which span gutter.
        if !pending_rowspans.is_empty() {
            self.simulate_and_measure_rowspans_in_auto_row(
                y,
                &mut resolved,
                &pending_rowspans,
                unbreakable_rows_left,
                row_group_data,
                disambiguator,
                engine,
            )?;
        }

        debug_assert!(breakable || resolved.len() <= 1);

        Ok(Some(resolved))
    }

    /// Layout a row with relative height. Such a row cannot break across
    /// multiple regions, but it may force a region break.
    fn layout_relative_row(
        &mut self,
        engine: &mut Engine,
        disambiguator: usize,
        v: Rel<Length>,
        y: usize,
    ) -> SourceResult<()> {
        let resolved = resolve_rel(&v, self.regions.base().rows, self.styles);
        let frame = self.layout_single_row(engine, disambiguator, resolved, y)?;

        if let Some(row_height) = &mut self.row_state.current_row_height {
            // Add to header height, as we are in a header row.
            *row_height = *row_height + resolved;
        }

        // Skip to fitting region, but only if we aren't part of an unbreakable
        // row group. We use 'may_progress_with_repeats' to stop trying if we
        // would skip to a region with the same height and where the same
        // headers would be repeated.
        let height = frame.height();
        while self.unbreakable_rows_left == 0
            && !self.regions.size.rows.fits(height)
            && self.may_progress_with_repeats()
        {
            self.finish_region(engine, false)?;

            // Don't skip multiple regions for gutter and don't push a row.
            if self.grid.is_gutter_track(y) {
                return Ok(());
            }
        }

        self.push_row(frame, y, true);

        Ok(())
    }

    /// Layout a row with fixed height and return its frame.
    fn layout_single_row(
        &mut self,
        engine: &mut Engine,
        disambiguator: usize,
        height: TermScalar,
        y: usize,
    ) -> SourceResult<TermFrame> {
        if !self.width.is_finite() {
            bail!(self.span, "cannot create grid with infinite width");
        }

        if !height.is_finite() {
            bail!(self.span, "cannot create grid with infinite height");
        }

        // Frame includes one extra column per content column for vlines,
        // plus one final vline.
        // Count content columns (not gutters) for vline calculation.
        let content_count = (0..self.grid.cols.len())
            .filter(|&x| !self.grid.is_gutter_track(x))
            .count();
        let vline_count = content_count + 1;
        let frame_width = self.width + TermScalar::new(vline_count as i32);
        // +1 row for the top hline border.
        let frame_height = height + TermScalar::ONE;
        let mut output = TermFrame::soft(TermSize::new(frame_width, frame_height));
        let mut offset = TermPoint::ZERO;

        for (x, &rcol) in self.rcols.iter().enumerate() {
            let is_gutter = self.grid.is_gutter_track(x);
            if let Some(cell) = self.grid.cell(x, y) {
                // Rowspans have a separate layout step
                if cell.rowspan.get() == 1 {
                    let width = self.cell_spanned_width(cell, x);
                    let size = TermSize::new(width, height);
                    let mut pod: TermRegions = TermRegion::new(size, Axes::splat(true)).into();
                    if self.grid.rows[y] == Sizing::Auto && self.unbreakable_rows_left == 0 {
                        pod.full = self.regions.full;
                    }
                    let locator = self.cell_locator(Axes::new(x, y), disambiguator);
                    let frame = layout_cell(
                        cell,
                        engine,
                        locator,
                        self.styles,
                        pod,
                        self.row_state.is_being_repeated,
                    )?
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| TermFrame::soft(TermSize::new(width, height)));
                    // Content columns: vline before cell at offset.col,
                    // cell at offset.col + 1, offset.row + 1.
                    let vline_pad = if is_gutter { TermScalar::ZERO } else { TermScalar::ONE };
                    let mut pos = TermPoint::new(
                        offset.col + vline_pad,
                        offset.row + TermScalar::ONE,
                    );
                    if self.is_rtl {
                        pos.col = frame_width - (pos.col + width);
                    }
                    output.push_frame(pos, frame);
                }
            }

            // Gutter columns don't add a vline column.
            if is_gutter {
                offset.col = offset.col + rcol;
            } else {
                offset.col = offset.col + rcol + TermScalar::ONE;
            }
        }

        Ok(output)
    }

    /// Layout a row spanning multiple regions.
    fn layout_multi_row(
        &mut self,
        engine: &mut Engine,
        disambiguator: usize,
        heights: &[TermScalar],
        y: usize,
    ) -> SourceResult<TermFragment> {
        // Prepare frames.
        let frame_width = self.width + TermScalar::new(self.grid.cols.len() as i32 + 1);
        let mut outputs: Vec<_> = heights
            .iter()
            .map(|&h| TermFrame::soft(TermSize::new(frame_width, h)))
            .collect();

        // Prepare regions.
        let size = TermSize::new(self.width, heights[0]);
        let mut pod: TermRegions = TermRegion::new(size, Axes::splat(true)).into();
        pod.full = self.regions.full;
        pod.backlog = heights[1..].to_vec();

        // Layout the row.
        let mut offset = TermPoint::ZERO;
        for (x, &rcol) in self.rcols.iter().enumerate() {
            if let Some(cell) = self.grid.cell(x, y) {
                // Rowspans have a separate layout step
                if cell.rowspan.get() == 1 {
                    let width = self.cell_spanned_width(cell, x);
                    pod.size.cols = width;

                    // Push the layouted frames into the individual output frames.
                    let locator = self.cell_locator(Axes::new(x, y), disambiguator);
                    let fragment = layout_cell(
                        cell,
                        engine,
                        locator,
                        self.styles,
                        pod.clone(),
                        self.row_state.is_being_repeated,
                    )?;
                    for (output, frame) in outputs.iter_mut().zip(fragment) {
                        let mut pos = offset;
                        if self.is_rtl {
                            // In RTL cells expand to the left, thus the
                            // position must additionally be offset by the
                            // cell's width.
                            pos.col = frame_width - (offset.col + width);
                        }
                        output.push_frame(pos, frame);
                    }
                }
            }

            offset.col = offset.col + rcol;
        }

        Ok(outputs)
    }

    /// Push a row frame into the current region.
    /// The `is_last` parameter must be `true` if this is the last frame which
    /// will be pushed for this particular row. It can be `false` for rows
    /// spanning multiple regions.
    fn push_row(&mut self, frame: TermFrame, y: usize, is_last: bool) {
        if !self.row_state.in_active_repeatable {
            // There is now a row after the rows equipped with orphan
            // prevention, so no need to keep moving them anymore.
            self.flush_orphans();
        }
        self.regions.size.rows -= frame.height();
        self.current.lrows.push(Row::Frame(frame, y, is_last));
    }

    /// Finish rows for one region.
    pub(super) fn finish_region(&mut self, engine: &mut Engine, last: bool) -> SourceResult<()> {
        // The latest rows have orphan prevention (headers) and no other rows
        // were placed, so remove those rows and try again in a new region,
        // unless this is the last region.
        if let Some(orphan_snapshot) = self.current.lrows_orphan_snapshot.take()
            && !last
        {
            self.current.lrows.truncate(orphan_snapshot);
            self.current.repeated_header_rows =
                self.current.repeated_header_rows.min(orphan_snapshot);

            if orphan_snapshot == 0 {
                // Removed all repeated headers.
                self.current.last_repeated_header_end = 0;
            }
        }

        if self
            .current
            .lrows
            .last()
            .is_some_and(|row| self.grid.is_gutter_track(row.index()))
        {
            // Remove the last row in the region if it is a gutter row.
            self.current.lrows.pop().unwrap();
            self.current.repeated_header_rows = self
                .current
                .repeated_header_rows
                .min(self.current.lrows.len());
        }

        // If no rows other than the footer have been laid out so far
        // (e.g. due to header orphan prevention), and there are rows
        // beside the footer, then don't lay it out at all.
        let footer_would_be_widow = matches!(&self.grid.footer, Some(footer) if footer.repeated)
            && self.current.lrows.is_empty()
            && self.current.could_progress_at_top;

        let mut laid_out_footer_start = None;
        if !footer_would_be_widow && let Some(footer) = &self.grid.footer {
            if footer.repeated
                && self
                    .current
                    .lrows
                    .iter()
                    .all(|row| row.index() < footer.start)
            {
                laid_out_footer_start = Some(footer.start);
                self.layout_footer(footer, engine, self.finished.len(), true)?;
            }
        }

        // Determine the height of existing rows in the region.
        let mut used = TermScalar::ZERO;
        let mut fr = Fr::zero();
        for row in &self.current.lrows {
            match row {
                Row::Frame(frame, _, _) => used = used + frame.height(),
                Row::Fr(v, _, _) => fr += *v,
            }
        }

        // Determine the size of the grid in this region, expanding fully if
        // there are fr rows.
        let frame_width = self.width + TermScalar::new(self.grid.cols.len() as i32 + 1);
        let mut size = TermSize::new(frame_width, used).min(self.current.initial);
        if fr.get() > 0.0 && self.current.initial.rows.is_finite() {
            size.rows = self.current.initial.rows;
        }

        // The frame for the region.
        let mut output = TermFrame::soft(size);
        let mut pos = TermPoint::ZERO;
        let mut rrows = vec![];
        let current_region = self.finished.len();
        let mut repeated_header_row_height = TermScalar::ZERO;

        // Place finished rows and layout fractional rows.
        for (i, row) in std::mem::take(&mut self.current.lrows)
            .into_iter()
            .enumerate()
        {
            let (frame, y, is_last) = match row {
                Row::Frame(frame, y, is_last) => (frame, y, is_last),
                Row::Fr(v, y, disambiguator) => {
                    let remaining = self.regions.full - used;
                    let height = fr_share(v, fr, remaining);
                    (
                        self.layout_single_row(engine, disambiguator, height, y)?,
                        y,
                        true,
                    )
                }
            };

            let height = frame.height();
            if i < self.current.repeated_header_rows {
                repeated_header_row_height = repeated_header_row_height + height;
            }

            // Ensure rowspans which span this row will have enough space to
            // be laid out over it later.
            for rowspan in self
                .rowspans
                .iter_mut()
                .filter(|rowspan| (rowspan.y..rowspan.y + rowspan.rowspan).contains(&y))
                .filter(|rowspan| rowspan.max_resolved_row.is_none_or(|max_row| y > max_row))
            {
                // If the first region wasn't defined yet, it will have the
                // initial value of usize::MAX, so we can set it to the current
                // region's index.
                if rowspan.first_region > current_region {
                    rowspan.first_region = current_region;
                    // The rowspan starts at this region, precisely at this
                    // row. In other regions, it will start at dy = 0.
                    rowspan.dy = pos.row;
                    // When we layout the rowspan later, the full size of the
                    // pod must be equal to the full size of the first region
                    // it appears in.
                    rowspan.region_full = self.regions.full;
                }
                let amount_missing_heights = (current_region + 1)
                    .saturating_sub(rowspan.heights.len() + rowspan.first_region);

                // Ensure the vector of heights is long enough such that the
                // last height is the one for the current region.
                rowspan.heights.extend(std::iter::repeat_n(
                    TermScalar::ZERO,
                    amount_missing_heights,
                ));

                // Ensure that, in this region, the rowspan will span at least
                // this row.
                *rowspan.heights.last_mut().unwrap() = *rowspan.heights.last().unwrap() + height;

                if is_last {
                    // Do not extend the rowspan through this row again, even
                    // if it is repeated in a future region.
                    rowspan.max_resolved_row = Some(y);
                }
            }

            // We use a for loop over indices to avoid borrow checking
            // problems (we need to mutate the rowspans vector, so we can't
            // have an iterator actively borrowing it). We keep a separate
            // 'i' variable so we can step the counter back after removing
            // a rowspan (see explanation below).
            let mut i = 0;
            while let Some(rowspan) = self.rowspans.get(i) {
                // Layout any rowspans which end at this row, but only if this is
                // this row's last frame (to avoid having the rowspan stop being
                // laid out at the first frame of the row).
                // Any rowspans ending before this row are laid out even
                // on this row's first frame.
                if laid_out_footer_start.is_none_or(|footer_start| {
                    // If this is a footer row, then only lay out this rowspan
                    // if the rowspan is contained within the footer.
                    y < footer_start || rowspan.y >= footer_start
                }) && (rowspan.y + rowspan.rowspan < y + 1
                    || rowspan.y + rowspan.rowspan == y + 1 && is_last)
                {
                    // Rowspan ends at this or an earlier row, so we take
                    // it from the rowspans vector and lay it out.
                    let rowspan = self.rowspans.remove(i);
                    self.layout_rowspan(
                        rowspan,
                        Some((&mut output, repeated_header_row_height)),
                        engine,
                    )?;
                } else {
                    i += 1;
                }
            }

            output.push_frame(pos, frame);
            rrows.push(RowPiece { height, y });
            pos.row = pos.row + height;
        }

        self.finish_region_internal(
            output,
            rrows,
            FinishedHeaderRowInfo {
                repeated_amount: self.current.repeated_header_rows,
                last_repeated_header_end: self.current.last_repeated_header_end,
                repeated_height: repeated_header_row_height,
            },
        );

        if !last {
            self.current.repeated_header_rows = 0;
            self.current.last_repeated_header_end = 0;
            self.current.repeating_header_height = TermScalar::ZERO;
            self.current.repeating_header_heights.clear();

            let disambiguator = self.finished.len();
            if let Some(footer) = self.grid.footer.as_ref().and_then(Repeatable::as_repeated) {
                self.prepare_footer(footer, engine, disambiguator)?;
            }

            // Ensure rows don't try to overrun the footer.
            self.regions.size.rows -= self.current.footer_height;
            self.current.initial_after_repeats = self.regions.size.rows;

            if !self.repeating_headers.is_empty() || !self.pending_headers.is_empty() {
                // Add headers to the new region.
                self.layout_active_headers(engine)?;
            }
        }

        Ok(())
    }

    /// Advances to the next region, registering the finished output and
    /// resolved rows for the current region in the appropriate vectors.
    pub(super) fn finish_region_internal(
        &mut self,
        output: TermFrame,
        resolved_rows: Vec<RowPiece>,
        header_row_info: FinishedHeaderRowInfo,
    ) {
        self.finished.push(output);
        self.rrows.push(resolved_rows);
        self.regions.next();
        self.current.initial = self.regions.size;

        // Repeats haven't been laid out yet, so in the meantime, this will
        // represent the initial height after repeats laid out so far, and will
        // be gradually updated when preparing footers and repeating headers.
        self.current.initial_after_repeats = self.current.initial.rows;

        self.current.could_progress_at_top = self.regions.may_progress();

        if !self.grid.headers.is_empty() {
            self.finished_header_rows.push(header_row_info);
        }

        // Ensure orphan prevention is handled before resolving rows.
        debug_assert!(self.current.lrows_orphan_snapshot.is_none());
    }
}

/// Turn an iterator of extents into an iterator of offsets before, in between,
/// and after the extents, e.g. [10mm, 5mm] -> [0mm, 10mm, 15mm].
pub(super) fn points(
    extents: impl IntoIterator<Item = TermScalar>,
) -> impl Iterator<Item = TermScalar> {
    let mut offset = TermScalar::ZERO;
    std::iter::once(TermScalar::ZERO)
        .chain(extents)
        .map(move |extent| {
            offset = offset + extent;
            offset
        })
}
