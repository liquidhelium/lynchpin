//! List, enumeration, and definition-list layout helpers.
//!
//! Each function renders a single list item into a [`TermFrame`], composing
//! the bullet/number prefix with the body content.

use crossterm::style::{Attribute, ContentStyle};
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, Smart, StyleChain};

use crate::config::TermConfig;
use crate::frame::{TermFrame, TermSize};
use crate::inline::layout_paragraph;
use crate::stack::{compose_horizontal, compose_vertical};

// ── Bullet list ───────────────────────────────────────────────────────────────

/// Render a single bullet-list item.
///
/// Produces: `• ‹body›`
///
/// The bullet and body are composed horizontally with baseline alignment.
pub fn render_list_item(
    engine: &mut Engine,
    body: &Content,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let prefix = TermFrame::text("• ", ContentStyle::default());
    let body_frame = layout_paragraph(engine, body, config, styles, ContentStyle::default())?;
    Ok(compose_horizontal(vec![prefix, body_frame], 0))
}

// ── Numbered (enumeration) list ───────────────────────────────────────────────

/// Render a single enumeration item.
///
/// Produces: `N. ‹body›`
///
/// If `number` is `None` (auto), the current value of `*counter` is used and
/// the counter is advanced.  If `number` is `Some(n)`, that value is used and
/// the counter is set to `n + 1` for the next auto item.
pub fn render_enum_item(
    engine: &mut Engine,
    number: Smart<u64>,
    counter: &mut u64,
    body: &Content,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let n = match number {
        Smart::Auto => {
            let n = *counter;
            *counter += 1;
            n
        }
        Smart::Custom(n) => {
            // Explicit number: advance the counter past this item.
            *counter = n + 1;
            n
        }
    };

    let prefix = TermFrame::text(format!("{n}. "), ContentStyle::default());
    let body_frame = layout_paragraph(engine, body, config, styles, ContentStyle::default())?;
    Ok(compose_horizontal(vec![prefix, body_frame], 0))
}

// ── Definition (terms) list ───────────────────────────────────────────────────

/// Render a single definition-list item.
///
/// Produces: `**term**: ‹description›` (term is bold; all on one "logical"
/// line at this stage — the inline layout will wrap if needed).
pub fn render_term_item(
    engine: &mut Engine,
    term: &Content,
    desc: &Content,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let mut term_style = ContentStyle::default();
    term_style.attributes.set(Attribute::Bold);

    let term_frame = layout_paragraph(engine, term, config, styles, term_style)?;
    let sep_frame = TermFrame::text(": ", ContentStyle::default());
    let desc_frame = layout_paragraph(engine, desc, config, styles, ContentStyle::default())?;

    Ok(compose_horizontal(
        vec![term_frame, sep_frame, desc_frame],
        0,
    ))
}

// ── List-level wrappers ──────────────────────────────────────────────────────

pub fn render_list(
    engine: &mut Engine,
    elem: &typst::foundations::Packed<typst::model::ListElem>,
    config: &TermConfig,
    styles: typst::foundations::StyleChain,
) -> SourceResult<TermFrame> {
    let mut frames = Vec::new();
    for item in &elem.children {
        let f = render_list_item(engine, &item.body, config, styles)?;
        if !f.size().is_empty() {
            frames.push(f);
        }
    }
    Ok(if frames.is_empty() {
        TermFrame::new(TermSize::ZERO)
    } else {
        compose_vertical(frames, 0, 0)
    })
}

pub fn render_enum(
    engine: &mut Engine,
    elem: &typst::foundations::Packed<typst::model::EnumElem>,
    config: &TermConfig,
    styles: typst::foundations::StyleChain,
) -> SourceResult<TermFrame> {
    let mut counter = elem.start.get(styles).unwrap_or(1);
    let mut frames = Vec::new();
    for item in &elem.children {
        let number = item.number.get(styles);
        let f = render_enum_item(engine, number, &mut counter, &item.body, config, styles)?;
        if !f.size().is_empty() {
            frames.push(f);
        }
    }
    Ok(if frames.is_empty() {
        TermFrame::new(TermSize::ZERO)
    } else {
        compose_vertical(frames, 0, 0)
    })
}

pub fn render_terms(
    engine: &mut Engine,
    elem: &typst::foundations::Packed<typst::model::TermsElem>,
    config: &TermConfig,
    styles: typst::foundations::StyleChain,
) -> SourceResult<TermFrame> {
    let mut frames = Vec::new();
    for item in &elem.children {
        let f = render_term_item(engine, &item.term, &item.description, config, styles)?;
        if !f.size().is_empty() {
            frames.push(f);
        }
    }
    Ok(if frames.is_empty() {
        TermFrame::new(TermSize::ZERO)
    } else {
        compose_vertical(frames, 0, 0)
    })
}
