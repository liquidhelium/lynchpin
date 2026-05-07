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

pub mod stack;
pub mod shapes;
pub mod pad;
pub mod inline;
pub mod lists;
pub mod transforms;
pub mod grid;
pub mod flow;

// Re-export the primary output type.
pub use flow::TermPage;
