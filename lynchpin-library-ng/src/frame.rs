//! Core terminal grid frame infrastructure.
//!
//! Mirrors `typst-library/src/layout/frame.rs` with terminal units.
//!
//! [`TermFrame`] is the terminal equivalent of typst's `Frame`: a rectangular
//! region of terminal cells containing placed items (styled text and nested
//! frames).
//!
//! Coordinates are [`TermScalar`]s: columns (x) and rows (y), with origin at
//! the top-left corner.  The *baseline* row is used for vertical alignment
//! when composing items side-by-side (as in inline math layout).

use std::fmt::Write as FmtWrite;

use crossterm::style::{ContentStyle, Attribute, Color};
use ecow::EcoString;
use unicode_width::UnicodeWidthChar;

use typst::introspection::Tag;
use typst::foundations::Label;

use crate::scalar::TermScalar;
use tracing::debug;

// ── Public type aliases ──────────────────────────────────────────────────────

/// Column coordinate (0 = leftmost column).
pub type Col = TermScalar;

/// Row coordinate (0 = topmost row).
pub type Row = TermScalar;

// ── TermSize ──────────────────────────────────────────────────────────────────

/// Width × height measured in terminal cells.
///
/// Mirrors paged `Size { x: Abs, y: Abs }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TermSize {
    /// Width in terminal columns.
    pub cols: Col,
    /// Height in terminal rows.
    pub rows: Row,
}

impl TermSize {
    pub const ZERO: Self = Self {
        cols: TermScalar::ZERO,
        rows: TermScalar::ZERO,
    };

    #[inline]
    pub fn new(cols: Col, rows: Row) -> Self {
        Self { cols, rows }
    }

    /// True if either dimension is non-positive.
    #[inline]
    pub fn is_empty(self) -> bool {
        self.cols <= TermScalar::ZERO || self.rows <= TermScalar::ZERO
    }

    /// Component-wise maximum.
    #[inline]
    pub fn max(self, other: Self) -> Self {
        Self {
            cols: self.cols.max(other.cols),
            rows: self.rows.max(other.rows),
        }
    }

    /// Component-wise minimum.
    #[inline]
    pub fn min(self, other: Self) -> Self {
        Self {
            cols: self.cols.min(other.cols),
            rows: self.rows.min(other.rows),
        }
    }

    /// Map each component through a function.
    #[inline]
    pub fn map<F: Fn(TermScalar) -> TermScalar>(self, f: F) -> Self {
        Self {
            cols: f(self.cols),
            rows: f(self.rows),
        }
    }

    /// Whether the size is finite in both axes.
    #[inline]
    pub fn is_finite(self) -> bool {
        self.cols.is_finite() && self.rows.is_finite()
    }
}

impl std::ops::Add for TermSize {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            cols: self.cols + rhs.cols,
            rows: self.rows + rhs.rows,
        }
    }
}

impl std::ops::Sub for TermSize {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            cols: self.cols - rhs.cols,
            rows: self.rows - rhs.rows,
        }
    }
}

// ── TermPoint ─────────────────────────────────────────────────────────────────

/// Column + row position.  Origin = top-left of the containing frame.
///
/// Mirrors paged `Point { x: Abs, y: Abs }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TermPoint {
    pub col: Col,
    pub row: Row,
}

impl TermPoint {
    pub const ZERO: Self = Self {
        col: TermScalar::ZERO,
        row: TermScalar::ZERO,
    };

    #[inline]
    pub fn new(col: Col, row: Row) -> Self {
        Self { col, row }
    }

    #[inline]
    pub fn with_col(self, col: Col) -> Self {
        Self { col, ..self }
    }

    #[inline]
    pub fn with_row(self, row: Row) -> Self {
        Self { row, ..self }
    }
}

impl std::ops::Add for TermPoint {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            col: self.col + rhs.col,
            row: self.row + rhs.row,
        }
    }
}

impl std::ops::AddAssign for TermPoint {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl std::ops::Sub for TermPoint {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            col: self.col - rhs.col,
            row: self.row - rhs.row,
        }
    }
}

impl std::ops::Neg for TermPoint {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            col: -self.col,
            row: -self.row,
        }
    }
}

// ── TermFrameItem ─────────────────────────────────────────────────────────────

/// An item placed in a [`TermFrame`].
///
/// Mirrors paged `FrameItem`.
#[derive(Debug, Clone)]
pub enum TermFrameItem {
    /// Styled text at this position.
    Text(EcoString, ContentStyle),
    /// A nested sub-frame (e.g. for inline math, blocks).
    Frame(TermFrame),
    /// A horizontal rule (character repeated across columns).
    Rule {
        /// Fill character.
        ch: char,
        /// Number of columns.
        width: Col,
        /// Style for the rule.
        style: ContentStyle,
    },
    /// Approximated shape (ASCII/Unicode art).
    Shape(TermShape),
    /// Approximated image placeholder.
    Image(TermImage),
    /// An introspection tag (mirrors paged `FrameItem::Tag`).
    Tag(Tag),
}

/// An approximated geometric shape for terminal display.
#[derive(Debug, Clone)]
pub struct TermShape {
    /// Shape geometry (line, rect, circle, path, …).
    pub geometry: TermGeometry,
    /// Optional fill character (None = no fill).
    pub fill: Option<char>,
    /// Stroke character for outlines.
    pub stroke_char: char,
    /// Style for the shape.
    pub style: ContentStyle,
    /// Source span.
    pub span: typst::syntax::Span,
}

/// Simplified geometry for terminal rendering.
#[derive(Debug, Clone)]
pub enum TermGeometry {
    /// A line from start to end (relative to placement point).
    Line {
        /// End-point offset (cols, rows).
        delta: TermPoint,
    },
    /// A filled/stroked rectangle.
    Rect {
        size: TermSize,
    },
    /// A circle/ellipse (approximated).
    Ellipse {
        size: TermSize,
    },
    /// Arbitrary path (line segments, approximated).
    Path {
        points: Vec<TermPoint>,
        closed: bool,
    },
}

/// An approximated image placeholder for terminal display.
#[derive(Debug, Clone)]
pub struct TermImage {
    /// Placeholder width in columns.
    pub width: Col,
    /// Placeholder height in rows.
    pub height: Row,
    /// Alt text or filename.
    pub alt: EcoString,
}

// ── TermFrame ─────────────────────────────────────────────────────────────────

/// A terminal frame: a sparse 2-D grid of [`TermFrameItem`]s.
///
/// Mirrors paged `Frame`.  Unlike `TermGrid` (which is a dense rasterised
/// array), `TermFrame` stores items at their logical positions and supports
/// nesting.
#[derive(Debug, Clone, Default)]
pub struct TermFrame {
    /// Total size of the frame.
    size: TermSize,
    /// Baseline row offset from the top (for inline alignment).
    baseline: Row,
    /// Placed items.
    items: Vec<(TermPoint, TermFrameItem)>,
    /// Content height before any expansion.
    content_height: Row,
}

impl TermFrame {
    /// Create an empty frame with the given size.
    pub fn new(size: TermSize) -> Self {
        Self {
            size,
            baseline: TermScalar::ZERO,
            items: Vec::new(),
            content_height: Row::ZERO,
        }
    }

    /// Create a soft (auto-sized) frame.
    pub fn soft(size: TermSize) -> Self {
        Self::new(size)
    }

    /// Create a frame containing a single text item.
    pub fn text(text: impl Into<EcoString>, style: ContentStyle, cols: Col, rows: Row) -> Self {
        let mut f = Self::new(TermSize::new(cols, rows));
        f.push_text(TermPoint::ZERO, text.into(), style);
        f
    }

    // ── Size accessors ───────────────────────────────────────────────────

    #[inline]
    pub fn size(&self) -> TermSize {
        self.size
    }

    #[inline]
    pub fn cols(&self) -> Col {
        self.size.cols
    }

    #[inline]
    pub fn rows(&self) -> Row {
        self.size.rows
    }

    #[inline]
    pub fn width(&self) -> Col {
        self.size.cols
    }

    #[inline]
    pub fn height(&self) -> Row {
        self.size.rows
    }

    #[inline]
    pub fn baseline(&self) -> Row {
        self.baseline
    }

    #[inline]
    pub fn ascent(&self) -> Row {
        self.baseline
    }

    #[inline]
    pub fn descent(&self) -> Row {
        self.size.rows - self.baseline
    }

    #[inline]
    pub fn has_baseline(&self) -> bool {
        self.baseline > TermScalar::ZERO
    }

    #[inline]
    pub fn content_height(&self) -> Row {
        self.content_height
    }

    /// Whether the frame has any items.
    #[inline]
    pub fn is_empty_items(&self) -> bool {
        self.items.is_empty()
    }

    /// Iterate over all placed items.
    #[inline]
    pub fn items(&self) -> &[(TermPoint, TermFrameItem)] {
        &self.items
    }

    // ── Mutators ─────────────────────────────────────────────────────────

    #[inline]
    pub fn set_size(&mut self, size: TermSize) {
        self.size = size;
    }

    #[inline]
    pub fn set_cols(&mut self, cols: Col) {
        self.size.cols = cols;
    }

    #[inline]
    pub fn set_rows(&mut self, rows: Row) {
        self.size.rows = rows;
    }

    #[inline]
    pub fn set_baseline(&mut self, baseline: Row) {
        self.baseline = baseline;
    }

    #[inline]
    pub fn set_content_height(&mut self, h: Row) {
        self.content_height = h;
    }

    /// Resize to `target`, aligning content within.
    pub fn resize(&mut self, target: TermSize, align: crate::Axes<Alignment>) {
        let dx = align_x(align.x, target.cols, self.size.cols);
        let dy = align_y(align.y, target.rows, self.size.rows);
        self.translate(TermPoint::new(dx, dy));
        self.size = target;
    }

    /// Clip the frame to a rectangle.
    pub fn clip(&mut self, _rect: TermSize) {
        // Terminal: clipping is a no-op for now.
        // In paged layout this creates a clip path.
    }

    // ── Item push ────────────────────────────────────────────────────────

    /// Push a text run at `pos`.
    pub fn push_text(&mut self, pos: TermPoint, text: EcoString, style: ContentStyle) {
        self.items.push((pos, TermFrameItem::Text(text, style)));
    }

    /// Push a nested frame at `pos`.
    pub fn push_frame(&mut self, pos: TermPoint, frame: TermFrame) {
        self.items.push((pos, TermFrameItem::Frame(frame)));
    }

    /// Push a horizontal rule.
    pub fn push_rule(&mut self, pos: TermPoint, ch: char, width: Col, style: ContentStyle) {
        self.items.push((pos, TermFrameItem::Rule { ch, width, style }));
    }

    /// Push a shape.
    pub fn push_shape(&mut self, pos: TermPoint, shape: TermShape) {
        self.items.push((pos, TermFrameItem::Shape(shape)));
    }

    /// Push an image placeholder.
    pub fn push_image(&mut self, pos: TermPoint, image: TermImage) {
        self.items.push((pos, TermFrameItem::Image(image)));
    }

    /// Push an introspection tag.
    pub fn push_tag(&mut self, pos: TermPoint, tag: Tag) {
        self.items.push((pos, TermFrameItem::Tag(tag)));
    }

    /// Generic push: push any [`TermFrameItem`] at `pos`.
    pub fn push(&mut self, pos: TermPoint, item: TermFrameItem) {
        self.items.push((pos, item));
    }

    /// Attach a label to this frame (terminal: stored via Tag mechanism).
    pub fn label(&mut self, _label: Label) {}

    /// Set the logical parent of this frame.
    pub fn set_parent(&mut self, _loc: typst::introspection::Location, _inherit: bool) {}

    /// Prepend an item at `pos` (for tags, etc.).
    pub fn prepend(&mut self, pos: TermPoint, item: TermFrameItem) {
        self.items.insert(0, (pos, item));
    }

    /// Push multiple items.
    pub fn push_multiple<I: IntoIterator<Item = (TermPoint, TermFrameItem)>>(
        &mut self,
        items: I,
    ) {
        self.items.extend(items);
    }

    /// Prepend multiple items.
    pub fn prepend_multiple<I: IntoIterator<Item = (TermPoint, TermFrameItem)>>(
        &mut self,
        items: I,
    ) {
        let mut new: Vec<_> = items.into_iter().collect();
        new.append(&mut self.items);
        self.items = new;
    }

    // ── Translate ────────────────────────────────────────────────────────

    /// Shift all items by `delta`.
    pub fn translate(&mut self, delta: TermPoint) {
        if delta == TermPoint::ZERO {
            return;
        }
        for (pos, _) in &mut self.items {
            *pos = *pos + delta;
        }
    }

    // ── Horizontal / vertical lines (raster helpers) ─────────────────────

    /// Draw a horizontal line of `ch` repeated for `width` columns.
    pub fn hline(&mut self, pos: TermPoint, width: Col, ch: char, style: ContentStyle) {
        self.push_rule(pos, ch, width, style);
    }

    /// Draw a vertical line of `ch` repeated for `height` rows.
    pub fn vline(&mut self, pos: TermPoint, height: Row, ch: char, style: ContentStyle) {
        let h = height.max(TermScalar::ZERO).get();
        for i in 0..h {
            self.push_text(
                TermPoint::new(pos.col, pos.row + TermScalar::new(i)),
                EcoString::from(ch),
                style,
            );
        }
    }

    // ── Rasterise ────────────────────────────────────────────────────────

    /// Render the frame into a dense [`TermGrid`].
    pub fn render(&self) -> TermGrid {
        let mut grid = TermGrid::blank(self.size.cols, self.size.rows);
        self.render_into(&mut grid, TermPoint::ZERO);
        grid
    }

    /// Render into an existing grid at `offset`.
    pub(crate) fn render_into(&self, grid: &mut TermGrid, offset: TermPoint) {
        static DEPTH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let d = DEPTH.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let is_small = self.size.rows < TermScalar::new(10);
        if is_small {
            debug!("[{}] render_into: size=({:.1},{:.1}) offset=({:.6},{:.6}) items={}",
                d, self.size.cols.raw(), self.size.rows.raw(),
                offset.col.raw(), offset.row.raw(), self.items.len());
        }
        for (pos, item) in &self.items {
            let p = *pos + offset;
            match item {
                TermFrameItem::Text(t, style) => {
                    grid.put_text(p.col, p.row, t, *style);
                }
                TermFrameItem::Frame(f) => {
                    if is_small {
                        debug!("[{}]   -> subframe size=({:.1},{:.1}) pos=({:.6},{:.6}) p=({:.6},{:.6})",
                            d, f.size.cols.raw(), f.size.rows.raw(),
                            pos.col.raw(), pos.row.raw(), p.col.raw(), p.row.raw());
                    }
                    f.render_into(grid, p);
                }
                _ => {}
            }
        }
        DEPTH.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

// ── TermCell ──────────────────────────────────────────────────────────────────

/// A single cell in a [`TermGrid`].
#[derive(Debug, Clone, Copy)]
pub struct TermCell {
    pub ch: char,
    pub style: ContentStyle,
}

impl Default for TermCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: ContentStyle::default(),
        }
    }
}

impl TermCell {
    pub fn is_blank(&self) -> bool {
        self.ch == ' ' && self.style == ContentStyle::default()
    }
}

// ── TermGrid ──────────────────────────────────────────────────────────────────

/// A rasterised 2-D array of [`TermCell`]s (row-major).
pub struct TermGrid {
    pub cols: Col,
    pub rows: Row,
    cells: Vec<TermCell>,
}

impl TermGrid {
    pub fn blank(cols: Col, rows: Row) -> Self {
        let n = (cols.max(TermScalar::ZERO) * rows.max(TermScalar::ZERO)).get().max(0) as usize;
        Self {
            cols,
            rows,
            cells: vec![TermCell::default(); n],
        }
    }

    fn idx(&self, col: Col, row: Row) -> Option<usize> {
        let col = col.round();
        let row = row.round();
        if col >= TermScalar::ZERO
            && row >= TermScalar::ZERO
            && col < self.cols
            && row < self.rows
        {
            Some((row * self.cols + col).get() as usize)
        } else {
            None
        }
    }

    pub fn get(&self, col: Col, row: Row) -> TermCell {
        self.idx(col, row)
            .map(|i| self.cells[i])
            .unwrap_or_default()
    }

    /// Number of cells in the grid (for debugging).
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Get a cell by linear index (for debugging).
    pub fn cell_at(&self, i: usize) -> TermCell {
        self.cells.get(i).copied().unwrap_or_default()
    }

    pub fn set(&mut self, col: Col, row: Row, cell: TermCell) {
        if let Some(i) = self.idx(col, row) {
            self.cells[i] = cell;
        }
    }

    /// Write a text run at `(col, row)`, advancing columns for wide characters.
    pub fn put_text(&mut self, mut col: Col, row: Row, text: &str, style: ContentStyle) {
        if row < TermScalar::ZERO || row >= self.rows {
            return;
        }
        for ch in text.chars() {
            if col >= self.cols {
                break;
            }
            let w = char_cols(ch);
            if w == TermScalar::ZERO {
                continue;
            }
            if col >= TermScalar::ZERO {
                debug!("put_text: '{}' at ({:?},{:?}) grid_size=({:?},{:?})", ch, col, row, self.cols, self.rows);
                self.set(col, row, TermCell { ch, style });
            }
            col = col + w;
        }
    }

    /// Draw a horizontal rule.
    pub fn put_rule(&mut self, col: Col, row: Row, ch: char, width: Col, style: ContentStyle) {
        let w = width.get();
        for i in 0..w {
            self.set(col + TermScalar::new(i), row, TermCell { ch, style });
        }
    }

    /// Draw a shape (simplified — fills bounding box).
    pub fn put_shape(&mut self, col: Col, row: Row, shape: &TermShape) {
        // Very simple approximation: fill the bounding box with the stroke char.
        let (w, h) = match &shape.geometry {
            TermGeometry::Line { delta } => {
                let w = delta.col.abs();
                let h = delta.row.abs();
                let max_dim = w.max(h);
                if max_dim == TermScalar::ZERO {
                    return;
                }
                // Draw a diagonal-ish line
                for i in 0..max_dim.get() {
                    let t = TermScalar::new(i);
                    let x = col + if w > TermScalar::ZERO { t * delta.col / w } else { TermScalar::ZERO };
                    let y = row + if h > TermScalar::ZERO { t * delta.row / h } else { TermScalar::ZERO };
                    self.set(x, y, TermCell { ch: shape.stroke_char, style: shape.style });
                }
                return;
            }
            TermGeometry::Rect { size } | TermGeometry::Ellipse { size } => {
                (size.cols, size.rows)
            }
            TermGeometry::Path { points, .. } => {
                if points.is_empty() {
                    return;
                }
                let max_col = points.iter().map(|p| p.col).max().unwrap();
                let max_row = points.iter().map(|p| p.row).max().unwrap();
                (max_col, max_row)
            }
        };
        for r in 0..h.get() {
            for c in 0..w.get() {
                self.set(
                    col + TermScalar::new(c),
                    row + TermScalar::new(r),
                    TermCell { ch: shape.stroke_char, style: shape.style },
                );
            }
        }
    }

    /// Draw an image placeholder.
    pub fn put_image(&mut self, col: Col, row: Row, img: &TermImage) {
        let label = format!("[img: {}]", img.alt);
        self.put_text(col, row, &label, ContentStyle::default());
    }

    /// Render to an ANSI-escaped string.
    pub fn to_ansi(&self) -> String {
        let mut out = String::new();
        let default_style = ContentStyle::default();

        for row in 0..self.rows.get() {
            let row_scalar = TermScalar::new(row);
            // Find last non-blank column.
            let mut line_end: i32 = 0;
            for col in 0..self.cols.get() {
                if !self.get(TermScalar::new(col), row_scalar).is_blank() {
                    line_end = col + 1;
                }
            }

            let mut cur_style = default_style;
            let mut have_style = false;

            let mut col: i32 = 0;
            while col < line_end {
                let cell = self.get(TermScalar::new(col), row_scalar);
                if cell.style != cur_style {
                    if have_style {
                        out.push_str("\x1b[0m");
                    }
                    apply_style(&mut out, &cell.style);
                    have_style = cell.style != default_style;
                    cur_style = cell.style;
                }
                let w = char_cols(cell.ch).get();
                out.push(cell.ch);
                col += w.max(1);
            }

            if have_style {
                out.push_str("\x1b[0m");
            }
            out.push('\n');
        }

        out
    }
}

// ── Alignment helpers ─────────────────────────────────────────────────────────

/// Horizontal alignment for frame resizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Start,
    Center,
    End,
}

/// A pair of alignments (horizontal, vertical).
pub type Axes<T> = typst::layout::Axes<T>;

fn align_x(align: Alignment, total: Col, content: Col) -> Col {
    match align {
        Alignment::Start => TermScalar::ZERO,
        Alignment::Center => (total - content).max(TermScalar::ZERO) / TermScalar::new(2),
        Alignment::End => (total - content).max(TermScalar::ZERO),
    }
}

fn align_y(align: Alignment, total: Row, content: Row) -> Row {
    match align {
        Alignment::Start => TermScalar::ZERO,
        Alignment::Center => (total - content).max(TermScalar::ZERO) / TermScalar::new(2),
        Alignment::End => (total - content).max(TermScalar::ZERO),
    }
}

// ── ANSI style helpers ────────────────────────────────────────────────────────

fn apply_style(out: &mut String, style: &ContentStyle) {
    let mut codes = Vec::new();

    if let Some(c) = style.foreground_color {
        push_color(&mut codes, true, c);
    }
    if let Some(c) = style.background_color {
        push_color(&mut codes, false, c);
    }
    let attrs = style.attributes;
    if attrs.has(Attribute::Bold) { codes.push("1"); }
    if attrs.has(Attribute::Dim) { codes.push("2"); }
    if attrs.has(Attribute::Italic) { codes.push("3"); }
    if attrs.has(Attribute::Underlined) { codes.push("4"); }
    if attrs.has(Attribute::SlowBlink) { codes.push("5"); }
    if attrs.has(Attribute::RapidBlink) { codes.push("6"); }
    if attrs.has(Attribute::Reverse) { codes.push("7"); }
    if attrs.has(Attribute::Hidden) { codes.push("8"); }
    if attrs.has(Attribute::CrossedOut) { codes.push("9"); }

    if !codes.is_empty() {
        let _ = write!(out, "\x1b[{}m", codes.join(";"));
    }
}

fn push_color(codes: &mut Vec<&str>, is_fg: bool, color: Color) {
    match color {
        Color::Reset => {
            codes.push(if is_fg { "39" } else { "49" });
        }
        Color::Black => codes.push(if is_fg { "30" } else { "40" }),
        Color::DarkGrey => codes.push(if is_fg { "90" } else { "100" }),
        Color::Red => codes.push(if is_fg { "31" } else { "41" }),
        Color::DarkRed => codes.push(if is_fg { "91" } else { "101" }),
        Color::Green => codes.push(if is_fg { "32" } else { "42" }),
        Color::DarkGreen => codes.push(if is_fg { "92" } else { "102" }),
        Color::Yellow => codes.push(if is_fg { "33" } else { "43" }),
        Color::DarkYellow => codes.push(if is_fg { "93" } else { "103" }),
        Color::Blue => codes.push(if is_fg { "34" } else { "44" }),
        Color::DarkBlue => codes.push(if is_fg { "94" } else { "104" }),
        Color::Magenta => codes.push(if is_fg { "35" } else { "45" }),
        Color::DarkMagenta => codes.push(if is_fg { "95" } else { "105" }),
        Color::Cyan => codes.push(if is_fg { "36" } else { "46" }),
        Color::DarkCyan => codes.push(if is_fg { "96" } else { "106" }),
        Color::White => codes.push(if is_fg { "37" } else { "47" }),
        Color::Grey => codes.push(if is_fg { "97" } else { "107" }),
        _ => {}
    }
}

// ── Unicode width helpers ─────────────────────────────────────────────────────

pub fn char_cols(ch: char) -> Col {
    let w = UnicodeWidthChar::width(ch).unwrap_or(1);
    TermScalar::new(w as i32)
}

pub fn text_cols(text: &str) -> Col {
    let mut total: i32 = 0;
    for ch in text.chars() {
        total += UnicodeWidthChar::width(ch).unwrap_or(1) as i32;
    }
    TermScalar::new(total)
}
