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
use typst::introspection::Locator;

// ── TermBlockBody ────────────────────────────────────────────────────────────

/// The content of a terminal block element — mirrors paged [`BlockBody`].
#[derive(Debug, Clone, PartialEq, Hash)]
pub enum TermBlockBody {
    /// Normal content — the flow pipeline realizes and layouts it.
    Content(Content),
    /// Custom single-region layout callback.
    SingleLayouter(TermBlockCallback),
    /// Custom multi-region layout callback.
    MultiLayouter(TermMultiCallback),
}

impl Default for TermBlockBody {
    fn default() -> Self {
        Self::Content(Content::default())
    }
}

impl Reflect for TermBlockBody {
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

impl IntoValue for TermBlockBody {
    fn into_value(self) -> Value {
        match self {
            Self::Content(content) => content.into_value(),
            _ => Value::None,
        }
    }
}

impl FromValue for TermBlockBody {
    fn from_value(value: Value) -> typst::diag::HintedStrResult<Self> {
        Content::from_value(value).map(Self::Content)
    }
}

// ── TermBlockCallback ────────────────────────────────────────────────────────

/// A layout callback for terminal block elements.
///
/// Wraps a closure that takes the original element, engine, locator, styles,
/// and region, and produces a [`TermFrame`]. Mirrors paged `BlockSingleCallback`.
pub struct TermBlockCallback {
    captured: Content,
    f: Arc<dyn Fn(&Content, &mut Engine<'_>, Locator<'_>, StyleChain<'_>, TermRegion) -> SourceResult<TermFrame> + Send + Sync>,
}

impl TermBlockCallback {
    /// Create from a packed native element and a function/closure.
    pub fn new<T, F>(
        captured: Packed<T>,
        f: F,
    ) -> Self
    where
        T: NativeElement,
        F: Fn(&Packed<T>, &mut Engine<'_>, Locator<'_>, StyleChain<'_>, TermRegion) -> SourceResult<TermFrame>
            + Send + Sync + 'static,
    {
        Self {
            captured: captured.pack(),
            f: Arc::new(move |content, engine, locator, styles, region| {
                let packed: &Packed<T> = content
                    .to_packed()
                    .expect("TermBlockCallback: wrong element type");
                f(packed, engine, locator, styles, region)
            }),
        }
    }

    /// Create from a plain [`Content`] value and a function/closure.
    ///
    /// Use this when the captured value isn't a native element (e.g. a styled
    /// body fragment produced by `Content::set(...)`).
    pub fn new_content<F>(captured: Content, f: F) -> Self
    where
        F: Fn(&Content, &mut Engine<'_>, Locator<'_>, StyleChain<'_>, TermRegion) -> SourceResult<TermFrame>
            + Send + Sync + 'static,
    {
        Self {
            captured,
            f: Arc::new(f),
        }
    }

    /// Invoke the callback.
    pub fn call(
        &self,
        engine: &mut Engine<'_>,
        locator: Locator<'_>,
        styles: StyleChain<'_>,
        region: TermRegion,
    ) -> SourceResult<TermFrame> {
        (self.f)(&self.captured, engine, locator, styles, region)
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

// ── TermMultiCallback ────────────────────────────────────────────────────────

/// A multi-region layout callback for terminal block elements.
///
/// Wraps a closure that takes the original element, engine, locator, styles,
/// and regions, and produces a [`TermFragment`]. Mirrors paged `BlockMultiCallback`.
pub struct TermMultiCallback {
    captured: Content,
    f: Arc<dyn Fn(&Content, &mut Engine<'_>, Locator<'_>, StyleChain<'_>, TermRegions) -> SourceResult<TermFragment> + Send + Sync>,
}

impl TermMultiCallback {
    /// Create from a packed native element and a function/closure.
    pub fn new<T, F>(
        captured: Packed<T>,
        f: F,
    ) -> Self
    where
        T: NativeElement,
        F: Fn(&Packed<T>, &mut Engine<'_>, Locator<'_>, StyleChain<'_>, TermRegions) -> SourceResult<TermFragment>
            + Send + Sync + 'static,
    {
        Self {
            captured: captured.pack(),
            f: Arc::new(move |content, engine, locator, styles, regions| {
                let packed: &Packed<T> = content
                    .to_packed()
                    .expect("TermMultiCallback: wrong element type");
                f(packed, engine, locator, styles, regions)
            }),
        }
    }

    /// Invoke the callback.
    pub fn call(
        &self,
        engine: &mut Engine<'_>,
        locator: Locator<'_>,
        styles: StyleChain<'_>,
        regions: TermRegions,
    ) -> SourceResult<TermFragment> {
        (self.f)(&self.captured, engine, locator, styles, regions)
    }
}

impl Clone for TermMultiCallback {
    fn clone(&self) -> Self {
        Self {
            captured: self.captured.clone(),
            f: Arc::clone(&self.f),
        }
    }
}

impl Debug for TermMultiCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TermMultiCallback")
            .field("captured", &self.captured)
            .finish()
    }
}

impl PartialEq for TermMultiCallback {
    fn eq(&self, other: &Self) -> bool {
        self.captured == other.captured
    }
}

impl std::hash::Hash for TermMultiCallback {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.captured.hash(state);
    }
}

impl Reflect for TermMultiCallback {
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

impl IntoValue for TermMultiCallback {
    fn into_value(self) -> Value {
        Value::None
    }
}

impl FromValue for TermMultiCallback {
    fn from_value(_value: Value) -> typst::diag::HintedStrResult<Self> {
        Err(typst::diag::HintedString::from(
            "cannot construct TermMultiCallback from value",
        ))
    }
}

// ── TermPage / TermDocument ──────────────────────────────────────────────────

/// A single terminal page — mirrors paged `Page`.
#[derive(Debug, Clone)]
pub struct TermPage {
    /// The page content frame.
    pub inner: TermFrame,
    /// Page number (1-based).
    pub number: usize,
}

/// A terminal document — mirrors paged `PagedDocument`.
pub type TermDocument = Vec<TermPage>;

// ── TermInlineElem ──────────────────────────────────────────────────────────

/// An inline-level element with a custom layouter.
///
/// Mirrors paged `InlineElem`.  The callback receives the available region
/// width and returns inline items that are placed into the paragraph flow.
#[typst_macros::elem]
pub struct TermInlineElem {
    /// The layout callback.
    pub cb: Option<TermInlineCallback>,
}

/// Callback for [`TermInlineElem`]

/// Callback for [`TermInlineElem`] — mirrors paged `InlineCallback`.
///
/// Returns a vector of inline items that are placed into the paragraph.
pub struct TermInlineCallback {
    captured: Content,
    f: Arc<dyn Fn(&Content, &mut Engine<'_>, Locator<'_>, StyleChain<'_>, TermRegion) -> SourceResult<Vec<TermInlineItem>> + Send + Sync>,
}

impl TermInlineCallback {
    pub fn new<T, F>(captured: Packed<T>, f: F) -> Self
    where
        T: NativeElement,
        F: Fn(&Packed<T>, &mut Engine<'_>, Locator<'_>, StyleChain<'_>, TermRegion) -> SourceResult<Vec<TermInlineItem>>
            + Send + Sync + 'static,
    {
        Self {
            captured: captured.pack(),
            f: Arc::new(move |content, engine, locator, styles, region| {
                let packed: &Packed<T> = content.to_packed().expect("TermInlineCallback: wrong element type");
                f(packed, engine, locator, styles, region)
            }),
        }
    }

    pub fn call(&self, engine: &mut Engine<'_>, locator: Locator<'_>, styles: StyleChain<'_>, region: TermRegion) -> SourceResult<Vec<TermInlineItem>> {
        (self.f)(&self.captured, engine, locator, styles, region)
    }
}

impl Clone for TermInlineCallback {
    fn clone(&self) -> Self {
        Self { captured: self.captured.clone(), f: Arc::clone(&self.f) }
    }
}

impl Debug for TermInlineCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TermInlineCallback").field("captured", &self.captured).finish()
    }
}

impl PartialEq for TermInlineCallback {
    fn eq(&self, other: &Self) -> bool { self.captured == other.captured }
}

impl std::hash::Hash for TermInlineCallback {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) { self.captured.hash(state); }
}

impl Reflect for TermInlineCallback {
    fn input() -> typst::foundations::CastInfo { typst::foundations::CastInfo::Any }
    fn output() -> typst::foundations::CastInfo { typst::foundations::CastInfo::Any }
    fn castable(_: &Value) -> bool { false }
}

impl IntoValue for TermInlineCallback {
    fn into_value(self) -> Value { Value::None }
}

impl FromValue for TermInlineCallback {
    fn from_value(_value: Value) -> typst::diag::HintedStrResult<Self> {
        Err(typst::diag::HintedString::from("cannot construct TermInlineCallback from value"))
    }
}

/// An item produced by an inline-level layouter.
#[derive(Debug, Clone)]
pub enum TermInlineItem {
    /// A text run.
    Text(ecow::EcoString, crossterm::style::ContentStyle),
    /// A sub-frame with baseline.
    Frame(TermFrame),
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
    /// The body — either content or a layout callback.
    pub body: Option<TermBlockBody>,
}
