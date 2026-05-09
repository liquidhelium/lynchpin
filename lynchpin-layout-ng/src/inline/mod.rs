//! Inline (paragraph) layout.
//!
//! Walks a typst content tree and produces a [`TermFrame`] for a single
//! paragraph.  Multi-row output arises from explicit [`LinebreakElem`]s or
//! automatic line-wrapping when [`TermConfig::width`] is set.
//!
//! Unlike paged layout, this module does **not** use HarfBuzz font shaping,
//! BiDi analysis, or Knuth-Plass linebreaking.  The terminal is a monospace
//! grid — character widths are determined by `wcwidth()`, and line wrapping
//! is greedy.
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
use typst::layout::{AlignElem, BoxElem, HElem, HideElem};
use typst::math::EquationElem;
use typst::model::{EmphElem, ParElem, StrongElem};
use typst::text::{
    DecoLine, HighlightElem, LinebreakElem, OverlineElem, SpaceElem, StrikeElem, SubElem,
    SuperElem, TextElem, UnderlineElem, WeightDelta,
};
use typst::visualize::Paint;
use typst::layout::FixedAlignment;

use lynchpin_library_ng::{
    Col, Row, TermFrame, TermPoint, TermSize, TermConfig, TermScalar, char_cols, text_cols,
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
    /// Display width in terminal columns.
    fn width(&self) -> Col {
        match self {
            InlineItem::Text(t, _) => text_cols(t),
            InlineItem::Space(w) => *w,
            InlineItem::Frame(f) => f.cols(),
            InlineItem::Break => TermScalar::ZERO,
        }
    }

    /// Rows strictly above the baseline (ascent).
    fn ascent(&self) -> Row {
        match self {
            InlineItem::Frame(f) => f.ascent(),
            _ => TermScalar::ZERO,
        }
    }

    /// Rows at or below the baseline (descent).
    fn descent(&self) -> Row {
        match self {
            InlineItem::Text(_, _) | InlineItem::Space(_) => TermScalar::ONE,
            InlineItem::Frame(f) => f.descent(),
            InlineItem::Break => TermScalar::ZERO,
        }
    }
}

// ── Paragraph situation ──────────────────────────────────────────────────────

/// Distinguishes between different kinds of paragraphs for first-line indent.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ParSituation {
    /// The paragraph is the first thing in the flow.
    First,
    /// The paragraph follows another paragraph.
    Consecutive,
    /// Any other kind of paragraph.
    Other,
}

/// Paragraph-level configuration extracted from styles.
struct ParaConfig {
    /// Whether to justify (spread spaces to fill the line width).
    justify: bool,
    /// First-line indent in columns, or zero.
    first_line_indent: Col,
    /// Hanging indent (all lines except first) in columns.
    hanging_indent: Col,
    /// Text alignment.
    align: FixedAlignment,
}

/// Extract paragraph config from styles and situation.
fn para_config(styles: StyleChain, situation: Option<ParSituation>) -> ParaConfig {
    let justify = styles.get(ParElem::justify);
    let hanging_indent = styles
        .get(ParElem::hanging_indent)
        .resolve(styles);

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

// ── Content-tree walker ───────────────────────────────────────────────────────

/// Recursively walk `content` and append [`InlineItem`]s to `items`.
fn collect_items(
    content: &Content,
    styles: StyleChain,
    base_style: ContentStyle,
    items: &mut Vec<InlineItem>,
    engine: &mut Engine,
    config: &TermConfig,
) {
    // ── Transparent wrappers ──────────────────────────────────────────────
    if let Some(seq) = content.to_packed::<SequenceElem>() {
        for child in &seq.children {
            collect_items(child, styles, base_style, items, engine, config);
        }
    } else if let Some(s) = content.to_packed::<StyledElem>() {
        collect_items(
            &s.child,
            styles.chain(&s.styles),
            base_style,
            items,
            engine,
            config,
        );

    // ── Plain text ────────────────────────────────────────────────────────
    } else if let Some(elem) = content.to_packed::<TextElem>() {
        let text: EcoString = if let Some(case) = styles.get(TextElem::case) {
            case.apply(&elem.text).into()
        } else {
            elem.text.clone()
        };
        if text.is_empty() {
            return;
        }

        let mut style = base_style;

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
            match fill {
                Paint::Solid(c) => {
                    let r = c.to_linear_rgb();
                    style.foreground_color = Some(Color::Rgb {
                        r: (r.red * 256.0) as u8,
                        g: (r.green * 256.0) as u8,
                        b: (r.blue * 256.0) as u8,
                    });
                }
                Paint::Gradient(_) | Paint::Tiling(_) => {}
            }
        }

        items.push(InlineItem::Text(text, style));

    // ── Whitespace / break ────────────────────────────────────────────────
    } else if content.is::<SpaceElem>() {
        items.push(InlineItem::Space(TermScalar::ONE));
    } else if content.is::<LinebreakElem>() {
        items.push(InlineItem::Break);

    // ── Inline math ───────────────────────────────────────────────────────
    } else if let Some(eq) = content.to_packed::<EquationElem>() {
        if !eq.block.get(styles) {
            if let Ok(eq_frame) = crate::math::layout_equation_inline(eq, engine, config, styles) {
                items.push(InlineItem::Frame(eq_frame));
            }
        }

    // ── Layout containers ─────────────────────────────────────────────────
    } else if let Some(elem) = content.to_packed::<HideElem>() {
        let mut hidden: Vec<InlineItem> = Vec::new();
        collect_items(&elem.body, styles, base_style, &mut hidden, engine, config);
        let width: Col = hidden.iter().map(|i| i.width()).sum();
        if width > TermScalar::ZERO {
            items.push(InlineItem::Space(width));
        }
    } else if let Some(elem) = content.to_packed::<BoxElem>() {
        if let Some(body) = elem.body.get_ref(styles) {
            collect_items(body, styles, base_style, items, engine, config);
        }
    } else if let Some(elem) = content.to_packed::<HElem>() {
        if !elem.amount.is_zero() {
            items.push(InlineItem::Space(TermScalar::ONE));
        }

    // ── Inline styling wrappers (fallback) ────────────────────────────────
    } else if let Some(elem) = content.to_packed::<StrongElem>() {
        let mut new_style = base_style;
        new_style.attributes.set(Attribute::Bold);
        collect_items(&elem.body, styles, new_style, items, engine, config);
    } else if let Some(elem) = content.to_packed::<EmphElem>() {
        let mut new_style = base_style;
        new_style.attributes.set(Attribute::Italic);
        collect_items(&elem.body, styles, new_style, items, engine, config);
    } else if let Some(elem) = content.to_packed::<UnderlineElem>() {
        let mut new_style = base_style;
        new_style.attributes.set(Attribute::Underlined);
        collect_items(&elem.body, styles, new_style, items, engine, config);
    } else if let Some(elem) = content.to_packed::<StrikeElem>() {
        let mut new_style = base_style;
        new_style.attributes.set(Attribute::CrossedOut);
        collect_items(&elem.body, styles, new_style, items, engine, config);
    } else if let Some(elem) = content.to_packed::<OverlineElem>() {
        let mut new_style = base_style;
        new_style.attributes.set(Attribute::OverLined);
        collect_items(&elem.body, styles, new_style, items, engine, config);
    } else if let Some(elem) = content.to_packed::<HighlightElem>() {
        let mut new_style = base_style;
        new_style.background_color = Some(Color::Yellow);
        collect_items(&elem.body, styles, new_style, items, engine, config);
    } else if let Some(elem) = content.to_packed::<SubElem>() {
        let mut new_style = base_style;
        new_style.foreground_color = Some(Color::DarkGrey);
        collect_items(&elem.body, styles, new_style, items, engine, config);
    } else if let Some(elem) = content.to_packed::<SuperElem>() {
        let mut new_style = base_style;
        new_style.foreground_color = Some(Color::DarkGrey);
        collect_items(&elem.body, styles, new_style, items, engine, config);
    }
    // All other element types are silently skipped in the inline context.
}

// ── Line-frame builder ────────────────────────────────────────────────────────

/// Compose a slice of [`InlineItem`]s into a single-line [`TermFrame`].
///
/// Items are baseline-aligned: each item is placed at
/// `row = max_ascent − item.ascent()`.
fn build_line_frame(items: &[InlineItem], justify: bool, available: Col) -> TermFrame {
    if items.is_empty() {
        return TermFrame::new(TermSize::new(TermScalar::ZERO, TermScalar::ONE));
    }

    let max_ascent: Row = items.iter().map(|i| i.ascent()).max().unwrap_or(TermScalar::ZERO);
    let max_descent: Row = items.iter().map(|i| i.descent()).max().unwrap_or(TermScalar::ONE);
    let total_rows = (max_ascent + max_descent).max(TermScalar::ONE);
    let total_cols: Col = items.iter().map(|i| i.width()).sum();

    // Justification: spread extra space evenly between Space items.
    let extra_space = if justify && available > total_cols && total_cols > TermScalar::ZERO {
        let space_count = items.iter().filter(|i| matches!(i, InlineItem::Space(_))).count();
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

// ── Public entry points ───────────────────────────────────────────────────────

/// Layout inline `content` (a paragraph body or any content tree) into a
/// [`TermFrame`].
///
/// This is the main entry point for inline layout.  It works with raw
/// `&Content` and is called by block-level collectors as well as by
/// `layout_par`.
pub fn layout_paragraph(
    engine: &mut Engine,
    content: &Content,
    config: &TermConfig,
    styles: StyleChain,
    base_style: ContentStyle,
    situation: Option<ParSituation>,
    max_width: Col,
) -> SourceResult<TermFrame> {
    let pc = para_config(styles, situation);

    // ── Step 1: flatten the content tree into inline items ────────────────
    let mut items: Vec<InlineItem> = Vec::new();
    collect_items(content, styles, base_style, &mut items, engine, config);

    // ── Step 2: split at breaks and wrap at config.width ──────────────────
    let first_line_width = max_width - pc.first_line_indent;
    let rest_line_width = max_width - pc.hanging_indent;

    let mut lines: Vec<(Vec<InlineItem>, Col)> = Vec::new(); // (items, available_width)
    let mut current: Vec<InlineItem> = Vec::new();
    let mut current_width: Col = TermScalar::ZERO;
    let mut line_index: usize = 0;

    let effective_width = |idx: usize| -> Col {
        if idx == 0 { first_line_width } else { rest_line_width }
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
                        continue; // discard leading space on new line
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

    if lines.is_empty() {
        return Ok(TermFrame::new(TermSize::new(TermScalar::ZERO, TermScalar::ONE)));
    }

    // ── Step 3: build a frame per line, then stack vertically ─────────────
    let justify = pc.justify;
    let line_frames: Vec<TermFrame> = lines
        .iter()
        .map(|(l, avail)| build_line_frame(l, justify, *avail))
        .collect();

    let max_cols: Col = line_frames.iter().map(|f| f.cols()).max().unwrap_or(TermScalar::ZERO);
    let total_rows: Row = line_frames.iter().map(|f| f.rows().max(TermScalar::ONE)).sum();

    let first_baseline = line_frames.first().map(|f| f.baseline()).unwrap_or(TermScalar::ZERO);

    let mut out = TermFrame::new(TermSize::new(max_cols.max(TermScalar::ZERO), total_rows.max(TermScalar::ZERO)));
    out.set_baseline(first_baseline);

    let mut y: Row = TermScalar::ZERO;
    for frame in line_frames {
        let h = frame.rows().max(TermScalar::ONE);
        out.push_frame(TermPoint::new(TermScalar::ZERO, y), frame);
        y = y + h;
    }

    Ok(out)
}

/// Layout a `ParElem` — the block-level entry point called by flow
/// collectors.
///
/// This extracts the paragraph's body and situation-specific config, then
/// delegates to [`layout_paragraph`].
pub fn layout_par(
    elem: &Packed<ParElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
    base_style: ContentStyle,
    situation: Option<ParSituation>,
) -> SourceResult<TermFrame> {
    let max_width = lynchpin_library_ng::resolve_page_size(styles).cols;
    layout_paragraph(engine, &elem.body, config, styles, base_style, situation, max_width)
}
