//! `TermMathRun` — a linear sequence of math fragments with composition logic.
//!
//! Analogous to `typst-layout`'s `MathRun`.

use unicode_math_class::MathClass;

use crate::frame::{Col, Row, TermFrame, TermPoint, TermSize};

use super::fragment::{TermMathFragment, TermMathFrameFragment};

// ── TermMathRun ───────────────────────────────────────────────────────────────

/// A linear collection of [`TermMathFragment`]s with spacing rules applied.
#[derive(Debug, Default, Clone)]
pub struct TermMathRun(pub Vec<TermMathFragment>);

impl TermMathRun {
    /// Build a run from raw fragments, inserting automatic inter-fragment
    /// spacing and collapsing weak spacings.
    pub fn new(frags: Vec<TermMathFragment>) -> Self {
        let mut resolved: Vec<TermMathFragment> = Vec::with_capacity(frags.len());
        // Index of the last non-ignorant Frame fragment in `resolved`.
        let mut last: Option<usize> = None;

        for frag in frags {
            match &frag {
                // ── Explicit spacing ─────────────────────────────────────────
                TermMathFragment::Spacing(cols, weak) => {
                    let cols = *cols;
                    let weak = *weak;
                    // Explicit spacing resets auto-spacing context.
                    last = None;
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
                    resolved.push(frag);
                    continue;
                }

                // ── Rendered content ─────────────────────────────────────────
                TermMathFragment::Frame(_) => {}
            }

            // Insert automatic spacing between the previous non-ignorant
            // fragment and this one, unless the previous fragment was reset
            // (e.g. after an explicit Spacing or Linebreak).
            if !frag.is_ignorant() {
                if let Some(i) = last {
                    if let Some(gap) = auto_spacing(resolved[i].class(), frag.class()) {
                        // Insert the auto-spacing right after `last`.
                        resolved.insert(i + 1, TermMathFragment::Spacing(gap, false));
                    }
                }
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
            .unwrap_or(0)
    }

    /// Maximum descent of all height-contributing fragments.
    pub fn descent(&self) -> Row {
        self.0
            .iter()
            .filter(|f| !matches!(f, TermMathFragment::Align | TermMathFragment::Linebreak))
            .map(|f| f.descent())
            .max()
            .unwrap_or(0)
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
        let mut count = 1 + self
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
        let total_rows = (ascent + descent).max(0);
        let total_cols = self.total_width().max(0);

        let mut frame = TermFrame::new(TermSize::new(total_cols, total_rows));
        frame.set_baseline(ascent);

        let mut x: Col = 0;
        for frag in self.0 {
            let w = frag.width();
            match frag {
                TermMathFragment::Frame(ff) => {
                    let y = ascent - ff.ascent();
                    frame.push_frame(TermPoint::new(x, y), ff.frame);
                }
                TermMathFragment::Spacing(_, _) => {}
                TermMathFragment::Align | TermMathFragment::Linebreak => {}
            }
            x += w;
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

        let total_cols: Col = row_frames.iter().map(|f| f.cols()).max().unwrap_or(0);
        let row_gap: Row = 1;
        let total_rows: Row = row_frames.iter().map(|f| f.rows().max(1)).sum::<Row>()
            + row_gap * (row_count.saturating_sub(1)) as Row;

        let mut out = TermFrame::new(TermSize::new(total_cols, total_rows));
        out.set_baseline(total_rows / 2);

        let mut y: Row = 0;
        for (i, row_frame) in row_frames.into_iter().enumerate() {
            let row_h = row_frame.rows().max(1);
            out.push_frame(TermPoint::new(0, y), row_frame);
            y += row_h;
            if i + 1 < row_count {
                y += row_gap;
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
/// Simplified from the typst TeXbook-based spacing table.
fn auto_spacing(l: MathClass, r: MathClass) -> Option<Col> {
    use MathClass::*;

    match (l, r) {
        // No spacing adjacent to punctuation.
        (_, Punctuation) | (Punctuation, _) => None,

        // No spacing after opening or before closing delimiters.
        (Opening, _) | (_, Closing) | (Fence, _) | (_, Fence) => None,

        // No spacing between two consecutive relations.
        (Relation, Relation) => None,
        // Thick spacing around relations.
        (Relation, _) | (_, Relation) => Some(1),

        // Medium spacing around binary operators.
        (Binary, _) | (_, Binary) => Some(1),

        // No spacing around large operators before opening delimiters (handled
        // by Opening rule above).
        (Large, _) | (_, Large) => Some(1),

        _ => None,
    }
}

// ── Build a stretched delimiter frame ────────────────────────────────────────

/// Build a 1-column-wide [`TermFrame`] from a vertical list of characters.
///
/// Used by `lr.rs` and `mat.rs` to render stretched delimiters.
pub fn build_delimiter_frame(chars: Vec<char>, style: crossterm::style::ContentStyle) -> TermFrame {
    let height = chars.len() as Row;
    if height == 0 {
        return TermFrame::new(TermSize::ZERO);
    }
    let width: Col = chars
        .iter()
        .map(|c| crate::frame::char_cols(*c) as Col)
        .max()
        .unwrap_or(1);
    let mut frame = TermFrame::new(TermSize::new(width.max(1), height));
    frame.set_baseline(height / 2); // vertically centred for horizontal composition
    for (i, ch) in chars.into_iter().enumerate() {
        frame.push_text(TermPoint::new(0, i as Row), ch.to_string(), style);
    }
    frame
}
