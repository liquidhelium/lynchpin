//! Terminal-specific typst element definitions and show rules.
//!
//! This crate mirrors what `typst-library` does for the paged target:
//! define custom elements and register show rules for a terminal target.

pub mod config;
pub mod frame;
pub mod regions;
pub mod units;

use std::fmt::Debug;

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, NativeElement, Packed, StyleChain};
use typst::foundations::{IntoValue, Reflect, Value};
use typst::foundations::FromValue;
use crate::config::TermConfig;
use crate::frame::TermFrame;

// ── callback! macro (adapted from typst-library/src/layout/container.rs) ─────

macro_rules! callback {
    ($name:ident = ($($param:ident: $param_ty:ty),* $(,)?) -> $ret:ty) => {
        #[derive(Debug, Clone, Hash)]
        #[allow(clippy::derived_hash_with_manual_eq)]
        pub struct $name {
            captured: Content,
            f: fn(&Content, $($param_ty),*) -> $ret,
        }

        impl $name {
            pub fn new<T: NativeElement>(
                captured: Packed<T>,
                f: fn(&Packed<T>, $($param_ty),*) -> $ret,
            ) -> Self {
                Self {
                    captured: captured.pack(),
                    #[allow(clippy::missing_transmute_annotations)]
                    f: unsafe { std::mem::transmute(f) },
                }
            }

            pub fn call(&self, $($param: $param_ty),*) -> $ret {
                (self.f)(&self.captured, $($param),*)
            }
        }

        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                self.captured.eq(&other.captured)
            }
        }
    };
}

// ── TermBlockCallback ────────────────────────────────────────────────────────

callback! {
    TermBlockCallback = (
        engine: &mut Engine<'_>,
        config: &TermConfig,
        styles: StyleChain<'_>,
    ) -> SourceResult<TermFrame>
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
        Err(typst::diag::HintedString::from("cannot construct TermBlockCallback from value"))
    }
}

// ── TermBlockElem ────────────────────────────────────────────────────────────

/// A terminal-specific block element that carries a layout callback.
///
/// During block layout, the callback is invoked to produce a [`TermFrame`].
#[typst_macros::elem]
pub struct TermBlockElem {
    /// The layout callback.
    #[required]
    pub cb: TermBlockCallback,
}
