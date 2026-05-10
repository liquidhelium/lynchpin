//! Inline (paragraph) layout.
//!
//! Walks a typst content tree (or realized [`Pair`] stream) and produces a
//! [`TermFrame`] for a single paragraph.  Multi-row output arises from
//! explicit [`LinebreakElem`]s or automatic line-wrapping.
//!
//! Unlike paged layout, this module does **not** use HarfBuzz font shaping,
//! BiDi analysis, or Knuth-Plass linebreaking.  The terminal is a monospace
//! grid — character widths are determined by `wcwidth()`, and line wrapping
//! is greedy.  The API shape mirrors paged `layout_inline` / `layout_par`.
//!
//! # Inline item model
//!
//! The tree is first flattened into a sequence of [`InlineItem`]s:
//!
//! | Source element              | Inline item             |
//! |-----------------------------|-------------------------|
//! | `TextElem`                  | `Text(str, style)`      |
//! | `SpaceElem`                 | `Space(1)`              |
//! | `LinebreakElem`             | `Break`                 |
//! | `HElem` (non-zero amount)   | `Space(1)`              |
//! | `EquationElem` (inline)     | `Frame([eq])`           |
//! | `BoxElem`                   | recurse into body       |
//! | `SequenceElem`              | recurse into children   |
//! | `StyledElem`                | chain styles + recurse  |
//! | `StrongElem` (fallback)     | `Text` with bold        |
//! | `EmphElem`   (fallback)     | `Text` with italic      |
//! | Others                      | silently skipped        |
//!
//! Items are then split at `Break` boundaries; each segment becomes one row.
//! Within a row, items with different ascents/descents are baseline-aligned.

use crossterm::style::{Attribute, Color, ContentStyle};
use ecow::EcoString;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, Packed, Resolve, SequenceElem, StyleChain, StyledElem};
use typst::introspection::SplitLocator;
use typst::layout::FixedAlignment;
use typst::layout::{AlignElem, BoxElem, HElem, HideElem};
use typst::math::EquationElem;
use typst::model::{EmphElem, ParElem, StrongElem};
use typst::routines::Pair;
use typst::text::{
    DecoLine, HighlightElem, LinebreakElem, OverlineElem, SpaceElem, StrikeElem, SubElem,
    SuperElem, TextElem, UnderlineElem, WeightDelta,
};
use typst::visualize::Paint;

use lynchpin_library_ng::{
    Col, Row, TermConfig, TermFragment, TermFrame, TermInlineElem, TermInlineItem, TermPoint,
    TermRegion, TermScalar, TermSize, char_cols, text_cols,
};

// ── Inline item ───────────────────────────────────────────────────────────────

/// A single resolved item in the inline layout stream.
enum InlineItem {
    /// A styled text run.
    Text(EcoString, ContentStyle),
    /// Horizontal whitespace (always ≥ 1 col).
    Space(Col),
    /// A sub-frame with its own baseline (e.g. inline equation).
    Frame(TermFrame),
    /// Explicit line break — causes the current line to be flushed.
    Break,
}

impl InlineItem {
    fn width(&self) -> Col {
        match self {
            InlineItem::Text(t, _) => text_cols(t),
            InlineItem::Space(w) => *w,
            InlineItem::Frame(f) => f.cols(),
            InlineItem::Break => TermScalar::ZERO,
        }
    }
    fn ascent(&self) -> Row {
        match self {
            InlineItem::Frame(f) => f.ascent(),
            _ => TermScalar::ZERO,
        }
    }
    fn descent(&self) -> Row {
        match self {
            InlineItem::Text(_, _) | InlineItem::Space(_) => TermScalar::ONE,
            InlineItem::Frame(f) => f.descent(),
            InlineItem::Break => TermScalar::ZERO,
        }
    }
}

// ── Paragraph situation ──────────────────────────────────────────────────────

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ParSituation {
    First,
    Consecutive,
    Other,
}

struct ParaConfig {
    justify: bool,
    first_line_indent: Col,
    hanging_indent: Col,
    align: FixedAlignment,
}

fn para_config(styles: StyleChain, situation: Option<ParSituation>) -> ParaConfig {
    let justify = styles.get(ParElem::justify);
    let hanging_indent = styles.get(ParElem::hanging_indent).resolve(styles);

    let first_line_indent = if situation.is_some() {
        let amount = styles.get(ParElem::first_line_indent).amount;
        if amount != typst::layout::Length::zero() {
            let font_size = styles.get(TextElem::size).0.resolve(styles);
            TermScalar::from_f64((amount.at(font_size) / font_size).round())
        } else {
            TermScalar::ZERO
        }
    } else {
        TermScalar::ZERO
    };

    let hanging_cols = if situation.is_some() {
        let font_size = styles.get(TextElem::size).0.resolve(styles);
        TermScalar::from_f64((hanging_indent / font_size).round())
    } else {
        TermScalar::ZERO
    };

    let align = styles.get(AlignElem::alignment).resolve(styles).x;

    ParaConfig {
        justify,
        first_line_indent,
        hanging_indent: hanging_cols,
        align,
    }
}

// ── Pair → InlineItem conversion ──────────────────────────────────────────────

/// Convert realized [`Pair`]s into [`InlineItem`]s.
fn collect_items_from_pairs(
    pairs: &[Pair],
    items: &mut Vec<InlineItem>,
    engine: &mut Engine,
    region: TermSize,
) {
    for &(child, styles) in pairs {
        if let Some(elem) = child.to_packed::<TextElem>() {
            let text: EcoString = if let Some(case) = styles.get(TextElem::case) {
                case.apply(&elem.text).into()
            } else {
                elem.text.clone()
            };
            if text.is_empty() {
                continue;
            }

            let mut style = ContentStyle::default();
            let WeightDelta(delta) = styles.get(TextElem::delta);
            if delta > 0 {
                style.attributes.set(Attribute::Bold);
            }
            if styles.get(TextElem::emph).0 {
                style.attributes.set(Attribute::Italic);
            }
            for deco in styles.get_cloned(TextElem::deco).iter() {
                match &deco.line {
                    DecoLine::Underline { .. } => {
                        style.attributes.set(Attribute::Underlined);
                    }
                    DecoLine::Strikethrough { .. } => {
                        style.attributes.set(Attribute::CrossedOut);
                    }
                    DecoLine::Overline { .. } => {
                        style.attributes.set(Attribute::OverLined);
                    }
                    DecoLine::Highlight { .. } => {
                        style.background_color = Some(Color::Yellow);
                    }
                }
            }
            let fill = styles.get_cloned(TextElem::fill);
            if fill != typst::visualize::Color::BLACK.into() {
                if let Paint::Solid(c) = fill {
                    let r = c.to_linear_rgb();
                    style.foreground_color = Some(Color::Rgb {
                        r: (r.red * 256.0) as u8,
                        g: (r.green * 256.0) as u8,
                        b: (r.blue * 256.0) as u8,
                    });
                }
            }
            items.push(InlineItem::Text(text, style));
        } else if child.is::<SpaceElem>() {
            items.push(InlineItem::Space(TermScalar::ONE));
        } else if child.is::<LinebreakElem>() {
            items.push(InlineItem::Break);
        } else if let Some(elem) = child.to_packed::<ParElem>() {
            use typst::routines::{Arenas, RealizationKind};
            let arenas = Arenas::default();
            let mut kind = typst::routines::FragmentKind::Inline;
            if let Ok(body_pairs) = (engine.routines.realize)(
                RealizationKind::LayoutFragment { kind: &mut kind },
                engine,
                &mut typst::introspection::Locator::root().split(),
                &arenas,
                &elem.body,
                styles,
            ) {
                tracing::debug!("ParElem body: {} pairs", body_pairs.len()); for (bp, _) in &body_pairs { tracing::debug!("  body pair: {}", bp.elem().name()); } collect_items_from_pairs(&body_pairs, items, engine, region);
            }
        } else if let Some(elem) = child.to_packed::<TermInlineElem>() {
            let loc = typst::introspection::Locator::root();
            let region = TermRegion::new(region, typst::layout::Axes::new(false, false));
            if let Some(Ok(inline_items)) = elem
                .cb
                .get_ref(styles)
                .as_ref()
                .map(|cb| cb.call(engine, loc, styles, region))
            {
                for ii in inline_items {
                    match ii {
                        TermInlineItem::Text(t, s) => items.push(InlineItem::Text(t, s)),
                        TermInlineItem::Frame(f) => items.push(InlineItem::Frame(f)),
                    }
                }
            }
        } else {
            engine.sink.warn(typst::__warning!(
                child.span(),
                "{} was ignored during terminal layout",
                child.elem().name()
            ));
        }
        // Other element types are skipped in inline pair context.
    }
}

// ── Line-frame builder ────────────────────────────────────────────────────────

fn build_line_frame(items: &[InlineItem], justify: bool, available: Col) -> TermFrame {
    if items.is_empty() {
        return TermFrame::new(TermSize::new(TermScalar::ZERO, TermScalar::ONE));
    }

    let max_ascent: Row = items
        .iter()
        .map(|i| i.ascent())
        .max()
        .unwrap_or(TermScalar::ZERO);
    let max_descent: Row = items
        .iter()
        .map(|i| i.descent())
        .max()
        .unwrap_or(TermScalar::ONE);
    let total_rows = (max_ascent + max_descent).max(TermScalar::ONE);
    let total_cols: Col = items.iter().map(|i| i.width()).sum();

    let extra_space = if justify && available > total_cols && total_cols > TermScalar::ZERO {
        let space_count = items
            .iter()
            .filter(|i| matches!(i, InlineItem::Space(_)))
            .count();
        if space_count > 0 {
            (available - total_cols) / TermScalar::new(space_count as i32)
        } else {
            TermScalar::ZERO
        }
    } else {
        TermScalar::ZERO
    };

    let frame_width = if justify { available } else { total_cols };
    let mut frame = TermFrame::new(TermSize::new(frame_width.max(TermScalar::ZERO), total_rows));
    frame.set_baseline(max_ascent);

    let mut x: Col = TermScalar::ZERO;
    for item in items {
        let row = max_ascent - item.ascent();
        match item {
            InlineItem::Text(t, style) => {
                let w = text_cols(t);
                if w > TermScalar::ZERO {
                    frame.push_text(TermPoint::new(x, row), t.clone(), *style);
                }
                x = x + w;
            }
            InlineItem::Space(w) => {
                x = x + *w + extra_space;
            }
            InlineItem::Frame(f) => {
                let w = f.cols();
                frame.push_frame(TermPoint::new(x, row), f.clone());
                x = x + w;
            }
            InlineItem::Break => {}
        }
    }
    frame
}

// ── Greedy line wrapping (shared) ─────────────────────────────────────────────

/// Split [`InlineItem`]s into lines using greedy wrapping.
fn greedy_wrap(
    items: Vec<InlineItem>,
    max_width: Col,
    pc: &ParaConfig,
) -> Vec<(Vec<InlineItem>, Col)> {
    let first_line_width = max_width - pc.first_line_indent;
    let rest_line_width = max_width - pc.hanging_indent;

    let mut lines: Vec<(Vec<InlineItem>, Col)> = Vec::new();
    let mut current: Vec<InlineItem> = Vec::new();
    let mut current_width: Col = TermScalar::ZERO;
    let mut line_index: usize = 0;

    let effective_width = |idx: usize| {
        if idx == 0 {
            first_line_width
        } else {
            rest_line_width
        }
    };

    for item in items {
        match item {
            InlineItem::Break => {
                lines.push((std::mem::take(&mut current), effective_width(line_index)));
                current_width = TermScalar::ZERO;
                line_index += 1;
            }
            other => {
                let item_w = other.width();
                let limit = effective_width(line_index);
                if current_width + item_w > limit && !current.is_empty() {
                    if !matches!(other, InlineItem::Space(_)) {
                        lines.push((std::mem::take(&mut current), effective_width(line_index)));
                        current_width = TermScalar::ZERO;
                        line_index += 1;
                    } else {
                        continue;
                    }
                }
                current_width = current_width + item_w;
                current.push(other);
            }
        }
    }
    if !current.is_empty() {
        lines.push((current, effective_width(line_index)));
    }
    lines
}

/// Build frames from wrapped lines, stacked vertically.
fn build_paragraph_frame(lines: Vec<(Vec<InlineItem>, Col)>, justify: bool) -> TermFrame {
    if lines.is_empty() {
        return TermFrame::new(TermSize::new(TermScalar::ZERO, TermScalar::ONE));
    }

    let line_frames: Vec<TermFrame> = lines
        .iter()
        .map(|(l, avail)| build_line_frame(l, justify, *avail))
        .collect();

    let max_cols: Col = line_frames
        .iter()
        .map(|f| f.cols())
        .max()
        .unwrap_or(TermScalar::ZERO);
    let total_rows: Row = line_frames
        .iter()
        .map(|f| f.rows().max(TermScalar::ONE))
        .sum();
    let first_baseline = line_frames
        .first()
        .map(|f| f.baseline())
        .unwrap_or(TermScalar::ZERO);

    let mut out = TermFrame::new(TermSize::new(
        max_cols.max(TermScalar::ZERO),
        total_rows.max(TermScalar::ZERO),
    ));
    out.set_baseline(first_baseline);

    let mut y: Row = TermScalar::ZERO;
    for frame in line_frames {
        let h = frame.rows().max(TermScalar::ONE);
        out.push_frame(TermPoint::new(TermScalar::ZERO, y), frame);
        y = y + h;
    }
    out
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Layout already-realized inline-level [`Pair`]s.
///
/// This is the terminal equivalent of paged `layout_inline(&[Pair], ...)`.
/// It is the primary entry point for inline layout from the flow collector.
pub fn layout_inline<'a>(
    engine: &mut Engine,
    children: &[Pair<'a>],
    _locator: &mut SplitLocator<'a>,
    shared: StyleChain<'a>,
    region: TermSize,
    _expand: bool,
) -> SourceResult<TermFragment> {
    let pc = para_config(shared, None);

    let mut items: Vec<InlineItem> = Vec::new();
    collect_items_from_pairs(children, &mut items, engine, region);

    let lines = greedy_wrap(items, region.cols, &pc);
    Ok(vec![build_paragraph_frame(lines, pc.justify)])
}

/// Layout a [`ParElem`] — the block-level entry point called by flow
/// collectors.
pub fn layout_par(
    elem: &Packed<ParElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
    base_style: ContentStyle,
    situation: Option<ParSituation>,
) -> SourceResult<TermFrame> {
    layout_paragraph(
        engine,
        &elem.body,
        config,
        styles,
        base_style,
        situation,
        TermScalar::new(80),
    )
}

/// Layout inline `content` (a paragraph body or any content tree) into a
/// [`TermFrame`].  Realizes internally, then uses the pair walker.
pub fn layout_paragraph(
    engine: &mut Engine,
    content: &Content,
    config: &TermConfig,
    styles: StyleChain,
    base_style: ContentStyle,
    situation: Option<ParSituation>,
    max_width: Col,
) -> SourceResult<TermFrame> {
    // Realize with Inline kind — no PAR grouping, show rules still apply.
    use typst::routines::Arenas;
    use typst::model::DocumentInfo;
    let arenas = Arenas::default();
    let children = lynchpin_term_realize::realize_term(
        engine, &arenas, &mut DocumentInfo::default(), content, styles,
        lynchpin_term_realize::TermRealizationKind::Inline,
    )?;

    let pc = para_config(styles, situation);
    let region = TermSize::new(max_width, TermScalar::INFINITY);
    let mut items: Vec<InlineItem> = Vec::new();
    collect_items_from_pairs(&children, &mut items, engine, region);

    let lines = greedy_wrap(items, max_width, &pc);
    Ok(build_paragraph_frame(lines, pc.justify))
}
