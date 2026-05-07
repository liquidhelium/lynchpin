//! Padding helpers.
//!
//! Functions for adding whitespace around [`TermFrame`]s.  All padding
//! functions create a new frame with the original placed inside; they never
//! modify the original frame.

use crate::frame::{Col, Row, TermFrame, TermPoint, TermSize};

// ── Uniform padding ───────────────────────────────────────────────────────────

/// Wrap `frame` in a new frame with the given padding on each side.
///
/// The resulting frame has:
/// - `cols` = `frame.cols() + left + right`
/// - `rows` = `frame.rows() + top + bottom`
/// - `baseline` = `frame.baseline() + top`
///
/// Negative padding is clamped to 0 in the output dimensions (though the
/// original frame is still placed at `(left, top)`, which may be negative).
pub fn pad_frame(frame: TermFrame, left: Col, right: Col, top: Row, bottom: Row) -> TermFrame {
    let new_cols = frame.cols() + left + right;
    let new_rows = frame.rows() + top + bottom;
    let new_baseline = frame.baseline() + top;

    let mut out = TermFrame::new(TermSize::new(new_cols.max(0), new_rows.max(0)));
    out.set_baseline(new_baseline.max(0));
    out.push_frame(TermPoint::new(left, top), frame);
    out
}

// ── Axis-specific helpers ─────────────────────────────────────────────────────

/// Add only horizontal padding (no top/bottom change).
#[inline]
pub fn pad_h(frame: TermFrame, left: Col, right: Col) -> TermFrame {
    pad_frame(frame, left, right, 0, 0)
}

/// Add only vertical padding (no left/right change).
#[inline]
pub fn pad_v(frame: TermFrame, top: Row, bottom: Row) -> TermFrame {
    pad_frame(frame, 0, 0, top, bottom)
}
