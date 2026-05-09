//! Terminal layout engine.
//!
//! The terminal equivalent of `lynchpin-layout`.  All modules mirror their
//! paged counterparts but use terminal types (`TermFrame`, `TermScalar`,
//! `TermSize`, …) in place of paged types (`Frame`, `Abs`, `Size`, …).

mod flow;
mod grid;
mod image;
mod inline;
mod lists;
mod math;
mod modifiers;
mod pad;
mod pages;
mod repeat;
mod rules;
mod shapes;
mod stack;
mod transforms;

pub use self::flow::{layout_term_fragment, layout_term_frame};
pub use self::pages::layout_term_document;
pub use self::rules::register;
