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

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, StyleChain};
use typst::routines::Pair;

use lynchpin_library::frame::{Col, Row, TermFrame, TermPoint, TermSize};

use crate::config::TermConfig;
use crate::stack::compose_vertical;

pub mod collect;
pub mod compose;
pub mod distribute;

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
    /// Absolute vertical spacing (amount, weak).
    Abs(Row, bool),
    /// Fractional spacing (for stack layout).
    Fr(typst::layout::Fr),
}

impl Item {
    /// Whether this item can be migrated to the next region.
    pub fn migratable(&self) -> bool {
        matches!(self, Item::Abs(..))
    }
}

/// Layout a single content node as a block, returning one frame.
/// Used by stack/place callbacks that need recursive block layout.
pub fn layout_block(
    engine: &mut Engine,
    content: &Content,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    use typst::introspection::Locator;
    use typst::layout::Axes;
    use typst::routines::Arenas;
    use typst::model::DocumentInfo;
    use lynchpin_term_realize::{realize_term, TermRealizationKind};
    use lynchpin_library::frame::{Col, TermSize};
    use lynchpin_library::regions::TermRegions;

    let arenas = Arenas::default();
    let mut info = DocumentInfo::default();
    let pairs = realize_term(
        engine, &arenas, &mut info, content, styles,
        TermRealizationKind::Document,
    )?;
    let locator = Locator::root();
    let children = collect::collect(engine, &pairs, config, locator)?;
    let size = TermSize::new(config.width.unwrap_or(80) as Col, 10000);
    let regions = TermRegions::one(size, Axes::splat(false));
    let frames = distribute::distribute(engine, children, config, regions)?;
    Ok(frames.into_iter().next().unwrap_or_else(|| TermFrame::new(TermSize::ZERO)))
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
            Item::Abs(r, _) if *r > 0 => frames.push(TermFrame::new(TermSize::new(0, *r))),
            Item::Placed { .. } | Item::Fr(_) | Item::Frame(_) | Item::Abs(..) => {}
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

fn align_pos(a: typst::layout::FixedAlignment, available: Col) -> Col {
    match a {
        typst::layout::FixedAlignment::Start => 0,
        typst::layout::FixedAlignment::Center => available / 2,
        typst::layout::FixedAlignment::End => available,
    }
}

// ── Document entry point

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
    let pairs: Vec<Pair<'a>> = pairs.into_iter().collect();

    // Create root locator (terminal doesn't use introspection, but collect needs one).
    let locator = typst::introspection::Locator::root();
    let children = collect::collect(engine, &pairs, config, locator)?;

    // Create a single-region setup using terminal width and generous height.
    let size = lynchpin_library::frame::TermSize::new(
        config.width.unwrap_or(80) as lynchpin_library::frame::Col,
        10000,
    );
    let regions = lynchpin_library::regions::TermRegions::one(size, typst::layout::Axes::splat(false));

    // Use compose to handle work, floats, and footnotes.
    let mut work = compose::Work::new(children);
    let frame = compose::compose(engine, &mut work, config, regions)?;
    Ok(vec![TermPage { frame }])
}
