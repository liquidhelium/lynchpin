//! Transform element layout stubs.
//!
//! `move`, `rotate`, `scale`, and `skew` are all visual-only transforms that
//! have no meaningful terminal equivalent.  The strategy is:
//!
//! - **`move`**: offset is ignored; the body is laid out normally.
//! - **`rotate`**: for angles near ±90° / ±270°, rows and columns are swapped
//!   (best-effort transposition); all other angles are laid out normally.
//! - **`scale`** / **`skew`**: scaling and shearing are ignored; body is laid
//!   out normally.
//!
//! All four delegates to the private [`layout_content`] helper, which in turn
//! calls the inline paragraph layout so that styled text, math stubs, etc. all
//! render correctly.

use crossterm::style::ContentStyle;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, Packed, StyleChain};
use typst::layout::{MoveElem, RotateElem, ScaleElem, SkewElem};

use crate::config::TermConfig;
use crate::frame::{Col, Row, TermFrame, TermPoint, TermSize};
use crate::inline::layout_paragraph;

// ── Private helpers ───────────────────────────────────────────────────────────

/// Layout arbitrary content as an inline paragraph.
///
/// This is the single dispatch point for all transform stubs.  It forwards to
/// [`layout_paragraph`] with a default base style and no wrap limit (the
/// transform may itself be placed inside a constrained container, but we
/// do not have that width information here).
fn layout_content(
    content: &Content,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    layout_paragraph(engine, content, config, styles, ContentStyle::default())
}

/// Transpose a frame (swap rows and columns).
///
/// Each item's `(col, row)` position becomes `(row, col)`.  The size is
/// updated accordingly.  Used by `layout_rotate` for 90°/270° rotations.
fn transpose_frame(frame: TermFrame) -> TermFrame {
    let new_cols = frame.rows();
    let new_rows = frame.cols();
    let new_baseline = (new_rows / 2).max(0);

    let mut out = TermFrame::new(TermSize::new(new_cols.max(0), new_rows.max(0)));
    out.set_baseline(new_baseline);

    // Translate each item: (c, r) → (r, c).
    for (pos, item) in frame.items().iter().cloned() {
        let new_pos = TermPoint::new(pos.row as Col, pos.col as Row);
        match item {
            crate::frame::TermFrameItem::Text(t, style) => {
                out.push_text(new_pos, t, style);
            }
            crate::frame::TermFrameItem::Frame(f) => {
                out.push_frame(new_pos, transpose_frame(f));
            }
        }
    }

    out
}

// ── Public layout functions ───────────────────────────────────────────────────

/// Layout a `#move(…)` element.
///
/// The `dx`/`dy` translation is ignored in the terminal; the body is laid out
/// at its natural position.
pub fn layout_move(
    elem: &Packed<MoveElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    layout_content(&elem.body, engine, config, styles)
}

/// Layout a `#rotate(…)` element.
///
/// For angles very close to ±90° or ±270° the frame is transposed (rows ↔
/// cols) as a rough approximation.  For all other angles the body is rendered
/// without rotation (terminal cells are not individually rotatable).
pub fn layout_rotate(
    elem: &Packed<RotateElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let frame = layout_content(&elem.body, engine, config, styles)?;

    // Check if the angle is close to a quarter-turn.
    let angle_deg = elem.angle.get(styles).to_deg();
    let norm = ((angle_deg % 360.0) + 360.0) % 360.0; // normalise to [0, 360)
    let near_90  = (norm -  90.0).abs() < 10.0;
    let near_270 = (norm - 270.0).abs() < 10.0;

    if near_90 || near_270 {
        // Best-effort transposition for right-angle rotations.
        Ok(transpose_frame(frame))
    } else {
        // All other angles: ignore rotation, return content as-is.
        Ok(frame)
    }
}

/// Layout a `#scale(…)` element.
///
/// Scaling is ignored in the terminal; the body is laid out at its natural
/// size.
pub fn layout_scale(
    elem: &Packed<ScaleElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    layout_content(&elem.body, engine, config, styles)
}

/// Layout a `#skew(…)` element.
///
/// Shearing is ignored in the terminal; the body is laid out normally.
pub fn layout_skew(
    elem: &Packed<SkewElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    layout_content(&elem.body, engine, config, styles)
}
