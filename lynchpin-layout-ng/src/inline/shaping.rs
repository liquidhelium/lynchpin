use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;
use std::sync::Arc;

use az::SaturatingAs;
use crossterm::style::ContentStyle;
use ecow::EcoString;
use rustybuzz::{BufferFlags, Feature, ShapePlan, UnicodeBuffer};
use ttf_parser::Tag;
use ttf_parser::gsub::SubstitutionSubtable;
use typst_library::World;
use typst_library::engine::Engine;
use typst_library::foundations::{Smart, StyleChain};
use typst_library::layout::{Abs, Dir, Em, Rel};
use typst_library::model::{JustificationLimits, ParElem};
use typst_library::text::{
    Font, FontFamily, FontVariant, Glyph, Lang, Region, ShiftSettings, TextEdgeBounds, TextElem,
    TextItem, families, features, is_default_ignorable, language, variant,
};
use typst_utils::SliceExt;
use unicode_bidi::{BidiInfo, Level as BidiLevel};
use unicode_script::{Script, UnicodeScript};

use lynchpin_library_ng::*;

use super::{Item, Range, SpanMapper, decorate};
// NOTE: FrameModifyText is expected from crate::modifiers (Agent F)

const SHY: char = '\u{ad}';
const SHY_STR: &str = "\u{ad}";
const HYPHEN: char = '-';
const HYPHEN_STR: &str = "-";

/// The result of shaping text.
#[derive(Clone)]
pub struct ShapedText<'a> {
    pub base: usize,
    pub text: &'a str,
    pub dir: Dir,
    pub lang: Lang,
    pub region: Option<Region>,
    pub styles: StyleChain<'a>,
    pub variant: FontVariant,
    pub glyphs: Glyphs<'a>,
}

/// A copy-on-write collection of glyphs.
#[derive(Clone)]
pub struct Glyphs<'a> {
    inner: Cow<'a, [ShapedGlyph]>,
    kept: Range,
}

impl<'a> Glyphs<'a> {
    pub fn from_slice(glyphs: &'a [ShapedGlyph]) -> Self {
        Self {
            inner: Cow::Borrowed(glyphs),
            kept: 0..glyphs.len(),
        }
    }

    pub fn from_vec(glyphs: Vec<ShapedGlyph>) -> Self {
        let len = glyphs.len();
        Self {
            inner: Cow::Owned(glyphs),
            kept: 0..len,
        }
    }

    pub fn to_mut(&mut self) -> &mut [ShapedGlyph] {
        &mut self.inner.to_mut()[self.kept.clone()]
    }

    pub fn trim(&mut self, f: impl FnMut(&ShapedGlyph) -> bool + Copy) {
        let (start, end) = self.inner.split_prefix_suffix(f);
        self.kept = start..end;
    }

    pub fn all(&self) -> &[ShapedGlyph] {
        self.inner.as_ref()
    }

    pub fn is_fully_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<'a> Deref for Glyphs<'a> {
    type Target = [ShapedGlyph];

    fn deref(&self) -> &Self::Target {
        &self.inner[self.kept.clone()]
    }
}

/// A single glyph resulting from shaping.
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    pub font: Font,
    pub glyph_id: u16,
    pub x_advance: Em,
    pub x_offset: Em,
    pub y_offset: Em,
    pub size: Abs,
    pub adjustability: Adjustability,
    pub range: Range,
    pub safe_to_break: bool,
    pub c: char,
    pub is_justifiable: bool,
    pub script: Script,
}

#[derive(Debug, Default, Clone)]
pub struct Adjustability {
    pub stretchability: (Em, Em),
    pub shrinkability: (Em, Em),
}

impl ShapedGlyph {
    pub fn is_space(&self) -> bool {
        is_space(self.c)
    }

    pub fn is_justifiable(&self) -> bool {
        self.is_justifiable
    }

    pub fn is_cj_script(&self) -> bool {
        is_cj_script(self.c, self.script)
    }

    pub fn is_cjk_punctuation(&self) -> bool {
        self.is_cjk_left_aligned_punctuation(CjkPunctStyle::Gb)
            || self.is_cjk_right_aligned_punctuation()
            || self.is_cjk_center_aligned_punctuation(CjkPunctStyle::Gb)
    }

    pub fn is_cjk_left_aligned_punctuation(&self, style: CjkPunctStyle) -> bool {
        is_cjk_left_aligned_punctuation(self.c, self.x_advance, self.stretchability(), style)
    }

    pub fn is_cjk_right_aligned_punctuation(&self) -> bool {
        is_cjk_right_aligned_punctuation(self.c, self.x_advance, self.stretchability())
    }

    pub fn is_cjk_center_aligned_punctuation(&self, style: CjkPunctStyle) -> bool {
        is_cjk_center_aligned_punctuation(self.c, style)
    }

    pub fn is_letter_or_number(&self) -> bool {
        matches!(
            self.c.script(),
            Script::Latin | Script::Greek | Script::Cyrillic
        ) || matches!(self.c, '#' | '$' | '%' | '&')
            || self.c.is_ascii_digit()
    }

    pub fn base_adjustability(
        &self,
        style: CjkPunctStyle,
        limits: &JustificationLimits,
        font_size: Abs,
        stretchable: bool,
    ) -> Adjustability {
        let width = self.x_advance;

        let limited = |v: Em| v.min(width * 0.75);

        if self.is_space() {
            let max = limits.spacing().max + limits.tracking().max;
            let min = limits.spacing().min + limits.tracking().min;
            Adjustability {
                stretchability: (
                    Em::zero(),
                    (max - Rel::one())
                        .map(|length| Em::from_length(length, font_size))
                        .relative_to(width)
                        .max(Em::zero()),
                ),
                shrinkability: (
                    Em::zero(),
                    limited(
                        (Rel::one() - min)
                            .map(|length| Em::from_length(length, font_size))
                            .relative_to(width),
                    ),
                ),
            }
        } else if self.is_cjk_left_aligned_punctuation(style) {
            Adjustability {
                stretchability: (Em::zero(), Em::zero()),
                shrinkability: (Em::zero(), width / 2.0),
            }
        } else if self.is_cjk_right_aligned_punctuation() {
            Adjustability {
                stretchability: (Em::zero(), Em::zero()),
                shrinkability: (width / 2.0, Em::zero()),
            }
        } else if self.is_cjk_center_aligned_punctuation(style) {
            Adjustability {
                stretchability: (Em::zero(), Em::zero()),
                shrinkability: (width / 4.0, width / 4.0),
            }
        } else if stretchable {
            Adjustability {
                stretchability: (
                    Em::zero(),
                    Em::from_length(limits.tracking().max, font_size).max(Em::zero()),
                ),
                shrinkability: (
                    Em::zero(),
                    limited(Em::from_length(-limits.tracking().min, font_size)),
                ),
            }
        } else {
            Adjustability::default()
        }
    }

    pub fn stretchability(&self) -> (Em, Em) {
        self.adjustability.stretchability
    }

    pub fn shrinkability(&self) -> (Em, Em) {
        self.adjustability.shrinkability
    }

    pub fn shrink_left(&mut self, amount: Em) {
        self.x_offset -= amount;
        self.x_advance -= amount;
        self.adjustability.shrinkability.0 -= amount;
    }

    pub fn shrink_right(&mut self, amount: Em) {
        self.x_advance -= amount;
        self.adjustability.shrinkability.1 -= amount;
    }
}

impl<'a> ShapedText<'a> {
    /// Build the shaped text's frame.
    pub fn build(
        &self,
        engine: &Engine,
        spans: &SpanMapper,
        justification_ratio: f64,
        extra_justification: Abs,
    ) -> TermFrame {
        let (top, bottom) = self.measure(engine);
        let size = TermSize {
            cols: TermScalar::from_f64(self.width().to_raw()),
            rows: TermScalar::from_f64((top + bottom).to_raw()),
        };

        let mut offset = Abs::zero();
        let mut frame = TermFrame::soft(size);
        frame.set_baseline(TermScalar::from_f64(top.to_raw()));

        let font_size = self.styles.resolve(TextElem::size);
        let shift = self.styles.resolve(TextElem::baseline);
        let decos = self.styles.get_cloned(TextElem::deco);
        let fill = self.styles.get_ref(TextElem::fill);
        let stroke = self.styles.resolve(TextElem::stroke);
        let span_offset = self.styles.get(TextElem::span_offset);

        let text_style = ContentStyle::default();

        let mut i = 0;
        for ((font, y_offset, glyph_size), group) in self
            .glyphs
            .all()
            .group_by_key(|g| (g.font.clone(), g.y_offset, g.size))
        {
            let mut range = group[0].range.clone();
            for glyph in group {
                range.start = range.start.min(glyph.range.start);
                range.end = range.end.max(glyph.range.end);
            }

            let pos = TermPoint {
                col: TermScalar::from_f64(offset.to_raw()),
                row: TermScalar::from_f64((top + shift - y_offset.at(font_size)).to_raw()),
            };

            let glyphs: Vec<Glyph> = group
                .iter()
                .map(|shaped: &ShapedGlyph| {
                    let kept = self.glyphs.kept.contains(&i);

                    let (x_advance, x_offset) = if kept {
                        let adjustability_left = if justification_ratio < 0.0 {
                            shaped.shrinkability().0
                        } else {
                            shaped.stretchability().0
                        };
                        let adjustability_right = if justification_ratio < 0.0 {
                            shaped.shrinkability().1
                        } else {
                            shaped.stretchability().1
                        };

                        let justification_left = adjustability_left * justification_ratio;
                        let mut justification_right = adjustability_right * justification_ratio;
                        if shaped.is_justifiable() {
                            justification_right += Em::from_abs(extra_justification, glyph_size);
                        }

                        (
                            shaped.x_advance + justification_left + justification_right,
                            shaped.x_offset + justification_left,
                        )
                    } else {
                        (Em::zero(), Em::zero())
                    };
                    i += 1;

                    let mut span = spans.span_at(shaped.range.start);
                    span.1 = span.1.saturating_add(span_offset.saturating_as());

                    Glyph {
                        id: shaped.glyph_id,
                        x_advance,
                        x_offset,
                        y_advance: Em::zero(),
                        y_offset: Em::zero(),
                        range: (shaped.range.start - range.start).saturating_as()
                            ..(shaped.range.end - range.start).saturating_as(),
                        span,
                    }
                })
                .collect();

            let item = TextItem {
                font,
                size: glyph_size,
                lang: self.lang,
                region: self.region,
                fill: fill.clone(),
                stroke: stroke.clone().map(|s| s.unwrap_or_default()),
                text: self.text[range.start - self.base..range.end - self.base].into(),
                glyphs,
            };

            let width = item.width();
            let text_str: EcoString = item.text.clone();

            if decos.is_empty() {
                frame.push_text(pos, text_str, text_style);
            } else {
                frame.push_text(pos, text_str.clone(), text_style);
                let mut deco_frame = TermFrame::soft(TermSize {
                    cols: TermScalar::from_f64(width.to_raw()),
                    rows: TermScalar::from_f64((top + bottom).to_raw()),
                });
                for deco in &decos {
                    decorate(
                        &mut deco_frame,
                        deco,
                        &item,
                        width,
                        shift,
                        pos,
                    );
                }
                frame.push_frame(
                    TermPoint {
                        col: TermScalar::ZERO,
                        row: TermScalar::ZERO,
                    },
                    deco_frame,
                );
            }

            offset += width;
        }

        frame
    }

    pub fn width(&self) -> Abs {
        self.glyphs.iter().map(|g| g.x_advance.at(g.size)).sum()
    }

    pub fn measure(&self, engine: &Engine) -> (Abs, Abs) {
        let mut top = Abs::zero();
        let mut bottom = Abs::zero();

        let size = self.styles.resolve(TextElem::size);
        let top_edge = self.styles.get(TextElem::top_edge);
        let bottom_edge = self.styles.get(TextElem::bottom_edge);

        let mut expand = |font: &Font, bounds: TextEdgeBounds| {
            let (t, b) = font.edges(top_edge, bottom_edge, size, bounds);
            top.set_max(t);
            bottom.set_max(b);
        };

        if self.glyphs.is_fully_empty() {
            let world = engine.world;
            for family in families(self.styles) {
                if let Some(font) = world
                    .book()
                    .select(family.as_str(), self.variant)
                    .and_then(|id| world.font(id))
                {
                    expand(&font, TextEdgeBounds::Zero);
                    break;
                }
            }
        } else {
            for g in self.glyphs.iter() {
                expand(&g.font, TextEdgeBounds::Glyph(g.glyph_id));
            }
        }

        (top, bottom)
    }

    pub fn justifiables(&self) -> usize {
        self.glyphs.iter().filter(|g| g.is_justifiable()).count()
    }

    pub fn cjk_justifiable_at_last(&self) -> bool {
        self.glyphs
            .last()
            .map(|g| g.is_cj_script() || g.is_cjk_punctuation())
            .unwrap_or(false)
    }

    pub fn stretchability(&self) -> Abs {
        self.glyphs
            .iter()
            .map(|g| (g.stretchability().0 + g.stretchability().1).at(g.size))
            .sum()
    }

    pub fn shrinkability(&self) -> Abs {
        self.glyphs
            .iter()
            .map(|g| (g.shrinkability().0 + g.shrinkability().1).at(g.size))
            .sum()
    }

    pub fn reshape(&'a self, engine: &Engine, text_range: Range) -> ShapedText<'a> {
        let text = &self.text[text_range.start - self.base..text_range.end - self.base];
        if let Some(glyphs) = self.slice_safe_to_break(text_range.clone()) {
            #[cfg(debug_assertions)]
            assert_all_glyphs_in_range(glyphs, text, text_range.clone());
            Self {
                base: text_range.start,
                text,
                dir: self.dir,
                lang: self.lang,
                region: self.region,
                styles: self.styles,
                variant: self.variant,
                glyphs: Glyphs::from_slice(glyphs),
            }
        } else {
            shape(
                engine,
                text_range.start,
                text,
                self.styles,
                self.dir,
                self.lang,
                self.region,
            )
        }
    }

    pub fn empty(&self) -> Self {
        Self {
            text: "",
            glyphs: Glyphs::from_slice(&[]),
            ..*self
        }
    }

    pub fn hyphen(
        engine: &Engine,
        fallback: bool,
        base: &ShapedText<'a>,
        pos: usize,
        soft: bool,
    ) -> Option<Self> {
        let world = engine.world;
        let book = world.book();
        let fallback_func = if fallback {
            Some(|| book.select_fallback(None, base.variant, "-"))
        } else {
            None
        };
        let mut chain = families(base.styles)
            .filter(|family| family.covers().is_none_or(|c| c.is_match("-")))
            .map(|family| book.select(family.as_str(), base.variant))
            .chain(fallback_func.iter().map(|f| f()))
            .flatten();

        chain.find_map(|id| {
            let font = world.font(id)?;
            let ttf = font.ttf();
            let glyph_id = ttf.glyph_index('-')?;
            let x_advance = font.to_em(ttf.glyph_hor_advance(glyph_id)?);
            let size = base.styles.resolve(TextElem::size);
            let (c, text) = if soft {
                (SHY, SHY_STR)
            } else {
                (HYPHEN, HYPHEN_STR)
            };

            Some(ShapedText {
                base: pos,
                text,
                dir: base.dir,
                lang: base.lang,
                region: base.region,
                styles: base.styles,
                variant: base.variant,
                glyphs: Glyphs::from_vec(vec![ShapedGlyph {
                    font,
                    glyph_id: glyph_id.0,
                    x_advance,
                    x_offset: Em::zero(),
                    y_offset: Em::zero(),
                    size,
                    adjustability: Adjustability::default(),
                    range: pos..pos + text.len(),
                    safe_to_break: true,
                    c,
                    is_justifiable: false,
                    script: Script::Common,
                }]),
            })
        })
    }

    fn slice_safe_to_break(&self, text_range: Range) -> Option<&[ShapedGlyph]> {
        let Range { mut start, mut end } = text_range;
        if !self.dir.is_positive() {
            std::mem::swap(&mut start, &mut end);
        }

        let left = self.find_safe_to_break(start)?;
        let right = self.find_safe_to_break(end)?;
        Some(&self.glyphs[left..right])
    }

    fn find_safe_to_break(&self, text_index: usize) -> Option<usize> {
        let ltr = self.dir.is_positive();

        let len = self.glyphs.len();
        if text_index == self.base {
            return Some(if ltr { 0 } else { len });
        } else if text_index == self.base + self.text.len() {
            return Some(if ltr { len } else { 0 });
        }

        let found = self.glyphs.binary_search_by(|g: &ShapedGlyph| {
            let ordering = g.range.start.cmp(&text_index);
            if ltr {
                ordering
            } else {
                ordering.reverse()
            }
        });

        let mut idx = match found {
            Ok(idx) => idx,
            Err(idx) => {
                return (idx > 0
                    && self.glyphs[idx - 1].range.end == text_index
                    && self.text[text_index - self.base..].starts_with('\n'))
                .then_some(idx);
            }
        };

        let dec = if ltr {
            usize::checked_sub
        } else {
            usize::checked_add
        };
        while let Some(next) = dec(idx, 1) {
            if self
                .glyphs
                .get(next)
                .is_none_or(|g| g.range.start != text_index)
            {
                break;
            }
            idx = next;
        }

        self.glyphs[idx]
            .safe_to_break
            .then_some(idx + usize::from(!ltr))
    }
}

impl Debug for ShapedText<'_> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.text.fmt(f)
    }
}

/// Group a range of text by BiDi level and script, shape the runs and generate items.
pub fn shape_range<'a>(
    items: &mut Vec<(Range, Item<'a>)>,
    engine: &Engine,
    text: &'a str,
    bidi: &BidiInfo<'a>,
    range: Range,
    styles: StyleChain<'a>,
) {
    let script = styles.get(TextElem::script);
    let lang = styles.get(TextElem::lang);
    let region = styles.get(TextElem::region);
    let mut process = |range: Range, level: BidiLevel| {
        let dir = if level.is_ltr() { Dir::LTR } else { Dir::RTL };
        let shaped = shape(
            engine,
            range.start,
            &text[range.clone()],
            styles,
            dir,
            lang,
            region,
        );
        items.push((range, Item::Text(shaped)));
    };

    let mut prev_level = BidiLevel::ltr();
    let mut prev_script = Script::Unknown;
    let mut cursor = range.start;

    for i in range.clone() {
        if !text.is_char_boundary(i) {
            continue;
        }

        let level = bidi.levels[i];
        let curr_script = match script {
            Smart::Auto => text[i..]
                .chars()
                .next()
                .map_or(Script::Unknown, |c| c.script()),
            Smart::Custom(_) => Script::Unknown,
        };

        if level != prev_level || !is_compatible(curr_script, prev_script) {
            if cursor < i {
                process(cursor..i, prev_level);
            }
            cursor = i;
            prev_level = level;
            prev_script = curr_script;
        } else if is_generic_script(prev_script) {
            prev_script = curr_script;
        }
    }

    process(cursor..range.end, prev_level);
}

fn is_generic_script(script: Script) -> bool {
    matches!(script, Script::Unknown | Script::Common | Script::Inherited)
}

fn is_compatible(a: Script, b: Script) -> bool {
    is_generic_script(a) || is_generic_script(b) || a == b
}

/// Shape text into [`ShapedText`].
#[allow(clippy::too_many_arguments)]
fn shape<'a>(
    engine: &Engine,
    base: usize,
    text: &'a str,
    styles: StyleChain<'a>,
    dir: Dir,
    lang: Lang,
    region: Option<Region>,
) -> ShapedText<'a> {
    let size = styles.resolve(TextElem::size);
    let shift_settings = styles.get(TextElem::shift_settings);
    let mut ctx = ShapingContext {
        engine,
        size,
        glyphs: vec![],
        used: vec![],
        styles,
        variant: variant(styles),
        features: features(styles),
        fallback: styles.get(TextElem::fallback),
        dir,
        shift_settings,
    };

    if !text.is_empty() {
        shape_segment(&mut ctx, base, text, families(styles));
    }

    track_and_space(&mut ctx);
    calculate_adjustability(&mut ctx, lang, region);

    #[cfg(debug_assertions)]
    assert_all_glyphs_in_range(&ctx.glyphs, text, base..(base + text.len()));
    #[cfg(debug_assertions)]
    assert_glyph_ranges_in_order(&ctx.glyphs, dir);

    ShapedText {
        base,
        text,
        dir,
        lang,
        region,
        styles,
        variant: ctx.variant,
        glyphs: Glyphs::from_vec(ctx.glyphs),
    }
}

struct ShapingContext<'a, 'v> {
    engine: &'a Engine<'v>,
    glyphs: Vec<ShapedGlyph>,
    used: Vec<Font>,
    styles: StyleChain<'a>,
    size: Abs,
    variant: FontVariant,
    features: Vec<rustybuzz::Feature>,
    fallback: bool,
    dir: Dir,
    shift_settings: Option<ShiftSettings>,
}

/// Shape text with font fallback using the `families` iterator.
fn shape_segment<'a>(
    ctx: &mut ShapingContext,
    base: usize,
    text: &str,
    mut families: impl Iterator<Item = &'a FontFamily> + Clone,
) {
    if text
        .chars()
        .all(|c| c == '\n' || c == '\t' || is_default_ignorable(c))
    {
        return;
    }

    let world = ctx.engine.world;
    let book = world.book();
    let mut selection = None;
    let mut covers = None;
    for family in families.by_ref() {
        selection = book
            .select(family.as_str(), ctx.variant)
            .and_then(|id| world.font(id))
            .filter(|font| !ctx.used.contains(font));
        if selection.is_some() {
            covers = family.covers();
            break;
        }
    }

    if selection.is_none() && ctx.fallback {
        let first = ctx.used.first().map(Font::info);
        selection = book
            .select_fallback(first, ctx.variant, text)
            .and_then(|id| world.font(id))
            .filter(|font| !ctx.used.contains(font));
    }

    let Some(font) = selection else {
        if let Some(font) = ctx.used.first().cloned() {
            shape_tofus(ctx, base, text, font);
        }
        return;
    };

    if covers.is_none() {
        ctx.used.push(font.clone());
    }

    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.set_language(language(ctx.styles));
    if let Some(script) = ctx
        .styles
        .get(TextElem::script)
        .custom()
        .and_then(|script| rustybuzz::Script::from_iso15924_tag(Tag::from_bytes(script.as_bytes())))
    {
        buffer.set_script(script)
    }
    buffer.set_direction(match ctx.dir {
        Dir::LTR => rustybuzz::Direction::LeftToRight,
        Dir::RTL => rustybuzz::Direction::RightToLeft,
        _ => unimplemented!("vertical text layout"),
    });
    buffer.guess_segment_properties();

    buffer.set_flags(BufferFlags::REMOVE_DEFAULT_IGNORABLES);

    let (script_shift, script_compensation, scale, shift_feature) = ctx
        .shift_settings
        .map_or((Em::zero(), Em::zero(), Em::one(), None), |settings| {
            determine_shift(text, &font, settings)
        });

    let has_shift_feature = shift_feature.is_some();
    if let Some(feat) = shift_feature {
        ctx.features.push(feat)
    }

    let plan = create_shape_plan(
        &font,
        buffer.direction(),
        buffer.script(),
        buffer.language().as_ref(),
        &ctx.features,
    );

    if has_shift_feature {
        ctx.features.pop();
    }

    let buffer = rustybuzz::shape_with_plan(font.rusty(), &plan, buffer);
    let infos = buffer.glyph_infos();
    let pos = buffer.glyph_positions();
    let ltr = ctx.dir.is_positive();

    let is_covered = |offset| {
        let end = text[offset..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| offset + i)
            .unwrap_or(text.len());
        covers.is_none_or(|cov| cov.is_match(&text[offset..end]))
    };

    let mut i = 0;
    while i < infos.len() {
        let info = &infos[i];
        let cluster = info.cluster as usize;

        if info.glyph_id != 0 && is_covered(cluster) {
            let start = base + cluster;

            let mut k = i;
            let step: isize = if ltr { 1 } else { -1 };
            let end = loop {
                let Some((next, next_info)) = k
                    .checked_add_signed(step)
                    .and_then(|n| infos.get(n).map(|info| (n, info)))
                else {
                    break base + text.len();
                };

                if next_info.cluster != info.cluster {
                    break base + next_info.cluster as usize;
                }

                k = next;
            };

            let c = text[cluster..].chars().next().unwrap();
            let script = c.script();
            let x_advance = font.to_em(pos[i].x_advance);
            ctx.glyphs.push(ShapedGlyph {
                font: font.clone(),
                glyph_id: info.glyph_id as u16,
                x_advance,
                x_offset: font.to_em(pos[i].x_offset) + script_compensation,
                y_offset: font.to_em(pos[i].y_offset) + script_shift,
                size: scale.at(ctx.size),
                adjustability: Adjustability::default(),
                range: start..end,
                safe_to_break: !info.unsafe_to_break(),
                c,
                is_justifiable: is_justifiable(
                    c,
                    script,
                    x_advance,
                    Adjustability::default().stretchability,
                ),
                script,
            });
        } else {
            let k = i;
            while infos
                .get(i + 1)
                .is_some_and(|info| info.glyph_id == 0 || !is_covered(info.cluster as usize))
            {
                i += 1;
            }

            let start = infos[if ltr { k } else { i }].cluster as usize;
            let end = if ltr {
                i.checked_add(1)
            } else {
                k.checked_sub(1)
            }
            .and_then(|last| infos.get(last))
            .map_or(text.len(), |info| info.cluster as usize);

            let remove = base + start..base + end;
            while ctx
                .glyphs
                .last()
                .is_some_and(|g| remove.contains(&g.range.start))
            {
                ctx.glyphs.pop();
            }

            shape_segment(ctx, base + start, &text[start..end], families.clone());
        }

        i += 1;
    }

    ctx.used.pop();
}

fn determine_shift(
    text: &str,
    font: &Font,
    settings: ShiftSettings,
) -> (Em, Em, Em, Option<Feature>) {
    settings
        .typographic
        .then(|| {
            let gsub = font.rusty().tables().gsub?;
            let lookups = gsub.features.find(settings.kind.feature())?.lookup_indices;
            text.chars()
                .all(|c| {
                    let Some(i) = font.rusty().glyph_index(c) else {
                        return false;
                    };
                    lookups
                        .into_iter()
                        .flat_map(|i| gsub.lookups.get(i))
                        .flat_map(|lookup| lookup.subtables.into_iter::<SubstitutionSubtable>())
                        .any(|subtable| subtable.coverage().contains(i))
                })
                .then(|| {
                    (
                        Em::zero(),
                        Em::zero(),
                        Em::one(),
                        Some(Feature::new(settings.kind.feature(), 1, ..)),
                    )
                })
        })
        .flatten()
        .unwrap_or_else(|| {
            let script_metrics = settings.kind.read_metrics(font.metrics());
            (
                settings.shift.unwrap_or(script_metrics.vertical_offset),
                script_metrics.horizontal_offset,
                settings.size.unwrap_or(script_metrics.height),
                None,
            )
        })
}

/// Create a shape plan.
#[comemo::memoize]
pub fn create_shape_plan(
    font: &Font,
    direction: rustybuzz::Direction,
    script: rustybuzz::Script,
    language: Option<&rustybuzz::Language>,
    features: &[rustybuzz::Feature],
) -> Arc<ShapePlan> {
    Arc::new(rustybuzz::ShapePlan::new(
        font.rusty(),
        direction,
        Some(script),
        language,
        features,
    ))
}

fn shape_tofus(ctx: &mut ShapingContext, base: usize, text: &str, font: Font) {
    let x_advance = font.x_advance(0).unwrap_or_default();
    let add_glyph = |(cluster, c): (usize, char)| {
        let start = base + cluster;
        let end = start + c.len_utf8();
        let script = c.script();
        ctx.glyphs.push(ShapedGlyph {
            font: font.clone(),
            glyph_id: 0,
            x_advance,
            x_offset: Em::zero(),
            y_offset: Em::zero(),
            size: ctx.size,
            adjustability: Adjustability::default(),
            range: start..end,
            safe_to_break: true,
            c,
            is_justifiable: is_justifiable(
                c,
                script,
                x_advance,
                Adjustability::default().stretchability,
            ),
            script,
        });
    };
    if ctx.dir.is_positive() {
        text.char_indices().for_each(add_glyph);
    } else {
        text.char_indices().rev().for_each(add_glyph);
    }
}

fn track_and_space(ctx: &mut ShapingContext) {
    let tracking = Em::from_abs(ctx.styles.resolve(TextElem::tracking), ctx.size);
    let spacing = ctx
        .styles
        .resolve(TextElem::spacing)
        .map(|abs| Em::from_abs(abs, ctx.size));

    let mut glyphs = ctx.glyphs.iter_mut().peekable();
    while let Some(glyph) = glyphs.next() {
        if glyph.c == '\u{00A0}' {
            glyph.x_advance -= nbsp_delta(&glyph.font).unwrap_or_default();
        }

        if glyph.is_space() {
            glyph.x_advance = spacing.relative_to(glyph.x_advance);
        }

        if glyphs
            .peek()
            .is_some_and(|next| glyph.range.start != next.range.start)
        {
            glyph.x_advance += tracking;
        }
    }
}

fn calculate_adjustability(ctx: &mut ShapingContext, lang: Lang, region: Option<Region>) {
    let style = cjk_punct_style(lang, region);
    let limits = ctx.styles.get(ParElem::justification_limits);
    let font_size = ctx.size;

    let mut glyphs = ctx.glyphs.iter_mut().peekable();
    while let Some(glyph) = glyphs.next() {
        let stretchable = glyphs
            .peek()
            .is_none_or(|next| glyph.range.start != next.range.start);

        glyph.adjustability = glyph.base_adjustability(style, &limits, font_size, stretchable);
    }

    let mut glyphs = ctx.glyphs.iter_mut().peekable();
    while let Some(glyph) = glyphs.next() {
        if glyph.is_cjk_punctuation() && matches!(style, CjkPunctStyle::Cns) {
            continue;
        }

        let Some(next) = glyphs.peek_mut() else {
            continue;
        };
        let width = glyph.x_advance;
        let delta = width / 2.0;
        if glyph.is_cjk_punctuation()
            && next.is_cjk_punctuation()
            && (glyph.shrinkability().1 + next.shrinkability().0) >= delta
        {
            let left_delta = glyph.shrinkability().1.min(delta);
            glyph.shrink_right(left_delta);
            next.shrink_left(delta - left_delta);
        }
    }
}

fn nbsp_delta(font: &Font) -> Option<Em> {
    let space = font.ttf().glyph_index(' ')?.0;
    let nbsp = font.ttf().glyph_index('\u{00A0}')?.0;
    Some(font.x_advance(nbsp)? - font.x_advance(space)?)
}

#[cfg(debug_assertions)]
fn assert_all_glyphs_in_range(glyphs: &[ShapedGlyph], text: &str, range: Range) {
    if glyphs
        .iter()
        .any(|g| g.range.start < range.start || g.range.end > range.end)
    {
        panic!("one or more glyphs in {text:?} fell out of range");
    }
}

#[cfg(debug_assertions)]
fn assert_glyph_ranges_in_order(glyphs: &[ShapedGlyph], dir: Dir) {
    if glyphs.is_empty() {
        return;
    }

    for i in 0..(glyphs.len() - 1) {
        let a = &glyphs[i];
        let b = &glyphs[i + 1];
        let ord = a.range.start.cmp(&b.range.start);
        let ord = if dir.is_positive() {
            ord
        } else {
            ord.reverse()
        };
        if ord == std::cmp::Ordering::Greater {
            panic!(
                "glyph ranges should be monotonically {}, \
                 but found glyphs out of order:\n\n\
                 first: {a:#?}\nsecond: {b:#?}",
                if dir.is_positive() {
                    "increasing"
                } else {
                    "decreasing"
                },
            );
        }
    }
}

pub const BEGIN_PUNCT_PAT: &[char] = &[
    '\u{201C}', '\u{2018}', '\u{300A}', '\u{3008}', '\u{FF08}', '\u{300E}', '\u{300C}',
    '\u{3010}', '\u{3016}', '\u{3014}', '\u{FF3B}', '\u{FF5B}',
];
pub const END_PUNCT_PAT: &[char] = &[
    '\u{201D}', '\u{2019}', '\u{FF0C}', '\u{FF0E}', '\u{3002}', '\u{3001}', '\u{FF1A}',
    '\u{FF1B}', '\u{300B}', '\u{3009}', '\u{FF09}', '\u{300F}', '\u{300D}', '\u{3011}',
    '\u{3017}', '\u{3015}', '\u{FF3D}', '\u{FF5D}', '\u{FF1F}', '\u{FF01}',
];

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CjkPunctStyle {
    Gb,
    Cns,
    Jis,
}

pub fn cjk_punct_style(lang: Lang, region: Option<Region>) -> CjkPunctStyle {
    match (lang, region.as_ref().map(Region::as_str)) {
        (Lang::CHINESE, Some("TW" | "HK")) => CjkPunctStyle::Cns,
        (Lang::JAPANESE, _) => CjkPunctStyle::Jis,
        _ => CjkPunctStyle::Gb,
    }
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\u{00A0}' | '\u{3000}')
}

pub fn is_of_cj_script(c: char) -> bool {
    is_cj_script(c, c.script())
}

fn is_cj_script(c: char, script: Script) -> bool {
    use Script::*;
    matches!(script, Hiragana | Katakana | Han) || c == '\u{30FC}'
}

fn is_cjk_left_aligned_punctuation(
    c: char,
    x_advance: Em,
    stretchability: (Em, Em),
    style: CjkPunctStyle,
) -> bool {
    use CjkPunctStyle::*;

    if matches!(c, '\u{201D}' | '\u{2019}') && x_advance + stretchability.1 == Em::one() {
        return true;
    }

    if matches!(style, Gb | Jis) && matches!(c, '\u{FF0C}' | '\u{3002}' | '\u{FF0E}' | '\u{3001}' | '\u{FF1A}' | '\u{FF1B}') {
        return true;
    }

    if matches!(style, Gb) && matches!(c, '\u{FF1F}' | '\u{FF01}') {
        return true;
    }

    matches!(
        c,
        '\u{300B}' | '\u{FF09}' | '\u{300F}' | '\u{300D}' | '\u{3011}' | '\u{3017}' | '\u{3015}' | '\u{3009}' | '\u{FF3D}' | '\u{FF5D}'
    )
}

fn is_cjk_right_aligned_punctuation(c: char, x_advance: Em, stretchability: (Em, Em)) -> bool {
    if matches!(c, '\u{201C}' | '\u{2018}') && x_advance + stretchability.0 == Em::one() {
        return true;
    }
    matches!(
        c,
        '\u{300A}' | '\u{FF08}' | '\u{300E}' | '\u{300C}' | '\u{3010}' | '\u{3016}' | '\u{3014}' | '\u{3008}' | '\u{FF3B}' | '\u{FF5B}'
    )
}

fn is_cjk_center_aligned_punctuation(c: char, style: CjkPunctStyle) -> bool {
    if matches!(style, CjkPunctStyle::Cns) && matches!(c, '\u{FF0C}' | '\u{3002}' | '\u{FF0E}' | '\u{3001}' | '\u{FF1A}' | '\u{FF1B}')
    {
        return true;
    }

    matches!(c, '\u{30FB}' | '\u{00B7}')
}

fn is_justifiable(c: char, script: Script, x_advance: Em, stretchability: (Em, Em)) -> bool {
    let style = CjkPunctStyle::Gb;
    is_space(c)
        || is_cj_script(c, script)
        || is_cjk_left_aligned_punctuation(c, x_advance, stretchability, style)
        || is_cjk_right_aligned_punctuation(c, x_advance, stretchability)
        || is_cjk_center_aligned_punctuation(c, style)
}
