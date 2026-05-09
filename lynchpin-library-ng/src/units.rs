//! Conversion utilities: typst lengths → terminal columns/rows.
//!
//! Terminal layout uses integer cell coordinates while paged layout uses
//! floating-point `Abs` values.  This module provides pure conversion
//! functions that replicate the paged track-sizing logic with terminal
//! units.

use typst::foundations::{Resolve, StyleChain};
use typst::layout::{Abs, Fr, Length, Rel, Sizing, TrackSizings};
use typst::text::TextElem;

use crate::frame::{Col, Row};
use crate::scalar::TermScalar;

// ── Basic conversion ──────────────────────────────────────────────────────────

/// Convert an absolute length to terminal columns (1 col ≈ font size).
#[inline]
pub fn abs_to_cols(abs: Abs, font_size: Abs) -> Col {
    TermScalar::from_f64((abs / font_size).round().max(1.0))
}

/// Convert an absolute length to terminal rows.
#[inline]
pub fn abs_to_rows(abs: Abs, font_size: Abs) -> Row {
    TermScalar::from_f64((abs / font_size).round().max(1.0))
}

/// Resolve a `Rel<Length>` to terminal columns.
pub fn rel_to_cols(rel: &Rel<Length>, styles: StyleChain) -> Col {
    let font_size = styles.get(TextElem::size).0.resolve(styles);
    let abs: Abs = rel.resolve(styles).relative_to(font_size);
    abs_to_cols(abs, font_size)
}

/// Resolve a `Rel<Length>` to terminal rows.
pub fn rel_to_rows(rel: &Rel<Length>, styles: StyleChain) -> Row {
    let font_size = styles.get(TextElem::size).0.resolve(styles);
    let abs: Abs = rel.resolve(styles).relative_to(font_size);
    abs_to_rows(abs, font_size)
}

// ── Track sizing ──────────────────────────────────────────────────────────────

/// Distribute `remaining` columns among fractional tracks.
fn share_fr(total_fr: Fr, fr: Fr, remaining: Col) -> Col {
    if total_fr == Fr::zero() || remaining <= TermScalar::ZERO {
        return TermScalar::ZERO;
    }
    let share = remaining.get() as f64 * (fr.get() as f64 / total_fr.get() as f64);
    TermScalar::from_f64(share.round())
}

/// Shrink auto-sized columns fairly so that total fits in `available`.
fn shrink_auto(cols: &mut [Col], available: Col) {
    let count = cols.iter().filter(|&&c| c > TermScalar::ZERO).count();
    if count == 0 {
        return;
    }

    let mut last;
    let mut fair = Col::ZERO;
    let mut redistribute = available;
    let mut overlarge = count;
    let mut changed = true;

    while changed && overlarge > 0 {
        changed = false;
        last = fair;
        fair = TermScalar::from_f64(redistribute.get() as f64 / overlarge as f64);

        for &c in cols.iter() {
            if c > TermScalar::ZERO && c <= fair && c > last {
                redistribute = redistribute - c;
                overlarge -= 1;
                changed = true;
            }
        }
    }

    for c in cols.iter_mut() {
        if *c > fair {
            *c = fair;
        }
    }
}

/// Resolve a full set of track sizings.
///
/// Replicates the paged column-sizing algorithm:
/// 1. Resolve `Rel` (fixed) tracks first.
/// 2. Distribute remaining space to `Auto` tracks.
/// 3. If there is still space, grow `Fr` tracks; if not, shrink `Auto`.
///
/// `auto_sizes` should contain the measured content width for each
/// `Auto` track (0 for non-auto tracks).
///
/// Returns one `Col` per track in `tracks`.
pub fn resolve_tracks(
    tracks: &TrackSizings,
    available: Col,
    auto_sizes: &[Col],
    styles: StyleChain,
) -> Vec<Col> {
    let n = tracks.0.len().max(1);
    let mut resolved = vec![TermScalar::ZERO; n];
    let mut fixed_total = TermScalar::ZERO;
    let mut total_fr = Fr::zero();

    // Phase 1: resolve Rel tracks, accumulate Fr.
    for (i, sizing) in tracks.0.iter().enumerate() {
        match sizing {
            Sizing::Rel(rel) => {
                let c = rel_to_cols(rel, styles);
                resolved[i] = c;
                fixed_total = fixed_total + c;
            }
            Sizing::Auto => {}
            Sizing::Fr(fr) => {
                total_fr += *fr;
            }
        }
    }

    let remaining = (available - fixed_total).max(TermScalar::ZERO);

    // Phase 2: assign auto sizes.
    let mut auto_total = TermScalar::ZERO;
    for (i, sizing) in tracks.0.iter().enumerate() {
        if *sizing == Sizing::Auto {
            let c = auto_sizes
                .get(i)
                .copied()
                .unwrap_or(TermScalar::ONE)
                .max(TermScalar::ONE);
            resolved[i] = c;
            auto_total = auto_total + c;
        }
    }

    // Phase 3: grow Fr or shrink Auto.
    let leftover = remaining - auto_total;
    if leftover > TermScalar::ZERO && total_fr != Fr::zero() {
        // Grow fractional tracks.
        for (i, sizing) in tracks.0.iter().enumerate() {
            if let Sizing::Fr(fr) = sizing {
                resolved[i] = share_fr(total_fr, *fr, leftover);
            }
        }
    } else if leftover < TermScalar::ZERO {
        // Shrink auto columns to fit.
        shrink_auto(&mut resolved, remaining);
    }

    // Ensure minimum width of 1.
    for c in &mut resolved {
        *c = (*c).max(TermScalar::ONE);
    }

    resolved
}

// ── Spacing ───────────────────────────────────────────────────────────────────

/// Resolve a `Spacing` value to terminal columns.
pub fn spacing_to_cols(spacing: &typst::layout::Spacing, styles: StyleChain) -> Col {
    match spacing {
        typst::layout::Spacing::Rel(rel) => rel_to_cols(rel, styles),
        typst::layout::Spacing::Fr(_) => TermScalar::ONE,
    }
}

/// Resolve a `Spacing` value to terminal rows.
pub fn spacing_to_rows(spacing: &typst::layout::Spacing, styles: StyleChain) -> Row {
    match spacing {
        typst::layout::Spacing::Rel(rel) => rel_to_cols(rel, styles),
        typst::layout::Spacing::Fr(_) => TermScalar::ONE,
    }
}
