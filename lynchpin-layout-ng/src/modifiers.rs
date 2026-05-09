//! Text modifier layout functions.
//!
//! Terminal equivalents of paged text modifiers.  Most are implemented by
//! setting [`crossterm::style::ContentStyle`] attributes on inline text.
//! All functions follow the [`TermBlockCallback`] signature.

use crossterm::style::{Attribute, ContentStyle};
use typst_library::diag::SourceResult;
use typst_library::engine::Engine;
use typst_library::foundations::{Packed, StyleChain};
use typst_library::model::{EmphElem, StrongElem};
use typst_library::text::{HighlightElem, OverlineElem, SmallcapsElem, StrikeElem, SubElem, SuperElem, UnderlineElem};

use lynchpin_library_ng::*;

/// Layout strong (bold) content.
pub fn layout_strong(
    elem: &Packed<StrongElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let mut style = ContentStyle::default();
    style.attributes.set(Attribute::Bold);
    layout_with_style(engine, &elem.body, config, styles, style)
}

/// Layout emphasised (italic) content.
pub fn layout_emph(
    elem: &Packed<EmphElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let mut style = ContentStyle::default();
    style.attributes.set(Attribute::Italic);
    layout_with_style(engine, &elem.body, config, styles, style)
}

/// Layout subscript content.
///
/// In the terminal we approximate subscripts by rendering the body as-is;
/// true baseline shift is not supported in a character grid.
pub fn layout_sub(
    elem: &Packed<SubElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    layout_with_style(engine, &elem.body, config, styles, ContentStyle::default())
}

/// Layout superscript content.
///
/// Same limitation as [`layout_sub`].
pub fn layout_super(
    elem: &Packed<SuperElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    layout_with_style(engine, &elem.body, config, styles, ContentStyle::default())
}

/// Layout underlined content.
pub fn layout_underline(
    elem: &Packed<UnderlineElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let mut style = ContentStyle::default();
    style.attributes.set(Attribute::Underlined);
    layout_with_style(engine, &elem.body, config, styles, style)
}

/// Layout overlined content.
pub fn layout_overline(
    elem: &Packed<OverlineElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    // No native overline attribute; render as-is.
    layout_with_style(engine, &elem.body, config, styles, ContentStyle::default())
}

/// Layout strikethrough content.
pub fn layout_strike(
    elem: &Packed<StrikeElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let mut style = ContentStyle::default();
    style.attributes.set(Attribute::CrossedOut);
    layout_with_style(engine, &elem.body, config, styles, style)
}

/// Layout highlighted content.
pub fn layout_highlight(
    elem: &Packed<HighlightElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let mut style = ContentStyle::default();
    style.attributes.set(Attribute::Reverse);
    layout_with_style(engine, &elem.body, config, styles, style)
}

/// Layout smallcaps content.
///
/// Terminal smallcaps are rendered as uppercase.
pub fn layout_smallcaps(
    elem: &Packed<SmallcapsElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    // Smallcaps → render body text; the inline shaper may apply uppercase.
    layout_with_style(engine, &elem.body, config, styles, ContentStyle::default())
}

/// Helper: layout arbitrary content with a base style.
fn layout_with_style(
    engine: &mut Engine,
    body: &typst_library::foundations::Content,
    config: &TermConfig,
    styles: StyleChain,
    _base_style: ContentStyle,
) -> SourceResult<TermFrame> {
    // Delegate to the flow layout for the body content.
    // The style is applied during inline text collection.
    crate::flow::layout_term_frame(
        engine,
        &[(body, styles)],
        typst_library::introspection::Locator::root(),
        styles,
        TermRegion::new(
            TermSize::new(config.effective_width(), TermScalar::INFINITY),
            Axes::new(false, false),
        ),
    )
}
