//! Repeat layout: tile a frame horizontally.
//!
//! Terminal equivalent of the paged `repeat.rs`, using `TermScalar` for
//! width arithmetic.

use typst_library::diag::{SourceResult, bail};
use typst_library::engine::Engine;
use typst_library::foundations::{Packed, Resolve, StyleChain};
use typst_library::layout::{AlignElem, Axes, RepeatElem};
use typst_library::text::TextElem;

use lynchpin_library_ng::*;
use lynchpin_library_ng::units::abs_to_cols;

/// Layout the repeated content.
///
/// Tiles the body frame horizontally to fill the available width,
/// optionally justifying the gap between repetitions.
#[typst_macros::time(span = elem.span())]
pub fn layout_repeat(
    elem: &Packed<RepeatElem>,
    engine: &mut Engine,
    _config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    // Layout a single piece with no expansion.
    let pod_region = TermRegion::new(
        TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
        Axes::new(false, false),
    );
    let piece = crate::flow::layout_term_frame(engine,&elem.body,
        typst_library::introspection::Locator::root(),
        styles,
        pod_region,
    )?;

    let width: Col = piece.size().cols;
    let fill: Col = lynchpin_library_ng::resolve_page_size(styles).cols;

    if !width.is_finite() || !fill.is_finite() {
        bail!(elem.span(), "repeat with no size restrictions");
    }

    let height = piece.size().rows.max(TermScalar::ONE);
    let mut frame = TermFrame::new(TermSize::new(fill, height));
    if piece.has_baseline() {
        frame.set_baseline(piece.baseline());
    }

    let font_size = styles.get(TextElem::size).0.resolve(styles);
    let gap: Col = abs_to_cols(elem.gap.resolve(styles), font_size);

    // N * w + (N - 1) * g ≤ F
    // N * (w + g) ≤ F + g
    // N ≤ (F + g) / (w + g)
    let count = if width > TermScalar::ZERO && (width + gap) > TermScalar::ZERO {
        let n = ((fill + gap).get() as f64 / (width + gap).get() as f64).floor() as i32;
        n.max(0)
    } else {
        0
    };

    let count_scalar = TermScalar::from_f64(count as f64);
    let remaining = fill - (count_scalar * (width + gap)).max(TermScalar::ZERO);
    let mut gap = gap;

    let justify = elem.justify.get(styles);
    if justify && count > 1 {
        gap += TermScalar::from_f64(remaining.get() as f64 / ((count - 1) as f64));
    }

    let align = styles.get(AlignElem::alignment).resolve(styles);
    let mut offset: Col = TermScalar::ZERO;
    if count <= 1 || !justify {
        offset = match align.x {
            typst_library::layout::FixedAlignment::Start => TermScalar::ZERO,
            typst_library::layout::FixedAlignment::Center => TermScalar::from_f64(remaining.get() as f64 / 2.0),
            typst_library::layout::FixedAlignment::End => remaining,
            // Fractional alignment handled as zero
        };
    }

    if width > TermScalar::ZERO {
        let max_count = (count as usize).min(1000);
        for _ in 0..max_count {
            frame.push_frame(TermPoint::new(offset, TermScalar::ZERO), piece.clone());
            offset += width + gap;
        }
    }

    Ok(frame)
}
