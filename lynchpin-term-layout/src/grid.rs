//! Grid / table terminal layout.
//!
//! Processes `GridElem` / `TableElem` children directly (since we skip
//! built-in show rules, the `CellGrid` synthesize step does not run).
//!
//! Key differences from the paged layout:
//! - No page breaking / headers / footers.
//! - Grid has no default stroke (no borders drawn).
//! - Table has default stroke (box-drawing borders drawn).
//! - Cell-level stroke overrides are not yet supported.
//! - Track sizings are auto-sized from content.

use crossterm::style::ContentStyle;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, Packed, Smart, StyleChain};
use typst::layout::GridElem;
use typst::model::TableElem;

use crate::config::TermConfig;
use crate::frame::{Col, Row, TermFrame, TermPoint, TermSize};
use crate::inline::layout_paragraph;

/// A laid-out cell ready for grid assembly.
struct CellInfo {
    x: usize,
    y: usize,
    w: usize, // colspan
    h: usize, // rowspan
    frame: TermFrame,
}

// ── Shared layout engine ─────────────────────────────────────────────────────

fn layout_cells<I>(
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
    cells: I,
    declared_cols: usize,
    has_borders: bool,
) -> SourceResult<TermFrame>
where
    I: IntoIterator<Item = (Content, Smart<usize>, Smart<usize>, usize, usize)>,
{
    // ── Step 1: auto-position cells ──────────────────────────────────────
    let mut placed: Vec<CellInfo> = Vec::new();
    let mut auto_x: usize = 0;
    let mut auto_y: usize = 0;
    let wrap_at = declared_cols.max(1);

    for (body, x_smart, y_smart, colspan, _rowspan) in cells {
        let (x, y) = match (x_smart, y_smart) {
            (Smart::Custom(x), Smart::Custom(y)) => (x, y),
            (Smart::Custom(x), _) => {
                let y_pos = placed
                    .iter()
                    .filter(|c| c.x == x)
                    .map(|c| c.y + c.h)
                    .max()
                    .unwrap_or(0);
                (x, y_pos)
            }
            (_, Smart::Custom(y)) => (y, y),
            _ => {
                let pos = (auto_x, auto_y);
                auto_x += colspan;
                if auto_x >= wrap_at {
                    auto_x = 0;
                    auto_y += 1;
                }
                pos
            }
        };

        let frame = layout_paragraph(engine, &body, config, styles, ContentStyle::default())?;
        placed.push(CellInfo {
            x,
            y,
            w: colspan.max(1),
            h: frame.rows().max(1) as usize,
            frame,
        });
    }

    if placed.is_empty() {
        return Ok(TermFrame::new(TermSize::ZERO));
    }

    // ── Step 2: determine grid extent ────────────────────────────────────
    let ncols = declared_cols.max(placed.iter().map(|c| c.x + c.w).max().unwrap_or(1));
    let nrows = placed.iter().map(|c| c.y + c.h).max().unwrap_or(1);

    // ── Step 3: compute column widths ─────────────────────────────────────
    let mut col_widths: Vec<Col> = vec![0; ncols];
    for cell in &placed {
        if cell.w == 1 {
            col_widths[cell.x] = col_widths[cell.x].max(cell.frame.cols() as Col);
        }
    }
    for cell in &placed {
        if cell.w > 1 {
            let used: Col = col_widths[cell.x..cell.x + cell.w].iter().sum();
            let need = cell.frame.cols() as Col;
            if need > used {
                col_widths[cell.x + cell.w - 1] += need - used;
            }
        }
    }
    for w in &mut col_widths {
        *w = (*w).max(1);
    }

    // ── Step 4: compute row heights ───────────────────────────────────────
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
            if need > used {
                row_heights[cell.y + cell.h - 1] += need - used;
            }
        }
    }

    // ── Step 5: build the frame ───────────────────────────────────────────
    if has_borders {
        build_bordered_frame(&placed, &col_widths, &row_heights, ncols, nrows, config)
    } else {
        build_borderless_frame(&placed, &col_widths, &row_heights, ncols, nrows)
    }
}

// ── Bordered layout (tables) ─────────────────────────────────────────────────

fn build_bordered_frame(
    placed: &[CellInfo],
    col_widths: &[Col],
    row_heights: &[Row],
    ncols: usize,
    nrows: usize,
    config: &TermConfig,
) -> SourceResult<TermFrame> {
    let is_unicode = config.mode.is_unicode();
    let (h, v, tl, tm, tr, ml, mm, mr, bl, bm, br): (
        char,
        char,
        char,
        char,
        char,
        char,
        char,
        char,
        char,
        char,
        char,
    ) = if is_unicode {
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
        frame.push_text(
            TermPoint::new(x, 0),
            ch.to_string(),
            ContentStyle::default(),
        );
        x += 1;
        frame.push_text(
            TermPoint::new(x, 0),
            repeat_char(h, col_widths[ci] as usize),
            ContentStyle::default(),
        );
        x += col_widths[ci];
    }
    frame.push_text(
        TermPoint::new(x, 0),
        tr.to_string(),
        ContentStyle::default(),
    );

    let mut row_top: Row = 1;
    for ri in 0..nrows {
        let rh = row_heights[ri];

        // Place cells.
        for cell in placed {
            if cell.y == ri {
                let cell_x = cell_left(col_widths, cell.x, true);
                let cell_w = spanned_width(col_widths, cell.x, cell.w);
                let cx = cell_x + ((cell_w - cell.frame.cols()).max(0) / 2);
                let cy = row_top + ((rh as Row - cell.frame.rows()).max(0) / 2);
                frame.push_frame(TermPoint::new(cx, cy), cell.frame.clone());
            }
        }

        // Vertical lines.
        for r in 0..rh {
            let yr = row_top + r;
            let mut x: Col = 0;
            for ci in 0..ncols {
                frame.push_text(
                    TermPoint::new(x, yr),
                    v.to_string(),
                    ContentStyle::default(),
                );
                x += 1 + col_widths[ci];
            }
            frame.push_text(
                TermPoint::new(x, yr),
                v.to_string(),
                ContentStyle::default(),
            );
        }

        row_top += rh;

        // Row separator or bottom border.
        if ri + 1 < nrows {
            x = 0;
            for ci in 0..ncols {
                let ch = if ci == 0 { ml } else { mm };
                frame.push_text(
                    TermPoint::new(x, row_top),
                    ch.to_string(),
                    ContentStyle::default(),
                );
                x += 1;
                frame.push_text(
                    TermPoint::new(x, row_top),
                    repeat_char(h, col_widths[ci] as usize),
                    ContentStyle::default(),
                );
                x += col_widths[ci];
            }
            frame.push_text(
                TermPoint::new(x, row_top),
                mr.to_string(),
                ContentStyle::default(),
            );
            row_top += 1;
        }
    }

    // Bottom border.
    x = 0;
    for ci in 0..ncols {
        let ch = if ci == 0 { bl } else { bm };
        frame.push_text(
            TermPoint::new(x, row_top),
            ch.to_string(),
            ContentStyle::default(),
        );
        x += 1;
        frame.push_text(
            TermPoint::new(x, row_top),
            repeat_char(h, col_widths[ci] as usize),
            ContentStyle::default(),
        );
        x += col_widths[ci];
    }
    frame.push_text(
        TermPoint::new(x, row_top),
        br.to_string(),
        ContentStyle::default(),
    );

    Ok(frame)
}

// ── Borderless layout (grids) ────────────────────────────────────────────────

fn build_borderless_frame(
    placed: &[CellInfo],
    col_widths: &[Col],
    row_heights: &[Row],
    ncols: usize,
    nrows: usize,
) -> SourceResult<TermFrame> {
    // 1 space gap between columns, 0 row gap.
    let gap: Col = 1;
    let total_cols: Col = col_widths.iter().sum::<Col>() + gap * (ncols.saturating_sub(1)) as Col;
    let total_rows: Row = row_heights.iter().sum::<Row>();
    let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));

    let mut row_top: Row = 0;
    for ri in 0..nrows {
        let rh = row_heights[ri];

        for cell in placed {
            if cell.y == ri {
                let cell_x = cell_left(col_widths, cell.x, false);
                let cell_w = spanned_width(col_widths, cell.x, cell.w);
                let cx = cell_x + ((cell_w - cell.frame.cols()).max(0) / 2);
                let cy = row_top + ((rh as Row - cell.frame.rows()).max(0) / 2);
                frame.push_frame(TermPoint::new(cx, cy), cell.frame.clone());
            }
        }
        row_top += rh;
    }

    Ok(frame)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Compute the left column position for a cell at column `x`.
fn cell_left(col_widths: &[Col], x: usize, bordered: bool) -> Col {
    // Each column is separated by 1 cell (either a border '│' or a space).
    let mut pos: Col = if bordered { 1 } else { 0 };
    for ci in 0..x {
        pos += col_widths[ci] + 1;
    }
    pos
}

/// Compute the total width spanned by a cell covering `w` columns starting at `x`.
fn spanned_width(col_widths: &[Col], x: usize, w: usize) -> Col {
    // Content width plus the 1-cell gaps between spanned columns.
    col_widths[x..x + w].iter().sum::<Col>() + (w.saturating_sub(1)) as Col
}

fn repeat_char(ch: char, n: usize) -> String {
    std::iter::repeat(ch).take(n).collect()
}

// ── Grid ──────────────────────────────────────────────────────────────────────

pub fn layout_grid(
    elem: &Packed<GridElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    use typst::layout::{GridChild, GridItem};

    let mut cells = Vec::new();
    for child in &elem.children {
        match child {
            GridChild::Item(item) => {
                if let GridItem::Cell(cell) = item {
                    let x = cell.x.get(styles);
                    let y = cell.y.get(styles);
                    let colspan = cell.colspan.get(styles).get() as usize;
                    let rowspan = cell.rowspan.get(styles).get() as usize;
                    cells.push((cell.body.clone(), x, y, colspan, rowspan));
                }
            }
            _ => {}
        }
    }

    let ncols = elem.columns.get_cloned(styles).0.len().max(1);
    // Grid: no borders by default.
    layout_cells(engine, config, styles, cells, ncols, false)
}

// ── Table ─────────────────────────────────────────────────────────────────────

pub fn layout_table(
    elem: &Packed<TableElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    use typst::model::{TableChild, TableItem};

    let mut cells = Vec::new();
    for child in &elem.children {
        match child {
            TableChild::Item(item) => {
                if let TableItem::Cell(cell) = item {
                    let x = cell.x.get(styles);
                    let y = cell.y.get(styles);
                    let colspan = cell.colspan.get(styles).get() as usize;
                    let rowspan = cell.rowspan.get(styles).get() as usize;
                    cells.push((cell.body.clone(), x, y, colspan, rowspan));
                }
            }
            _ => {}
        }
    }

    let ncols = elem.columns.get_cloned(styles).0.len().max(1);
    // Table: borders by default.
    layout_cells(engine, config, styles, cells, ncols, true)
}
