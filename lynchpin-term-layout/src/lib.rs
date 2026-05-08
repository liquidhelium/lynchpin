//! Terminal layout engine for typst.
//!
//! This crate provides a terminal-based renderer for typst content,
//! producing `TermFrame` structures that can be rasterised into ANSI
//! terminal output.
//!
//! # Module map
//!
//! | Module         | Role                                                       |
//! |----------------|------------------------------------------------------------|
//! | `config`       | [`RenderMode`] and [`TermConfig`] (ASCII vs Unicode)       |
//! | `frame`        | [`TermFrame`], [`TermGrid`], [`TermSize`], [`TermPoint`]    |
//! | `stack`        | Horizontal/vertical frame composition helpers              |
//! | `shapes`       | Stretched delimiter and vertical-bar frames                |
//! | `pad`          | Padding wrappers                                           |
//! | `inline`       | Inline/paragraph layout                                    |
//! | `lists`        | Bullet, numbered, and definition-list item rendering       |
//! | `transforms`   | `move`/`rotate`/`scale`/`skew` stubs                      |
//! | `grid`         | Grid/table layout stubs                                    |
//! | `flow`         | Block-level layout and document entry point                |
//! | `math`         | Math equation layout                                       |

pub mod config;
pub mod frame;
pub mod math;

pub mod flow;
pub mod grid;
pub mod inline;
pub mod lists;
pub mod pad;
pub mod shapes;
pub mod stack;
pub mod transforms;

// Re-export the primary output type.
pub use flow::TermPage;
use typst::{
    foundations::{Resolve, StyleChain},
    layout::{Length, Rel, Spacing},
    text::TextElem,
};

pub fn to_length(length: &Rel<Length>, styles: StyleChain) -> usize {
    let font_size = styles.get(TextElem::size).0;
    (length.relative_to(font_size).resolve(styles) / font_size.resolve(styles)).round() as usize
}

pub fn eval_spacing(styles: StyleChain<'_>, s: &Spacing) -> usize {
    let rows = match s {
        Spacing::Rel(r) => to_length(r, styles),
        Spacing::Fr(_) => 1,
    };
    rows
}
