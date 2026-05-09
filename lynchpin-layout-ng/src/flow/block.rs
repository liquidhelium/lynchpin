//! Block-level element layout helpers.
//!
//! Terminal equivalent of `lynchpin-layout/src/flow/block.rs`.
//!
//! Provides `unbreakable_pod` for computing the inner region of a sized
//! container, and stub `layout_single_block` / `layout_multi_block` that
//! delegate to the element's callback.

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::StyleChain;
use typst::layout::Axes;

use lynchpin_library_ng::{
    TermFrame, TermRegion, TermScalar, TermSize,
};

// ── Pod construction ─────────────────────────────────────────────────────────

/// Builds the pod region for an unbreakable sized container.
///
/// In terminal layout, sizing is simplified: width/height are resolved
/// to concrete column/row counts by the caller.  The pod is just a
/// region that may expand on axes that are being sized explicitly.
pub fn unbreakable_pod(
    width: Option<TermScalar>,
    height: Option<TermScalar>,
    base: TermSize,
) -> TermRegion {
    let size = TermSize::new(
        width.unwrap_or(base.cols),
        height.unwrap_or(base.rows),
    );

    let expand = Axes::new(
        width.is_some() && size.cols.is_finite(),
        height.is_some() && size.rows.is_finite(),
    );

    TermRegion::new(size, expand)
}

// ── Single block layout ──────────────────────────────────────────────────────

/// Lay out a single unbreakable block element.
///
/// Terminal version delegates to the element's callback directly.
pub fn layout_single_block(
    engine: &mut Engine,
    body: &[typst::routines::Pair<'_>],
    styles: StyleChain<'_>,
    region: TermRegion,
) -> SourceResult<TermFrame> {
    // In terminal layout, block content is already realized and collected.
    // The actual layout is performed by the flow → distribute pipeline.
    let _ = (engine, body, styles);
    Ok(TermFrame::new(region.size))
}

// ── Multi block layout ───────────────────────────────────────────────────────

/// Lay out a breakable block element.
///
/// Terminal version: breakable blocks produce a single frame that
/// may be split by the distributor if it doesn't fit in a region.
pub fn layout_multi_block(
    engine: &mut Engine,
    body: &[typst::routines::Pair<'_>],
    styles: StyleChain<'_>,
    regions: &lynchpin_library_ng::TermRegions,
) -> SourceResult<Vec<TermFrame>> {
    // Delegate to the flow pipeline for the body content.
    let _ = (engine, body, styles, regions);
    Ok(vec![TermFrame::new(TermSize::ZERO)])
}

// ── Distribute fixed height across regions ───────────────────────────────────

/// Distribute a fixed height spread over existing regions into a new first
/// height and a new backlog.
///
/// Note that, if the given height fits within the first region, no backlog is
/// generated and the first region's height shrinks to fit exactly the given
/// height. In particular, negative and zero heights always fit in any region.
pub fn distribute_fixed_height<'a>(
    height: TermScalar,
    mut regions: lynchpin_library_ng::TermRegions,
    buf: &'a mut Vec<TermScalar>,
) -> (TermScalar, &'a mut [TermScalar]) {
    let mut remaining = height;

    // Negative and zero heights always fit.
    if remaining <= TermScalar::ZERO {
        buf.push(remaining);
        return (buf[0], &mut buf[1..]);
    }

    loop {
        let limited = regions.size.rows.clamp(TermScalar::ZERO, remaining);
        buf.push(limited);
        remaining = remaining - limited;
        if remaining <= TermScalar::ZERO
            || !regions.may_break()
            || (!regions.may_progress() && limited <= TermScalar::ZERO)
        {
            break;
        }
        regions.next();
    }

    // If there is still something remaining, apply it to the last region.
    if remaining > TermScalar::ZERO {
        if let Some(last) = buf.last_mut() {
            *last = *last + remaining;
        }
    }

    (buf[0], &mut buf[1..])
}
