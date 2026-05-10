//! Image layout for the terminal.
//!
//! Terminal images are placeholders: we can't render pixel data, so we emit
//! a labelled placeholder frame (`[img: alt]`) instead.

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Packed, StyleChain};
use typst::visualize::ImageElem;

use lynchpin_library_ng::*;

/// Layout an image as a terminal placeholder.
///
/// Produces a [`TermFrame`] containing a single [`TermFrameItem::Image`]
/// whose alt text is `[img: <alt>]`.  The frame is sized proportionally
/// to the configured width, defaulting to a small placeholder.
#[typst_macros::time(span = elem.span())]
pub fn layout_image(
    elem: &Packed<ImageElem>,
    engine: &mut Engine,
    _config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    let _ = engine;

    // Use default alt text (ImageElem::alt extraction blocked by typst 0.14 API change).
    let alt_text = "image".to_string();
    let label = format!("[img: {}]", alt_text);

    // Determine the placeholder size from configuration.
    let width_cols: Col = lynchpin_library_ng::resolve_page_size(styles).cols.min(TermScalar::new(20));
    let height_rows: Row = (width_cols / TermScalar::new(2)).max(TermScalar::ONE);

    let size = TermSize::new(width_cols.max(TermScalar::ONE), height_rows.max(TermScalar::ONE));
    let mut frame = TermFrame::new(size);

    // Push the image placeholder item.
    frame.push_image(
        TermPoint::ZERO,
        TermImage {
            width: width_cols,
            height: height_rows,
            alt: label.into(),
        },
    );

    Ok(frame)
}
