//! Padding helpers.
//!
//! Terminal equivalents of the paged `pad.rs` functions, adapted to use
//! `TermScalar` and `TermSize` in place of `Abs` and `Size`.

use typst_library::foundations::{Resolve, StyleChain};
use typst_library::layout::{Abs, Rel, Sides};
use typst_library::text::TextElem;

use lynchpin_library_ng::*;
use lynchpin_library_ng::units::{abs_to_cols, abs_to_rows};

/// Shrink a region size by an inset relative to the size itself.
///
/// This mirrors paged `shrink()`:
/// ```ignore
/// size - inset.sum_by_axis().relative_to(size)
/// ```
pub fn shrink(size: TermSize, inset: &Sides<Rel<Abs>>, styles: StyleChain) -> TermSize {
    let font_size = styles.get(TextElem::size).0.resolve(styles);
    let left = abs_to_cols(inset.left.relative_to(font_size), font_size);
    let right = abs_to_cols(inset.right.relative_to(font_size), font_size);
    let top = abs_to_rows(inset.top.relative_to(font_size), font_size);
    let bottom = abs_to_rows(inset.bottom.relative_to(font_size), font_size);

    TermSize::new(
        (size.cols - left - right).max(TermScalar::ZERO),
        (size.rows - top - bottom).max(TermScalar::ZERO),
    )
}

/// Shrink the components of possibly multiple `TermRegions` by an inset
/// relative to the regions themselves.
///
/// Mirrors paged `shrink_multiple()`.
pub fn shrink_multiple(
    size: &mut TermSize,
    full: &mut Row,
    backlog: &mut [Row],
    last: &mut Option<Row>,
    inset: &Sides<Rel<Abs>>,
    styles: StyleChain,
) {
    let font_size = styles.get(TextElem::size).0.resolve(styles);
    let left = abs_to_cols(inset.left.relative_to(font_size), font_size);
    let right = abs_to_cols(inset.right.relative_to(font_size), font_size);
    let top = abs_to_rows(inset.top.relative_to(font_size), font_size);
    let bottom = abs_to_rows(inset.bottom.relative_to(font_size), font_size);

    let dx = left + right;
    let dy = top + bottom;

    size.cols = (size.cols - dx).max(TermScalar::ZERO);
    size.rows = (size.rows - dy).max(TermScalar::ZERO);
    *full = (*full - dy).max(TermScalar::ZERO);
    for item in backlog.iter_mut() {
        *item = (*item - dy).max(TermScalar::ZERO);
    }
    *last = last.map(|v| (v - dy).max(TermScalar::ZERO));
}

/// Grow a frame's size by an inset relative to the grown size.
///
/// This is the inverse operation of [`shrink`].  Mirrors paged `grow()`.
///
/// For the horizontal axis the derivation is:
///
/// Let w be the grown target width,
///     s be the given width,
///     l be the left inset,
///     r be the right inset,
///     p = l + r.
///
/// We want that: w - l.resolve(w) - r.resolve(w) = s
///
/// Thus: w - p.resolve(w) = s
///   <=> w - p.rel * w - p.abs = s
///   <=> (1 - p.rel) * w = s + p.abs
///   <=> w = (s + p.abs) / (1 - p.rel)
pub fn grow(frame: &mut TermFrame, inset: &Sides<Rel<Abs>>, styles: StyleChain) {
    let font_size = styles.get(TextElem::size).0.resolve(styles);
    let left = abs_to_cols(inset.left.relative_to(font_size), font_size);
    let right = abs_to_cols(inset.right.relative_to(font_size), font_size);
    let top = abs_to_rows(inset.top.relative_to(font_size), font_size);
    let bottom = abs_to_rows(inset.bottom.relative_to(font_size), font_size);

    let dx = left + right;
    let dy = top + bottom;

    // Grow the frame's size by the insets.
    let new_cols = frame.size().cols + dx;
    let new_rows = frame.size().rows + dy;

    // Apply the padding inversely such that the grown size padded
    // yields the frame's size.
    frame.set_size(TermSize::new(new_cols, new_rows));

    // Translate everything in the frame inwards.
    frame.translate(TermPoint::new(left, top));
}
