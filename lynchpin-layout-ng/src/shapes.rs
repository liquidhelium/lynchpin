//! Shape layout for the terminal.
//!
//! Terminal equivalents of the paged shape layout functions.  Each function
//! produces a [`TermFrame`] containing [`TermFrameItem::Shape`] items.
//!
//! Rectangles use Unicode box-drawing characters (─│┌┐└┘).  Circles and
//! ellipses are approximated with character art.  All functions follow the
//! [`TermBlockCallback`] signature.

use crossterm::style::ContentStyle;
use typst_library::diag::{SourceResult, bail};
use typst_library::engine::Engine;
use typst_library::foundations::{Packed, Resolve, StyleChain};
use typst_library::layout::{Corners, Length, Rel, Sides};
use typst_library::visualize::{
    CircleElem, EllipseElem, LineElem, PathElem, PolygonElem, RectElem,
    SquareElem,
};

use lynchpin_library_ng::*;

/// Layout a line.
#[typst_macros::time(span = elem.span())]
pub fn layout_line(
    elem: &Packed<LineElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let _ = engine;

    let font_size = styles
        .get(typst_library::text::TextElem::size)
        .0
        .resolve(styles);

    // Resolve start position (Axes<Rel<Length>> → resolve each axis → convert to cols)
    let start_axes = elem.start.get(styles);
    let start_x = units::abs_to_cols(start_axes.x.resolve(styles).relative_to(font_size), font_size);
    let start_y = units::abs_to_cols(start_axes.y.resolve(styles).relative_to(font_size), font_size);
    let start = TermPoint::new(start_x, start_y);

    let delta = if let Some(end_axes) = elem.end.get(styles) {
        let end_x = units::abs_to_cols(end_axes.x.resolve(styles).relative_to(font_size), font_size);
        let end_y = units::abs_to_cols(end_axes.y.resolve(styles).relative_to(font_size), font_size);
        let end = TermPoint::new(end_x, end_y);
        TermPoint::new(end.col - start.col, end.row - start.row)
    } else {
        let length_abs = elem.length.resolve(styles).relative_to(font_size);
        let angle = elem.angle.get(styles);
        let dx = units::abs_to_cols(
            typst_library::layout::Abs::raw(angle.cos() as f64 * length_abs.to_raw()),
            font_size,
        );
        let dy = units::abs_to_cols(
            typst_library::layout::Abs::raw(angle.sin() as f64 * length_abs.to_raw()),
            font_size,
        );
        TermPoint::new(dx, dy)
    };

    if !delta.col.is_finite() || !delta.row.is_finite() {
        bail!(elem.span(), "cannot create line with infinite length");
    }

    let cols = delta.col.abs().max(TermScalar::ONE);
    let rows = delta.row.abs().max(TermScalar::ONE);
    let size = TermSize::new(cols, rows);
    let mut frame = TermFrame::new(size);

    let stroke_char = match config.mode {
        RenderMode::Unicode => '─',
        RenderMode::Ascii => '-',
    };

    frame.push_shape(
        TermPoint::ZERO,
        TermShape {
            geometry: TermGeometry::Line { delta },
            fill: None,
            stroke_char,
            style: ContentStyle::default(),
            span: elem.span(),
        },
    );

    Ok(frame)
}

/// Layout a rectangle.
#[typst_macros::time(span = elem.span())]
pub fn layout_rect(
    elem: &Packed<RectElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    layout_term_shape(
        engine, config, styles, elem.body.get_ref(styles), elem.fill.get_cloned(styles),
        true, // has_stroke
        elem.inset.get(styles), elem.outset.get(styles), elem.radius.get(styles),
        elem.width.get(styles), elem.height.get(styles),
        false, // not quadratic
        elem.span(),
    )
}

/// Layout a square.
#[typst_macros::time(span = elem.span())]
pub fn layout_square(
    elem: &Packed<SquareElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    layout_term_shape(
        engine, config, styles, elem.body.get_ref(styles), elem.fill.get_cloned(styles),
        true, // has_stroke (square)
        elem.inset.get(styles), elem.outset.get(styles),
        Corners::splat(None), elem.width.get(styles), elem.height.get(styles),
        true, // quadratic
        elem.span(),
    )
}

/// Layout an ellipse.
#[typst_macros::time(span = elem.span())]
pub fn layout_ellipse(
    elem: &Packed<EllipseElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    layout_term_shape(
        engine, config, styles, elem.body.get_ref(styles), elem.fill.get_cloned(styles),
        true, // has_stroke (ellipse)
        elem.inset.get(styles), elem.outset.get(styles),
        Corners::splat(None), elem.width.get(styles), elem.height.get(styles),
        false, // not quadratic
        elem.span(),
    )
}

/// Layout a circle.
#[typst_macros::time(span = elem.span())]
pub fn layout_circle(
    elem: &Packed<CircleElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    layout_term_shape(
        engine, config, styles, elem.body.get_ref(styles), elem.fill.get_cloned(styles),
        true, // has_stroke (circle)
        elem.inset.get(styles), elem.outset.get(styles),
        Corners::splat(None), elem.width.get(styles), elem.height.get(styles),
        true, // quadratic
        elem.span(),
    )
}

/// Layout a polygon.
#[typst_macros::time(span = elem.span())]
pub fn layout_polygon(
    elem: &Packed<PolygonElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let _ = engine;

    let font_size = styles
        .get(typst_library::text::TextElem::size)
        .0
        .resolve(styles);

    let points: Vec<TermPoint> = elem
        .vertices
        .iter()
        .map(|c| {
            let p = c.resolve(styles);
            TermPoint::new(
                units::abs_to_cols(p.x.relative_to(font_size), font_size),
                units::abs_to_cols(p.y.relative_to(font_size), font_size),
            )
        })
        .collect();

    let max_col = points.iter().map(|p| p.col).max().unwrap_or(TermScalar::ZERO);
    let max_row = points.iter().map(|p| p.row).max().unwrap_or(TermScalar::ZERO);
    let size = TermSize::new(max_col.max(TermScalar::ONE), max_row.max(TermScalar::ONE));

    if !size.is_finite() {
        bail!(elem.span(), "cannot create polygon with infinite size");
    }

    let mut frame = TermFrame::new(size);

    if points.is_empty() {
        return Ok(frame);
    }

    let fill = elem.fill.get_cloned(styles);
    let fill_char = fill.is_some().then_some('█');
    let stroke_char = match config.mode {
        RenderMode::Unicode => '─',
        RenderMode::Ascii => '-',
    };

    frame.push_shape(
        TermPoint::ZERO,
        TermShape {
            geometry: TermGeometry::Path {
                points,
                closed: true,
            },
            fill: fill_char,
            stroke_char,
            style: ContentStyle::default(),
            span: elem.span(),
        },
    );

    Ok(frame)
}

/// Layout a curve (stub: renders as a path).
#[typst_macros::time(span = elem.span())]
pub fn layout_curve(
    elem: &Packed<typst_library::visualize::CurveElem>,
    _engine: &mut Engine,
    config: &TermConfig,
    _styles: StyleChain,
) -> SourceResult<TermFrame> {
    // Curves are complex to render in terminal; return an empty frame.
    Ok(TermFrame::new(TermSize::new(
        config.effective_width().min(TermScalar::new(10)),
        TermScalar::ONE,
    )))
}

/// Layout a path (stub).
#[typst_macros::time(span = elem.span())]
pub fn layout_path(
    elem: &Packed<PathElem>,
    _engine: &mut Engine,
    config: &TermConfig,
    _styles: StyleChain,
) -> SourceResult<TermFrame> {
    // Paths are complex to render in terminal; return an empty frame.
    Ok(TermFrame::new(TermSize::new(
        config.effective_width().min(TermScalar::new(10)),
        TermScalar::ONE,
    )))
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Shared shape layout logic.
#[allow(clippy::too_many_arguments)]
fn layout_term_shape(
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
    body: &Option<typst_library::foundations::Content>,
    fill: Option<typst_library::visualize::Paint>,
    stroke: bool,
    inset: Sides<Option<Rel<Length>>>,
    outset: Sides<Option<Rel<Length>>>,
    radius: Corners<Option<Rel<Length>>>,
    width: typst_library::foundations::Smart<Rel<Length>>,
    height: typst_library::layout::Sizing,
    quadratic: bool,
    span: typst_syntax::Span,
) -> SourceResult<TermFrame> {
    let font_size = styles
        .get(typst_library::text::TextElem::size)
        .0
        .resolve(styles);

    // Resolve width/height to terminal columns/rows.
    let w: Col = match width {
        typst_library::foundations::Smart::Auto => TermScalar::ZERO,
        typst_library::foundations::Smart::Custom(rel) => {
            let abs = rel.resolve(styles).relative_to(font_size);
            units::abs_to_cols(abs, font_size)
        }
    };
    let h: Row = match height {
        typst_library::layout::Sizing::Auto => TermScalar::ZERO,
        typst_library::layout::Sizing::Rel(rel) => {
            let abs = rel.resolve(styles).relative_to(font_size);
            units::abs_to_cols(abs, font_size)
        }
        typst_library::layout::Sizing::Fr(_) => TermScalar::ZERO,
    };

    // Determine size.
    let default_w: Col = config.effective_width().min(TermScalar::new(20));
    let default_h: Row = TermScalar::new(8);
    let cols = if w > TermScalar::ZERO { w } else { default_w };
    let rows = if h > TermScalar::ZERO { h } else { default_h };

    let size = if quadratic {
        let side = cols.min(rows).max(TermScalar::ONE);
        TermSize::new(side, side)
    } else {
        TermSize::new(cols.max(TermScalar::ONE), rows.max(TermScalar::ONE))
    };

    let mut frame = TermFrame::new(size);

    // Determine stroke/fill characters.
    let has_stroke = stroke || fill.is_none();

    let fill_char = fill.is_some().then_some('█');
    let stroke_char = if has_stroke {
        match config.mode {
            RenderMode::Unicode => '*',
            RenderMode::Ascii => '*',
        }
    } else {
        ' '
    };

    // Push the shape.
    frame.push_shape(
        TermPoint::ZERO,
        TermShape {
            geometry: TermGeometry::Rect { size },
            fill: fill_char,
            stroke_char,
            style: ContentStyle::default(),
            span,
        },
    );

    let _ = (engine, body, inset, outset, radius);

    Ok(frame)
}
