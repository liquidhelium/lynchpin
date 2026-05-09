//! Terminal-specific typst element definitions and foundation types.
//!
//! This crate provides the foundational types for terminal layout — the
//! terminal equivalents of typst's `Abs`, `Frame`, `Region`, etc.
//!
//! # Module map
//!
//! | Module      | Paged equivalent              |
//! |-------------|-------------------------------|
//! | `scalar`    | `typst_utils::Scalar` + `Abs` |
//! | `frame`     | `Frame`, `FrameItem`, `Size`, `Point` |
//! | `regions`   | `Region`, `Regions`           |
//! | `config`    | Page setup                    |
//! | `units`     | Length conversion             |

pub mod config;
pub mod frame;
pub mod regions;
pub mod scalar;
pub mod units;

// Re-export the most-used types.
pub use config::{RenderMode, TermConfig, resolve_page_size};
pub use frame::{
    Alignment, Axes, Col, Row, TermCell, TermFrame, TermFrameItem, TermGeometry, TermGrid,
    TermImage, TermPoint, TermShape, TermSize, char_cols, text_cols,
};
pub use regions::{TermRegion, TermRegions};
pub use scalar::TermScalar;

use std::fmt::Debug;
use std::sync::Arc;

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, NativeElement, Packed, StyleChain};
use typst::foundations::{IntoValue, Reflect, Value, FromValue};

// ── TermBlockCallback ────────────────────────────────────────────────────────

/// A layout callback for terminal block elements.
///
/// Wraps a closure that takes the original element, engine, config, and styles,
/// and produces a [`TermFrame`]. Accepts both `fn` pointers and closures.
pub struct TermBlockCallback {
    captured: Content,
    f: Arc<dyn Fn(&Content, &mut Engine<'_>, &TermConfig, StyleChain<'_>) -> SourceResult<TermFrame> + Send + Sync>,
}

impl TermBlockCallback {
    /// Create from a packed native element and a function/closure.
    pub fn new<T, F>(
        captured: Packed<T>,
        f: F,
    ) -> Self
    where
        T: NativeElement,
        F: Fn(&Packed<T>, &mut Engine<'_>, &TermConfig, StyleChain<'_>) -> SourceResult<TermFrame>
            + Send + Sync + 'static,
    {
        Self {
            captured: captured.pack(),
            f: Arc::new(move |content, engine, config, styles| {
                let packed: &Packed<T> = content
                    .to_packed()
                    .expect("TermBlockCallback: wrong element type");
                f(packed, engine, config, styles)
            }),
        }
    }

    /// Invoke the callback.
    pub fn call(
        &self,
        engine: &mut Engine<'_>,
        config: &TermConfig,
        styles: StyleChain<'_>,
    ) -> SourceResult<TermFrame> {
        (self.f)(&self.captured, engine, config, styles)
    }
}

impl Clone for TermBlockCallback {
    fn clone(&self) -> Self {
        Self {
            captured: self.captured.clone(),
            f: Arc::clone(&self.f),
        }
    }
}

impl Debug for TermBlockCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TermBlockCallback")
            .field("captured", &self.captured)
            .finish()
    }
}

impl PartialEq for TermBlockCallback {
    fn eq(&self, other: &Self) -> bool {
        self.captured == other.captured
    }
}

impl std::hash::Hash for TermBlockCallback {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.captured.hash(state);
    }
}

impl Reflect for TermBlockCallback {
    fn input() -> typst::foundations::CastInfo {
        typst::foundations::CastInfo::Any
    }
    fn output() -> typst::foundations::CastInfo {
        typst::foundations::CastInfo::Any
    }
    fn castable(_: &Value) -> bool {
        false
    }
}

impl IntoValue for TermBlockCallback {
    fn into_value(self) -> Value {
        Value::None
    }
}

impl FromValue for TermBlockCallback {
    fn from_value(_value: Value) -> typst::diag::HintedStrResult<Self> {
        Err(typst::diag::HintedString::from(
            "cannot construct TermBlockCallback from value",
        ))
    }
}

// ── TermFragment ─────────────────────────────────────────────────────────────

/// A sequence of terminal frames — terminal equivalent of `Fragment`.
pub type TermFragment = Vec<TermFrame>;

#[inline]
pub fn fragment_into_frames(fragment: TermFragment) -> Vec<TermFrame> {
    fragment
}

#[inline]
pub fn fragment_from_frame(frame: TermFrame) -> TermFragment {
    vec![frame]
}

#[inline]
pub fn fragment_from_frames(frames: Vec<TermFrame>) -> TermFragment {
    frames
}

// ── TermBlockElem ────────────────────────────────────────────────────────────

/// A terminal-specific block element that carries a layout callback.
///
/// Mirrors paged `BlockElem` with `BlockBody::SingleLayouter` /
/// `MultiLayouter`.  During block layout, the callback is invoked to
/// produce a [`TermFrame`].
#[typst_macros::elem]
pub struct TermBlockElem {
    /// The layout callback.
    #[required]
    pub cb: TermBlockCallback,
}
