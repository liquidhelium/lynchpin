//! Core terminal grid frame infrastructure.
//!
//! [`TermFrame`] is the terminal equivalent of typst's `Frame`: a rectangular
//! region of terminal cells containing placed items (styled text and nested
//! frames).
//!
//! Coordinates are integers: columns (x) and rows (y), with origin at the
//! top-left corner. The *baseline* row is used for vertical alignment when
//! composing items side-by-side (as in inline math layout).

use std::fmt::Write as FmtWrite;

use crossterm::style::{Attribute, Color, ContentStyle};
use ecow::EcoString;
use unicode_width::UnicodeWidthChar;

// ── Scalar types ──────────────────────────────────────────────────────────────

/// Column coordinate (0 = leftmost column).
pub type Col = i32;

/// Row coordinate (0 = topmost row).
pub type Row = i32;

// ── TermSize ──────────────────────────────────────────────────────────────────

/// Width × height measured in terminal cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TermSize {
    /// Width in terminal columns.
    pub cols: Col,
    /// Height in terminal rows.
    pub rows: Row,
}

impl TermSize {
    pub const ZERO: Self = Self { cols: 0, rows: 0 };

    #[inline]
    pub fn new(cols: Col, rows: Row) -> Self {
        Self { cols, rows }
    }

    /// True if either dimension is non-positive.
    #[inline]
    pub fn is_empty(self) -> bool {
        self.cols <= 0 || self.rows <= 0
    }

    /// Component-wise maximum.
    #[inline]
    pub fn max(self, other: Self) -> Self {
        Self { cols: self.cols.max(other.cols), rows: self.rows.max(other.rows) }
    }
}

impl std::ops::Add for TermSize {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self { cols: self.cols + rhs.cols, rows: self.rows + rhs.rows }
    }
}

// ── TermPoint ─────────────────────────────────────────────────────────────────

/// Column + row position.  Origin = top-left of the containing frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TermPoint {
    pub col: Col,
    pub row: Row,
}

impl TermPoint {
    pub const ZERO: Self = Self { col: 0, row: 0 };

    #[inline]
    pub fn new(col: Col, row: Row) -> Self {
        Self { col, row }
    }

    #[inline]
    pub fn with_col(col: Col) -> Self {
        Self { col, row: 0 }
    }

    #[inline]
    pub fn with_row(row: Row) -> Self {
        Self { col: 0, row }
    }
}

impl std::ops::Add for TermPoint {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self { col: self.col + rhs.col, row: self.row + rhs.row }
    }
}

impl std::ops::AddAssign for TermPoint {
    fn add_assign(&mut self, rhs: Self) {
        self.col += rhs.col;
        self.row += rhs.row;
    }
}

impl std::ops::Sub for TermPoint {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self { col: self.col - rhs.col, row: self.row - rhs.row }
    }
}

impl std::ops::Neg for TermPoint {
    type Output = Self;
    fn neg(self) -> Self {
        Self { col: -self.col, row: -self.row }
    }
}

// ── TermFrameItem ─────────────────────────────────────────────────────────────

/// A single item that can be placed inside a [`TermFrame`].
#[derive(Debug, Clone)]
pub enum TermFrameItem {
    /// A styled text run starting at the item's position.
    ///
    /// The string may include wide Unicode characters; their display width is
    /// determined by `char_cols` / `text_cols`.
    Text(EcoString, ContentStyle),

    /// A nested frame, placed with its top-left at the item's position.
    Frame(TermFrame),
}

// ── TermFrame ─────────────────────────────────────────────────────────────────

/// A rectangular region of terminal cells containing placed items.
///
/// # Coordinate system
/// The origin (0, 0) is the **top-left** corner of the frame.
/// Column increases right; row increases down.
///
/// # Baseline
/// `baseline` is the row of the text baseline used for vertical alignment
/// when composing items side-by-side (inline math, inline text).
///
/// * `ascent()  == baseline`          — rows strictly above the baseline
/// * `descent() == rows() - baseline` — rows at or below the baseline
///
/// For a single-line text frame, `baseline = 0` and `descent = 1`.
/// For a fraction whose bar is at row 1, `baseline = 1`.
#[derive(Debug, Clone)]
pub struct TermFrame {
    size: TermSize,
    baseline: Row,
    items: Vec<(TermPoint, TermFrameItem)>,
}

impl TermFrame {
    // ── Constructors ─────────────────────────────────────────────────────────

    /// Empty frame of the given size; baseline = 0.
    pub fn new(size: TermSize) -> Self {
        Self { size, baseline: 0, items: Vec::new() }
    }

    /// Empty frame with baseline centred (useful for tall symmetric constructs
    /// like brackets or operators that centre on the math axis).
    pub fn soft(size: TermSize) -> Self {
        let baseline = if size.rows > 1 { (size.rows - 1) / 2 } else { 0 };
        Self { size, baseline, items: Vec::new() }
    }

    /// Single-row text frame. Width is inferred from `text_cols(t)`.
    pub fn text(t: impl Into<EcoString>, style: ContentStyle) -> Self {
        let t: EcoString = t.into();
        let cols = text_cols(&t) as Col;
        let mut f = Self::new(TermSize::new(cols, 1));
        if !t.is_empty() {
            f.items.push((TermPoint::ZERO, TermFrameItem::Text(t, style)));
        }
        f
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    #[inline] pub fn size(&self)     -> TermSize  { self.size }
    #[inline] pub fn cols(&self)     -> Col       { self.size.cols }
    #[inline] pub fn rows(&self)     -> Row       { self.size.rows }
    #[inline] pub fn baseline(&self) -> Row       { self.baseline }
    #[inline] pub fn ascent(&self)   -> Row       { self.baseline }
    #[inline] pub fn descent(&self)  -> Row       { self.size.rows - self.baseline }
    #[inline] pub fn is_empty_items(&self) -> bool { self.items.is_empty() }
    #[inline] pub fn items(&self)    -> &[(TermPoint, TermFrameItem)] { &self.items }

    // ── Mutators ──────────────────────────────────────────────────────────────

    pub fn set_size(&mut self, s: TermSize)   { self.size = s; }
    pub fn set_cols(&mut self, c: Col)        { self.size.cols = c; }
    pub fn set_rows(&mut self, r: Row)        { self.size.rows = r; }
    pub fn set_baseline(&mut self, row: Row)  { self.baseline = row; }

    /// Place a styled text run at `pos`.
    pub fn push_text(
        &mut self,
        pos: TermPoint,
        t: impl Into<EcoString>,
        style: ContentStyle,
    ) {
        let t: EcoString = t.into();
        if !t.is_empty() {
            self.items.push((pos, TermFrameItem::Text(t, style)));
        }
    }

    /// Place a nested frame at `pos` (relative to this frame's top-left).
    pub fn push_frame(&mut self, pos: TermPoint, frame: TermFrame) {
        self.items.push((pos, TermFrameItem::Frame(frame)));
    }

    /// Shift all placed items by `delta`.
    pub fn translate(&mut self, delta: TermPoint) {
        for (p, _) in &mut self.items {
            *p = *p + delta;
        }
    }

    // ── Drawing primitives ────────────────────────────────────────────────────

    /// Draw a horizontal run of `ch` at row `pos.row`, columns
    /// `pos.col .. pos.col + width`.
    pub fn hline(&mut self, pos: TermPoint, width: Col, ch: char, style: ContentStyle) {
        if width <= 0 {
            return;
        }
        let cw = char_cols(ch) as Col;
        if cw == 0 {
            return;
        }
        let mut s = String::with_capacity((width as usize).div_ceil(cw as usize) * ch.len_utf8());
        let mut filled = 0;
        while filled + cw <= width {
            s.push(ch);
            filled += cw;
        }
        if !s.is_empty() {
            self.push_text(pos, s, style);
        }
    }

    /// Draw a vertical run of `ch` from `pos` downwards for `height` rows.
    pub fn vline(&mut self, pos: TermPoint, height: Row, ch: char, style: ContentStyle) {
        let s: EcoString = ch.to_string().into();
        for r in 0..height {
            self.push_text(TermPoint::new(pos.col, pos.row + r), s.clone(), style);
        }
    }

    // ── Rendering ─────────────────────────────────────────────────────────────

    /// Rasterise this frame into a [`TermGrid`].
    pub fn render(&self) -> TermGrid {
        let mut grid = TermGrid::blank(self.size.cols.max(0), self.size.rows.max(0));
        self.render_into(&mut grid, 0, 0);
        grid
    }

    pub(crate) fn render_into(&self, grid: &mut TermGrid, base_col: Col, base_row: Row) {
        for (pos, item) in &self.items {
            let col = base_col + pos.col;
            let row = base_row + pos.row;
            match item {
                TermFrameItem::Text(t, style) => grid.put_text(col, row, t, *style),
                TermFrameItem::Frame(f)       => f.render_into(grid, col, row),
            }
        }
    }
}

// ── TermCell ──────────────────────────────────────────────────────────────────

/// A single rendered terminal cell.
#[derive(Debug, Clone, PartialEq)]
pub struct TermCell {
    pub ch: char,
    pub style: ContentStyle,
}

impl Default for TermCell {
    fn default() -> Self {
        Self { ch: ' ', style: ContentStyle::default() }
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
    cells: Vec<TermCell>, // index = row * cols + col
}

impl TermGrid {
    pub fn blank(cols: Col, rows: Row) -> Self {
        let n = (cols.max(0) * rows.max(0)) as usize;
        Self { cols, rows, cells: vec![TermCell::default(); n] }
    }

    fn idx(&self, col: Col, row: Row) -> Option<usize> {
        if col >= 0 && row >= 0 && col < self.cols && row < self.rows {
            Some((row * self.cols + col) as usize)
        } else {
            None
        }
    }

    pub fn get(&self, col: Col, row: Row) -> TermCell {
        self.idx(col, row).map(|i| self.cells[i].clone()).unwrap_or_default()
    }

    pub fn set(&mut self, col: Col, row: Row, cell: TermCell) {
        if let Some(i) = self.idx(col, row) {
            self.cells[i] = cell;
        }
    }

    /// Write a text run at `(col, row)`, advancing columns for wide characters.
    /// Silently clips to grid bounds.
    pub fn put_text(&mut self, mut col: Col, row: Row, text: &str, style: ContentStyle) {
        if row < 0 || row >= self.rows {
            return;
        }
        for ch in text.chars() {
            if col >= self.cols {
                break;
            }
            let w = char_cols(ch) as Col;
            if w == 0 {
                continue; // combining / zero-width — skip for now
            }
            if col >= 0 {
                self.set(col, row, TermCell { ch, style });
            }
            col += w;
        }
    }

    /// Render to an ANSI-escaped string suitable for direct terminal output.
    ///
    /// Trailing spaces on each row are trimmed.
    pub fn to_ansi(&self) -> String {
        let mut out = String::new();
        let default_style = ContentStyle::default();

        for row in 0..self.rows {
            // Find last non-blank column to avoid trailing spaces.
            let mut line_end: Col = 0;
            for col in 0..self.cols {
                if !self.get(col, row).is_blank() {
                    line_end = col + 1;
                }
            }

            let mut cur_style = default_style;
            let mut have_style = false;

            let mut col: Col = 0;
            while col < line_end {
                let cell = self.get(col, row);
                if cell.style != cur_style {
                    if have_style {
                        out.push_str("\x1b[0m");
                    }
                    apply_style(&mut out, &cell.style);
                    have_style = cell.style != default_style;
                    cur_style = cell.style;
                }
                let w = char_cols(cell.ch) as Col;
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

// ── ANSI style helpers ────────────────────────────────────────────────────────

fn apply_style(out: &mut String, s: &ContentStyle) {
    if let Some(fg) = s.foreground_color {
        push_color(out, fg, false);
    }
    if let Some(bg) = s.background_color {
        push_color(out, bg, true);
    }
    // Check individual attributes via bitmask helpers.
    let attrs = s.attributes;
    if attrs.has(Attribute::Bold)       { out.push_str("\x1b[1m"); }
    if attrs.has(Attribute::Dim)        { out.push_str("\x1b[2m"); }
    if attrs.has(Attribute::Italic)     { out.push_str("\x1b[3m"); }
    if attrs.has(Attribute::Underlined) { out.push_str("\x1b[4m"); }
    if attrs.has(Attribute::Reverse)    { out.push_str("\x1b[7m"); }
    if attrs.has(Attribute::CrossedOut) { out.push_str("\x1b[9m"); }
    if attrs.has(Attribute::OverLined)  { out.push_str("\x1b[53m"); }
}

fn push_color(out: &mut String, c: Color, bg: bool) {
    let (base, hi) = if bg { (40, 100) } else { (30, 90) };
    match c {
        Color::Black       => { let _ = write!(out, "\x1b[{}m", base);     }
        Color::DarkRed     => { let _ = write!(out, "\x1b[{}m", base + 1); }
        Color::DarkGreen   => { let _ = write!(out, "\x1b[{}m", base + 2); }
        Color::DarkYellow  => { let _ = write!(out, "\x1b[{}m", base + 3); }
        Color::DarkBlue    => { let _ = write!(out, "\x1b[{}m", base + 4); }
        Color::DarkMagenta => { let _ = write!(out, "\x1b[{}m", base + 5); }
        Color::DarkCyan    => { let _ = write!(out, "\x1b[{}m", base + 6); }
        Color::Grey        => { let _ = write!(out, "\x1b[{}m", base + 7); }
        Color::DarkGrey    => { let _ = write!(out, "\x1b[{}m", hi);       }
        Color::Red         => { let _ = write!(out, "\x1b[{}m", hi + 1);   }
        Color::Green       => { let _ = write!(out, "\x1b[{}m", hi + 2);   }
        Color::Yellow      => { let _ = write!(out, "\x1b[{}m", hi + 3);   }
        Color::Blue        => { let _ = write!(out, "\x1b[{}m", hi + 4);   }
        Color::Magenta     => { let _ = write!(out, "\x1b[{}m", hi + 5);   }
        Color::Cyan        => { let _ = write!(out, "\x1b[{}m", hi + 6);   }
        Color::White       => { let _ = write!(out, "\x1b[{}m", hi + 7);   }
        Color::Rgb { r, g, b } => {
            if bg { let _ = write!(out, "\x1b[48;2;{r};{g};{b}m"); }
            else  { let _ = write!(out, "\x1b[38;2;{r};{g};{b}m"); }
        }
        Color::AnsiValue(n) => {
            if bg { let _ = write!(out, "\x1b[48;5;{n}m"); }
            else  { let _ = write!(out, "\x1b[38;5;{n}m"); }
        }
        _ => {}
    }
}

// ── Width helpers ─────────────────────────────────────────────────────────────

/// Display column width of a single character.
/// Returns 1 for most characters, 2 for wide East-Asian characters,
/// 0 for combining / zero-width characters.
#[inline]
pub fn char_cols(c: char) -> usize {
    c.width().unwrap_or(1)
}

/// Display column width of a string (sum of `char_cols` for each char).
#[inline]
pub fn text_cols(s: &str) -> usize {
    s.chars().map(char_cols).sum()
}
