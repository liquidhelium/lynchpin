//! Block / flow layout and document entry point.
//!
//! [`layout_document`] is the top-level entry point: it consumes a realized
//! [`Pair`] stream produced by `realize_term` and returns a single
//! [`TermPage`] containing all blocks stacked vertically.
//!
//! # Block model
//!
//! Each top-level element from the realize stream is dispatched as follows:
//!
//! | Element          | Action                                              |
//! |------------------|-----------------------------------------------------|
//! | `ParElem`        | inline layout via [`layout_paragraph`]              |
//! | `HeadingElem`    | bold + colored inline layout                        |
//! | `ListElem`       | bullet items via [`render_list_item`]               |
//! | `EnumElem`       | numbered items via [`render_enum_item`]             |
//! | `TermsElem`      | definition items via [`render_term_item`]           |
//! | `RawElem`        | monospace, syntax-colored inline layout             |
//! | `RawLine`        | single highlighted code line                        |
//! | `EquationElem`   | block: `[equation]` stub                            |
//! | `BlockElem`      | recurse into body                                   |
//! | `BoxElem`        | recurse into body                                   |
//! | `HElem`          | ignored (horizontal spacing has no block meaning)   |
//! | `VElem`          | inserts extra blank rows                            |
//! | `LinebreakElem`  | inserts one blank row                               |
//! | `ParbreakElem`   | inserts one blank row                               |
//! | `PagebreakElem`  | separator line `──────`                             |
//! | `SequenceElem`   | transparent — recurse into children                 |
//! | `StyledElem`     | transparent — chain styles and recurse              |
//! | Others           | warning + skip                                      |
//!
//! Blocks are separated by a 1-row gap.

use crossterm::style::{Attribute, Color, ContentStyle};
use typst::__warning;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, SequenceElem, StyleChain, StyledElem};
use typst::layout::{BlockBody, BlockElem, BoxElem, HElem, PagebreakElem, VElem};
use typst::math::EquationElem;
use typst::model::{EnumElem, HeadingElem, ListElem, ParElem, ParbreakElem, TermsElem};
use typst::routines::Pair;
use typst::text::{LinebreakElem, RawContent, RawElem, RawLine, SpaceElem, TextElem};

use crate::config::TermConfig;
use crate::frame::{Row, TermFrame, TermSize};
use crate::inline::layout_paragraph;
use crate::lists::{render_enum_item, render_list_item, render_term_item};
use crate::stack::compose_vertical;

// ── TermPage ──────────────────────────────────────────────────────────────────

/// A single page of terminal output.
///
/// Terminal rendering has no hard page breaks; the entire document is usually
/// one continuous page.
#[derive(Debug, Clone)]
pub struct TermPage {
    pub frame: TermFrame,
}

// ── Internal flow state ───────────────────────────────────────────────────────

/// Mutable state threaded through the recursive block handler.
struct FlowState<'cfg> {
    config:       &'cfg TermConfig,
    /// Accumulated block frames (not yet composed).
    blocks:       Vec<TermFrame>,
    /// Current enumeration counter (reset when a new `EnumElem` starts).
    enum_counter: u64,
}

impl<'cfg> FlowState<'cfg> {
    fn new(config: &'cfg TermConfig) -> Self {
        Self { config, blocks: Vec::new(), enum_counter: 1 }
    }

    /// Push a block frame.  Empty frames are silently dropped.
    fn push(&mut self, frame: TermFrame) {
        if !frame.size().is_empty() {
            self.blocks.push(frame);
        }
    }

    /// Insert a blank gap of `rows` rows.
    fn push_blank(&mut self, rows: Row) {
        if rows > 0 {
            self.blocks.push(TermFrame::new(TermSize::new(0, rows)));
        }
    }
}

// ── Heading color ─────────────────────────────────────────────────────────────

fn heading_color(level: usize) -> Color {
    match level {
        1 => Color::Yellow,
        2 => Color::Cyan,
        3 => Color::Green,
        _ => Color::Blue,
    }
}

// ── Raw line body layout ──────────────────────────────────────────────────────

/// Layout a syntax-highlighted `RawLine` body into a [`TermFrame`].
///
/// The body is a sequence of `TextElem`s optionally wrapped in `StyledElem`s
/// carrying `TextElem::fill` color overrides.  We delegate to `layout_paragraph`
/// with a dark-grey base style to mimic the code colour scheme.
fn layout_raw_line(
    engine: &mut Engine,
    body: &Content,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let mut style = ContentStyle::default();
    style.foreground_color = Some(Color::DarkGrey);
    layout_paragraph(engine, body, config, styles, style)
}

// ── Block-level content handler ───────────────────────────────────────────────

/// Recursively handle a single content node, appending block frames to
/// `state.blocks`.
fn handle_block(
    state: &mut FlowState<'_>,
    engine: &mut Engine,
    child: &Content,
    styles: StyleChain,
) -> SourceResult<()> {
    // ── Transparent wrappers ──────────────────────────────────────────────────
    if let Some(seq) = child.to_packed::<SequenceElem>() {
        for c in &seq.children {
            handle_block(state, engine, c, styles)?;
        }

    } else if let Some(s) = child.to_packed::<StyledElem>() {
        handle_block(state, engine, &s.child, styles.chain(&s.styles))?;

    // ── Paragraph ─────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<ParElem>() {
        let frame = layout_paragraph(
            engine, &elem.body, state.config, styles, ContentStyle::default(),
        )?;
        state.push(frame);

    // ── Heading ───────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<HeadingElem>() {
        let level = elem.resolve_level(styles).get() as usize;
        let mut style = ContentStyle::default();
        style.attributes.set(Attribute::Bold);
        style.foreground_color = Some(heading_color(level));
        let frame = layout_paragraph(engine, &elem.body, state.config, styles, style)?;
        state.push(frame);

    // ── Bullet list ───────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<ListElem>() {
        let mut item_frames = Vec::with_capacity(elem.children.len());
        for item in &elem.children {
            let f = render_list_item(engine, &item.body, state.config, styles)?;
            item_frames.push(f);
        }
        if !item_frames.is_empty() {
            state.push(compose_vertical(item_frames, 0, 0));
        }

    // ── Enumeration list ──────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<EnumElem>() {
        // Reset the enum counter to elem.start (or 1 if unset).
        state.enum_counter = elem.start.get(styles).unwrap_or(1);
        let mut item_frames = Vec::with_capacity(elem.children.len());
        for item in &elem.children {
            let number = item.number.get(styles);
            let f = render_enum_item(
                engine, number, &mut state.enum_counter,
                &item.body, state.config, styles,
            )?;
            item_frames.push(f);
        }
        if !item_frames.is_empty() {
            state.push(compose_vertical(item_frames, 0, 0));
        }

    // ── Definition list ───────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<TermsElem>() {
        let mut item_frames = Vec::with_capacity(elem.children.len());
        for item in &elem.children {
            let f = render_term_item(
                engine, &item.term, &item.description, state.config, styles,
            )?;
            item_frames.push(f);
        }
        if !item_frames.is_empty() {
            // Use a 1-row gap between definition items for readability.
            state.push(compose_vertical(item_frames, 1, 0));
        }

    // ── Code blocks ───────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<RawLine>() {
        // A RawLine arriving directly (e.g. from a user show rule).
        let f = layout_raw_line(engine, &elem.body, state.config, styles)?;
        state.push(f);

    } else if let Some(elem) = child.to_packed::<RawElem>() {
        let is_block = elem.block.get(styles);
        let lines = elem.lines.as_deref().unwrap_or_default();

        if !lines.is_empty() {
            let mut line_frames = Vec::with_capacity(lines.len());
            for line in lines {
                let f = layout_raw_line(engine, &line.body, state.config, styles)?;
                line_frames.push(f);
            }
            let code_frame = compose_vertical(line_frames, 0, 0);
            if is_block {
                state.push(code_frame);
            } else {
                // Inline raw: treat as a single block (common when raw appears
                // outside a paragraph after realization).
                state.push(code_frame);
            }
        } else {
            // Fallback when synthesized lines are absent.
            let text: ecow::EcoString = match &elem.text {
                RawContent::Text(t) => t.clone(),
                RawContent::Lines(ls) => ls
                    .iter()
                    .map(|(s, _)| s.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
                    .into(),
            };
            let mut style = ContentStyle::default();
            style.foreground_color = Some(Color::DarkGrey);
            state.push(TermFrame::text(text, style));
        }

    // ── Block / box containers ────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<BlockElem>() {
        if let Some(BlockBody::Content(body)) = elem.body.get_ref(styles) {
            handle_block(state, engine, body, styles)?;
        }
        // BlockBody::MultiLayouter / SingleLayouter → skip (not applicable to terminal)

    } else if let Some(elem) = child.to_packed::<BoxElem>() {
        if let Some(body) = elem.body.get_ref(styles) {
            handle_block(state, engine, body, styles)?;
        }

    // ── Block equations ───────────────────────────────────────────────────────
    } else if let Some(eq) = child.to_packed::<EquationElem>() {
        if eq.block.get(styles) {
            let frame = crate::math::layout_equation_block(
                eq, engine, state.config, styles,
            )?;
            state.push(frame);
        } else {
            // An inline equation at block level — wrap it like a paragraph.
            let frame = layout_paragraph(
                engine, child, state.config, styles, ContentStyle::default(),
            )?;
            state.push(frame);
        }

    // ── Spacing ───────────────────────────────────────────────────────────────
    } else if child.is::<SpaceElem>() {
        // Horizontal space has no block-level meaning; skip.

    } else if child.is::<LinebreakElem>() || child.is::<ParbreakElem>() {
        // Insert a visual blank row to separate adjacent blocks.
        state.push_blank(1);

    } else if child.is::<HElem>() {
        // Horizontal spacing at block level: skip.

    } else if child.is::<VElem>() {
        // Vertical spacing: insert a blank row.
        state.push_blank(1);

    // ── Page break ───────────────────────────────────────────────────────────
    } else if child.is::<PagebreakElem>() {
        // Render as a horizontal separator line.
        let width = state.config.width.unwrap_or(40);
        let sep_char = if state.config.mode.is_unicode() { '─' } else { '-' };
        let sep = TermFrame::text(
            std::iter::repeat(sep_char).take(width as usize).collect::<String>(),
            ContentStyle::default(),
        );
        state.push(sep);

    // ── TextElem at block level (bare text outside a paragraph) ──────────────
    } else if let Some(_elem) = child.to_packed::<TextElem>() {
        // Rare after realize_term, but handle gracefully.
        let frame = layout_paragraph(
            engine, child, state.config, styles, ContentStyle::default(),
        )?;
        state.push(frame);

    // ── Unknown ───────────────────────────────────────────────────────────────
    } else {
        engine.sink.warn(__warning!(
            child.span(),
            "{} was ignored during terminal layout",
            child.elem().name()
        ));
    }

    Ok(())
}

// ── Document entry point ──────────────────────────────────────────────────────

/// Layout the full document into terminal pages.
///
/// Processes the realized [`Pair`] stream produced by `realize_term`, emits
/// one [`TermPage`] containing all block content stacked vertically with a
/// 1-row gap between blocks.
///
/// # Notes
///
/// Terminal output has no hard page breaks; this always returns exactly one
/// `TermPage`.  If the document uses `#pagebreak()` a visual separator is
/// inserted instead.
pub fn layout_document<'a>(
    engine: &mut Engine,
    pairs: impl IntoIterator<Item = Pair<'a>>,
    config: &TermConfig,
    _base_styles: StyleChain,
) -> SourceResult<Vec<TermPage>> {
    let mut state = FlowState::new(config);

    for (child, styles) in pairs {
        handle_block(&mut state, engine, child, styles)?;
    }

    // Compose all block frames vertically with a 1-row gap between each.
    let page_frame = if state.blocks.is_empty() {
        TermFrame::new(TermSize::ZERO)
    } else {
        compose_vertical(state.blocks, 1, 0)
    };

    Ok(vec![TermPage { frame: page_frame }])
}
