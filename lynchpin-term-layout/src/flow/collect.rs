//! Collect realized pairs into prepared [`Child`]ren.
//!
//! Mirrors `lynchpin-layout/src/flow/collect.rs`.

use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Packed, Resolve, Smart, StyleChain};
use typst::introspection::{Locator, SplitLocator, Tag, TagElem};
use typst::layout::{Axes, FixedAlignment, Fr, PlaceElem, VElem};
use typst::model::{EnumElem, HeadingElem, ListElem, ParElem, TermsElem};
use typst::routines::Pair;
use typst::text::{LinebreakElem, RawElem, RawLine, SpaceElem};
use typst::layout::HElem;
use typst::model::ParbreakElem;

use lynchpin_library::frame::{Row, TermFrame};
use lynchpin_library::TermBlockElem;

use crate::config::TermConfig;

// ── Child enum ───────────────────────────────────────────────────────────────

/// A prepared child ready for distribution into regions.
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
    /// A place flush.
    Flush,
    /// An explicit column break.
    Break(bool),
}

// ── LineChild ────────────────────────────────────────────────────────────────

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
    pub fr: Option<Fr>,
}

impl SingleChild<'_> {
    pub fn layout(
        &self,
        engine: &mut Engine,
        config: &TermConfig,
    ) -> SourceResult<TermFrame> {
        self.elem.cb.call(engine, config, self.styles)
    }
}

// ── MultiChild ───────────────────────────────────────────────────────────────

pub struct MultiChild<'a> {
    pub elem: &'a Packed<TermBlockElem>,
    pub styles: StyleChain<'a>,
    pub locator: Locator<'a>,
    pub align: Axes<FixedAlignment>,
    pub breakable: bool,
}

impl MultiChild<'_> {
    /// Layout the full content (first region).
    pub fn layout(
        &self,
        engine: &mut Engine,
        config: &TermConfig,
    ) -> SourceResult<TermFrame> {
        self.elem.cb.call(engine, config, self.styles)
    }
}

// ── PlacedChild ──────────────────────────────────────────────────────────────

pub struct PlacedChild<'a> {
    pub align_x: Option<FixedAlignment>,
    pub align_y: Option<FixedAlignment>,
    pub elem: &'a Packed<PlaceElem>,
    pub styles: StyleChain<'a>,
}

impl PlacedChild<'_> {
    pub fn layout(
        &self,
        engine: &mut Engine,
        config: &TermConfig,
    ) -> SourceResult<TermFrame> {
        super::layout_block(engine, &self.elem.body, config, self.styles)
    }
}

// ── Collector ────────────────────────────────────────────────────────────────

struct Collector<'a, 'x, 'y> {
    engine: &'x mut Engine<'y>,
    children: &'x [Pair<'a>],
    config: &'x TermConfig,
    locator: SplitLocator<'a>,
    output: Vec<Child<'a>>,
}

/// Collect realized pairs into prepared children.
pub fn collect<'a>(
    engine: &mut Engine<'_>,
    children: &[Pair<'a>],
    config: &TermConfig,
    locator: Locator<'a>,
) -> SourceResult<Vec<Child<'a>>> {
    Collector {
        engine,
        children,
        config,
        locator: locator.split(),
        output: Vec::with_capacity(children.len()),
    }
    .run()
}

impl<'a> Collector<'a, '_, '_> {
    fn run(mut self) -> SourceResult<Vec<Child<'a>>> {
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
                // Break.
                self.output.push(Child::Break(false));
            } else if child.is::<SpaceElem>() || child.is::<HElem>() {
                // Skip.
            } else {
                // Unknown → warn.
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
        use typst::layout::Spacing;
        self.output.push(match elem.amount {
            Spacing::Rel(rel) => {
                let cells = lynchpin_library::units::rel_to_cols(&rel, styles) as Row;
                Child::Rel(cells, elem.weak.get(styles))
            }
            Spacing::Fr(fr) => Child::Fr(fr),
        });
    }

    fn par(&mut self, elem: &'a Packed<ParElem>, styles: StyleChain<'a>) -> SourceResult<()> {
        use crossterm::style::ContentStyle;
        let frame = crate::inline::layout_paragraph(
            self.engine, &elem.body, self.config, styles, ContentStyle::default(),
        )?;
        let need = frame.rows();
        let align = Axes::new(FixedAlignment::Start, FixedAlignment::Start);
        self.output.push(Child::Line(LineChild {
            frame,
            align,
            need,
        }));
        Ok(())
    }

    fn block(&mut self, elem: &'a Packed<TermBlockElem>, styles: StyleChain<'a>) -> SourceResult<()> {
        let loc = self.locator.next(&elem.span());
        self.output.push(Child::Single(SingleChild {
            elem,
            styles,
            locator: loc,
            align: Axes::new(FixedAlignment::Start, FixedAlignment::Start),
            fr: None,
        }));
        Ok(())
    }

    fn place(&mut self, elem: &'a Packed<PlaceElem>, styles: StyleChain<'a>) -> SourceResult<()> {
        let (ax, ay) = match elem.alignment.get(styles) {
            Smart::Custom(a) => (
                a.x().map(|x| x.resolve(styles)),
                a.y().map(|y| y.resolve(styles)),
            ),
            _ => (None, None),
        };

        self.output.push(Child::Placed(PlacedChild {
            align_x: ax,
            align_y: ay,
            elem,
            styles,
        }));
        Ok(())
    }

    fn heading(&mut self, elem: &'a Packed<HeadingElem>, styles: StyleChain<'a>) -> SourceResult<()> {
        use crossterm::style::{Attribute, Color, ContentStyle};
        let level = elem.resolve_level(styles).get() as usize;
        let mut style = ContentStyle::default();
        style.attributes.set(Attribute::Bold);
        style.foreground_color = Some(match level {
            1 => Color::Yellow, 2 => Color::Cyan, 3 => Color::Green, _ => Color::Blue,
        });
        let frame = crate::inline::layout_paragraph(
            self.engine, &elem.body, self.config, styles, style,
        )?;
        self.output.push(Child::Line(LineChild {
            need: frame.rows(),
            frame,
            align: Axes::new(FixedAlignment::Start, FixedAlignment::Start),
        }));
        Ok(())
    }

    fn list(&mut self, elem: &'a Packed<ListElem>, styles: StyleChain<'a>) -> SourceResult<()> {
        let frame = crate::lists::render_list(self.engine, elem, self.config, styles)?;
        if !frame.size().is_empty() {
            self.output.push(Child::Line(LineChild {
                need: frame.rows(),
                frame,
                align: Axes::new(FixedAlignment::Start, FixedAlignment::Start),
            }));
        }
        Ok(())
    }

    fn enum_(&mut self, elem: &'a Packed<EnumElem>, styles: StyleChain<'a>) -> SourceResult<()> {
        let frame = crate::lists::render_enum(self.engine, elem, self.config, styles)?;
        if !frame.size().is_empty() {
            self.output.push(Child::Line(LineChild {
                need: frame.rows(),
                frame,
                align: Axes::new(FixedAlignment::Start, FixedAlignment::Start),
            }));
        }
        Ok(())
    }

    fn terms(&mut self, elem: &'a Packed<TermsElem>, styles: StyleChain<'a>) -> SourceResult<()> {
        let frame = crate::lists::render_terms(self.engine, elem, self.config, styles)?;
        if !frame.size().is_empty() {
            self.output.push(Child::Line(LineChild {
                need: frame.rows(),
                frame,
                align: Axes::new(FixedAlignment::Start, FixedAlignment::Start),
            }));
        }
        Ok(())
    }

    fn raw_line(&mut self, elem: &'a Packed<RawLine>, styles: StyleChain<'a>) -> SourceResult<()> {
        use crossterm::style::{Color, ContentStyle};
        let mut style = ContentStyle::default();
        style.foreground_color = Some(Color::DarkGrey);
        let frame = crate::inline::layout_paragraph(
            self.engine, &elem.body, self.config, styles, style,
        )?;
        self.output.push(Child::Line(LineChild {
            need: frame.rows(),
            frame,
            align: Axes::new(FixedAlignment::Start, FixedAlignment::Start),
        }));
        Ok(())
    }

    fn raw(&mut self, elem: &'a Packed<RawElem>, styles: StyleChain<'a>) -> SourceResult<()> {
        use crossterm::style::{Color, ContentStyle};
        let lines = elem.lines.as_deref().unwrap_or_default();
        let mut line_frames = Vec::new();
        let mut style = ContentStyle::default();
        style.foreground_color = Some(Color::DarkGrey);
        for line in lines {
            let f = crate::inline::layout_paragraph(
                self.engine, &line.body, self.config, styles, style,
            )?;
            if !f.size().is_empty() {
                line_frames.push(f);
            }
        }
        if !line_frames.is_empty() {
            let frame = crate::stack::compose_vertical(line_frames, 0, 0);
            self.output.push(Child::Line(LineChild {
                need: frame.rows(),
                frame,
                align: Axes::new(FixedAlignment::Start, FixedAlignment::Start),
            }));
        }
        Ok(())
    }
}
