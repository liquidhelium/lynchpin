//! List and enumeration layout.
//!
//! Terminal equivalents of `typst-layout/src/lists.rs`.  Both functions build
//! a [`CellGrid`] and delegate to [`GridLayouter`], exactly mirroring paged.

use comemo::Track;
use typst_library::diag::SourceResult;
use typst_library::engine::Engine;
use typst_library::foundations::{Content, Context, Depth, Packed, StyleChain};
use typst_library::introspection::Locator;
use typst_library::layout::grid::resolve::{Cell, CellGrid};
use typst_library::layout::{Axes, Fragment, HAlignment, Length, Sizing, VAlignment};
use typst_library::model::{EnumElem, ListElem, ParbreakElem, ParElem};
use typst_library::text::TextElem;

use lynchpin_library::*;

/// Resolve the vertical gutter for a list-like element.
fn resolve_gutter(
    spacing: typst_library::foundations::Smart<Length>,
    tight: bool,
    styles: StyleChain,
) -> Length {
    if let typst_library::foundations::Smart::Custom(len) = spacing {
        len
    } else if tight {
        // In terminal layout every character occupies exactly one row.  The
        // paged leading (0.65 em) rounds up to 1 row, making tight lists look
        // the same as non-tight ones.  Use zero spacing instead so tight list
        // items appear directly adjacent, matching the user's expectation.
        Length::zero()
    } else {
        styles.get(ParElem::spacing)
    }
}

/// Layout a bullet list — mirrors paged `layout_list`.
#[typst_macros::time(span = elem.span())]
pub fn layout_list(
    elem: &Packed<ListElem>,
    engine: &mut Engine,
    locator: Locator,
    styles: StyleChain,
    regions: TermRegions,
) -> SourceResult<TermFragment> {
    let indent = elem.indent.get(styles);
    let body_indent = elem.body_indent.get(styles);
    let tight = elem.tight.get(styles);
    let gutter = resolve_gutter(elem.spacing.get(styles), tight, styles);

    let Depth(depth) = styles.get(ListElem::depth);
    let marker = elem
        .marker
        .get_ref(styles)
        .resolve(engine, styles, depth)?
        .aligned(HAlignment::Start + VAlignment::Top);

    let mut cells = vec![];
    for item in &elem.children {
        let mut body = item.body.clone();
        if !tight {
            body += ParbreakElem::shared();
        }
        let body = body.set(ListElem::depth, Depth(1));

        cells.push(Cell::new(Content::empty()));
        cells.push(Cell::new(marker.clone()));
        cells.push(Cell::new(Content::empty()));
        cells.push(Cell::new(body));
    }

    let grid = CellGrid::new(
        Axes::with_x(&[
            Sizing::Rel(indent.into()),
            Sizing::Auto,
            Sizing::Rel(body_indent.into()),
            Sizing::Auto,
        ]),
        Axes::with_y(&[gutter.into()]),
        cells,
    );
    crate::grid::GridLayouter::new(&grid, regions, locator, styles, elem.span())
        .layout(engine)
}

/// Layout an enumeration — mirrors paged `layout_enum`.
#[typst_macros::time(span = elem.span())]
pub fn layout_enum(
    elem: &Packed<EnumElem>,
    engine: &mut Engine,
    locator: Locator,
    styles: StyleChain,
    regions: TermRegions,
) -> SourceResult<TermFragment> {
    let numbering = elem.numbering.get_ref(styles);
    let reversed = elem.reversed.get(styles);
    let indent = elem.indent.get(styles);
    let body_indent = elem.body_indent.get(styles);
    let tight = elem.tight.get(styles);
    let gutter = resolve_gutter(elem.spacing.get(styles), tight, styles);
    let number_align = elem.number_align.get(styles);

    let mut cells = vec![];
    let mut number = elem
        .start
        .get(styles)
        .unwrap_or_else(|| if reversed { elem.children.len() as u64 } else { 1 });
    let mut parents = styles.get_cloned(EnumElem::parents);
    let full = elem.full.get(styles);

    for item in &elem.children {
        number = item.number.get(styles).unwrap_or(number);

        let context = Context::new(None, Some(styles));
        let resolved = if full {
            parents.push(number);
            let content = numbering
                .apply(engine, context.track(), &parents)?
                .display();
            parents.pop();
            content
        } else {
            numbering
                .apply(engine, context.track(), &[number])?
                .display()
        }
        .aligned(number_align)
        .set(TextElem::overhang, false);

        let mut body = item.body.clone();
        if !tight {
            body += ParbreakElem::shared();
        }
        let body = body.set(EnumElem::parents, smallvec::smallvec![number]);

        cells.push(Cell::new(Content::empty()));
        cells.push(Cell::new(resolved));
        cells.push(Cell::new(Content::empty()));
        cells.push(Cell::new(body));

        number = if reversed {
            number.saturating_sub(1)
        } else {
            number.saturating_add(1)
        };
    }

    let grid = CellGrid::new(
        Axes::with_x(&[
            Sizing::Rel(indent.into()),
            Sizing::Auto,
            Sizing::Rel(body_indent.into()),
            Sizing::Auto,
        ]),
        Axes::with_y(&[gutter.into()]),
        cells,
    );
    crate::grid::GridLayouter::new(&grid, regions, locator, styles, elem.span())
        .layout(engine)
}
