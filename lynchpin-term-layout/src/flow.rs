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

use comemo::Track;
use crossterm::style::{Attribute, Color, ContentStyle};
use typst::__warning;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, Resolve, SequenceElem, StyleChain, StyledElem};
use typst::layout::{
    BlockBody, BlockElem, BoxElem, HElem, HideElem, LayoutElem, PagebreakElem, PlaceElem, VElem,
};
use typst::math::EquationElem;
use typst::model::{EnumElem, HeadingElem, ListElem, ParElem, ParbreakElem, TermsElem};
use typst::routines::Pair;
use typst::text::{LinebreakElem, RawContent, RawElem, RawLine, SpaceElem, TextElem};

use lynchpin_library::TermBlockElem;

use crate::config::TermConfig;
use crate::frame::{Col, Row, TermFrame, TermPoint, TermSize};
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

// ── Item model (mirrors paged distribute.rs) ────────────────────────────────

/// A laid-out item waiting for page assembly.
pub enum Item {
    /// A regular block frame with its alignment.
    Frame(TermFrame),
    /// An absolutely placed frame (from `#place`). Stamped onto the page,
    /// replacing only non-blank cells in the target area.
    Placed {
        frame: TermFrame,
        /// Horizontal alignment (None = flow position).
        align_x: Option<typst::layout::FixedAlignment>,
        /// Vertical alignment (None = flow position).
        align_y: Option<typst::layout::FixedAlignment>,
        /// Terminal columns offset (dx, dy).
        delta: (Col, Row),
    },
    /// Absolute vertical spacing.
    Abs(Row),
    /// Fractional spacing (for stack layout).
    Fr(typst::layout::Fr),
}

/// Layout a single content node as a block, returning one frame.
/// Used by stack/place callbacks that need recursive block layout.
pub fn layout_block(
    engine: &mut Engine,
    content: &Content,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let mut state = FlowState::new(config);
    handle_block(&mut state, engine, content, styles)?;
    finalize(state.items, config)
}

// ── Finalize: assemble items into a single frame ─────────────────────────────

fn finalize(items: Vec<Item>, config: &TermConfig) -> SourceResult<TermFrame> {
    if items.is_empty() {
        return Ok(TermFrame::new(TermSize::ZERO));
    }
    let width = config.width.unwrap_or(80) as Col;
    // First pass: compute total height needed.
    let mut frames: Vec<TermFrame> = Vec::new();
    for item in &items {
        match item {
            Item::Frame(f) if !f.size().is_empty() => frames.push(f.clone()),
            Item::Abs(r) if *r > 0 => frames.push(TermFrame::new(TermSize::new(0, *r))),
            Item::Placed { .. } | Item::Fr(_) | Item::Frame(_) | Item::Abs(_) => {}
        }
    }
    if frames.is_empty() && !items.iter().any(|i| matches!(i, Item::Placed { .. })) {
        return Ok(TermFrame::new(TermSize::ZERO));
    }

    // Compose regular frames vertically, or create a blank base for placed-only.
    let (mut base, regular_height) = if frames.is_empty() {
        let max_h = items.iter().filter_map(|i| match i {
            Item::Placed { frame, .. } => Some(frame.rows()),
            _ => None,
        }).max().unwrap_or(1);
        (TermFrame::new(TermSize::new(width, max_h)), 0)
    } else {
        let composed = compose_vertical(frames, 1, 0);
        let h = composed.rows();
        (composed, h)
    };

    // Second pass: stamp placed items onto the base.
    let flow_y = regular_height;
    for item in &items {
        if let Item::Placed { frame, align_x, align_y, delta } = item {
            let w = base.cols().max(width);
            let cx = match align_x {
                Some(a) => align_pos(*a, w - frame.cols()),
                None => 0,
            } + delta.0;
            let cy = match align_y {
                Some(a) => align_pos(*a, base.rows() - frame.rows()),
                None => flow_y,
            } + delta.1;
            // Ensure base is tall and wide enough.
            if cy + frame.rows() > base.rows() {
                base.set_rows(cy + frame.rows());
            }
            if cx + frame.cols() > base.cols() {
                base.set_cols(cx + frame.cols());
            }
            stamp_frame(&mut base, frame, cx, cy);
        }
    }
    Ok(base)
}

/// Stamp `src` onto `dst` at (col, row). Non-blank cells of `src` replace
/// cells in `dst`.
fn stamp_frame(dst: &mut TermFrame, src: &TermFrame, col: Col, row: Row) {
    for (pos, item) in src.items() {
        let tc = col + pos.col;
        let tr = row + pos.row;
        match item {
            lynchpin_library::frame::TermFrameItem::Text(t, style) => {
                dst.push_text(TermPoint::new(tc, tr), t.clone(), *style);
            }
            lynchpin_library::frame::TermFrameItem::Frame(f) => {
                dst.push_frame(TermPoint::new(tc, tr), f.clone());
            }
        }
    }
}

// ── Internal flow state ───────────────────────────────────────────────────────

/// Mutable state threaded through the recursive block handler.
struct FlowState<'cfg> {
    config: &'cfg TermConfig,
    /// Accumulated items (not yet composed).
    items: Vec<Item>,
    /// Current enumeration counter (reset when a new `EnumElem` starts).
    enum_counter: u64,
}

fn align_pos(a: typst::layout::FixedAlignment, available: Col) -> Col {
    match a {
        typst::layout::FixedAlignment::Start => 0,
        typst::layout::FixedAlignment::Center => available / 2,
        typst::layout::FixedAlignment::End => available,
    }
}

impl<'cfg> FlowState<'cfg> {
    fn new(config: &'cfg TermConfig) -> Self {
        Self {
            config,
            items: Vec::new(),
            enum_counter: 1,
        }
    }

    /// Push a block frame.
    fn push(&mut self, frame: TermFrame) {
        if !frame.size().is_empty() {
            self.items.push(Item::Frame(frame));
        }
    }

    /// Push a placed frame.
    fn push_placed(&mut self, frame: TermFrame, align_x: Option<typst::layout::FixedAlignment>, align_y: Option<typst::layout::FixedAlignment>, delta: (Col, Row)) {
        self.items.push(Item::Placed { frame, align_x, align_y, delta });
    }

    /// Insert a blank gap of `rows` rows.
    fn push_blank(&mut self, rows: Row) {
        if rows > 0 {
            self.items.push(Item::Abs(rows));
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
            engine,
            &elem.body,
            state.config,
            styles,
            ContentStyle::default(),
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
                engine,
                number,
                &mut state.enum_counter,
                &item.body,
                state.config,
                styles,
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
            let f = render_term_item(engine, &item.term, &item.description, state.config, styles)?;
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

    // ── Hide ──────────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<HideElem>() {
        let mut tmp = FlowState::new(state.config);
        handle_block(&mut tmp, engine, &elem.body, styles)?;
        // Measure total height of child items.
        let total_rows: Row = tmp.items.iter().map(|i| match i {
            Item::Frame(f) => f.rows().max(1),
            Item::Abs(r) => *r,
            _ => 0,
        }).sum();
        if total_rows > 0 {
            state.items.push(Item::Abs(total_rows));
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
            let frame = crate::math::layout_equation_block(eq, engine, state.config, styles)?;
            state.push(frame);
        } else {
            // Inline equation separated from its paragraph by realize_term.
            // Render as inline math block (left-aligned, no paragraph wrapping).
            let frame = crate::math::layout_equation_inline(eq, engine, state.config, styles)?;
            state.push(frame);
        }

    // ── Spacing ───────────────────────────────────────────────────────────────
    } else if child.is::<SpaceElem>() {
        // Horizontal space has no block-level meaning; skip.
    } else if child.is::<ParbreakElem>() {
        // Insert a visual blank row to separate adjacent blocks.
        state.push_blank(1);
    } else if child.is::<LinebreakElem>() {
        // automatically handled.
    } else if child.is::<HElem>() {
        // Horizontal spacing at block level: skip.
    } else if let Some(v) = child.to_packed::<VElem>() {
        // Vertical spacing: insert a blank row.
        state.push_blank(lynchpin_library::units::spacing_to_rows(&v.amount, styles) as i32);

    // ── Page break ───────────────────────────────────────────────────────────
    } else if child.is::<PagebreakElem>() {
        // Render as a horizontal separator line.
        let width = state.config.width.unwrap_or(40);
        let sep_char = if state.config.mode.is_unicode() {
            '─'
        } else {
            '-'
        };
        let sep = TermFrame::text(
            std::iter::repeat(sep_char)
                .take(width as usize)
                .collect::<String>(),
            ContentStyle::default(),
        );
        state.push(sep);

    // ── TextElem at block level (bare text outside a paragraph) ──────────────
    } else if let Some(_elem) = child.to_packed::<TextElem>() {
        // Rare after realize_term, but handle gracefully.
        let frame = layout_paragraph(engine, child, state.config, styles, ContentStyle::default())?;
        state.push(frame);

    // ── Place (handled here, not via show rule — see paged collect.rs) ───────
    } else if let Some(elem) = child.to_packed::<PlaceElem>() {
        let body_frame = layout_block(engine, &elem.body, state.config, styles)?;
        let (align_x, align_y) = match elem.alignment.get(styles) {
            typst::foundations::Smart::Custom(a) => (
                a.x().map(|x| x.resolve(styles)),
                a.y().map(|y| y.resolve(styles)),
            ),
            _ => (None, None),
        };
        state.push_placed(body_frame, align_x, align_y, (0, 0));

    // ── TermBlockElem (terminal-specific block with layout callback) ─────────
    } else if let Some(tb) = child.to_packed::<TermBlockElem>() {
        let frame = tb.cb.call(engine, state.config, styles)?;
        state.push(frame);

    // ── Layout ────────────────────────────────────────────────────────────────
    // #layout(func) needs recursive handle_block, so kept as special case.
    } else if let Some(elem) = child.to_packed::<LayoutElem>() {
        use typst::foundations::{Context, dict};
        use typst::layout::Abs;
        let width_cols = state.config.width.unwrap_or(80) as f64;
        let width = Abs::pt(width_cols * 8.4);
        let height = Abs::pt(1000.0);
        let loc = child.location();
        let context = Context::new(loc, Some(styles));
        let result = elem
            .func
            .call(
                engine,
                context.track(),
                [dict! { "width" => width, "height" => height }],
            )?
            .display();
        handle_block(state, engine, &result, styles)?;
    }
    // ── Unknown ───────────────────────────────────────────────────────────────
    else {
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

    // Collect pairs so we can merge inline equations back into paragraphs.
    let pairs: Vec<Pair<'a>> = pairs.into_iter().collect();
    let mut i = 0;

    while i < pairs.len() {
        let (child, styles) = &pairs[i];
        if child.to_packed::<ParElem>().is_some() {
            let mut kids: Vec<Content> = vec![(*child).clone()];
            let mut merged_styles = *styles;
            let mut j = i + 1;
            while j < pairs.len() {
                let (nchild, nstyles) = &pairs[j];
                let is_inline_eq = nchild
                    .to_packed::<EquationElem>()
                    .is_some_and(|eq| !eq.block.get(*nstyles));
                let is_par = nchild.to_packed::<ParElem>().is_some();
                if is_inline_eq || is_par {
                    kids.push((*nchild).clone());
                    merged_styles = *nstyles;
                    j += 1;
                } else {
                    break;
                }
            }
            i = j;
            let seq = Content::sequence(kids);
            handle_block(&mut state, engine, &seq, merged_styles)?;
        } else {
            handle_block(&mut state, engine, child, *styles)?;
            i += 1;
        }
    }

    // Compose all items into a single page frame.
    let page_frame = finalize(state.items, config)?;

    Ok(vec![TermPage { frame: page_frame }])
}
