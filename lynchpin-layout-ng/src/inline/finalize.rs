use typst_library::introspection::SplitLocator;
use typst_utils::Numeric;

use lynchpin_library_ng::*;

use super::*;

/// Turns the selected lines into frames.
#[typst_macros::time]
pub fn finalize(
    engine: &mut Engine,
    p: &Preparation,
    lines: &[Line],
    region: TermSize,
    expand: bool,
    locator: &mut SplitLocator<'_>,
) -> SourceResult<TermFragment> {
    let region_cols_abs = Abs::raw(region.cols.get() as f64);
    let region_rows_abs = Abs::raw(region.rows.get() as f64);

    // Determine the resulting width: Full width of the region if we should
    // expand or there's fractional spacing, fit-to-width otherwise.
    let width =
        if !region.cols.is_finite()
            || (!expand && lines.iter().all(|line| line.fr().is_zero()))
        {
            region_cols_abs.min(
                p.config.hanging_indent
                    + lines
                        .iter()
                        .map(|line| line.width)
                        .max()
                        .unwrap_or_default(),
            )
        } else {
            region_cols_abs
        };

    // Stack the lines into one frame per region.
    lines
        .iter()
        .map(|line| commit(engine, p, line, width, region_rows_abs, locator))
        .collect::<SourceResult<_>>()
}
