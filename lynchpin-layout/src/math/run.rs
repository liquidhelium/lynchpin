//! `TermMathRun` — a linear sequence of math fragments with composition logic.
//!
//! Analogous to `typst-layout`'s `MathRun`.

use crossterm::style::ContentStyle;
use ecow::EcoString;
use lynchpin_library::frame::{char_cols, Col, Row, TermFrame, TermPoint, TermSize};
use tracing::debug;

use super::fragment::{MathClass, TermMathFragment, TermMathFrameFragment};

// ── TermMathRun ───────────────────────────────────────────────────────────────

/// A linear collection of [`TermMathFragment`]s with spacing rules applied.
#[derive(Debug, Default, Clone)]
pub struct TermMathRun(pub Vec<TermMathFragment>);

impl TermMathRun {
    /// Build a run from raw fragments, inserting automatic inter-fragment
    /// spacing and collapsing weak spacings.
    ///
    /// Mirrors the behaviour of `typst-layout`'s `MathRun::new`:
    ///
    /// * `Space` (from `SpaceElem`) is **not** pushed; instead a
    ///   `pending_space` flag is set.  The flag is only materialised into a
    ///   `Spacing(1, false)` when the neighbouring fragments both have
    ///   `is_spaced() == false` and one of them has `is_spaced() == true`.
    ///   For all other pairs (operators, relations, …) the auto-spacing rules
    ///   take precedence and the soft-space is discarded.
    /// * Explicit `Spacing` resets both `last` and `pending_space`, so it
    ///   always disables automatic spacing.
    pub fn new(frags: Vec<TermMathFragment>) -> Self {
        let mut resolved: Vec<TermMathFragment> = Vec::with_capacity(frags.len());
        // Index of the last non-ignorant Frame fragment in `resolved`.
        let mut last: Option<usize> = None;
        // Whether a `Space` fragment (from SpaceElem) has been seen since the
        // last non-ignorant fragment.  Consumed (or discarded) on the next
        // non-ignorant fragment.
        let mut pending_space = false;

        for mut frag in frags {
            match &frag {
                // ── Soft space from SpaceElem ────────────────────────────────
                // Do NOT push.  Store as pending so the spacing() function can
                // decide whether to materialise it based on is_spaced().
                TermMathFragment::Space => {
                    if last.is_some() {
                        pending_space = true;
                    }
                    continue;
                }

                // ── Explicit spacing ─────────────────────────────────────────
                TermMathFragment::Spacing(cols, weak) => {
                    let cols = *cols;
                    let weak = *weak;
                    // Explicit spacing resets auto-spacing context.
                    last = None;
                    pending_space = false;
                    if weak {
                        match resolved.last_mut() {
                            // Skip leading weak spacing.
                            None => continue,
                            // Merge with an existing trailing weak spacing.
                            Some(TermMathFragment::Spacing(prev_cols, true)) => {
                                *prev_cols = (*prev_cols).max(cols);
                                continue;
                            }
                            Some(_) => {}
                        }
                    }
                    resolved.push(frag);
                    continue;
                }

                // ── Structural fragments ─────────────────────────────────────
                TermMathFragment::Align => {
                    resolved.push(frag);
                    continue;
                }
                TermMathFragment::Linebreak => {
                    last = None;
                    pending_space = false;
                    resolved.push(frag);
                    continue;
                }

                // ── Rendered content ─────────────────────────────────────────
                TermMathFragment::Frame(_) => {}
            }

            // Reclassify `Varying` operators (e.g. `+`, `-`) as `Binary` when
            // they follow a Normal / Alphabetic / Closing / Fence fragment,
            // i.e. they are acting as infix operators, not unary prefix.
            // Mirrors the upstream logic in `typst-layout`'s `MathRun::new`.
            if frag.class() == MathClass::Vary {
                if matches!(
                    last.map(|i| resolved[i].class()),
                    Some(
                        MathClass::Normal
                            | MathClass::Alphabetic
                            | MathClass::Closing
                            | MathClass::Fence
                    )
                ) {
                    frag.set_class(MathClass::Binary);
                }
            }

            // Insert automatic spacing between the previous non-ignorant
            // fragment and this one.
            if !frag.is_ignorant() {
                if let Some(i) = last {
                    let sp = pending_space;
                    if let Some(gap) = auto_spacing(&resolved[i], sp, &frag) {
                        // Insert the auto-spacing right after `last`.
                        resolved.insert(i + 1, TermMathFragment::Spacing(gap, false));
                    }
                }
                // Consume the pending soft-space (whether or not it was used).
                pending_space = false;
                // The current fragment will be at index `resolved.len()` after push.
                last = Some(resolved.len());
            }

            resolved.push(frag);
        }

        // Drop trailing weak spacing.
        if let Some(TermMathFragment::Spacing(_, true)) = resolved.last() {
            resolved.pop();
        }

        Self(resolved)
    }

    // ── Fragment queries ──────────────────────────────────────────────────────

    pub fn iter(&self) -> std::slice::Iter<'_, TermMathFragment> {
        self.0.iter()
    }

    /// Total render width (sum of all fragment widths, including spacing).
    pub fn total_width(&self) -> Col {
        self.0.iter().map(|f| f.width()).sum()
    }

    /// Maximum ascent of all height-contributing fragments.
    pub fn ascent(&self) -> Row {
        self.0
            .iter()
            .filter(|f| !matches!(f, TermMathFragment::Align | TermMathFragment::Linebreak))
            .map(|f| f.ascent())
            .max()
            .unwrap_or(Row::ZERO)
    }

    /// Maximum descent of all height-contributing fragments.
    pub fn descent(&self) -> Row {
        self.0
            .iter()
            .filter(|f| !matches!(f, TermMathFragment::Align | TermMathFragment::Linebreak))
            .map(|f| f.descent())
            .max()
            .unwrap_or(Row::ZERO)
    }

    /// Math class predicted for the composed output.
    pub fn class(&self) -> MathClass {
        if self.0.len() == 1 {
            self.0
                .first()
                .map(|f| f.class())
                .unwrap_or(MathClass::Normal)
        } else {
            MathClass::Normal
        }
    }

    /// Whether this run contains a [`Linebreak`](TermMathFragment::Linebreak).
    pub fn is_multiline(&self) -> bool {
        self.0
            .iter()
            .any(|f| matches!(f, TermMathFragment::Linebreak))
    }

    /// Number of visual rows (linebreak count + 1, ignoring a trailing
    /// linebreak).
    pub fn row_count(&self) -> usize {
        let mut count = 1
            + self
                .0
                .iter()
                .filter(|f| matches!(f, TermMathFragment::Linebreak))
                .count();
        // A trailing linebreak doesn't introduce an extra empty row.
        if let Some(TermMathFragment::Linebreak) = self.0.last() {
            count -= 1;
        }
        count
    }

    // ── Row splitting ─────────────────────────────────────────────────────────

    /// Split by [`Linebreak`](TermMathFragment::Linebreak)s, returning one
    /// `TermMathRun` per visual row.
    pub fn rows(&self) -> Vec<TermMathRun> {
        self.0
            .split(|f| matches!(f, TermMathFragment::Linebreak))
            .map(|slice| TermMathRun(slice.to_vec()))
            .collect()
    }

    // ── Frame construction ────────────────────────────────────────────────────

    /// Compose all fragments into a single row [`TermFrame`] with baseline
    /// alignment.
    ///
    /// * Total width  = sum of all fragment widths (including spacing).
    /// * Total height = max_ascent + max_descent.
    /// * Baseline     = max_ascent.
    /// * Each fragment `F` is placed at `(current_col, ascent − F.ascent())`.
    pub fn into_frame(self) -> TermFrame {
        let ascent = self.ascent();
        let descent = self.descent();
        let total_rows = (ascent + descent).max(Row::ZERO);
        let total_cols = self.total_width().max(Col::ZERO);

        debug!("TermMathRun::into_frame: n_frags={} ascent={:?} descent={:?} total_cols={:?} total_rows={:?}",
            self.0.len(), ascent, descent, total_cols, total_rows);
        for (i, frag) in self.0.iter().enumerate() {
            match frag {
                TermMathFragment::Frame(ff) => {
                    debug!("  frag[{}]: Frame cols={:?} rows={:?} ascent={:?} class={:?}",
                        i, ff.frame.cols(), ff.frame.rows(), ff.ascent(), ff.class);
                }
                _ => debug!("  frag[{}]: {:?}", i, std::mem::discriminant(frag)),
            }
        }

        let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));
        frame.set_baseline(ascent);

        let mut x: Col = Col::ZERO;
        for frag in self.0 {
            let w = frag.width();
            match frag {
                TermMathFragment::Frame(ff) => {
                    let y = ascent - ff.ascent();
                    frame.push_frame(TermPoint::new(x, y), ff.frame);
                }
                TermMathFragment::Spacing(..) => {}
                TermMathFragment::Space | TermMathFragment::Align | TermMathFragment::Linebreak => {}
            }
            x = x + w;
        }

        frame
    }

    /// If the run has exactly one fragment, return it unchanged.  Otherwise
    /// compose everything with [`into_frame`] and wrap in a
    /// [`TermMathFrameFragment`].
    pub fn into_fragment(self) -> TermMathFragment {
        if self.0.len() == 1 {
            return self.0.into_iter().next().unwrap();
        }

        let text_like = self
            .0
            .iter()
            .filter(|f| matches!(f, TermMathFragment::Frame(_)))
            .all(|f| f.is_text_like());

        TermMathFrameFragment::new(self.into_frame())
            .with_text_like(text_like)
            .into()
    }

    /// Compose rows vertically with a 1-row gap between them.
    ///
    /// Used for multi-line (aligned) equations.
    pub fn multiline_frame(self) -> TermFrame {
        let rows = self.rows();
        let row_count = rows.len();
        if row_count == 0 {
            return TermFrame::new(TermSize::ZERO);
        }

        // Compute each row's frame and dimensions.
        let row_frames: Vec<TermFrame> = rows.into_iter().map(|r| r.into_frame()).collect();

        let total_cols: Col = row_frames
            .iter()
            .map(|f| f.cols())
            .max()
            .unwrap_or(Col::ZERO);
        let row_gap: Row = Row::new(1);
        let total_rows: Row = row_frames
            .iter()
            .map(|f| f.rows().max(Row::new(1)))
            .sum::<Row>()
            + row_gap * (row_count.saturating_sub(1) as f64);

        let mut out = TermFrame::new(TermSize::new(total_cols, total_rows));
        out.set_baseline(Row::new(total_rows.get() / 2));

        let mut y: Row = Row::ZERO;
        for (i, row_frame) in row_frames.into_iter().enumerate() {
            let row_h = row_frame.rows().max(Row::new(1));
            out.push_frame(TermPoint::new(Col::ZERO, y), row_frame);
            y = y + row_h;
            if i + 1 < row_count {
                y = y + row_gap;
            }
        }

        out
    }
}

impl From<TermMathFragment> for TermMathRun {
    fn from(frag: TermMathFragment) -> Self {
        Self(vec![frag])
    }
}

// ── Spacing rules ─────────────────────────────────────────────────────────────

/// Return the automatic inter-fragment spacing in terminal columns, or `None`
/// for no spacing.
///
/// Implements the TeXbook-based math spacing table, simplified for terminal
/// rendering.  Mirrors `typst-layout`'s `spacing()` function.
///
/// `pending_space` is `true` when a `SpaceElem`-derived `Space` fragment was
/// seen between `l` and `r`.  It is only used as a fallback for the
/// `is_spaced()` rule; all class-based rules ignore it.
fn auto_spacing(l: &TermMathFragment, pending_space: bool, r: &TermMathFragment) -> Option<Col> {
    use MathClass::*;
    match (l.class(), r.class()) {
        // No spacing before punctuation.
        (_, Punctuation) => None,
        // Thin spacing after punctuation (comma in argument lists, etc.).
        (Punctuation, _) => Some(Col::new(1)),

        // No spacing after opening or before closing delimiters.
        (Opening, _)
        | (_, Closing)
        | (Fence, _)
        | (_, Fence) => None,

        // No spacing between two consecutive relations.
        (Relation, Relation) => None,
        // Thick spacing around relations.
        (Relation, _) | (_, Relation) => Some(Col::new(1)),

        // Medium spacing around binary operators.
        (Binary, _) | (_, Binary) => Some(Col::new(1)),

        // No thin spacing between a large operator and an opening delimiter.
        (Large, Opening) => None,
        // Thin spacing around large operators.
        (Large, _) | (_, Large) => Some(Col::new(1)),

        // Soft-space from SpaceElem: only materialise when at least one of the
        // adjacent fragments is "spaced" (multi-letter text operators, inline
        // boxes).  This matches the upstream `_ if (l.is_spaced() || r.is_spaced()) => space`
        // rule in `MathRun::new`.
        _ if pending_space && (l.is_spaced() || r.is_spaced()) => Some(Col::new(1)),

        _ => None,
    }
}

// ── Build a stretched delimiter frame ────────────────────────────────────────

/// Build a 1-column-wide [`TermFrame`] from a vertical list of characters.
///
/// Used by the left–right and matrix modules to render stretched delimiters.
pub fn build_delimiter_frame(chars: Vec<char>, style: ContentStyle) -> TermFrame {
    let height = chars.len() as i32;
    if height == 0 {
        return TermFrame::new(TermSize::ZERO);
    }
    let width: Col = chars
        .iter()
        .map(|c| char_cols(*c))
        .max()
        .unwrap_or(Col::new(1));
    let mut frame = TermFrame::new(TermSize::new(width.max(Col::new(1)), Row::new(height)));
    frame.set_baseline(Row::new(height / 2)); // vertically centred for horizontal composition
    for (i, ch) in chars.into_iter().enumerate() {
        frame.push_text(TermPoint::new(Col::ZERO, Row::new(i as i32)), EcoString::from(ch), style);
    }
    frame
}

// ── Horizontal composition ────────────────────────────────────────────────────

/// Compose frames horizontally, aligning on their baselines.
///
/// All frames are placed so their baseline rows line up.
/// The result frame has:
/// - `width`    = sum of all frame widths + `gap` between each pair
/// - `height`   = `max_ascent + max_descent`
/// - `baseline` = `max_ascent`
///
/// A `gap` of 0 means frames are placed immediately adjacent.
pub fn compose_horizontal(frames: Vec<TermFrame>, gap: Col) -> TermFrame {
    if frames.is_empty() {
        return TermFrame::new(TermSize::ZERO);
    }

    let n = frames.len();

    let max_ascent: Row = frames.iter().map(|f| f.ascent()).max().unwrap_or(Row::ZERO);
    let max_descent: Row = frames.iter().map(|f| f.descent()).max().unwrap_or(Row::new(1));
    let total_rows = (max_ascent + max_descent).max(Row::new(1));

    let content_width: Col = frames.iter().map(|f| f.cols()).sum();
    let gap_width: Col = gap * (n.saturating_sub(1) as f64);
    let total_cols = (content_width + gap_width).max(Col::ZERO);

    debug!("compose_horizontal: n={} max_ascent={:?} max_descent={:?} total_cols={:?} total_rows={:?}",
        n, max_ascent, max_descent, total_cols, total_rows);
    for (i, f) in frames.iter().enumerate() {
        debug!("  frame[{}]: cols={:?} rows={:?} ascent={:?} baseline={:?}",
            i, f.cols(), f.rows(), f.ascent(), f.baseline());
    }

    let mut out = TermFrame::new(TermSize::new(total_cols, total_rows));
    out.set_baseline(max_ascent);

    let mut x: Col = Col::ZERO;
    for (i, frame) in frames.into_iter().enumerate() {
        let w = frame.cols();
        let row = max_ascent - frame.ascent();
        out.push_frame(TermPoint::new(x, row), frame);
        x = x + w;
        if i + 1 < n {
            x = x + gap;
        }
    }

    out
}

// ── Vertical composition ──────────────────────────────────────────────────────

/// Compose frames vertically, centered horizontally.
///
/// Frames are stacked top-to-bottom with `gap` rows between each pair.
/// The result frame's baseline is determined by `baseline_idx`: it is placed
/// at the start of the frame at that index plus that frame's own baseline.
///
/// If `baseline_idx` is out of range it is clamped to the last frame.
pub fn compose_vertical(frames: Vec<TermFrame>, gap: Row, baseline_idx: usize) -> TermFrame {
    if frames.is_empty() {
        return TermFrame::new(TermSize::ZERO);
    }

    let n = frames.len();
    let max_cols: Col = frames.iter().map(|f| f.cols()).max().unwrap_or(Col::ZERO);

    // Pre-compute row heights (minimum 1 so zero-height frames still advance).
    let heights: Vec<Row> = frames.iter().map(|f| f.rows().max(Row::new(1))).collect();

    let gap_rows: Row = gap * (n.saturating_sub(1) as f64);
    let total_rows: Row = heights.iter().copied().sum::<Row>() + gap_rows;

    // Baseline: top of the baseline_idx frame + that frame's baseline.
    let bi = baseline_idx.min(n.saturating_sub(1));
    let bi_frame_baseline = frames[bi].baseline();
    let mut baseline_row: Row = Row::ZERO;
    for i in 0..bi {
        baseline_row = baseline_row + heights[i] + gap;
    }
    baseline_row = baseline_row + bi_frame_baseline;

    let mut out = TermFrame::new(TermSize::new(max_cols.max(Col::ZERO), total_rows.max(Row::ZERO)));
    out.set_baseline(baseline_row.max(Row::ZERO));

    let mut y: Row = Row::ZERO;
    for (i, frame) in frames.into_iter().enumerate() {
        let cx: Col = Col::ZERO; // left-aligned
        out.push_frame(TermPoint::new(cx, y), frame);
        y = y + heights[i];
        if i + 1 < n {
            y = y + gap;
        }
    }

    out
}

// ── Pad helper ────────────────────────────────────────────────────────────────

/// Add horizontal padding to a frame.
pub fn pad_h(frame: TermFrame, left: Col, right: Col) -> TermFrame {
    let new_cols = frame.cols() + left + right;
    let new_rows = frame.rows();
    let new_baseline = frame.baseline();

    let mut out = TermFrame::new(TermSize::new(new_cols.max(Col::ZERO), new_rows.max(Row::ZERO)));
    out.set_baseline(new_baseline.max(Row::ZERO));
    out.push_frame(TermPoint::new(left, Row::ZERO), frame);
    out
}
