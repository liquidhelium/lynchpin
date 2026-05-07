//! Grid / table layout (stub).
//!
//! For now these functions return a placeholder `[table]` / `[grid]` frame.
//! A full implementation would iterate over cells, measure each one, then
//! draw box-drawing borders (─ ─ │ ┼ …).

use crossterm::style::ContentStyle;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Packed, StyleChain};
use typst::layout::GridElem;
use typst::model::TableElem;

use crate::config::TermConfig;
use crate::frame::TermFrame;

// ── Grid ──────────────────────────────────────────────────────────────────────

/// Layout a `#grid(…)` element.
///
/// **Stub** — returns a `[grid]` placeholder frame.
///
/// A full implementation should:
/// 1. Resolve column / row track sizes.
/// 2. Layout each cell independently.
/// 3. Compose cells with `─` / `│` / corner box-drawing characters.
pub fn layout_grid(
    _elem: &Packed<GridElem>,
    _engine: &mut Engine,
    _config: &TermConfig,
    _styles: StyleChain,
) -> SourceResult<TermFrame> {
    // TODO: full grid layout
    Ok(TermFrame::text("[grid]", ContentStyle::default()))
}

// ── Table ─────────────────────────────────────────────────────────────────────

/// Layout a `#table(…)` element.
///
/// **Stub** — returns a `[table]` placeholder frame.
///
/// `TableElem` uses the same underlying grid machinery as `GridElem`; a real
/// implementation can share most code with `layout_grid`.
pub fn layout_table(
    _elem: &Packed<TableElem>,
    _engine: &mut Engine,
    _config: &TermConfig,
    _styles: StyleChain,
) -> SourceResult<TermFrame> {
    // TODO: full table layout
    Ok(TermFrame::text("[table]", ContentStyle::default()))
}
