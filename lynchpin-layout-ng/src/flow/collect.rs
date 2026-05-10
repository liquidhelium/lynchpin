//! Collect realized pairs into prepared [`Child`]ren for flow layout.
//!
//! Terminal equivalent of `lynchpin-layout/src/flow/collect.rs`.
//!
//! The collector walks the realized `Pair` stream and classifies each
//! element into one of the `Child` variants.  This pre-processing step
//! makes the downstream compose/distribute pipeline much simpler.

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Packed, Resolve, StyleChain};
use typst::introspection::{Locator, SplitLocator, Tag, TagElem};
use typst_utils::hash128;
use typst::layout::{
    Axes, FixedAlignment, Fr, PagebreakElem, PlaceElem, Spacing,
    VElem,
};
use typst::model::{EnumElem, HeadingElem, ListElem, ParElem, TermsElem};

use typst::layout::AlignElem;
use typst_utils::SliceExt;
use typst::routines::Pair;
use typst::text::{LinebreakElem, RawElem, RawLine, TextElem};

use lynchpin_library_ng::{
    Row, TermBlockBody, TermFragment, TermBlockElem, TermConfig, TermFrame, TermRegion, TermRegions,
    TermScalar, TermSize,
};

use super::block::{layout_single_block, layout_multi_block};

// ── Collector entry point ────────────────────────────────────────────────────

/// Collect realized pairs into prepared children.
pub fn collect<'a>(
    engine: &mut Engine<'_>,
    children: &[Pair<'a>],
    locator: Locator<'a>,
    base: TermSize,
    expand_x: bool,
    mode: super::FlowMode,
) -> SourceResult<Vec<Child<'a>>> {
    Collector {
        engine,
        children,
        base,
        expand_x,
        mode,
        locator: locator.split(),
        output: Vec::with_capacity(children.len()),
    }
    .run()
}

// ── Collector ────────────────────────────────────────────────────────────────

struct Collector<'a, 'x, 'y> {
    engine: &'x mut Engine<'y>,
    children: &'x [Pair<'a>],
    base: TermSize,
    expand_x: bool,
    mode: super::FlowMode,
    locator: SplitLocator<'a>,
    output: Vec<Child<'a>>,
}

impl<'a> Collector<'a, '_, '_> {
    fn run(mut self) -> SourceResult<Vec<Child<'a>>> {
        if matches!(self.mode, super::FlowMode::Inline) {
            return self.run_inline();
        }
        for &(child, styles) in self.children {
            if let Some(elem) = child.to_packed::<TagElem>() {
                self.output.push(Child::Tag(&elem.tag));
            } else if let Some(elem) = child.to_packed::<VElem>() {
                self.v(elem, styles);
            } else if let Some(elem) = child.to_packed::<ParElem>() {
                self.par(elem, styles)?;
            } else if let Some(elem) = child.to_packed::<TermBlockElem>() {
                self.block(elem, styles)?;
            } else if let Some(elem) = child.to_packed::<PlaceElem>() {
                self.place(elem, styles)?;
            } else if let Some(elem) = child.to_packed::<HeadingElem>() {
                self.heading(elem, styles)?;
            } else if let Some(elem) = child.to_packed::<ListElem>() {
                self.list(elem, styles)?;
            } else if let Some(elem) = child.to_packed::<EnumElem>() {
                self.enum_(elem, styles)?;
            } else if let Some(elem) = child.to_packed::<TermsElem>() {
                self.terms(elem, styles)?;
            } else if let Some(elem) = child.to_packed::<RawLine>() {
                self.raw_line(elem, styles)?;
            } else if let Some(elem) = child.to_packed::<RawElem>() {
                self.raw(elem, styles)?;
            } else if child.is::<LinebreakElem>() || child.is::<ParbreakElem>() {
                self.output.push(Child::Break(false));
            } else if let Some(elem) = child.to_packed::<PagebreakElem>() {
                self.output.push(Child::Break(!elem.weak.get(styles)));
            } else {
                // Unknown → warn and skip.
                self.engine.sink.warn(typst::__warning!(
                    child.span(),
                    "{} was ignored during terminal layout",
                    child.elem().name()
                ));
            }
        }
        Ok(self.output)
    }

    fn v(&mut self, elem: &'a Packed<VElem>, styles: StyleChain<'a>) {
        match elem.amount {
            Spacing::Rel(rel) => {
                let cells = lynchpin_library_ng::units::rel_to_cols(&rel, styles) as Row;
                self.output.push(Child::Rel(cells, elem.weak.get(styles)));
            }
            Spacing::Fr(fr) => {
                self.output.push(Child::Fr(fr));
            }
        }
    }

    fn run_inline(mut self) -> SourceResult<Vec<Child<'a>>> {
        let (start, end) = self.children.split_prefix_suffix(|(c, _)| c.is::<TagElem>());
        let inner = &self.children[start..end];
        let styles = StyleChain::trunk_from_pairs(inner).unwrap_or_default();

        let frames = crate::inline::layout_inline(
            self.engine, inner, &mut self.locator, styles, self.base, self.expand_x,
        )?;

        for (c, _) in &self.children[..start] {
            let elem = c.to_packed::<TagElem>().unwrap();
            self.output.push(Child::Tag(&elem.tag));
        }

        let leading = styles.resolve(ParElem::leading);
        self.lines(frames, leading, styles);

        for (c, _) in &self.children[end..] {
            let elem = c.to_packed::<TagElem>().unwrap();
            self.output.push(Child::Tag(&elem.tag));
        }
        Ok(self.output)
    }

    fn lines(&mut self, lines: TermFragment, leading: typst::layout::Abs, styles: StyleChain<'a>) {
        let align = styles.resolve(AlignElem::alignment);
        let costs = styles.get(TextElem::costs);
        let len = lines.len();
        let prevent_orphans = costs.orphan() > typst::layout::Ratio::zero() && len >= 2 && !lines[1].size().is_empty();
        let prevent_widows = costs.widow() > typst::layout::Ratio::zero() && len >= 2 && !lines[len - 2].size().is_empty();
        let prevent_all = len == 3 && prevent_orphans && prevent_widows;
        let height_at = |i| lines.get(i).map(|f: &TermFrame| f.rows()).unwrap_or(TermScalar::ZERO);
        let front_1 = height_at(0);
        let front_2 = height_at(1);
        let back_2 = height_at(len.saturating_sub(2));
        let back_1 = height_at(len.saturating_sub(1));

        for (i, frame) in lines.into_iter().enumerate() {
            let need = if prevent_all && i == 0 {
                front_1 + front_2 + back_1
            } else if prevent_orphans && i == 0 {
                front_1 + front_2
            } else if prevent_widows && i >= 2 && i + 2 == len {
                back_2 + back_1
            } else {
                frame.rows()
            };
            self.output.push(Child::Line(LineChild { frame, align, need }));
        }
    }

    fn par(
        &mut self,
        elem: &'a Packed<ParElem>,
        styles: StyleChain<'a>,
    ) -> SourceResult<()> {
        use crossterm::style::ContentStyle;
        let frame = crate::inline::layout_par(
            elem,
            self.engine,
            &TermConfig::default(),
            styles,
            ContentStyle::default(),
            Some(crate::inline::ParSituation::Consecutive),
        )?;
        let need = frame.rows();
        let align = Axes::new(
            FixedAlignment::Start.into(),
            FixedAlignment::Start.into(),
        );
        self.output.push(Child::Line(LineChild { frame, align, need }));
        Ok(())
    }

    fn block(
        &mut self,
        elem: &'a Packed<TermBlockElem>,
        styles: StyleChain<'a>,
    ) -> SourceResult<()> {
        let loc = self.locator.next(&elem.span());
        self.output.push(Child::Single(SingleChild {
            elem,
            styles,
            locator: loc,
            align: Axes::new(
                FixedAlignment::Start.into(),
                FixedAlignment::Start.into(),
            ),
            sticky: false,
            alone: false,
            fr: None,
        }));
        Ok(())
    }

    fn place(
        &mut self,
        elem: &'a Packed<PlaceElem>,
        styles: StyleChain<'a>,
    ) -> SourceResult<()> {
        use typst::foundations::Smart;

        let (ax, ay) = match elem.alignment.get(styles) {
            Smart::Custom(a) => {
                let x = a.x().map(|x| x.resolve(styles));
                let y = a.y().map(|y| y.resolve(styles));
                (x, y)
            }
            _ => (None, None),
        };

        let float = elem.float.get(styles);
        let clearance = {
            let abs = elem.clearance.resolve(styles);
            let font_size = styles.get(TextElem::size).0.resolve(styles);
            lynchpin_library_ng::units::abs_to_cols(abs, font_size)
        };

        self.output.push(Child::Placed(PlacedChild {
            align_x: ax,
            align_y: ay,
            scope: elem.scope.get(styles),
            float,
            clearance,
            delta: (TermScalar::ZERO, TermScalar::ZERO),
            elem,
            styles,
            location: self.locator.next_location(self.engine.introspector, hash128(&elem.span())),
            alignment: elem.alignment.get(styles),
        }));
        Ok(())
    }

    fn heading(
        &mut self,
        elem: &'a Packed<HeadingElem>,
        styles: StyleChain<'a>,
    ) -> SourceResult<()> {
        use crossterm::style::{Attribute, Color, ContentStyle};
        let level = elem.resolve_level(styles).get() as usize;
        let mut style = ContentStyle::default();
        style.attributes.set(Attribute::Bold);
        style.foreground_color = Some(match level {
            1 => Color::Yellow,
            2 => Color::Cyan,
            3 => Color::Green,
            _ => Color::Blue,
        });
        let frame = crate::inline::layout_paragraph(
            self.engine,
            &elem.body,
            &TermConfig::default(),
            styles,
            style,
            Some(crate::inline::ParSituation::First),
            self.base.cols,
        )?;
        let need = frame.rows();
        let align = Axes::new(
            FixedAlignment::Start.into(),
            FixedAlignment::Start.into(),
        );
        self.output.push(Child::Line(LineChild { frame, align, need }));
        Ok(())
    }

    fn list(
        &mut self,
        _elem: &'a Packed<ListElem>,
        _styles: StyleChain<'a>,
    ) -> SourceResult<()> {
        let frame = TermFrame::new(TermSize::new(self.base.cols, TermScalar::new(1)));
        let need = frame.rows();
        let align = Axes::new(
            FixedAlignment::Start.into(),
            FixedAlignment::Start.into(),
        );
        self.output.push(Child::Line(LineChild { frame, align, need }));
        Ok(())
    }

    fn enum_(
        &mut self,
        _elem: &'a Packed<EnumElem>,
        _styles: StyleChain<'a>,
    ) -> SourceResult<()> {
        let frame = TermFrame::new(TermSize::new(self.base.cols, TermScalar::new(1)));
        let need = frame.rows();
        let align = Axes::new(
            FixedAlignment::Start.into(),
            FixedAlignment::Start.into(),
        );
        self.output.push(Child::Line(LineChild { frame, align, need }));
        Ok(())
    }

    fn terms(
        &mut self,
        _elem: &'a Packed<TermsElem>,
        _styles: StyleChain<'a>,
    ) -> SourceResult<()> {
        let frame = TermFrame::new(TermSize::new(self.base.cols, TermScalar::new(1)));
        let need = frame.rows();
        let align = Axes::new(
            FixedAlignment::Start.into(),
            FixedAlignment::Start.into(),
        );
        self.output.push(Child::Line(LineChild { frame, align, need }));
        Ok(())
    }

    fn raw_line(
        &mut self,
        _elem: &'a Packed<RawLine>,
        _styles: StyleChain<'a>,
    ) -> SourceResult<()> {
        let frame = TermFrame::new(TermSize::new(self.base.cols, TermScalar::new(1)));
        let need = frame.rows();
        let align = Axes::new(
            FixedAlignment::Start.into(),
            FixedAlignment::Start.into(),
        );
        self.output.push(Child::Line(LineChild { frame, align, need }));
        Ok(())
    }

    fn raw(
        &mut self,
        _elem: &'a Packed<RawElem>,
        _styles: StyleChain<'a>,
    ) -> SourceResult<()> {
        let frame = TermFrame::new(TermSize::new(self.base.cols, TermScalar::new(1)));
        let need = frame.rows();
        let align = Axes::new(
            FixedAlignment::Start.into(),
            FixedAlignment::Start.into(),
        );
        self.output.push(Child::Line(LineChild { frame, align, need }));
        Ok(())
    }
}

// ── Typst re-exports for convenience ─────────────────────────────────────────

use typst::model::ParbreakElem;

// ── Child enum ───────────────────────────────────────────────────────────────

/// A prepared child ready for distribution into regions.
#[derive(Clone)]
pub enum Child<'a> {
    /// An introspection tag.
    Tag(&'a Tag),
    /// Relative spacing with a weakness level.
    Rel(Row, bool),
    /// Fractional spacing.
    Fr(Fr),
    /// An already laid-out line of a paragraph.
    Line(LineChild),
    /// An unbreakable block.
    Single(SingleChild<'a>),
    /// A breakable block.
    Multi(MultiChild<'a>),
    /// An absolutely or floatingly placed element.
    Placed(PlacedChild<'a>),
    /// A place flush event.
    Flush,
    /// An explicit column / page break.
    Break(bool),
}

// ── LineChild ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct LineChild {
    pub frame: TermFrame,
    pub align: Axes<FixedAlignment>,
    pub need: Row,
}

// ── SingleChild ──────────────────────────────────────────────────────────────

pub struct SingleChild<'a> {
    pub elem: &'a Packed<TermBlockElem>,
    pub styles: StyleChain<'a>,
    pub locator: Locator<'a>,
    pub align: Axes<FixedAlignment>,
    pub sticky: bool,
    pub alone: bool,
    pub fr: Option<Fr>,
}

impl Clone for SingleChild<'_> {
    fn clone(&self) -> Self {
        Self {
            elem: self.elem,
            styles: self.styles,
            locator: self.locator.relayout(),
            align: self.align,
            sticky: self.sticky,
            alone: self.alone,
            fr: self.fr,
        }
    }
}

impl SingleChild<'_> {
    /// Layout the single child into a frame.
    pub fn layout(
        &self,
        engine: &mut Engine,
        region: TermRegion,
    ) -> SourceResult<TermFrame> {
        layout_single_block(self.elem, engine, self.locator.relayout(), self.styles, region)
    }
}

// ── MultiChild ───────────────────────────────────────────────────────────────

pub struct MultiChild<'a> {
    pub elem: &'a Packed<TermBlockElem>,
    pub styles: StyleChain<'a>,
    pub locator: Locator<'a>,
    pub align: Axes<FixedAlignment>,
    pub sticky: bool,
}

impl Clone for MultiChild<'_> {
    fn clone(&self) -> Self {
        Self {
            elem: self.elem,
            styles: self.styles,
            locator: self.locator.relayout(),
            align: self.align,
            sticky: self.sticky,
        }
    }
}

impl MultiChild<'_> {
    /// Layout the full content (first region).
    pub fn layout(
        &self,
        engine: &mut Engine,
        region: TermRegion,
    ) -> SourceResult<TermFrame> {
        let regions = TermRegions::from(region);
        let fragment = layout_multi_block(self.elem, engine, self.locator.relayout(), self.styles, regions)?;
        Ok(fragment.into_iter().next().unwrap_or(TermFrame::new(TermSize::ZERO)))
    }
}

// ── MultiSpill ───────────────────────────────────────────────────────────────

/// Leftover state from a partially laid-out breakable block.
#[derive(Clone)]
pub struct MultiSpill<'a, 'b> {
    /// Whether a non-empty frame was already produced.
    pub exist_non_empty_frame: bool,
    /// The multi child being spilled.
    multi: &'b MultiChild<'a>,
    /// Height of the first region.
    first: Row,
    /// Full height.
    full: Row,
    /// Remaining backlog heights.
    backlog: Vec<Row>,
    /// Minimum backlog length.
    min_backlog_len: usize,
}

impl<'a, 'b> MultiSpill<'a, 'b> {
    /// Layout the spill, producing one frame per remaining region.
    pub fn layout(
        &self,
        engine: &mut Engine,
        region: TermRegion,
    ) -> SourceResult<Vec<TermFrame>> {
        // In terminal layout, multi spills are simplified:
        // we just re-layout the block and return whatever fits.
        let frame = self.multi.layout(engine, region)?;
        Ok(vec![frame])
    }

    /// The alignment of the multi child.
    pub fn align(&self) -> Axes<FixedAlignment> {
        self.multi.align
    }
}

// ── PlacedChild ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PlacedChild<'a> {
    pub align_x: Option<FixedAlignment>,
    pub align_y: Option<FixedAlignment>,
    pub scope: typst::layout::PlacementScope,
    pub float: bool,
    pub clearance: TermScalar,
    pub delta: (TermScalar, TermScalar),
    elem: &'a Packed<PlaceElem>,
    styles: StyleChain<'a>,
    location: typst::introspection::Location,
    alignment: typst::foundations::Smart<typst::layout::Alignment>,
}

impl PlacedChild<'_> {
    /// Layout the placed child into a frame.
    pub fn layout(
        &self,
        engine: &mut Engine,
        config: &TermConfig,
    ) -> SourceResult<TermFrame> {
        // TODO: PlacedChild should store a TermBlockElem and dispatch through
        // layout_single_block properly.
        let _ = (engine, config);
        Ok(TermFrame::new(TermSize::ZERO))
    }

    /// The location of this placed child.
    pub fn location(&self) -> typst::introspection::Location {
        self.location
    }
}
