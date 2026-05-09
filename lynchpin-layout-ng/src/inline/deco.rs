use crossterm::style::ContentStyle;
use kurbo::BezPath;
use ttf_parser::OutlineBuilder;
use typst_library::layout::{Abs, Em};
use typst_library::text::{BottomEdge, DecoLine, Decoration, TextEdgeBounds, TextItem, TopEdge};
use typst_library::visualize::FixedStroke;
use typst_syntax::Span;

use lynchpin_library_ng::*;

// NOTE: styled_rect is expected from crate::shapes (Agent F)

/// Convert Abs to TermScalar via raw f64 (1:1 numeric, not unit-aware).
#[inline]
fn abs_to_scalar(abs: Abs) -> TermScalar {
    TermScalar::from_f64(abs.to_raw())
}

/// Add line decorations to a single run of shaped text.
pub fn decorate(
    frame: &mut TermFrame,
    deco: &Decoration,
    text: &TextItem,
    width: Abs,
    shift: Abs,
    pos: TermPoint,
) {
    let font_metrics = text.font.metrics();

    if let DecoLine::Highlight {
        fill: _,
        stroke: _,
        top_edge: _,
        bottom_edge: _,
        radius: _,
    } = &deco.line
    {
        // Highlight decoration requires styled_rect from crate::shapes (Agent F).
        // For now, fall through to the line-based decoration.
        // TODO: implement highlight when styled_rect is available.
    }

    let (stroke, metrics, offset, evade, background) = match &deco.line {
        DecoLine::Strikethrough {
            stroke,
            offset,
            background,
        } => (
            stroke,
            font_metrics.strikethrough,
            offset,
            false,
            *background,
        ),
        DecoLine::Overline {
            stroke,
            offset,
            evade,
            background,
        } => (stroke, font_metrics.overline, offset, *evade, *background),
        DecoLine::Underline {
            stroke,
            offset,
            evade,
            background,
        } => (stroke, font_metrics.underline, offset, *evade, *background),
        DecoLine::Highlight { .. } => return,
    };

    let offset_abs = offset.unwrap_or(-metrics.position.at(text.size)) - shift;
    let _stroke = stroke.clone().unwrap_or(FixedStroke::from_pair(
        text.fill.as_decoration(),
        metrics.thickness.at(text.size),
    ));

    let min_width_abs = 0.162 * text.size;

    // Convert pos.col to Abs for arithmetic
    let pos_col_abs = Abs::raw(pos.col.get() as f64);
    let start_abs = pos_col_abs - deco.extent;
    let end_abs = pos_col_abs + width + deco.extent;

    let mut push_segment = |from: Abs, to: Abs, prepend: bool| {
        let origin = TermPoint {
            col: abs_to_scalar(from),
            row: pos.row + abs_to_scalar(offset_abs),
        };
        let delta = TermPoint {
            col: abs_to_scalar(to - from),
            row: TermScalar::ZERO,
        };

        if (to - from) >= min_width_abs || !evade {
            let term_shape = TermShape {
                geometry: TermGeometry::Line { delta },
                fill: None,
                stroke_char: '─',
                style: ContentStyle::default(),
                span: Span::detached(),
            };

            if prepend {
                frame.prepend(origin, TermFrameItem::Shape(term_shape));
            } else {
                frame.push_shape(origin, term_shape);
            }
        }
    };

    if !evade {
        push_segment(start_abs, end_abs, background);
        return;
    }

    // For terminal: we simplify evasive line decoration since we can't
    // do pixel-perfect glyph outline intersection on a character grid.
    // We draw the decoration as segments, using the same approach but
    // without kurbo glyph-path intersection (which requires f64 precision).
    //
    // Instead, we use a simplified approach: split the line at glyph
    // boundaries, skipping segments that overlap with glyph ink.
    // For terminal rendering, we approximate by keeping the full line.
    push_segment(start_abs, end_abs, background);
}

// Return the top/bottom edge of the text given the metric of the font.
fn determine_edges(text: &TextItem, top_edge: TopEdge, bottom_edge: BottomEdge) -> (Abs, Abs) {
    let mut top = Abs::zero();
    let mut bottom = Abs::zero();

    for g in text.glyphs.iter() {
        let (t, b) = text.font.edges(
            top_edge,
            bottom_edge,
            text.size,
            TextEdgeBounds::Glyph(g.id),
        );
        top.set_max(t);
        bottom.set_max(b);
    }

    (top, bottom)
}

/// Builds a kurbo [`BezPath`] for a glyph.
struct BezPathBuilder {
    path: BezPath,
    units_per_em: f64,
    font_size: Abs,
    x_offset: f64,
}

impl BezPathBuilder {
    fn new(units_per_em: f64, font_size: Abs, x_offset: f64) -> Self {
        Self {
            path: BezPath::new(),
            units_per_em,
            font_size,
            x_offset,
        }
    }

    fn finish(self) -> BezPath {
        self.path
    }

    fn p(&self, x: f32, y: f32) -> kurbo::Point {
        kurbo::Point::new(self.s(x) + self.x_offset, -self.s(y))
    }

    fn s(&self, v: f32) -> f64 {
        Em::from_units(v, self.units_per_em)
            .at(self.font_size)
            .to_raw()
    }
}

impl OutlineBuilder for BezPathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(self.p(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(self.p(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(self.p(x1, y1), self.p(x, y));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path
            .curve_to(self.p(x1, y1), self.p(x2, y2), self.p(x, y));
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}
