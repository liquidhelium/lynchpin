//! List, enumeration, and definition-list layout.
//!
//! Terminal equivalents of the paged list layout functions, adapted from
//! `lynchpin-term-layout` patterns.  Each function renders a complete list
//! element into a [`TermFrame`].

use typst_library::diag::SourceResult;
use typst_library::engine::Engine;
use typst_library::foundations::{Packed, Resolve, StyleChain};
use typst_library::model::{EnumElem, ListElem, TermsElem};
use typst_library::text::TextElem;

use lynchpin_library_ng::*;
use lynchpin_library_ng::units::abs_to_cols;

/// Layout a bullet list.
///
/// Each item is rendered as `• body` and items are stacked vertically.
#[typst_macros::time(span = elem.span())]
pub fn layout_list(
    elem: &Packed<ListElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let tight = elem.tight.get(styles);
    let gap: Row = if tight {
        TermScalar::ZERO
    } else {
        config.par_gap
    };

    let indent_raw = elem.indent.get(styles);
    let font_size = styles.get(TextElem::size).0.resolve(styles);
    let indent = abs_to_cols(indent_raw.at(font_size), font_size);
    let indent = if indent == TermScalar::ZERO { config.indent } else { indent };

    let mut frames: Vec<TermFrame> = Vec::new();

    for item in &elem.children {
        let bullet = TermFrame::text("• ", Default::default(), TermScalar::new(2), TermScalar::ONE);
        let body_frame = crate::flow::layout_term_frame_from_content(engine,&item.body,
            typst_library::introspection::Locator::root(),
            styles,
            TermRegion::new(
                TermSize::new(
                    lynchpin_library_ng::resolve_page_size(styles).cols - indent,
                    TermScalar::INFINITY,
                ),
                Axes::new(false, false),
            ),
        )?;

        // Compose bullet + body horizontally.
        let mut item_frame = TermFrame::new(TermSize::new(
            indent + body_frame.size().cols,
            body_frame.size().rows.max(TermScalar::ONE),
        ));
        item_frame.push_frame(TermPoint::new(TermScalar::ZERO, TermScalar::ZERO), bullet);
        item_frame.push_frame(TermPoint::new(indent, TermScalar::ZERO), body_frame);

        if !item_frame.size().is_empty() {
            frames.push(item_frame);
        }
    }

    Ok(compose_vertical(frames, gap, 0))
}

/// Layout an enumeration (numbered list).
///
/// Each item is rendered as `N. body` and items are stacked vertically.
#[typst_macros::time(span = elem.span())]
pub fn layout_enum(
    elem: &Packed<EnumElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let tight = elem.tight.get(styles);
    let gap: Row = if tight {
        TermScalar::ZERO
    } else {
        config.par_gap
    };

    let indent_raw = elem.indent.get(styles);
    let font_size = styles.get(TextElem::size).0.resolve(styles);
    let indent = abs_to_cols(indent_raw.at(font_size), font_size);
    let indent = if indent == TermScalar::ZERO { config.indent } else { indent };

    let reversed = elem.reversed.get(styles);
    let mut number = elem.start.get(styles).unwrap_or_else(|| {
        if reversed {
            elem.children.len() as u64
        } else {
            1
        }
    });

    let mut frames: Vec<TermFrame> = Vec::new();

    for item in &elem.children {
        number = item.number.get(styles).unwrap_or(number);

        let prefix_text = format!("{}. ", number);
        let prefix_cols = TermScalar::new(prefix_text.len() as i32);
        let prefix = TermFrame::text(prefix_text, Default::default(), prefix_cols, TermScalar::ONE);
        let body_frame = crate::flow::layout_term_frame_from_content(engine,&item.body,
            typst_library::introspection::Locator::root(),
            styles,
            TermRegion::new(
                TermSize::new(
                    lynchpin_library_ng::resolve_page_size(styles).cols - indent,
                    TermScalar::INFINITY,
                ),
                Axes::new(false, false),
            ),
        )?;

        let mut item_frame = TermFrame::new(TermSize::new(
            indent + body_frame.size().cols,
            body_frame.size().rows.max(TermScalar::ONE),
        ));
        item_frame.push_frame(TermPoint::new(TermScalar::ZERO, TermScalar::ZERO), prefix);
        item_frame.push_frame(TermPoint::new(indent, TermScalar::ZERO), body_frame);

        if !item_frame.size().is_empty() {
            frames.push(item_frame);
        }

        number = if reversed {
            number.saturating_sub(1)
        } else {
            number.saturating_add(1)
        };
    }

    Ok(compose_vertical(frames, gap, 0))
}

/// Layout a definition list (terms).
///
/// Each item is rendered as `term: description` and items are stacked
/// vertically.
#[typst_macros::time(span = elem.span())]
pub fn layout_terms(
    elem: &Packed<TermsElem>,
    engine: &mut Engine,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let tight = elem.tight.get(styles);
    let gap: Row = if tight {
        TermScalar::ZERO
    } else {
        config.par_gap
    };

    let separator = ": ".to_string(); // elem.separator is Content; use default

    let mut frames: Vec<TermFrame> = Vec::new();

    for item in &elem.children {
        let term_frame = crate::flow::layout_term_frame_from_content(engine,&item.term,
            typst_library::introspection::Locator::root(),
            styles,
            TermRegion::new(
                TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
                Axes::new(false, false),
            ),
        )?;
        let term_cols = term_frame.size().cols;

        let sep_text = format!("{} ", separator);
        let sep_cols = TermScalar::new(sep_text.len() as i32);
        let sep_frame = TermFrame::text(
            sep_text,
            Default::default(),
            sep_cols,
            TermScalar::ONE,
        );
        let sep_cols_size = sep_frame.size().cols;

        let desc_frame = crate::flow::layout_term_frame_from_content(engine,&item.description,
            typst_library::introspection::Locator::root(),
            styles,
            TermRegion::new(
                TermSize::new(
                    lynchpin_library_ng::resolve_page_size(styles).cols - term_cols - TermScalar::new(2),
                    TermScalar::INFINITY,
                ),
                Axes::new(false, false),
            ),
        )?;

        let total_cols = term_cols + sep_cols_size + desc_frame.size().cols;
        let total_rows = term_frame
            .size()
            .rows
            .max(sep_frame.size().rows)
            .max(desc_frame.size().rows)
            .max(TermScalar::ONE);

        let mut item_frame = TermFrame::new(TermSize::new(total_cols, total_rows));
        let mut x = TermScalar::ZERO;
        let term_cols2 = term_frame.size().cols;
        item_frame.push_frame(TermPoint::new(x, TermScalar::ZERO), term_frame);
        x += term_cols2;
        let sep_cols2 = sep_frame.size().cols;
        item_frame.push_frame(TermPoint::new(x, TermScalar::ZERO), sep_frame);
        x += sep_cols2;
        item_frame.push_frame(TermPoint::new(x, TermScalar::ZERO), desc_frame);

        if !item_frame.size().is_empty() {
            frames.push(item_frame);
        }
    }

    Ok(compose_vertical(frames, gap, 0))
}

/// Compose frames vertically, left-aligned.
fn compose_vertical(frames: Vec<TermFrame>, gap: Row, _baseline_idx: usize) -> TermFrame {
    if frames.is_empty() {
        return TermFrame::new(TermSize::ZERO);
    }

    let max_cols: Col = frames.iter().map(|f| f.size().cols).max().unwrap_or(TermScalar::ZERO);
    let total_rows: Row = frames.iter().map(|f| f.size().rows.max(TermScalar::ONE)).sum::<Row>()
        + gap * TermScalar::from_f64(frames.len().saturating_sub(1) as f64);

    let mut out = TermFrame::new(TermSize::new(max_cols, total_rows.max(TermScalar::ONE)));

    let mut y: Row = TermScalar::ZERO;
    let n = frames.len();
    for (i, frame) in frames.into_iter().enumerate() {
        let h = frame.size().rows.max(TermScalar::ONE);
        out.push_frame(TermPoint::new(TermScalar::ZERO, y), frame);
        y += h;
        if i + 1 < n {
            y += gap;
        }
    }

    out
}
