use std::fmt::{self, Debug, Formatter};
use std::ops::{Deref, DerefMut};

use typst_library::engine::Engine;
use typst_library::foundations::Resolve;
use typst_library::introspection::{SplitLocator, TagFlags};
use typst_library::layout::{Abs, Dir, Em, Fr};
use typst_library::model::ParLineMarker;
use typst_library::text::{Lang, TextElem, variant};
use typst_utils::Numeric;

use lynchpin_library_ng::*;

use super::*;
use crate::inline::linebreak::Trim;
use crate::inline::shaping::Adjustability;
// NOTE: layout_and_modify is expected from crate::modifiers (Agent F)

const SHY: char = '\u{ad}';
const HYPHEN: char = '-';
const EN_DASH: char = '\u{2013}';
const EM_DASH: char = '\u{2014}';
const LINE_SEPARATOR: char = '\u{2028}';

/// A layouted line, consisting of a sequence of layouted inline items that are
/// mostly borrowed from the preparation phase.
pub struct Line<'a> {
    /// The items the line is made of.
    pub items: Items<'a>,
    /// The exact natural width of the line.
    pub width: Abs,
    /// Whether the line should be justified.
    pub justify: bool,
    /// Whether the line ends with a hyphen or dash.
    pub dash: Option<Dash>,
}

impl Line<'_> {
    /// Create an empty line.
    pub fn empty() -> Self {
        Self {
            items: Items::new(),
            width: Abs::zero(),
            justify: false,
            dash: None,
        }
    }

    /// How many glyphs are in the text where we can insert additional
    /// space when encountering underfull lines.
    pub fn justifiables(&self) -> usize {
        let mut count = 0;
        for shaped in self.items.iter().filter_map(Item::text) {
            count += shaped.justifiables();
        }
        if self
            .items
            .trailing_text()
            .map(|s| s.cjk_justifiable_at_last())
            .unwrap_or(false)
        {
            count -= 1;
        }
        count
    }

    /// How much the line can stretch.
    pub fn stretchability(&self) -> Abs {
        self.items
            .iter()
            .filter_map(Item::text)
            .map(|s| s.stretchability())
            .sum()
    }

    /// How much the line can shrink.
    pub fn shrinkability(&self) -> Abs {
        self.items
            .iter()
            .filter_map(Item::text)
            .map(|s| s.shrinkability())
            .sum()
    }

    /// Whether the line has items with negative width.
    pub fn has_negative_width_items(&self) -> bool {
        self.items.iter().any(|item| match item {
            Item::Absolute(amount, _) => *amount < Abs::zero(),
            Item::Frame(frame) => Abs::raw(frame.cols().get() as f64) < Abs::zero(),
            _ => false,
        })
    }

    /// The sum of fractions in the line.
    pub fn fr(&self) -> Fr {
        self.items
            .iter()
            .filter_map(|item| match item {
                Item::Fractional(fr, _) => Some(*fr),
                _ => None,
            })
            .sum()
    }
}

/// A dash at the end of a line.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Dash {
    /// A soft hyphen added to break a word.
    Soft,
    /// A regular hyphen, present in a compound word.
    Hard,
    /// Another kind of dash.
    Other,
}

/// Create a line which spans the given range.
pub fn line<'a>(
    engine: &Engine,
    p: &'a Preparation,
    range: Range,
    breakpoint: Breakpoint,
    pred: Option<&Line<'a>>,
) -> Line<'a> {
    let full = &p.text[range.clone()];

    let justify =
        full.ends_with(LINE_SEPARATOR) || (p.config.justify && breakpoint != Breakpoint::Mandatory);

    let dash = if breakpoint.is_hyphen() || full.ends_with(SHY) {
        Some(Dash::Soft)
    } else if full.ends_with(HYPHEN) {
        Some(Dash::Hard)
    } else if full.ends_with([EN_DASH, EM_DASH]) {
        Some(Dash::Other)
    } else {
        None
    };

    let trim = breakpoint.trim(range.start, full);
    let trimmed_range = range.start..trim.layout;

    let mut items = Items::new();

    if let Some(pred) = pred
        && pred.dash == Some(Dash::Hard)
        && let Some(base) = pred.items.trailing_text()
        && should_repeat_hyphen(base.lang, full)
        && let Some(hyphen) =
            ShapedText::hyphen(engine, p.config.fallback, base, trim.shaping, false)
    {
        items.push(Item::Text(hyphen), LogicalIndex::START_HYPHEN);
    }

    collect_items(&mut items, engine, p, range, &trim);

    if dash == Some(Dash::Soft)
        && let Some(base) = items.trailing_text()
        && let Some(hyphen) =
            ShapedText::hyphen(engine, p.config.fallback, base, trim.shaping, true)
    {
        items.push(Item::Text(hyphen), LogicalIndex::END_HYPHEN);
    }

    trim_weak_spacing(&mut items);
    adjust_cj_at_line_boundaries(p, trimmed_range, &mut items);
    adjust_glyph_stretch_at_line_end(p, &mut items);

    let width = items.iter().map(Item::natural_width).sum();

    Line {
        items,
        width,
        justify,
        dash,
    }
}

/// Collects / reshapes all items for the line with the given `range`.
fn collect_items<'a>(
    items: &mut Items<'a>,
    engine: &Engine,
    p: &'a Preparation,
    range: Range,
    trim: &Trim,
) {
    let mut fallback = None;

    reorder(p, range.clone(), |subrange, rtl| {
        let from = items.len();
        collect_range(engine, p, subrange, trim, items, &mut fallback);
        if rtl {
            items.reorder(from);
        }
    });

    if !items.iter().any(|item| matches!(item, Item::Text(_)))
        && let Some((idx, fallback)) = fallback
    {
        items.push(fallback, idx);
    }
}

/// Trims weak spacing from the start and end of the line.
fn trim_weak_spacing(items: &mut Items) {
    let prefix = items
        .iter()
        .take_while(|item| matches!(item, Item::Absolute(_, true)))
        .count();
    if prefix > 0 {
        items.drain(..prefix);
    }

    while matches!(items.iter().next_back(), Some(Item::Absolute(_, true))) {
        items.pop();
    }
}

/// Calls `f` for the BiDi-reordered ranges of a line.
fn reorder<F>(p: &Preparation, range: Range, mut f: F)
where
    F: FnMut(Range, bool),
{
    let Some(bidi) = &p.bidi else {
        f(range, p.config.dir == Dir::RTL);
        return;
    };

    if range.is_empty() {
        f(range, p.config.dir == Dir::RTL);
        return;
    }

    let para = bidi
        .paragraphs
        .iter()
        .find(|para| para.range.contains(&range.start))
        .unwrap();

    let (levels, runs) = bidi.visual_runs(para, range.clone());

    for run in runs {
        let rtl = levels[run.start].is_rtl();
        f(run, rtl)
    }
}

/// Collects / reshapes all items for the given `subrange` with continuous direction.
fn collect_range<'a>(
    engine: &Engine,
    p: &'a Preparation,
    range: Range,
    trim: &Trim,
    items: &mut Items<'a>,
    fallback: &mut Option<(LogicalIndex, ItemEntry<'a>)>,
) {
    for (i, (subrange, item)) in p.slice(range.clone()) {
        let idx = LogicalIndex::from_item_index(i);

        let Item::Text(shaped) = item else {
            items.push(item, idx);
            continue;
        };

        let sliced =
            range.start.max(subrange.start)..range.end.min(subrange.end).min(trim.shaping);

        let split = subrange.start < sliced.start || sliced.end < subrange.end;

        if sliced.is_empty() {
            *fallback = Some((idx, ItemEntry::from(Item::Text(shaped.empty()))));
            continue;
        }

        let mut item: ItemEntry = if split {
            let reshaped = shaped.reshape(engine, sliced);
            Item::Text(reshaped).into()
        } else {
            item.into()
        };

        if trim.layout < range.end {
            let shaped = item.text_mut().unwrap();
            shaped.glyphs.trim(|glyph| trim.layout < glyph.range.end);
        }

        items.push(item, idx);
    }
}

/// Add spacing around punctuation marks for CJ glyphs at line boundaries.
fn adjust_cj_at_line_boundaries(p: &Preparation, range: Range, items: &mut Items) {
    let text = &p.text[range];

    if text.starts_with(BEGIN_PUNCT_PAT)
        || (p.config.cjk_latin_spacing && text.starts_with(is_of_cj_script))
    {
        adjust_cj_at_line_start(p, items);
    }

    if text.ends_with(END_PUNCT_PAT)
        || (p.config.cjk_latin_spacing && text.ends_with(is_of_cj_script))
    {
        adjust_cj_at_line_end(p, items);
    }
}

/// Remove stretchability from the last glyph in the line.
fn adjust_glyph_stretch_at_line_end(p: &Preparation, items: &mut Items) {
    let glyph_limits = &p.config.justification_limits.tracking();
    if glyph_limits.min.is_zero() && glyph_limits.max.is_zero() {
        return;
    }

    let Some(shaped) = items.trailing_text_mut() else {
        return;
    };
    let Some(glyph) = shaped.glyphs.to_mut().last_mut() else {
        return;
    };
    glyph.adjustability = Adjustability::default();
}

/// Add spacing around punctuation marks for CJ glyphs at the line start.
fn adjust_cj_at_line_start(p: &Preparation, items: &mut Items) {
    let Some(shaped) = items.leading_text_mut() else {
        return;
    };
    let Some(glyph) = shaped.glyphs.first() else {
        return;
    };

    if glyph.is_cjk_right_aligned_punctuation() {
        let glyph = shaped.glyphs.to_mut().first_mut().unwrap();
        let shrink = glyph.shrinkability().0;
        glyph.shrink_left(shrink);
    } else if p.config.cjk_latin_spacing && glyph.is_cj_script() && glyph.x_offset > Em::zero() {
        let glyph = shaped.glyphs.to_mut().first_mut().unwrap();
        let shrink = glyph.x_offset;
        glyph.x_advance -= shrink;
        glyph.x_offset = Em::zero();
        glyph.adjustability.shrinkability.0 = Em::zero();
    }
}

/// Add spacing around punctuation marks for CJ glyphs at the line end.
fn adjust_cj_at_line_end(p: &Preparation, items: &mut Items) {
    let Some(shaped) = items.trailing_text_mut() else {
        return;
    };
    let Some(glyph) = shaped.glyphs.last() else {
        return;
    };

    let style = cjk_punct_style(shaped.lang, shaped.region);

    if glyph.is_cjk_left_aligned_punctuation(style) {
        let shrink = glyph.shrinkability().1;
        let punct = shaped.glyphs.to_mut().last_mut().unwrap();
        punct.shrink_right(shrink);
    } else if p.config.cjk_latin_spacing
        && glyph.is_cj_script()
        && (glyph.x_advance - glyph.x_offset) > Em::one()
    {
        let shrink = glyph.x_advance - glyph.x_offset - Em::one();
        let glyph = shaped.glyphs.to_mut().last_mut().unwrap();
        glyph.x_advance -= shrink;
        glyph.adjustability.shrinkability.1 = Em::zero();
    }
}

/// Whether a hyphen should be repeated at the start of the line.
fn should_repeat_hyphen(lang: Lang, following_text: &str) -> bool {
    match lang {
        Lang::LOWER_SORBIAN
        | Lang::CZECH
        | Lang::CROATIAN
        | Lang::POLISH
        | Lang::PORTUGUESE
        | Lang::SLOVAK => true,
        Lang::SPANISH => following_text
            .chars()
            .next()
            .is_some_and(|c| !c.is_uppercase()),
        _ => false,
    }
}

/// Apply the current baseline shift and italic compensation to a frame.
pub fn apply_shift<'a>(
    world: &comemo::Tracked<'a, dyn typst_library::World + 'a>,
    frame: &mut TermFrame,
    styles: StyleChain,
) {
    let mut baseline = styles.resolve(TextElem::baseline);
    let mut compensation = Abs::zero();
    if let Some(scripts) = styles.get_ref(TextElem::shift_settings) {
        let font_metrics = styles
            .get_ref(TextElem::font)
            .into_iter()
            .find_map(|family| {
                world
                    .book()
                    .select(family.as_str(), variant(styles))
                    .and_then(|id| world.font(id))
            })
            .map_or(*scripts.kind.default_metrics(), |f| {
                *scripts.kind.read_metrics(f.metrics())
            });
        baseline -= scripts
            .shift
            .unwrap_or(font_metrics.vertical_offset)
            .resolve(styles);
        compensation += font_metrics.horizontal_offset.resolve(styles);
    }
    frame.translate(TermPoint {
        col: TermScalar::from_f64(compensation.to_raw()),
        row: TermScalar::from_f64(baseline.to_raw()),
    });
}

/// Commit to a line and build its frame.
#[allow(clippy::too_many_arguments)]
pub fn commit(
    engine: &mut Engine,
    p: &Preparation,
    line: &Line,
    width: Abs,
    full: Abs,
    locator: &mut SplitLocator<'_>,
) -> SourceResult<TermFrame> {
    let mut remaining = width - line.width - p.config.hanging_indent;
    let mut offset = Abs::zero();

    if p.config.dir == Dir::LTR {
        offset += p.config.hanging_indent;
    }

    // Handle hanging punctuation to the left.
    if let Some(text) = line.items.leading_text()
        && let Some(glyph) = text.glyphs.first()
        && !text.dir.is_positive()
        && text.styles.get(TextElem::overhang)
        && (line.items.len() > 1 || text.glyphs.len() > 1)
    {
        let amount = overhang(glyph.c) * glyph.x_advance.at(glyph.size);
        offset -= amount;
        remaining += amount;
    }

    // Handle hanging punctuation to the right.
    if let Some(text) = line.items.trailing_text()
        && let Some(glyph) = text.glyphs.last()
        && text.dir.is_positive()
        && text.styles.get(TextElem::overhang)
        && (line.items.len() > 1 || text.glyphs.len() > 1)
    {
        let amount = overhang(glyph.c) * glyph.x_advance.at(glyph.size);
        remaining += amount;
    }

    let fr = line.fr();
    let mut justification_ratio = 0.0;
    let mut extra_justification = Abs::zero();

    let shrinkability = line.shrinkability();
    let stretchability = line.stretchability();
    if remaining < Abs::zero() && shrinkability > Abs::zero() {
        justification_ratio = (remaining / shrinkability).max(-1.0);
        remaining = (remaining + shrinkability).min(Abs::zero());
    } else if line.justify && fr.is_zero() {
        if stretchability > Abs::zero() {
            justification_ratio = (remaining / stretchability).min(1.0);
            remaining = (remaining - stretchability).max(Abs::zero());
        }

        let justifiables = line.justifiables();
        if justifiables > 0 && remaining > Abs::zero() {
            extra_justification = remaining / justifiables as f64;
            remaining = Abs::zero();
        }
    }

    let mut top = Abs::zero();
    let mut bottom = Abs::zero();

    let mut frames = vec![];
    for &(idx, ref item) in line.items.indexed_iter() {
        let mut push = |offset: &mut Abs, frame: TermFrame, idx: LogicalIndex| {
            let w = Abs::raw(frame.cols().get() as f64);
            top.set_max(Abs::raw(frame.baseline().get() as f64));
            bottom.set_max(Abs::raw(frame.rows().get() as f64) - Abs::raw(frame.baseline().get() as f64));
            frames.push((*offset, frame, idx));
            *offset += w;
        };

        match &**item {
            Item::Absolute(v, _) => {
                offset += *v;
            }
            Item::Fractional(v, elem) => {
                let amount = v.share(fr, remaining);
                if let Some((elem, loc, styles)) = elem {
                    let region = TermSize {
                        cols: TermScalar::from_f64(amount.to_raw()),
                        rows: TermScalar::from_f64(full.to_raw()),
                    };
                    // NOTE: layout_and_modify is expected from crate::modifiers (Agent F)
                    // For now call layout_box directly.
                    let mut frame = layout_box(elem, engine, loc.relayout(), *styles, region)?;
                    apply_shift(&engine.world, &mut frame, *styles);
                    push(&mut offset, frame, idx);
                } else {
                    offset += amount;
                }
            }
            Item::Text(shaped) => {
                let frame =
                    shaped.build(engine, &p.spans, justification_ratio, extra_justification);
                push(&mut offset, frame, idx);
            }
            Item::Frame(frame) => {
                push(&mut offset, frame.clone(), idx);
            }
            Item::Tag(tag) => {
                let frame = TermFrame::soft(TermSize {
                    cols: TermScalar::ZERO,
                    rows: TermScalar::ZERO,
                });
                // NOTE: push_tag is not available on TermFrame.
                // Tags are attached via TermFrameItem placeholder.
                let _ = tag;
                frames.push((offset, frame, idx));
            }
            Item::Skip(_) => {}
        }
    }

    if !fr.is_zero() {
        remaining = Abs::zero();
    }

    let size = TermSize {
        cols: TermScalar::from_f64(width.to_raw()),
        rows: TermScalar::from_f64((top + bottom).to_raw()),
    };
    let mut output = TermFrame::soft(size);
    output.set_baseline(TermScalar::from_f64(top.to_raw()));

    if let Some(marker) = &p.config.numbering_marker {
        add_par_line_marker(&mut output, marker, engine, locator, TermScalar::from_f64(top.to_raw()));
    }

    frames.sort_unstable_by_key(|(_, _, idx)| *idx);

    for (offset, frame, _) in frames {
        let x = TermScalar::from_f64((offset + p.config.align.position(remaining)).to_raw());
        let y = TermScalar::from_f64(top.to_raw()) - frame.baseline();
        output.push_frame(TermPoint { col: x, row: y }, frame);
    }

    Ok(output)
}

/// Adds a paragraph line marker to a paragraph line's output frame.
fn add_par_line_marker(
    output: &mut TermFrame,
    marker: &typst_library::foundations::Packed<ParLineMarker>,
    engine: &mut Engine,
    locator: &mut SplitLocator,
    top: TermScalar,
) {
    let mut marker = marker.clone();
    let key = typst_utils::hash128(&marker);
    let loc = locator.next_location(engine.introspector, key);
    marker.set_location(loc);

    let pos = TermPoint {
        col: TermScalar::ZERO,
        row: top,
    };
    let flags = TagFlags {
        introspectable: false,
        tagged: false,
    };
    // NOTE: push_tag is not available on TermFrame.
    // Tags need to be tracked via a separate mechanism.
    let _ = (output, pos, marker, flags, loc, key);
    // output.push_tag(pos, Tag::Start(marker.pack(), flags));
    // output.push_tag(pos, Tag::End(loc, key, flags));
}

/// How much a character should hang into the end margin.
fn overhang(c: char) -> f64 {
    match c {
        '\u{2013}' | '\u{2014}' => 0.2,
        '-' | '\u{ad}' => 0.55,
        '.' | ',' => 0.8,
        ':' | ';' => 0.3,
        '\u{60C}' | '\u{6D4}' => 0.4,
        _ => 0.0,
    }
}

/// A collection of owned or borrowed inline items.
pub struct Items<'a>(Vec<(LogicalIndex, ItemEntry<'a>)>);

impl<'a> Items<'a> {
    /// Create empty items.
    pub fn new() -> Self {
        Self(vec![])
    }

    /// Push a new item.
    pub fn push(&mut self, entry: impl Into<ItemEntry<'a>>, idx: LogicalIndex) {
        self.0.push((idx, entry.into()));
    }

    /// Iterate over the items.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Item<'a>> {
        self.0.iter().map(|(_, item)| &**item)
    }

    /// Iterate over the items with the indices that define their logical order.
    pub fn indexed_iter(&self) -> impl DoubleEndedIterator<Item = &(LogicalIndex, ItemEntry<'a>)> {
        self.0.iter()
    }

    /// Access the first item (skipping tags), if it is text.
    pub fn leading_text(&self) -> Option<&ShapedText<'a>> {
        self.0.iter().find(|(_, item)| !item.is_tag())?.1.text()
    }

    /// Access the first item (skipping tags) mutably, if it is text.
    pub fn leading_text_mut(&mut self) -> Option<&mut ShapedText<'a>> {
        self.0
            .iter_mut()
            .find(|(_, item)| !item.is_tag())?
            .1
            .text_mut()
    }

    /// Access the last item (skipping tags), if it is text.
    pub fn trailing_text(&self) -> Option<&ShapedText<'a>> {
        self.0
            .iter()
            .rev()
            .find(|(_, item)| !item.is_tag())?
            .1
            .text()
    }

    /// Access the last item (skipping tags) mutably, if it is text.
    pub fn trailing_text_mut(&mut self) -> Option<&mut ShapedText<'a>> {
        self.0
            .iter_mut()
            .rev()
            .find(|(_, item)| !item.is_tag())?
            .1
            .text_mut()
    }

    /// Reorder the items starting at the given index to RTL.
    pub fn reorder(&mut self, from: usize) {
        self.0[from..].reverse()
    }
}

impl<'a> Deref for Items<'a> {
    type Target = Vec<(LogicalIndex, ItemEntry<'a>)>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Items<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Debug for Items<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.0).finish()
    }
}

/// We use indices to remember the logical (as opposed to visual) order of items.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct LogicalIndex(usize);

impl LogicalIndex {
    const START_HYPHEN: Self = Self(0);
    const END_HYPHEN: Self = Self(usize::MAX);

    /// Create a logical index from the index of an item in the [`p.items`](Preparation::items).
    const fn from_item_index(i: usize) -> Self {
        Self(i + 1)
    }
}

/// A reference to or a boxed item.
pub enum ItemEntry<'a> {
    Ref(&'a Item<'a>),
    Box(Box<Item<'a>>),
}

impl<'a> ItemEntry<'a> {
    fn text_mut(&mut self) -> Option<&mut ShapedText<'a>> {
        match self {
            Self::Ref(item) => {
                let text = item.text()?;
                *self = Self::Box(Box::new(Item::Text(text.clone())));
                match self {
                    Self::Box(item) => item.text_mut(),
                    _ => unreachable!(),
                }
            }
            Self::Box(item) => item.text_mut(),
        }
    }
}

impl<'a> Deref for ItemEntry<'a> {
    type Target = Item<'a>;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Ref(item) => item,
            Self::Box(item) => item,
        }
    }
}

impl Debug for ItemEntry<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<'a> From<&'a Item<'a>> for ItemEntry<'a> {
    fn from(item: &'a Item<'a>) -> Self {
        Self::Ref(item)
    }
}

impl<'a> From<Item<'a>> for ItemEntry<'a> {
    fn from(item: Item<'a>) -> Self {
        Self::Box(Box::new(item))
    }
}
