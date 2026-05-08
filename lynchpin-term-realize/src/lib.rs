//! Terminal content realization for typst.
//!
//! This crate extracts the `realize_term` function from `lynchpin`
//! so that both `lynchpin-term-layout` and `lynchpin` can depend on
//! it without creating a circular dependency.
//!
//! `realize_term` walks a typst content tree and produces a flat
//! stream of `(Content, StyleChain)` pairs with the following guarantees:
//! - No `TagElem` elements.
//! - Inline content is grouped into `ParElem`.
//! - List/enum/term items are grouped into their container elements.
//! - User-defined show rules are applied; built-in rules are not.

pub mod realize;

pub use realize::{TermRealizationKind, realize_term};
