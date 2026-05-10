//! Terminal-specific show rules.
//!
//! These rules mirror the paged rules in `lynchpin-layout/src/rules.rs`,
//! but produce [`TermBlockElem`] with terminal-specific callbacks instead
//! of `BlockElem::multi_layouter` / `BlockElem::single_layouter`.
//!
//! Each rule wraps the original element in a [`TermBlockElem`] whose
//! callback calls the corresponding terminal layout function.

use comemo::Track;
use typst::foundations::{Content, NativeElement, NativeRuleMap, ShowFn};
use typst::layout::{
    AlignElem, ColumnsElem, GridCell, GridElem, HideElem, LayoutElem, MoveElem, PadElem,
    RepeatElem, RotateElem, ScaleElem, SkewElem, StackElem,
};
use typst::math::EquationElem;
use typst::model::{EmphElem, StrongElem};
use typst::model::{
    EnumElem, FigureCaption, FigureElem, FootnoteElem, FootnoteEntry, HeadingElem, ListElem,
    QuoteElem, RefElem, TableCell, TableElem, TermsElem,
};
use typst::text::{
    HighlightElem, ItalicToggle, LinebreakElem, OverlineElem, RawElem, RawLine, ScriptKind,
    ShiftSettings, SmallcapsElem, StrikeElem, SubElem, SuperElem, TextElem, TextSize,
    UnderlineElem, WeightDelta,
};
use typst::visualize::{
    CircleElem, CurveElem, EllipseElem, ImageElem, LineElem, PathElem, PolygonElem, RectElem,
    SquareElem,
};

use lynchpin_library_ng::{
    Col, Row, TermBlockBody, TermBlockCallback, TermBlockElem, TermConfig, TermFrame,
    TermInlineCallback, TermInlineElem, TermInlineItem, TermPoint, TermRegion, TermRegions,
    TermScalar, TermSize,
};

/// Register terminal show rules into `rules`.
pub fn register(rules: &mut NativeRuleMap) {
    use typst::foundations::Target;

    // ── Model ────────────────────────────────────────────────────────────
    rules.register(Target::Paged, HEADING_RULE);
    rules.register(Target::Paged, LIST_RULE);
    rules.register(Target::Paged, ENUM_RULE);
    rules.register(Target::Paged, TERMS_RULE);
    rules.register(Target::Paged, FIGURE_RULE);
    rules.register(Target::Paged, FIGURE_CAPTION_RULE);
    rules.register(Target::Paged, QUOTE_RULE);
    rules.register(Target::Paged, FOOTNOTE_RULE);
    rules.register(Target::Paged, FOOTNOTE_ENTRY_RULE);
    rules.register(Target::Paged, REF_RULE);
    rules.register(Target::Paged, TABLE_RULE);
    rules.register(Target::Paged, TABLE_CELL_RULE);

    // ── Text ─────────────────────────────────────────────────────────────
    rules.register(Target::Paged, STRONG_RULE);
    rules.register(Target::Paged, EMPH_RULE);
    rules.register(Target::Paged, SUB_RULE);
    rules.register(Target::Paged, SUPER_RULE);
    rules.register(Target::Paged, UNDERLINE_RULE);
    rules.register(Target::Paged, OVERLINE_RULE);
    rules.register(Target::Paged, STRIKE_RULE);
    rules.register(Target::Paged, HIGHLIGHT_RULE);
    rules.register(Target::Paged, SMALLCAPS_RULE);
    rules.register(Target::Paged, RAW_RULE);
    rules.register(Target::Paged, RAW_LINE_RULE);

    // ── Layout ───────────────────────────────────────────────────────────
    rules.register(Target::Paged, ALIGN_RULE);
    rules.register(Target::Paged, PAD_RULE);
    rules.register(Target::Paged, COLUMNS_RULE);
    rules.register(Target::Paged, STACK_RULE);
    rules.register(Target::Paged, GRID_RULE);
    rules.register(Target::Paged, GRID_CELL_RULE);
    rules.register(Target::Paged, MOVE_RULE);
    rules.register(Target::Paged, SCALE_RULE);
    rules.register(Target::Paged, ROTATE_RULE);
    rules.register(Target::Paged, SKEW_RULE);
    rules.register(Target::Paged, REPEAT_RULE);
    rules.register(Target::Paged, HIDE_RULE);
    rules.register(Target::Paged, LAYOUT_RULE);

    // ── Visualize ────────────────────────────────────────────────────────
    rules.register(Target::Paged, IMAGE_RULE);
    rules.register(Target::Paged, LINE_RULE);
    rules.register(Target::Paged, RECT_RULE);
    rules.register(Target::Paged, SQUARE_RULE);
    rules.register(Target::Paged, ELLIPSE_RULE);
    rules.register(Target::Paged, CIRCLE_RULE);
    rules.register(Target::Paged, POLYGON_RULE);
    rules.register(Target::Paged, CURVE_RULE);
    rules.register(Target::Paged, PATH_RULE);

    // ── Math ─────────────────────────────────────────────────────────────
    rules.register(Target::Paged, EQUATION_RULE);
}

// ── Inline text rules (mirror paged) ────────────────────────────────────────

const STRONG_RULE: ShowFn<StrongElem> = |elem, _, styles| {
    Ok(elem
        .body
        .clone()
        .set(TextElem::delta, WeightDelta(elem.delta.get(styles))))
};

const EMPH_RULE: ShowFn<EmphElem> =
    |elem, _, _| Ok(elem.body.clone().set(TextElem::emph, ItalicToggle(true)));

// ── Model rules ──────────────────────────────────────────────────────────────

const HEADING_RULE: ShowFn<HeadingElem> = |elem, engine, styles| {
    let mut realized = elem.body.clone();

    // Handle numbering if present.
    if let Some(numbering) = elem.numbering.get_ref(styles) {
        if let Some(location) = elem.location() {
            let numbering = typst::introspection::Counter::of(HeadingElem::ELEM)
                .display_at_loc(engine, location, styles, numbering)?
                .spanned(elem.span());
            let spacing = typst::layout::HElem::new(typst::layout::Em::new(0.3).into())
                .with_weak(true)
                .pack();
            realized = numbering + spacing + realized;
        }
    }

    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::Content(realized)))
        .pack()
        .spanned(elem.span()))
};

const LIST_RULE: ShowFn<ListElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::lists::layout_list(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const ENUM_RULE: ShowFn<EnumElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::lists::layout_enum(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const TERMS_RULE: ShowFn<TermsElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::lists::layout_terms(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const FIGURE_RULE: ShowFn<FigureElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                // Figures: layout the body (with optional caption).
                crate::flow::layout_term_frame(engine, &elem.body, locator, styles, region.into())
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const FIGURE_CAPTION_RULE: ShowFn<FigureCaption> = |elem, engine, styles| {
    let realized = elem.realize(engine, styles)?;
    let page_width = lynchpin_library_ng::resolve_page_size(styles).cols;
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            move |_elem, eng, locator, st, region| {
                crate::flow::layout_term_frame(eng, &realized, locator, st, region)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const QUOTE_RULE: ShowFn<QuoteElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                crate::flow::layout_term_frame(engine, &elem.body, locator, styles, region)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const FOOTNOTE_RULE: ShowFn<FootnoteElem> = |elem, engine, styles| {
    let (_, num) = elem.realize(engine, styles)?;
    let sup = format!("[^{}]", num.plain_text());
    let sup_cols = TermScalar::new(sup.len() as i32);
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            move |_elem, _eng, _locator, _st, _region| {
                Ok(TermFrame::text(
                    sup.clone(),
                    Default::default(),
                    sup_cols,
                    TermScalar::ONE,
                ))
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const FOOTNOTE_ENTRY_RULE: ShowFn<FootnoteEntry> = |elem, engine, styles| {
    let (prefix, body) = elem.realize(engine, styles)?;
    let pw = lynchpin_library_ng::resolve_page_size(styles).cols;
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            move |_elem, eng, locator, st, region| {
                let prefix_frame =
                    crate::flow::layout_term_frame(eng, &prefix, locator.relayout(), st, region)?;
                let body_frame = crate::flow::layout_term_frame(eng, &body, locator, st, region)?;
                // Compose prefix + body horizontally.
                let prefix_cols = prefix_frame.size().cols;
                let total_cols = prefix_cols + body_frame.size().cols;
                let total_rows = prefix_frame
                    .size()
                    .rows
                    .max(body_frame.size().rows)
                    .max(TermScalar::ONE);
                let mut out = TermFrame::new(TermSize::new(total_cols, total_rows));
                out.push_frame(TermPoint::ZERO, prefix_frame);
                out.push_frame(TermPoint::new(prefix_cols, TermScalar::ZERO), body_frame);
                Ok(out)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const REF_RULE: ShowFn<RefElem> = |elem, engine, styles| {
    let realized = elem.realize(engine, styles)?;
    let pw = lynchpin_library_ng::resolve_page_size(styles).cols;
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            move |_elem, eng, locator, st, region| {
                crate::flow::layout_term_frame(eng, &realized, locator, st, region)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const TABLE_RULE: ShowFn<TableElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let regions = TermRegions::from(region);
                let fragment = crate::grid::layout_table(elem, engine, locator, styles, regions)?;
                // Compose all frames in the fragment into one.
                let frames = lynchpin_library_ng::fragment_into_frames(fragment);
                Ok(compose_frames(frames))
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const TABLE_CELL_RULE: ShowFn<TableCell> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                crate::flow::layout_term_frame(engine, &elem.body, locator, styles, region)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

// ── Text rules ───────────────────────────────────────────────────────────────

const SUB_RULE: ShowFn<SubElem> = |elem, _, styles| {
    use typst::layout::{Em, Length};
    let font_size = styles.resolve(TextElem::size);
    Ok(elem.body.clone().set(
        TextElem::shift_settings,
        Some(ShiftSettings {
            typographic: elem.typographic.get(styles),
            shift: elem
                .baseline
                .get(styles)
                .map(|l| -Em::from_length(l, font_size)),
            size: elem
                .size
                .get(styles)
                .map(|t| Em::from_length(t.0, font_size)),
            kind: ScriptKind::Sub,
        }),
    ))
};

const SUPER_RULE: ShowFn<SuperElem> = |elem, _, styles| {
    use typst::layout::{Em, Length};
    let font_size = styles.resolve(TextElem::size);
    Ok(elem.body.clone().set(
        TextElem::shift_settings,
        Some(ShiftSettings {
            typographic: elem.typographic.get(styles),
            shift: elem
                .baseline
                .get(styles)
                .map(|l| -Em::from_length(l, font_size)),
            size: elem
                .size
                .get(styles)
                .map(|t| Em::from_length(t.0, font_size)),
            kind: ScriptKind::Super,
        }),
    ))
};

const UNDERLINE_RULE: ShowFn<UnderlineElem> = |elem, _, styles| {
    Ok(elem.body.clone().set(
        TextElem::deco,
        smallvec::smallvec![typst::text::Decoration {
            line: typst::text::DecoLine::Underline {
                stroke: Default::default(),
                offset: elem.offset.resolve(styles),
                evade: elem.evade.get(styles),
                background: elem.background.get(styles),
            },
            extent: elem.extent.resolve(styles),
        }],
    ))
};

const OVERLINE_RULE: ShowFn<OverlineElem> = |elem, _, styles| {
    Ok(elem.body.clone().set(
        TextElem::deco,
        smallvec::smallvec![typst::text::Decoration {
            line: typst::text::DecoLine::Overline {
                stroke: Default::default(),
                offset: elem.offset.resolve(styles),
                evade: elem.evade.get(styles),
                background: elem.background.get(styles),
            },
            extent: elem.extent.resolve(styles),
        }],
    ))
};

const STRIKE_RULE: ShowFn<StrikeElem> = |elem, _, styles| {
    Ok(elem.body.clone().set(
        TextElem::deco,
        smallvec::smallvec![typst::text::Decoration {
            line: typst::text::DecoLine::Strikethrough {
                stroke: Default::default(),
                offset: elem.offset.resolve(styles),
                background: elem.background.get(styles),
            },
            extent: elem.extent.resolve(styles),
        }],
    ))
};

const HIGHLIGHT_RULE: ShowFn<HighlightElem> = |elem, _, styles| {
    Ok(elem.body.clone().set(
        TextElem::deco,
        smallvec::smallvec![typst::text::Decoration {
            line: typst::text::DecoLine::Highlight {
                fill: elem.fill.get_cloned(styles),
                stroke: Default::default(),
                top_edge: elem.top_edge.get(styles),
                bottom_edge: elem.bottom_edge.get(styles),
                radius: elem.radius.resolve(styles).unwrap_or_default(),
            },
            extent: elem.extent.resolve(styles),
        }],
    ))
};

const SMALLCAPS_RULE: ShowFn<SmallcapsElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::modifiers::layout_smallcaps(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const RAW_RULE: ShowFn<RawElem> = |elem, _, styles| {
    let lines = elem.lines.as_deref().unwrap_or_default();

    let mut seq = typst::ecow::EcoVec::with_capacity((2 * lines.len()).saturating_sub(1));
    for (i, line) in lines.iter().enumerate() {
        if i != 0 {
            seq.push(LinebreakElem::shared().clone());
        }
        seq.push(line.clone().pack());
    }

    let mut realized = Content::sequence(seq);

    if elem.block.get(styles) {
        realized = realized.aligned(elem.align.get(styles).into());
        Ok(TermBlockElem::new()
            .with_body(Some(TermBlockBody::Content(realized)))
            .pack()
            .spanned(elem.span()))
    } else {
        Ok(realized)
    }
};
const RAW_LINE_RULE: ShowFn<RawLine> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                crate::flow::layout_term_frame(engine, &elem.body, locator, styles, region)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

// ── Layout rules ─────────────────────────────────────────────────────────────

const ALIGN_RULE: ShowFn<AlignElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                crate::flow::layout_term_frame(engine, &elem.body, locator, styles, region)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const PAD_RULE: ShowFn<PadElem> = |elem, _, _styles| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let mut fragment =
                    crate::flow::layout_term_frame(engine, &elem.body, locator, styles, region)?;

                // PadElem insets resolve to Rel<Abs>, exactly what grow() expects.
                let padding = typst::layout::Sides::new(
                    elem.left.resolve(styles),
                    elem.top.resolve(styles),
                    elem.right.resolve(styles),
                    elem.bottom.resolve(styles),
                );
                crate::pad::grow(&mut fragment, &padding, styles);

                Ok(fragment)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const COLUMNS_RULE: ShowFn<ColumnsElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                crate::flow::layout_term_frame(engine, &elem.body, locator, styles, region)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const STACK_RULE: ShowFn<StackElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::stack::layout_stack(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const GRID_RULE: ShowFn<GridElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let regions = TermRegions::from(region);
                let fragment = crate::grid::layout_grid(elem, engine, locator, styles, regions)?;
                let frames = lynchpin_library_ng::fragment_into_frames(fragment);
                Ok(compose_frames(frames))
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const GRID_CELL_RULE: ShowFn<GridCell> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                crate::flow::layout_term_frame(engine, &elem.body, locator, styles, region)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const MOVE_RULE: ShowFn<MoveElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::transforms::layout_move(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const SCALE_RULE: ShowFn<ScaleElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::transforms::layout_scale(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const ROTATE_RULE: ShowFn<RotateElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::transforms::layout_rotate(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const SKEW_RULE: ShowFn<SkewElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::transforms::layout_skew(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const REPEAT_RULE: ShowFn<RepeatElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::repeat::layout_repeat(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const HIDE_RULE: ShowFn<HideElem> = |elem, _, _| {
    // Hidden elements produce an empty frame.
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |_elem, _engine, _locator, _styles, _region| Ok(TermFrame::new(TermSize::ZERO)),
        ))))
        .pack()
        .spanned(elem.span()))
};

const LAYOUT_RULE: ShowFn<LayoutElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let width_i64: i64 =
                    lynchpin_library_ng::resolve_page_size(styles).cols.get() as i64;
                let height_i64: i64 =
                    lynchpin_library_ng::resolve_page_size(styles).rows.get() as i64;
                let loc = elem.location();
                let context = typst::foundations::Context::new(loc, Some(styles));
                let result = elem
                    .func
                    .call(
                        engine,
                        context.track(),
                        [typst::foundations::dict! {
                            "width" => width_i64,
                            "height" => height_i64,
                        }],
                    )?
                    .display();
                crate::flow::layout_term_frame(engine, &result, locator, styles, region)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

// ── Visualize rules ──────────────────────────────────────────────────────────

const IMAGE_RULE: ShowFn<ImageElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::image::layout_image(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const LINE_RULE: ShowFn<LineElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::shapes::layout_line(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const RECT_RULE: ShowFn<RectElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::shapes::layout_rect(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const SQUARE_RULE: ShowFn<SquareElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::shapes::layout_square(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const ELLIPSE_RULE: ShowFn<EllipseElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::shapes::layout_ellipse(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const CIRCLE_RULE: ShowFn<CircleElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::shapes::layout_circle(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const POLYGON_RULE: ShowFn<PolygonElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::shapes::layout_polygon(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const CURVE_RULE: ShowFn<CurveElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::shapes::layout_curve(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

const PATH_RULE: ShowFn<PathElem> = |elem, _, _| {
    Ok(TermBlockElem::new()
        .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
            elem.clone(),
            |elem, engine, locator, styles, region| {
                let _ = (&locator, &region);
                let config = TermConfig::default();
                crate::shapes::layout_path(elem, engine, &config, styles)
            },
        ))))
        .pack()
        .spanned(elem.span()))
};

// ── Math rules ───────────────────────────────────────────────────────────────

const EQUATION_RULE: ShowFn<EquationElem> = |elem, _, styles| {
    let block = elem.block.get(styles);
    if block {
        Ok(TermBlockElem::new()
            .with_body(Some(TermBlockBody::SingleLayouter(TermBlockCallback::new(
                elem.clone(),
                move |elem, engine, locator, styles, region| {
                    let _ = (&locator, &region);
                    let config = TermConfig::default();
                    crate::math::layout_equation_block(elem, engine, &config, styles)
                },
            ))))
            .pack()
            .spanned(elem.span()))
    } else {
        // Inline math: don't wrap — the inline module's collect_items
        // already handles EquationElem with block=false.
        Ok(TermInlineElem::new()
            .with_cb(Some(TermInlineCallback::new(
                elem.clone(),
                move |elem, engine, locator, styles, region| {
                    let _ = (&locator, &region);
                    let config = TermConfig::default();
                    Ok(vec![TermInlineItem::Frame(
                        crate::math::layout_equation_block(elem, engine, &config, styles)?,
                    )])
                },
            )))
            .pack())
    }
};

// ── Re-export core types needed by the rules ─────────────────────────────────

// (types are now imported at top of file)

/// Compose multiple frames into a single frame by stacking vertically.
fn compose_frames(frames: Vec<TermFrame>) -> TermFrame {
    if frames.is_empty() {
        return TermFrame::new(TermSize::ZERO);
    }
    if frames.len() == 1 {
        return frames.into_iter().next().unwrap();
    }
    let max_cols: Col = frames
        .iter()
        .map(|f| f.size().cols)
        .max()
        .unwrap_or(TermScalar::ZERO);
    let total_rows: Row = frames
        .iter()
        .map(|f| f.size().rows.max(TermScalar::ONE))
        .sum();
    let mut out = TermFrame::new(TermSize::new(max_cols, total_rows.max(TermScalar::ONE)));
    let mut y: Row = TermScalar::ZERO;
    for frame in frames {
        let h = frame.size().rows.max(TermScalar::ONE);
        out.push_frame(TermPoint::new(TermScalar::ZERO, y), frame);
        y = y + h;
    }
    out
}
