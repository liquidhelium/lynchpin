//! Transform element layout stubs.
//!
//! `move`, `rotate`, `scale`, and `skew` are all visual-only transforms that
//! have no meaningful terminal equivalent.  The strategy is:
//!
//! - **`move`**: the body is laid out and then the resulting frame is
//!   translated by the resolved delta.
//! - **`rotate`** / **`scale`** / **`skew`**: the body is laid out as-is;
//!   the transform is a no-op.

use typst_library::diag::SourceResult;
use typst_library::engine::Engine;
use typst_library::foundations::{Packed, Resolve, StyleChain};
use typst_library::layout::{
    Axes, MoveElem, RotateElem, ScaleElem, SkewElem,
};
use typst_library::text::TextElem;

use lynchpin_library::*;
use lynchpin_library::units::abs_to_cols;

/// Layout the moved content.
///
/// The frame is translated by the resolved `dx`/`dy` offsets (converted
/// to terminal columns).
#[typst_macros::time(span = elem.span())]
pub fn layout_move(
    elem: &Packed<MoveElem>,
    engine: &mut Engine,
    _config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let mut frame = crate::flow::layout_term_frame(engine,&elem.body,
        typst_library::introspection::Locator::root(),
        styles,
        TermRegion::new(
            TermSize::new(lynchpin_library::resolve_page_size(styles).cols, TermScalar::INFINITY),
            Axes::new(false, false),
        ),
    )?;

    let dx = elem.dx.resolve(styles);
    let dy = elem.dy.resolve(styles);
    let font_size = styles
        .get(TextElem::size)
        .0
        .resolve(styles);

    let dx_cols = abs_to_cols(dx.relative_to(font_size), font_size);
    let dy_rows = abs_to_cols(dy.relative_to(font_size), font_size);

    if dx_cols != TermScalar::ZERO || dy_rows != TermScalar::ZERO {
        frame.translate(TermPoint::new(dx_cols, dy_rows));
    }

    Ok(frame)
}

/// Layout the rotated content.
///
/// Terminal equivalent is a no-op: the body is laid out as-is.
#[typst_macros::time(span = elem.span())]
pub fn layout_rotate(
    elem: &Packed<RotateElem>,
    engine: &mut Engine,
    _config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let _ = elem.angle.get(styles); // rotation angle ignored
    crate::flow::layout_term_frame(engine,&elem.body,
        typst_library::introspection::Locator::root(),
        styles,
        TermRegion::new(
            TermSize::new(lynchpin_library::resolve_page_size(styles).cols, TermScalar::INFINITY),
            Axes::new(false, false),
        ),
    )
}

/// Layout the scaled content.
///
/// Terminal equivalent is a no-op: the body is laid out at its natural size.
#[typst_macros::time(span = elem.span())]
pub fn layout_scale(
    elem: &Packed<ScaleElem>,
    engine: &mut Engine,
    _config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let _ = (elem.x.get(styles), elem.y.get(styles)); // scale factors ignored
    crate::flow::layout_term_frame(engine,&elem.body,
        typst_library::introspection::Locator::root(),
        styles,
        TermRegion::new(
            TermSize::new(lynchpin_library::resolve_page_size(styles).cols, TermScalar::INFINITY),
            Axes::new(false, false),
        ),
    )
}

/// Layout the skewed content.
///
/// Terminal equivalent is a no-op: the body is laid out normally.
#[typst_macros::time(span = elem.span())]
pub fn layout_skew(
    elem: &Packed<SkewElem>,
    engine: &mut Engine,
    _config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let _ = (elem.ax.get(styles), elem.ay.get(styles)); // skew angles ignored
    crate::flow::layout_term_frame(engine,&elem.body,
        typst_library::introspection::Locator::root(),
        styles,
        TermRegion::new(
            TermSize::new(lynchpin_library::resolve_page_size(styles).cols, TermScalar::INFINITY),
            Axes::new(false, false),
        ),
    )
}
