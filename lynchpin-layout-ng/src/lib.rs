//! Terminal layout engine — the terminal equivalent of `typst-layout`.
//!
//! This crate provides the complete layout pipeline for terminal output,
//! mirroring paged layout's structure file-for-file where possible.
pub mod flow;
pub mod pages;
pub mod inline;
pub mod grid;
pub mod math;
pub mod image;
pub mod lists;
pub mod modifiers;
pub mod pad;
pub mod repeat;
pub mod rules;
pub mod shapes;
pub mod stack;
pub mod transforms;

// Re-export commonly used types.
pub use pages::{TermDocument, TermPage};
