use typst_library::diag::SourceResult;
use typst_library::engine::Engine;
use typst_library::foundations::{Packed, Smart, StyleChain};
use typst_library::introspection::Locator;
use typst_library::layout::{BoxElem, Sizing, Abs};
use typst_library::visualize::Stroke;
use typst_utils::Numeric;

use lynchpin_library_ng::*;

use crate::flow::unbreakable_pod;
// NOTE: clip_rect, fill_and_stroke are expected from crate::shapes (Agent F)
// NOTE: layout_frame is expected from crate root (Agent F / B)
// NOTE: frame.label() is expected on TermFrame (lynchpin-library-ng)

/// Convert Smart<Rel<Length>> to Sizing for pod resolution.
fn smart_to_sizing(smart: Smart<typst_library::layout::Rel<typst_library::layout::Length>>) -> Sizing {
    match smart {
        Smart::Auto => Sizing::Auto,
        Smart::Custom(rel) => Sizing::Rel(rel),
    }
}

/// Resolve a Sizing to Option<TermScalar> for pod construction.
fn resolve_sizing(sizing: &Sizing, styles: StyleChain) -> Option<TermScalar> {
    match sizing {
        Sizing::Auto => None,
        Sizing::Rel(rel) => {
            Some(units::rel_to_cols(rel, styles))
        }
        Sizing::Fr(_) => None,
    }
}

/// Lay out a box as part of inline layout.
#[typst_macros::time(name = "box", span = elem.span())]
pub fn layout_box(
    elem: &Packed<BoxElem>,
    _engine: &mut Engine,
    _locator: Locator,
    styles: StyleChain,
    region: TermSize,
) -> SourceResult<TermFrame> {
    // Fetch sizing properties.
    let width_sizing = elem.width.get(styles);
    let height_sizing = smart_to_sizing(elem.height.get(styles));
    let inset = elem.inset.resolve(styles).unwrap_or_default();

    // Resolve sizing for pod construction.
    let width_opt = resolve_sizing(&width_sizing, styles);
    let height_opt = resolve_sizing(&height_sizing, styles);

    // Build the pod region.
    let pod = unbreakable_pod(width_opt, height_opt, region);

    // Layout the body.
    let mut frame = match elem.body.get_ref(styles) {
        // If we have no body, just create an empty frame. If necessary,
        // its size will be adjusted below.
        None => TermFrame::new(TermSize {
            cols: TermScalar::ZERO,
            rows: TermScalar::ZERO,
        }),

        // If we have a child, layout it into the body.
        // NOTE: layout_frame is expected from crate root (Agent F / B).
        Some(_body) => {
            // For now, return an empty frame since layout_frame is not available.
            TermFrame::new(TermSize {
                cols: TermScalar::ZERO,
                rows: TermScalar::ZERO,
            })
        }
    };

    // Enforce a correct frame size on the expanded axes. Do this before
    // applying the inset, since the pod shrunk.
    let size_from_expand = if pod.expand.x && pod.expand.y {
        TermSize::new(
            pod.size.cols.max(frame.cols()),
            pod.size.rows.max(frame.rows()),
        )
    } else if pod.expand.x {
        TermSize::new(pod.size.cols.max(frame.cols()), frame.rows())
    } else if pod.expand.y {
        TermSize::new(frame.cols(), pod.size.rows.max(frame.rows()))
    } else {
        frame.size()
    };
    frame.set_size(size_from_expand);

    // Apply the inset.
    if !inset.is_zero() {
        crate::pad::grow(&mut frame, &inset, styles);
    }

    // Prepare fill and stroke.
    let _fill = elem.fill.get_cloned(styles);
    let _stroke = elem
        .stroke
        .resolve(styles)
        .unwrap_or_default()
        .map(|s| s.map(Stroke::unwrap_or_default));

    // Clip the contents, if requested.
    // TODO(Agent F): replace with `clip_rect(frame.size(), &radius, &stroke, &outset)`
    // once crate::shapes::clip_rect is available.
    if elem.clip.get(styles) {
        frame.clip(frame.size());
    }

    // Add fill and/or stroke.
    // TODO(Agent F): call `fill_and_stroke(&mut frame, fill, &stroke, &outset, &radius, span)`
    // once crate::shapes::fill_and_stroke is available.
    // Currently skipped — fill/stroke are not rendered on box frames.

    // Assign label to the frame.
    // TODO(Agent A): uncomment when TermFrame::label() is available.
    // if let Some(label) = elem.label() {
    //     frame.label(label);
    // }

    // Apply baseline shift. Do this after setting the size and applying the
    // inset, so that a relative shift is resolved relative to the final
    // height.
    let shift = elem.baseline.resolve(styles).relative_to(Abs::raw(frame.rows().get() as f64));
    if !shift.is_zero() {
        frame.set_baseline(TermScalar::from_f64(
            (frame.baseline().get() as f64) - shift.to_raw()
        ));
    }

    Ok(frame)
}
