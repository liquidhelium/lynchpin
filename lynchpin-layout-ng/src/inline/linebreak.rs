use std::ops::{Add, Sub};
use std::sync::LazyLock;

use az::SaturatingAs;
use icu_properties::LineBreak;
use icu_properties::maps::{CodePointMapData, CodePointMapDataBorrowed};
use icu_provider::AsDeserializingBufferProvider;
use icu_provider_adapters::fork::ForkByKeyProvider;
use icu_provider_blob::BlobDataProvider;
use icu_segmenter::LineSegmenter;
use typst_library::engine::Engine;
use typst_library::layout::{Abs, Em};
use typst_library::model::Linebreaks;
use typst_library::text::{Lang, TextElem};
use typst_syntax::link_prefix;
use unicode_segmentation::UnicodeSegmentation;

use super::*;

/// The cost of a line or inline layout.
type Cost = f64;

// Cost parameters.
const DEFAULT_HYPH_COST: Cost = 135.0;
const DEFAULT_RUNT_COST: Cost = 100.0;

// Other parameters.
const MIN_RATIO: f64 = -1.0;
const MIN_APPROX_RATIO: f64 = -0.5;
const BOUND_EPS: f64 = 1e-3;

/// The ICU blob data.
fn blob() -> BlobDataProvider {
    BlobDataProvider::try_new_from_static_blob(typst_assets::icu::ICU).unwrap()
}

/// The general line break segmenter.
static SEGMENTER: LazyLock<LineSegmenter> =
    LazyLock::new(|| LineSegmenter::try_new_lstm_with_buffer_provider(&blob()).unwrap());

/// The line break segmenter for Chinese/Japanese text.
static CJ_SEGMENTER: LazyLock<LineSegmenter> = LazyLock::new(|| {
    let cj_blob =
        BlobDataProvider::try_new_from_static_blob(typst_assets::icu::ICU_CJ_SEGMENT).unwrap();
    let cj_provider = ForkByKeyProvider::new(cj_blob, blob());
    LineSegmenter::try_new_lstm_with_buffer_provider(&cj_provider).unwrap()
});

/// The Unicode line break properties for each code point.
static LINEBREAK_DATA: LazyLock<CodePointMapData<LineBreak>> =
    LazyLock::new(|| icu_properties::maps::load_line_break(&blob().as_deserializing()).unwrap());

// Zero width space.
const ZWS: char = '\u{200B}';

/// A line break opportunity.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Breakpoint {
    /// Just a normal opportunity (e.g. after a space).
    Normal,
    /// A mandatory breakpoint (after '\n' or at the end of the text).
    Mandatory,
    /// An opportunity for hyphenating and how many chars are before/after it
    /// in the word.
    Hyphen(u8, u8),
}

impl Breakpoint {
    /// Trim a line before this breakpoint.
    pub fn trim(self, start: usize, line: &str) -> Trim {
        match self {
            Self::Normal => {
                let trimmed = line.trim_end_matches(|c: char| c.is_whitespace() || c == ZWS);
                Trim {
                    layout: start + trimmed.len(),
                    shaping: start + line.len(),
                }
            }
            Self::Mandatory => {
                let lb = LINEBREAK_DATA.as_borrowed();
                let trimmed = line.trim_end_matches(|c| {
                    matches!(
                        lb.get(c),
                        LineBreak::MandatoryBreak
                            | LineBreak::CarriageReturn
                            | LineBreak::LineFeed
                            | LineBreak::NextLine
                    )
                });
                Trim::uniform(start + trimmed.len())
            }
            Self::Hyphen(..) => Trim::uniform(start + line.len()),
        }
    }

    /// Whether this is a hyphen breakpoint.
    pub fn is_hyphen(self) -> bool {
        matches!(self, Self::Hyphen(..))
    }
}

/// How to trim the end of a line.
///
/// It's an invariant that `self.layout <= self.shaping`.
pub struct Trim {
    /// The text in the range `layout..shaping` should be shaped but should not
    /// affect layout.
    pub layout: usize,
    /// The text should only be shaped up until the given text offset.
    pub shaping: usize,
}

impl Trim {
    /// Create an instance with equal layout and shaping trim.
    fn uniform(trim: usize) -> Self {
        Self {
            layout: trim,
            shaping: trim,
        }
    }
}

/// Breaks the text into lines.
pub fn linebreak<'a>(engine: &Engine, p: &'a Preparation<'a>, width: Abs) -> Vec<Line<'a>> {
    match p.config.linebreaks {
        Linebreaks::Simple => linebreak_simple(engine, p, width),
        Linebreaks::Optimized => linebreak_optimized(engine, p, width),
    }
}

/// Performs line breaking in simple first-fit style.
#[typst_macros::time]
fn linebreak_simple<'a>(engine: &Engine, p: &'a Preparation<'a>, width: Abs) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(16);
    let mut start = 0;
    let mut last = None;

    breakpoints(p, |end, breakpoint| {
        let mut attempt = line(engine, p, start..end, breakpoint, lines.last());

        if !width.fits(attempt.width)
            && let Some((last_attempt, last_end)) = last.take()
        {
            lines.push(last_attempt);
            start = last_end;
            attempt = line(engine, p, start..end, breakpoint, lines.last());
        }

        if breakpoint == Breakpoint::Mandatory || !width.fits(attempt.width) {
            lines.push(attempt);
            start = end;
            last = None;
        } else {
            last = Some((attempt, end));
        }
    });

    if let Some((line, _)) = last {
        lines.push(line);
    }

    lines
}

/// Performs line breaking in optimized Knuth-Plass style.
#[typst_macros::time]
fn linebreak_optimized<'a>(engine: &Engine, p: &'a Preparation<'a>, width: Abs) -> Vec<Line<'a>> {
    let metrics = CostMetrics::compute(p);

    let upper_bound = linebreak_optimized_approximate(engine, p, width, &metrics);

    linebreak_optimized_bounded(engine, p, width, &metrics, upper_bound)
}

/// Performs line breaking in optimized Knuth-Plass style, but with an upper
/// bound on the cost.
#[typst_macros::time]
fn linebreak_optimized_bounded<'a>(
    engine: &Engine,
    p: &'a Preparation<'a>,
    width: Abs,
    metrics: &CostMetrics,
    upper_bound: Cost,
) -> Vec<Line<'a>> {
    /// An entry in the dynamic programming table.
    struct Entry<'a> {
        pred: usize,
        total: Cost,
        line: Line<'a>,
        end: usize,
    }

    let mut table = vec![Entry {
        pred: 0,
        total: 0.0,
        line: Line::empty(),
        end: 0,
    }];

    let mut active = 0;
    let mut prev_end = 0;

    breakpoints(p, |end, breakpoint| {
        let mut best: Option<Entry> = None;
        let mut line_lower_bound = None;

        for (pred_index, pred) in table.iter().enumerate().skip(active) {
            let start = pred.end;
            let unbreakable = prev_end == start;

            if line_lower_bound.is_some_and(|lower| pred.total + lower > upper_bound + BOUND_EPS) {
                continue;
            }

            let attempt = line(engine, p, start..end, breakpoint, Some(&pred.line));

            let (line_ratio, line_cost) = ratio_and_cost(
                p,
                metrics,
                width,
                &pred.line,
                &attempt,
                breakpoint,
                unbreakable,
            );

            if line_ratio < metrics.min_ratio && active == pred_index {
                active += 1;
            }

            let total = pred.total + line_cost;

            if line_ratio > 0.0 && line_lower_bound.is_none() && !attempt.has_negative_width_items()
            {
                line_lower_bound = Some(line_cost);
            }

            if total > upper_bound + BOUND_EPS {
                continue;
            }

            if best.as_ref().is_none_or(|best| best.total >= total) {
                best = Some(Entry {
                    pred: pred_index,
                    total,
                    line: attempt,
                    end,
                });
            }
        }

        if breakpoint == Breakpoint::Mandatory {
            active = table.len();
        }

        table.extend(best);
        prev_end = end;
    });

    let mut lines = Vec::with_capacity(16);
    let mut idx = table.len() - 1;

    if table[idx].end != p.text.len() {
        #[cfg(debug_assertions)]
        panic!("bounded inline layout is incomplete");

        #[cfg(not(debug_assertions))]
        return linebreak_optimized_bounded(engine, p, width, metrics, f64::INFINITY);
    }

    while idx != 0 {
        table.truncate(idx + 1);
        let entry = table.pop().unwrap();
        lines.push(entry.line);
        idx = entry.pred;
    }

    lines.reverse();
    lines
}

/// Runs the normal Knuth-Plass algorithm, but instead of building proper lines
/// (which is costly) to determine costs, it determines approximate costs using
/// cumulative arrays.
#[typst_macros::time]
fn linebreak_optimized_approximate(
    engine: &Engine,
    p: &Preparation,
    width: Abs,
    metrics: &CostMetrics,
) -> Cost {
    let estimates = Estimates::compute(p);

    /// An entry in the dynamic programming table.
    struct Entry {
        pred: usize,
        total: Cost,
        end: usize,
        unbreakable: bool,
        breakpoint: Breakpoint,
    }

    let mut table = vec![Entry {
        pred: 0,
        total: 0.0,
        end: 0,
        unbreakable: false,
        breakpoint: Breakpoint::Mandatory,
    }];

    let mut active = 0;
    let mut prev_end = 0;

    breakpoints(p, |end, breakpoint| {
        let mut best: Option<Entry> = None;
        for (pred_index, pred) in table.iter().enumerate().skip(active) {
            let start = pred.end;
            let unbreakable = prev_end == start;

            let justify = p.config.justify && breakpoint != Breakpoint::Mandatory;

            let consecutive_dash = pred.breakpoint.is_hyphen() && breakpoint.is_hyphen();

            let trimmed_end = start + p.text[start..end].trim_end().len();
            let line_ratio = raw_ratio(
                p,
                width,
                estimates.widths.estimate(start..trimmed_end)
                    + if breakpoint.is_hyphen() {
                        metrics.approx_hyphen_width
                    } else {
                        Abs::zero()
                    },
                estimates.stretchability.estimate(start..trimmed_end),
                estimates.shrinkability.estimate(start..trimmed_end),
                estimates.justifiables.estimate(start..trimmed_end),
            );

            let line_cost = raw_cost(
                metrics,
                breakpoint,
                line_ratio,
                justify,
                unbreakable,
                consecutive_dash,
                true,
            );

            if line_ratio < metrics.min_ratio && active == pred_index {
                active += 1;
            }

            let total = pred.total + line_cost;

            if best.as_ref().is_none_or(|best| best.total >= total) {
                best = Some(Entry {
                    pred: pred_index,
                    total,
                    end,
                    unbreakable,
                    breakpoint,
                });
            }
        }

        if breakpoint == Breakpoint::Mandatory {
            active = table.len();
        }

        table.extend(best);
        prev_end = end;
    });

    let mut indices = Vec::with_capacity(16);
    let mut idx = table.len() - 1;
    while idx != 0 {
        indices.push(idx);
        idx = table[idx].pred;
    }

    let mut pred = Line::empty();
    let mut start = 0;
    let mut exact = 0.0;

    for idx in indices.into_iter().rev() {
        let Entry {
            end,
            breakpoint,
            unbreakable,
            ..
        } = table[idx];

        let attempt = line(engine, p, start..end, breakpoint, Some(&pred));
        let (ratio, line_cost) =
            ratio_and_cost(p, metrics, width, &pred, &attempt, breakpoint, unbreakable);

        if ratio < metrics.min_ratio {
            return f64::INFINITY;
        }

        pred = attempt;
        start = end;
        exact += line_cost;
    }

    exact
}

/// Compute the stretch ratio and cost of a line.
#[allow(clippy::too_many_arguments)]
fn ratio_and_cost(
    p: &Preparation,
    metrics: &CostMetrics,
    available_width: Abs,
    pred: &Line,
    attempt: &Line,
    breakpoint: Breakpoint,
    unbreakable: bool,
) -> (f64, Cost) {
    let ratio = raw_ratio(
        p,
        available_width,
        attempt.width,
        attempt.stretchability(),
        attempt.shrinkability(),
        attempt.justifiables(),
    );

    let cost = raw_cost(
        metrics,
        breakpoint,
        ratio,
        attempt.justify,
        unbreakable,
        pred.dash.is_some() && attempt.dash.is_some(),
        false,
    );

    (ratio, cost)
}

/// Determine the stretch ratio for a line given raw metrics.
fn raw_ratio(
    p: &Preparation,
    available_width: Abs,
    line_width: Abs,
    stretchability: Abs,
    shrinkability: Abs,
    justifiables: usize,
) -> f64 {
    let mut delta = available_width - line_width;

    if delta.approx_eq(Abs::zero()) {
        delta = Abs::zero();
    }

    let adjustability = if delta >= Abs::zero() {
        stretchability
    } else {
        shrinkability
    };

    let mut ratio = delta / adjustability.max(Abs::zero());

    if ratio.is_nan() {
        ratio = 0.0;
    }

    if ratio > 1.0 {
        let extra_stretch = (delta - adjustability) / justifiables.max(1) as f64;
        ratio = 1.0 + extra_stretch / (p.config.font_size / 2.0);
    }

    ratio.clamp(MIN_RATIO - 1.0, 10.0)
}

/// Compute the cost of a line given raw metrics.
fn raw_cost(
    metrics: &CostMetrics,
    breakpoint: Breakpoint,
    ratio: f64,
    justify: bool,
    unbreakable: bool,
    consecutive_dash: bool,
    approx: bool,
) -> Cost {
    let badness = if ratio < metrics.min_ratio(approx) {
        1_000_000.0
    } else if breakpoint != Breakpoint::Mandatory || justify || ratio < 0.0 {
        100.0 * ratio.abs().powi(3)
    } else {
        0.0
    };

    let mut penalty = 0.0;

    if unbreakable && breakpoint == Breakpoint::Mandatory {
        penalty += metrics.runt_cost;
    }

    if let Breakpoint::Hyphen(l, r) = breakpoint {
        const LIMIT: u8 = 5;
        let steps = LIMIT.saturating_sub(l) + LIMIT.saturating_sub(r);
        let extra = 0.15 * steps as f64;
        penalty += (1.0 + extra) * metrics.hyph_cost;
    }

    if consecutive_dash {
        penalty += metrics.hyph_cost;
    }

    (1.0 + badness + penalty).powi(2)
}

/// Calls `f` for all possible points in the text where lines can broken.
fn breakpoints(p: &Preparation, mut f: impl FnMut(usize, Breakpoint)) {
    let text = p.text;

    if text.is_empty() {
        f(0, Breakpoint::Mandatory);
        return;
    }

    let hyphenate = p.config.hyphenate != Some(false);
    let lb = LINEBREAK_DATA.as_borrowed();
    let segmenter = match p.config.lang {
        Some(Lang::CHINESE | Lang::JAPANESE) => &CJ_SEGMENTER,
        _ => &SEGMENTER,
    };

    let mut last = 0;
    let mut iter = segmenter.segment_str(text).peekable();

    loop {
        let (head, tail) = text.split_at(last);
        if head.ends_with("://") || tail.starts_with("www.") {
            let (link, _) = link_prefix(tail);
            linebreak_link(link, |i| f(last + i, Breakpoint::Normal));
            last += link.len();
            while iter.peek().is_some_and(|&p| p < last) {
                iter.next();
            }
        }

        let Some(point) = iter.next() else { break };

        let Some(c) = text[..point].chars().next_back() else {
            continue;
        };

        let breakpoint = if point == text.len() {
            Breakpoint::Mandatory
        } else {
            const OBJ_REPLACE: char = '\u{FFFC}';
            match lb.get(c) {
                LineBreak::MandatoryBreak
                | LineBreak::CarriageReturn
                | LineBreak::LineFeed
                | LineBreak::NextLine => Breakpoint::Mandatory,

                LineBreak::CombiningMark
                    if text[point..].starts_with(OBJ_REPLACE) && last + c.len_utf8() == point =>
                {
                    continue;
                }

                _ => Breakpoint::Normal,
            }
        };

        if hyphenate && last < point {
            for segment in text[last..point].split_word_bounds() {
                if !segment.is_empty() && segment.chars().all(char::is_alphabetic) {
                    hyphenations(p, &lb, last, segment, &mut f);
                }
                last += segment.len();
            }
        }

        f(point, breakpoint);
        last = point;
    }
}

/// Generate breakpoints for hyphenations within a word.
fn hyphenations(
    p: &Preparation,
    lb: &CodePointMapDataBorrowed<LineBreak>,
    mut offset: usize,
    word: &str,
    mut f: impl FnMut(usize, Breakpoint),
) {
    let Some(lang) = lang_at(p, offset) else {
        return;
    };
    let count = word.chars().count();
    let end = offset + word.len();

    let mut chars = 0;
    for syllable in hypher::hyphenate(word, lang) {
        offset += syllable.len();
        chars += syllable.chars().count();

        if offset == end {
            continue;
        }

        if !hyphenate_at(p, offset) {
            continue;
        }

        if matches!(
            syllable.chars().next_back().map(|c| lb.get(c)),
            Some(LineBreak::Glue | LineBreak::WordJoiner | LineBreak::ZWJ)
        ) {
            continue;
        }

        let l = chars.saturating_as::<u8>();
        let r = (count - chars).saturating_as::<u8>();

        f(offset, Breakpoint::Hyphen(l, r));
    }
}

/// Produce linebreak opportunities for a link.
fn linebreak_link(link: &str, mut f: impl FnMut(usize)) {
    #[derive(PartialEq)]
    enum Class {
        Alphabetic,
        Digit,
        Open,
        Other,
    }

    impl Class {
        fn of(c: char) -> Self {
            if c.is_alphabetic() {
                Class::Alphabetic
            } else if c.is_numeric() {
                Class::Digit
            } else if matches!(c, '(' | '[') {
                Class::Open
            } else {
                Class::Other
            }
        }
    }

    let mut offset = 0;
    let mut prev = Class::Other;

    for (end, c) in link.char_indices() {
        let class = Class::of(c);

        if end > 0
            && prev != Class::Open
            && if class == Class::Other {
                prev == Class::Other
            } else {
                class != prev
            }
        {
            let piece = &link[offset..end];
            if piece.len() < 16 {
                offset = end;
                f(offset);
            } else {
                for c in piece.chars() {
                    offset += c.len_utf8();
                    f(offset);
                }
            }
        }

        prev = class;
    }
}

/// Whether hyphenation is enabled at the given offset.
fn hyphenate_at(p: &Preparation, offset: usize) -> bool {
    p.config.hyphenate.unwrap_or_else(|| {
        let (_, item) = p.get(offset);
        match item.text() {
            Some(text) => text
                .styles
                .get(TextElem::hyphenate)
                .unwrap_or(p.config.justify),
            None => false,
        }
    })
}

/// The text language at the given offset.
fn lang_at(p: &Preparation, offset: usize) -> Option<hypher::Lang> {
    let lang = p.config.lang.or_else(|| {
        let (_, item) = p.get(offset);
        let styles = item.text()?.styles;
        Some(styles.get(TextElem::lang))
    })?;

    let bytes = lang.as_str().as_bytes().try_into().ok()?;
    hypher::Lang::from_iso(bytes)
}

/// Resolved metrics relevant for cost computation.
struct CostMetrics {
    min_ratio: f64,
    min_approx_ratio: f64,
    approx_hyphen_width: Abs,
    hyph_cost: Cost,
    runt_cost: Cost,
}

impl CostMetrics {
    /// Compute shared metrics for inline layout optimization.
    fn compute(p: &Preparation) -> Self {
        Self {
            min_ratio: if p.config.justify { MIN_RATIO } else { 0.0 },
            min_approx_ratio: if p.config.justify {
                MIN_APPROX_RATIO
            } else {
                0.0
            },
            approx_hyphen_width: Em::new(0.33).at(p.config.font_size),
            hyph_cost: DEFAULT_HYPH_COST * p.config.costs.hyphenation().get(),
            runt_cost: DEFAULT_RUNT_COST * p.config.costs.runt().get(),
        }
    }

    /// The minimum line ratio we allow for shrinking.
    fn min_ratio(&self, approx: bool) -> f64 {
        if approx {
            self.min_approx_ratio
        } else {
            self.min_ratio
        }
    }
}

/// Estimated line metrics.
struct Estimates {
    widths: CumulativeVec<Abs>,
    stretchability: CumulativeVec<Abs>,
    shrinkability: CumulativeVec<Abs>,
    justifiables: CumulativeVec<usize>,
}

impl Estimates {
    /// Compute estimations for approximate Knuth-Plass layout.
    fn compute(p: &Preparation) -> Self {
        let cap = p.text.len();

        let mut widths = CumulativeVec::with_capacity(cap);
        let mut stretchability = CumulativeVec::with_capacity(cap);
        let mut shrinkability = CumulativeVec::with_capacity(cap);
        let mut justifiables = CumulativeVec::with_capacity(cap);

        for (range, item) in p.items.iter() {
            if let Item::Text(shaped) = item {
                for g in shaped.glyphs.iter() {
                    let byte_len = g.range.len();
                    let stretch = g.stretchability().0 + g.stretchability().1;
                    let shrink = g.shrinkability().0 + g.shrinkability().1;
                    widths.push(byte_len, g.x_advance.at(g.size));
                    stretchability.push(byte_len, stretch.at(g.size));
                    shrinkability.push(byte_len, shrink.at(g.size));
                    justifiables.push(byte_len, g.is_justifiable() as usize);
                }
            } else {
                widths.push(range.len(), item.natural_width());
            }

            widths.adjust(range.end);
            stretchability.adjust(range.end);
            shrinkability.adjust(range.end);
            justifiables.adjust(range.end);
        }

        Self {
            widths,
            stretchability,
            shrinkability,
            justifiables,
        }
    }
}

/// An accumulative array of a metric.
struct CumulativeVec<T> {
    total: T,
    summed: Vec<T>,
}

impl<T> CumulativeVec<T>
where
    T: Default + Copy + Add<Output = T> + Sub<Output = T>,
{
    /// Create a new instance with the given capacity.
    fn with_capacity(capacity: usize) -> Self {
        let total = T::default();
        let mut summed = Vec::with_capacity(capacity);
        summed.push(total);
        Self { total, summed }
    }

    /// Adjust to cover the given byte length.
    fn adjust(&mut self, len: usize) {
        self.summed.resize(len, self.total);
    }

    /// Adds a new segment with the given byte length and metric.
    fn push(&mut self, byte_len: usize, metric: T) {
        self.total = self.total + metric;
        for _ in 0..byte_len {
            self.summed.push(self.total);
        }
    }

    /// Estimates the metrics for the line spanned by the range.
    #[track_caller]
    fn estimate(&self, range: Range) -> T {
        self.get(range.end) - self.get(range.start)
    }

    /// Get the metric at the given byte position.
    #[track_caller]
    fn get(&self, index: usize) -> T {
        match index.checked_sub(1) {
            None => T::default(),
            Some(i) => self.summed[i],
        }
    }
}
