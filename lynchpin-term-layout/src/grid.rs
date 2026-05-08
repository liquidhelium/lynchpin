//! Grid / table terminal layout.
//!
//! Mirrors the paged layout in `lynchpin-layout/src/grid/`:
//! 1. Auto-position cells.
//! 2. Measure auto column content widths.
//! 3. Resolve column tracks via `lynchpin_library::units::resolve_tracks`.
//! 4. Re-layout cells with resolved column widths.
//! 5. Measure row heights.
//! 6. Assemble into a bordered (table) or borderless (grid) [`TermFrame`].

use crossterm::style::ContentStyle;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, Packed, Smart, StyleChain};
use typst::layout::{GridElem, Sizing, TrackSizings};
use typst::model::TableElem;

use lynchpin_library::frame::{Col, Row, TermFrame, TermPoint, TermSize};
use lynchpin_library::units;

use crate::config::TermConfig;
use crate::inline::layout_paragraph;

/// A laid-out cell ready for grid assembly.
struct CellInfo {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    body: Content,   // original cell body for re-layout
    frame: TermFrame,
}

// ── Shared engine ────────────────────────────────────────────────────────────

fn layout_grid_impl(
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
    columns: &TrackSizings,
    _rows: &TrackSizings,
    cells: Vec<(Content, Smart<usize>, Smart<usize>, usize, usize)>,
    has_borders: bool,
) -> SourceResult<TermFrame> {
    let declared_cols = columns.0.len().max(1);
    let avail_width = config.width.unwrap_or(80) as Col;

    // ── Step 1: auto-position cells ──────────────────────────────────────
    let mut placed: Vec<CellInfo> = Vec::new();
    let mut auto_x: usize = 0;
    let mut auto_y: usize = 0;

    for (body, x_smart, y_smart, colspan, _rowspan) in cells {
        let (x, y) = match (x_smart, y_smart) {
            (Smart::Custom(x), Smart::Custom(y)) => (x, y),
            (Smart::Custom(x), _) => {
                let y_pos = placed.iter().filter(|c| c.x == x)
                    .map(|c| c.y + c.h).max().unwrap_or(0);
                (x, y_pos)
            }
            (_, Smart::Custom(y)) => (y, y),
            _ => {
                let pos = (auto_x, auto_y);
                auto_x += colspan.max(1);
                if auto_x >= declared_cols { auto_x = 0; auto_y += 1; }
                pos
            }
        };

        // Measure with available width so wrapping is accounted for.
        let max_w = if columns.0.get(x).is_some_and(|s| *s == Sizing::Auto) {
            // Auto columns: use generous width for natural measurement.
            Some(avail_width)
        } else {
            config.width.map(|w| w as Col)
        };
        let meas_config = TermConfig { width: max_w, ..*config };
        let frame = layout_paragraph(engine, &body, &meas_config, styles, ContentStyle::default())?;

        placed.push(CellInfo {
            x, y,
            w: colspan.max(1),
            h: frame.rows().max(1) as usize,
            body: body.clone(),
            frame,
        });
    }

    if placed.is_empty() {
        return Ok(TermFrame::new(TermSize::ZERO));
    }

    // ── Step 2: grid extent ──────────────────────────────────────────────
    let ncols = declared_cols.max(placed.iter().map(|c| c.x + c.w).max().unwrap_or(1));
    let nrows = placed.iter().map(|c| c.y + c.h).max().unwrap_or(1);

    // ── Step 3: measure auto column widths ───────────────────────────────
    // Replicate paged `measure_auto_columns` logic.
    let mut auto_sizes = vec![0; ncols];
    for cell in &placed {
        if cell.w == 1 {
            let col_w = cell.frame.cols() as Col;
            auto_sizes[cell.x] = auto_sizes[cell.x].max(col_w);
        }
    }
    // Multi-column cells: add extra width to the last spanned auto column.
    for cell in &placed {
        if cell.w > 1 {
            let used: Col = auto_sizes[cell.x..cell.x + cell.w].iter().sum();
            let need = cell.frame.cols() as Col;
            if need > used {
                // Find the last non-Rel column to add extra width.
                for k in (cell.x..cell.x + cell.w).rev() {
                    let s = columns.0.get(k);
                    if s.is_some_and(|s| !matches!(s, Sizing::Rel(_))) {
                        auto_sizes[k] += need - used;
                        break;
                    }
                }
            }
        }
    }

    // ── Step 4: resolve column tracks ────────────────────────────────────
    let col_widths = units::resolve_tracks(columns, avail_width, &auto_sizes, styles);

    // ── Step 5: re-layout cells with resolved widths ─────────────────────
    for cell in &mut placed {
        let cell_w = spanned_total(&col_widths, cell.x, cell.w);
        let re_config = TermConfig {
            width: Some(cell_w as i32),
            ..*config
        };
        let frame = layout_paragraph(engine, &cell.body, &re_config, styles, ContentStyle::default())?;
        let rows = frame.rows().max(1);
        cell.frame = frame;
        cell.h = rows as usize;
    }

    // ── Step 6: compute row heights ──────────────────────────────────────
    let mut row_heights: Vec<Row> = vec![1; nrows];
    for cell in &placed {
        if cell.h == 1 {
            row_heights[cell.y] = row_heights[cell.y].max(cell.frame.rows());
        }
    }
    for cell in &placed {
        if cell.h > 1 {
            let used: Row = row_heights[cell.y..cell.y + cell.h].iter().sum();
            let need = cell.frame.rows();
            if need > used { row_heights[cell.y + cell.h - 1] += need - used; }
        }
    }

    // ── Step 7: build frame ──────────────────────────────────────────────
    if has_borders {
        build_bordered(&placed, &col_widths, &row_heights, ncols, nrows, config)
    } else {
        build_borderless(&placed, &col_widths, &row_heights, ncols, nrows)
    }
}

// ── Bordered layout (tables) ─────────────────────────────────────────────────

fn build_bordered(
    placed: &[CellInfo],
    col_widths: &[Col],
    row_heights: &[Row],
    ncols: usize,
    nrows: usize,
    config: &TermConfig,
) -> SourceResult<TermFrame> {
    let is_unicode = config.mode.is_unicode();
    let (h, v, tl, tm, tr, ml, mm, mr, bl, bm, br): (char, char, char, char, char, char, char, char, char, char, char) =
        if is_unicode {
            ('─', '│', '┌', '┬', '┐', '├', '┼', '┤', '└', '┴', '┘')
        } else {
            ('-', '|', '+', '+', '+', '+', '+', '+', '+', '+', '+')
        };

    let total_cols: Col = col_widths.iter().sum::<Col>() + (ncols as Col + 1);
    let total_rows: Row = row_heights.iter().sum::<Row>() + (nrows as Row + 1);
    let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));

    // Top border.
    let mut x: Col = 0;
    for ci in 0..ncols {
        let ch = if ci == 0 { tl } else { tm };
        frame.push_text(TermPoint::new(x, 0), ch.to_string(), ContentStyle::default());
        x += 1;
        frame.push_text(TermPoint::new(x, 0), rep(h, col_widths[ci]), ContentStyle::default());
        x += col_widths[ci];
    }
    frame.push_text(TermPoint::new(x, 0), tr.to_string(), ContentStyle::default());

    let mut row_top: Row = 1;
    for ri in 0..nrows {
        let rh = row_heights[ri];

        // Place cells.
        for cell in placed {
            if cell.y == ri {
                let cx = cell_left(col_widths, cell.x, true);
                let cw = spanned_total(col_widths, cell.x, cell.w);
                let cx = cx + ((cw - cell.frame.cols()).max(0) / 2);
                let cy = row_top + ((rh as Row - cell.frame.rows()).max(0) / 2);
                frame.push_frame(TermPoint::new(cx, cy), cell.frame.clone());
            }
        }

        // Vertical lines.
        for r in 0..rh {
            let yr = row_top + r;
            let mut x: Col = 0;
            for ci in 0..ncols {
                frame.push_text(TermPoint::new(x, yr), v.to_string(), ContentStyle::default());
                x += 1 + col_widths[ci];
            }
            frame.push_text(TermPoint::new(x, yr), v.to_string(), ContentStyle::default());
        }

        row_top += rh;

        // Row separator.
        if ri + 1 < nrows {
            let mut x: Col = 0;
            for ci in 0..ncols {
                let ch = if ci == 0 { ml } else { mm };
                frame.push_text(TermPoint::new(x, row_top), ch.to_string(), ContentStyle::default());
                x += 1;
                frame.push_text(TermPoint::new(x, row_top), rep(h, col_widths[ci]), ContentStyle::default());
                x += col_widths[ci];
            }
            frame.push_text(TermPoint::new(x, row_top), mr.to_string(), ContentStyle::default());
            row_top += 1;
        }
    }

    // Bottom border.
    let mut x: Col = 0;
    for ci in 0..ncols {
        let ch = if ci == 0 { bl } else { bm };
        frame.push_text(TermPoint::new(x, row_top), ch.to_string(), ContentStyle::default());
        x += 1;
        frame.push_text(TermPoint::new(x, row_top), rep(h, col_widths[ci]), ContentStyle::default());
        x += col_widths[ci];
    }
    frame.push_text(TermPoint::new(x, row_top), br.to_string(), ContentStyle::default());

    Ok(frame)
}

// ── Borderless layout (grids) ────────────────────────────────────────────────

fn build_borderless(
    placed: &[CellInfo],
    col_widths: &[Col],
    row_heights: &[Row],
    ncols: usize,
    nrows: usize,
) -> SourceResult<TermFrame> {
    let gap: Col = 1;
    let total_cols: Col = col_widths.iter().sum::<Col>() + gap * (ncols.saturating_sub(1)) as Col;
    let total_rows: Row = row_heights.iter().sum::<Row>();
    let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));

    let mut row_top: Row = 0;
    for ri in 0..nrows {
        let rh = row_heights[ri];
        for cell in placed {
            if cell.y == ri {
                let cx = cell_left(col_widths, cell.x, false);
                let cw = spanned_total(col_widths, cell.x, cell.w);
                let cx = cx + ((cw - cell.frame.cols()).max(0) / 2);
                let cy = row_top + ((rh as Row - cell.frame.rows()).max(0) / 2);
                frame.push_frame(TermPoint::new(cx, cy), cell.frame.clone());
            }
        }
        row_top += rh;
    }

    Ok(frame)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn cell_left(col_widths: &[Col], x: usize, bordered: bool) -> Col {
    let mut pos: Col = if bordered { 1 } else { 0 };
    for ci in 0..x { pos += col_widths[ci] + 1; }
    pos
}

fn spanned_total(col_widths: &[Col], x: usize, w: usize) -> Col {
    col_widths[x..x + w].iter().sum::<Col>() + (w.saturating_sub(1)) as Col
}

fn rep(ch: char, n: Col) -> String {
    std::iter::repeat(ch).take(n as usize).collect()
}

// ── Public API ───────────────────────────────────────────────────────────────

pub fn layout_grid(
    elem: &Packed<GridElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    use typst::layout::{GridChild, GridItem};

    let columns = elem.columns.get_cloned(styles);
    let rows = elem.rows.get_cloned(styles);
    let mut cells = Vec::new();

    for child in &elem.children {
        if let GridChild::Item(item) = child {
            if let GridItem::Cell(cell) = item {
                let x = cell.x.get(styles);
                let y = cell.y.get(styles);
                let colspan = cell.colspan.get(styles).get() as usize;
                let rowspan = cell.rowspan.get(styles).get() as usize;
                cells.push((cell.body.clone(), x, y, colspan, rowspan));
            }
        }
    }

    layout_grid_impl(engine, config, styles, &columns, &rows, cells, false)
}

pub fn layout_table(
    elem: &Packed<TableElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    use typst::model::{TableChild, TableItem};

    let columns = elem.columns.get_cloned(styles);
    let rows = elem.rows.get_cloned(styles);
    let mut cells = Vec::new();

    for child in &elem.children {
        if let TableChild::Item(item) = child {
            if let TableItem::Cell(cell) = item {
                let x = cell.x.get(styles);
                let y = cell.y.get(styles);
                let colspan = cell.colspan.get(styles).get() as usize;
                let rowspan = cell.rowspan.get(styles).get() as usize;
                cells.push((cell.body.clone(), x, y, colspan, rowspan));
            }
        }
    }

    layout_grid_impl(engine, config, styles, &columns, &rows, cells, true)
}
