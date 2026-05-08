//! Inline (paragraph) layout.
//!
//! Walks a typst content tree and produces a [`TermFrame`] for a single
//! paragraph.  Multi-row output arises from explicit [`LinebreakElem`]s or
//! automatic line-wrapping when [`TermConfig::width`] is set.
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
//! | `EquationElem` (inline)     | `Frame([eq] stub)`      |
//! | `BoxElem`                   | recurse into body       |
//! | `SequenceElem`              | recurse into children   |
//! | `StyledElem`                | chain styles + recurse  |
//! | `StrongElem` (fallback)     | `Text` with bold        |
//! | `EmphElem`   (fallback)     | `Text` with italic      |
//! | Others                      | silently skipped        |
//!
//! Items are then split at `Break` boundaries; each segment becomes one row.
//! Within a row, items with different ascents/descents are baseline-aligned
//! (important for inline math frames that can be taller than 1 row).

use crossterm::style::{Attribute, Color, ContentStyle};
use ecow::EcoString;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, SequenceElem, StyleChain, StyledElem};
use typst::layout::{BoxElem, HElem, HideElem};
use typst::math::EquationElem;
use typst::model::{EmphElem, StrongElem};
use typst::text::{DecoLine, HighlightElem, LinebreakElem, OverlineElem, SpaceElem,
    StrikeElem, SubElem, SuperElem, TextElem, UnderlineElem, WeightDelta};
use typst::visualize::Paint;

use crate::config::TermConfig;
use crate::frame::{Col, Row, TermFrame, TermPoint, TermSize, text_cols};

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
            InlineItem::Text(t, _) => text_cols(t) as Col,
            InlineItem::Space(w) => *w,
            InlineItem::Frame(f) => f.cols(),
            InlineItem::Break => 0,
        }
    }

    /// Rows strictly above the baseline (ascent).
    fn ascent(&self) -> Row {
        match self {
            InlineItem::Frame(f) => f.ascent(),
            _ => 0,
        }
    }

    /// Rows at or below the baseline (descent).
    fn descent(&self) -> Row {
        match self {
            InlineItem::Text(_, _) | InlineItem::Space(_) => 1,
            InlineItem::Frame(f) => f.descent(),
            InlineItem::Break => 0,
        }
    }
}

// ── Content-tree walker ───────────────────────────────────────────────────────

/// Recursively walk `content` and append [`InlineItem`]s to `items`.
///
/// `styles` is the ambient style chain (chained through `StyledElem`s).
/// `base_style` is a `ContentStyle` accumulated from structural wrappers
/// (`StrongElem`, `EmphElem`, heading style, etc.) and merged with
/// `StyleChain`-derived attributes at each `TextElem`.
fn collect_items(
    content: &Content,
    styles: StyleChain,
    base_style: ContentStyle,
    items: &mut Vec<InlineItem>,
    engine: &mut Engine,
    config: &TermConfig,
) {
    // ── Transparent wrappers ──────────────────────────────────────────────────
    if let Some(seq) = content.to_packed::<SequenceElem>() {
        for child in &seq.children {
            collect_items(child, styles, base_style, items, engine, config);
        }

    } else if let Some(s) = content.to_packed::<StyledElem>() {
        collect_items(&s.child, styles.chain(&s.styles), base_style, items, engine, config);

    // ── Plain text ────────────────────────────────────────────────────────────
    } else if let Some(elem) = content.to_packed::<TextElem>() {
        let text: EcoString = if let Some(case) = styles.get(TextElem::case) {
            case.apply(&elem.text).into()
        } else {
            elem.text.clone()
        };
        if text.is_empty() {
            return;
        }

        // Start from the inherited base style, then layer in StyleChain attrs.
        let mut style = base_style;

        let WeightDelta(delta) = styles.get(TextElem::delta);
        if delta > 0 {
            style.attributes.set(Attribute::Bold);
        }
        if styles.get(TextElem::emph).0 {
            style.attributes.set(Attribute::Italic);
        }

        // Decorations (underline, strikethrough, overline, highlight) may be
        // set by the paged pipeline on the StyleChain via TextElem::deco.
        for deco in styles.get_cloned(TextElem::deco).iter() {
            match &deco.line {
                DecoLine::Underline { .. }    => { style.attributes.set(Attribute::Underlined); }
                DecoLine::Strikethrough { .. } => { style.attributes.set(Attribute::CrossedOut); }
                DecoLine::Overline { .. }      => { style.attributes.set(Attribute::OverLined); }
                DecoLine::Highlight { .. }     => { style.background_color = Some(Color::Yellow); }
            }
        }

        // Text fill colour.
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
                // Gradients and tilings are not renderable in a terminal; keep
                // whatever foreground colour was inherited.
                Paint::Gradient(_) | Paint::Tiling(_) => {}
            }
        }

        items.push(InlineItem::Text(text, style));

    // ── Whitespace ────────────────────────────────────────────────────────────
    } else if content.is::<SpaceElem>() {
        items.push(InlineItem::Space(1));

    } else if content.is::<LinebreakElem>() {
        items.push(InlineItem::Break);

    // ── Inline math ────────────────────────────────────────────────────────────────────
    } else if let Some(eq) = content.to_packed::<EquationElem>() {
        if !eq.block.get(styles) {
            // Use the real math layout engine.
            if let Ok(eq_frame) =
                crate::math::layout_equation_inline(eq, engine, config, styles)
            {
                items.push(InlineItem::Frame(eq_frame));
            }
        }
        // Block equations inside inline context: skip.

    // ── Layout containers ─────────────────────────────────────────────────────
    } else if let Some(elem) = content.to_packed::<HideElem>() {
        // Measure the body to get its display width, then emit equivalent
        // blank space so the hidden content still occupies the right amount
        // of columns.
        let mut hidden: Vec<InlineItem> = Vec::new();
        collect_items(&elem.body, styles, base_style, &mut hidden, engine, config);
        let width: Col = hidden.iter().map(|i| i.width()).sum();
        if width > 0 {
            items.push(InlineItem::Space(width));
        }

    } else if let Some(elem) = content.to_packed::<BoxElem>() {
        if let Some(body) = elem.body.get_ref(styles) {
            collect_items(body, styles, base_style, items, engine, config);
        }

    } else if let Some(elem) = content.to_packed::<HElem>() {
        if !elem.amount.is_zero() {
            items.push(InlineItem::Space(1));
        }

    // ── Inline styling wrappers (fallback — not re-realized by realize_term) ──
    // In the top-level realize stream StrongElem/EmphElem are converted to
    // TextElem::delta/emph via StyledElem.  Inside nested content (list bodies,
    // BoxElem bodies, …) they may still appear literally.
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
fn build_line_frame(items: &[InlineItem]) -> TermFrame {
    if items.is_empty() {
        // Emit an empty 1-row frame so that the line still occupies vertical
        // space (blank lines must be visible in the stacked output).
        return TermFrame::new(TermSize::new(0, 1));
    }

    let max_ascent: Row  = items.iter().map(|i| i.ascent()).max().unwrap_or(0);
    let max_descent: Row = items.iter().map(|i| i.descent()).max().unwrap_or(1);
    let total_rows       = (max_ascent + max_descent).max(1);
    let total_cols: Col  = items.iter().map(|i| i.width()).sum();

    let mut frame = TermFrame::new(TermSize::new(total_cols.max(0), total_rows));
    frame.set_baseline(max_ascent);

    let mut x: Col = 0;
    for item in items {
        let row = max_ascent - item.ascent();
        match item {
            InlineItem::Text(t, style) => {
                let w = text_cols(t) as Col;
                if w > 0 {
                    frame.push_text(TermPoint::new(x, row), t.clone(), *style);
                }
                x += w;
            }
            InlineItem::Space(w) => {
                x += w;
            }
            InlineItem::Frame(f) => {
                let w = f.cols();
                frame.push_frame(TermPoint::new(x, row), f.clone());
                x += w;
            }
            InlineItem::Break => {}
        }
    }

    frame
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Layout inline `content` (a paragraph body or any content tree) into a
/// [`TermFrame`].
///
/// # Parameters
///
/// - `content`    – The root of the content tree to render (e.g. `par.body`).
/// - `config`     – Terminal render config (used for line-wrap width).
/// - `styles`     – Ambient style chain at the call site.
/// - `base_style` – Additional `ContentStyle` applied on top of everything
///                  (useful for headings where the caller wants bold + color).
///
/// # Output
///
/// A multi-row [`TermFrame`] where each row corresponds to one wrapped or
/// explicitly-broken line.  The frame's baseline is the baseline of the first
/// line.
pub fn layout_paragraph(
    engine: &mut Engine,
    content: &Content,
    config: &TermConfig,
    styles: StyleChain,
    base_style: ContentStyle,
) -> SourceResult<TermFrame> {
    // ── Step 1: flatten the content tree into inline items ──────────────────
    let mut items: Vec<InlineItem> = Vec::new();
    collect_items(content, styles, base_style, &mut items, engine, config);

    // ── Step 2: split at explicit breaks and wrap at config.width ─────────────
    let mut lines: Vec<Vec<InlineItem>> = Vec::new();
    let mut current: Vec<InlineItem>    = Vec::new();
    let mut current_width: Col          = 0;

    for item in items {
        match item {
            InlineItem::Break => {
                // Flush the current line (even if empty — an explicit break
                // always produces at least one row).
                lines.push(std::mem::take(&mut current));
                current_width = 0;
            }
            other => {
                let item_w = other.width();

                // Greedy line-wrapping: if the item would push us past the
                // configured width AND the current line is non-empty, start a
                // new line first.  Spaces at the start of a continuation line
                // are not emitted (they're absorbed into the break).
                if let Some(max_cols) = config.width {
                    if current_width + item_w > max_cols && !current.is_empty() {
                        // Don't start a new line with a pure space.
                        if !matches!(other, InlineItem::Space(_)) {
                            lines.push(std::mem::take(&mut current));
                            current_width = 0;
                        } else {
                            // Discard the leading space on the new line.
                            continue;
                        }
                    }
                }

                current_width += item_w;
                current.push(other);
            }
        }
    }
    // Flush any trailing content.
    if !current.is_empty() {
        lines.push(current);
    }

    // If there was no content at all, produce a single empty row.
    if lines.is_empty() {
        return Ok(TermFrame::new(TermSize::new(0, 1)));
    }

    // ── Step 3: build a frame per line, then stack vertically ─────────────────
    let line_frames: Vec<TermFrame> = lines
        .iter()
        .map(|l| build_line_frame(l))
        .collect();

    let max_cols: Col  = line_frames.iter().map(|f| f.cols()).max().unwrap_or(0);
    let total_rows: Row = line_frames.iter().map(|f| f.rows().max(1)).sum();

    let first_baseline = line_frames.first().map(|f| f.baseline()).unwrap_or(0);

    let mut out = TermFrame::new(TermSize::new(max_cols.max(0), total_rows.max(0)));
    out.set_baseline(first_baseline);

    let mut y: Row = 0;
    for frame in line_frames {
        let h = frame.rows().max(1);
        out.push_frame(TermPoint::new(0, y), frame);
        y += h;
    }

    Ok(out)
}
