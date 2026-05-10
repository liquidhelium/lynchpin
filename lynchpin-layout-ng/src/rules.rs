//! Terminal-specific show rules.
//!
//! These rules mirror the paged rules in `lynchpin-layout/src/rules.rs`,
//! but produce [`TermBlockElem`] with terminal-specific callbacks instead
//! of `BlockElem::multi_layouter` / `BlockElem::single_layouter`.
//!
//! Each rule wraps the original element in a [`TermBlockElem`] whose
//! callback calls the corresponding terminal layout function.

use typst::foundations::{NativeElement, NativeRuleMap, ShowFn};
use comemo::Track;
use typst::layout::{
    AlignElem, ColumnsElem, GridCell, GridElem, HideElem, LayoutElem, MoveElem, PadElem,
    RepeatElem, RotateElem, ScaleElem, SkewElem, StackElem,
};
use typst::math::EquationElem;
use typst::model::{
    EnumElem, FigureCaption, FigureElem, FootnoteElem, FootnoteEntry, HeadingElem, ListElem,
    QuoteElem, RefElem, TableCell, TableElem, TermsElem,
};
use typst::text::{
    HighlightElem, ItalicToggle, OverlineElem, RawElem, RawLine, SmallcapsElem, StrikeElem,
    SubElem, SuperElem, TextElem, UnderlineElem, WeightDelta,
};
use typst::model::{EmphElem, StrongElem};
use typst::visualize::{
    CircleElem, CurveElem, EllipseElem, ImageElem, LineElem, PathElem, PolygonElem, RectElem,
    SquareElem,
};

use lynchpin_library_ng::{
    Col, Row, TermBlockCallback, TermBlockElem, TermFrame, TermPoint,
    TermRegion, TermRegions, TermScalar, TermSize,
};

/// Register terminal show rules into `rules`.
pub fn register(rules: &mut NativeRuleMap) {
    use typst::foundations::Target;

    // ── Model ────────────────────────────────────────────────────────────
    rules.register(Target::Paged, LIST_RULE);
    rules.register(Target::Paged, ENUM_RULE);
    rules.register(Target::Paged, TERMS_RULE);
    rules.register(Target::Paged, HEADING_RULE);
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

const LIST_RULE: ShowFn<ListElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::lists::layout_list(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const ENUM_RULE: ShowFn<EnumElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::lists::layout_enum(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const TERMS_RULE: ShowFn<TermsElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::lists::layout_terms(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const HEADING_RULE: ShowFn<HeadingElem> = |elem, _, _styles| {
    // Headings are realized as styled text followed by a paragraph break.
    let body = elem.body.clone();
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        move |_elem, engine, config, styles| {
            let region_size = TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY);
            let region = TermRegion::new(
                region_size,
                typst::layout::Axes::new(false, false),
            );
            crate::flow::layout_term_frame(
                engine,
                &[(&body, styles)],
                typst::introspection::Locator::root(),
                styles,
                region,
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const FIGURE_RULE: ShowFn<FigureElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            // Figures: layout the body (with optional caption).
            let region_size = TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY);
            let region = TermRegion::new(
                region_size,
                typst::layout::Axes::new(false, false),
            );
            crate::flow::layout_term_frame(
                engine,
                &[(&elem.body, styles)],
                typst::introspection::Locator::root(),
                styles,
                region,
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const FIGURE_CAPTION_RULE: ShowFn<FigureCaption> = |elem, engine, styles| {
    let realized = elem.realize(engine, styles)?;
    let page_width = lynchpin_library_ng::resolve_page_size(styles).cols;
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        move |_elem, eng, config, st| {
            let region = TermRegion::new(
                TermSize::new(page_width, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            crate::flow::layout_term_frame(
                eng,
                &[(&realized, st)],
                typst::introspection::Locator::root(),
                st,
                region,
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const QUOTE_RULE: ShowFn<QuoteElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let region = TermRegion::new(
                TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            crate::flow::layout_term_frame(
                engine,
                &[(&elem.body, styles)],
                typst::introspection::Locator::root(),
                styles,
                region,
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const FOOTNOTE_RULE: ShowFn<FootnoteElem> = |elem, engine, styles| {
    let (_, num) = elem.realize(engine, styles)?;
    let sup = format!("[^{}]", num.plain_text());
    let sup_cols = TermScalar::new(sup.len() as i32);
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        move |_elem, _eng, _config, _st| {
            Ok(TermFrame::text(sup.clone(), Default::default(), sup_cols, TermScalar::ONE))
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const FOOTNOTE_ENTRY_RULE: ShowFn<FootnoteEntry> = |elem, engine, styles| {
    let (prefix, body) = elem.realize(engine, styles)?;
    let pw = lynchpin_library_ng::resolve_page_size(styles).cols;
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        move |_elem, eng, config, st| {
            let region = TermRegion::new(
                TermSize::new(pw, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            let prefix_frame = crate::flow::layout_term_frame(
                eng,
                &[(&prefix, st)],
                typst::introspection::Locator::root(),
                st,
                region,
            )?;
            let body_frame = crate::flow::layout_term_frame(
                eng,
                &[(&body, st)],
                typst::introspection::Locator::root(),
                st,
                region,
            )?;
            // Compose prefix + body horizontally.
            let prefix_cols = prefix_frame.size().cols;
            let total_cols = prefix_cols + body_frame.size().cols;
            let total_rows = prefix_frame.size().rows.max(body_frame.size().rows).max(TermScalar::ONE);
            let mut out = TermFrame::new(TermSize::new(total_cols, total_rows));
            out.push_frame(TermPoint::ZERO, prefix_frame);
            out.push_frame(
                TermPoint::new(prefix_cols, TermScalar::ZERO),
                body_frame,
            );
            Ok(out)
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const REF_RULE: ShowFn<RefElem> = |elem, engine, styles| {
    let realized = elem.realize(engine, styles)?;
    let pw = lynchpin_library_ng::resolve_page_size(styles).cols;
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        move |_elem, eng, config, st| {
            let region = TermRegion::new(
                TermSize::new(pw, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            crate::flow::layout_term_frame(
                eng,
                &[(&realized, st)],
                typst::introspection::Locator::root(),
                st,
                region,
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const TABLE_RULE: ShowFn<TableElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let locator = typst::introspection::Locator::root();
            let regions = TermRegions::one(
                TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            let fragment = crate::grid::layout_table(elem, engine, locator, styles, regions)?;
            // Compose all frames in the fragment into one.
            let frames = lynchpin_library_ng::fragment_into_frames(fragment);
            Ok(compose_frames(frames))
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const TABLE_CELL_RULE: ShowFn<TableCell> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let region = TermRegion::new(
                TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            crate::flow::layout_term_frame(
                engine,
                &[(&elem.body, styles)],
                typst::introspection::Locator::root(),
                styles,
                region,
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

// ── Text rules ───────────────────────────────────────────────────────────────

const SUB_RULE: ShowFn<SubElem> = |elem, _, styles| {
    use typst::text::{ShiftSettings, ScriptKind, TextSize};
    use typst::layout::{Em, Length};
    let font_size = styles.resolve(TextElem::size);
    Ok(elem.body.clone().set(
        TextElem::shift_settings,
        Some(ShiftSettings {
            typographic: elem.typographic.get(styles),
            shift: elem.baseline.get(styles).map(|l| -Em::from_length(l, font_size)),
            size: elem.size.get(styles).map(|t| Em::from_length(t.0, font_size)),
            kind: ScriptKind::Sub,
        }),
    ))
};

const SUPER_RULE: ShowFn<SuperElem> = |elem, _, styles| {
    use typst::text::{ShiftSettings, ScriptKind, TextSize};
    use typst::layout::{Em, Length};
    let font_size = styles.resolve(TextElem::size);
    Ok(elem.body.clone().set(
        TextElem::shift_settings,
        Some(ShiftSettings {
            typographic: elem.typographic.get(styles),
            shift: elem.baseline.get(styles).map(|l| -Em::from_length(l, font_size)),
            size: elem.size.get(styles).map(|t| Em::from_length(t.0, font_size)),
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
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::modifiers::layout_smallcaps(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const RAW_RULE: ShowFn<RawElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, _engine, config, styles| {
            use typst::text::RawContent;
            let text: ecow::EcoString = match &elem.text {
                RawContent::Text(t) => t.clone(),
                RawContent::Lines(lines) => {
                    let mut s = ecow::EcoString::new();
                    for (i, (line, _)) in lines.iter().enumerate() {
                        if i > 0 { s.push('\n'); }
                        s.push_str(line);
                    }
                    s
                }
            };
            let lines: Vec<&str> = text.lines().collect();
            let max_width: i32 = lines.iter().map(|l: &&str| l.len() as i32).max().unwrap_or(0);
            let max_width = TermScalar::new(max_width).min(lynchpin_library_ng::resolve_page_size(styles).cols);
            let height = TermScalar::new(lines.len() as i32).max(TermScalar::ONE);

            let mut frame = TermFrame::new(TermSize::new(max_width, height));
            for (i, line) in lines.iter().enumerate() {
                let truncated: String = line.chars().take(max_width.get() as usize).collect();
                frame.push_text(
                    TermPoint::new(TermScalar::ZERO, TermScalar::new(i as i32)),
                    ecow::EcoString::from(truncated),
                    Default::default(),
                );
            }
            Ok(frame)
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const RAW_LINE_RULE: ShowFn<RawLine> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let region = TermRegion::new(
                TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            crate::flow::layout_term_frame(
                engine,
                &[(&elem.body, styles)],
                typst::introspection::Locator::root(),
                styles,
                region,
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

// ── Layout rules ─────────────────────────────────────────────────────────────

const ALIGN_RULE: ShowFn<AlignElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let region = TermRegion::new(
                TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            crate::flow::layout_term_frame(
                engine,
                &[(&elem.body, styles)],
                typst::introspection::Locator::root(),
                styles,
                region,
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const PAD_RULE: ShowFn<PadElem> = |elem, _, _styles| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let region = TermRegion::new(
                TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            let mut fragment = crate::flow::layout_term_frame(
                engine,
                &[(&elem.body, styles)],
                typst::introspection::Locator::root(),
                styles,
                region,
            )?;

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
    ))
    .pack()
    .spanned(elem.span()))
};

const COLUMNS_RULE: ShowFn<ColumnsElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let region = TermRegion::new(
                TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            crate::flow::layout_term_frame(
                engine,
                &[(&elem.body, styles)],
                typst::introspection::Locator::root(),
                styles,
                region,
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const STACK_RULE: ShowFn<StackElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::stack::layout_stack(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const GRID_RULE: ShowFn<GridElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let locator = typst::introspection::Locator::root();
            let regions = TermRegions::one(
                TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            let fragment = crate::grid::layout_grid(elem, engine, locator, styles, regions)?;
            let frames = lynchpin_library_ng::fragment_into_frames(fragment);
            Ok(compose_frames(frames))
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const GRID_CELL_RULE: ShowFn<GridCell> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let region = TermRegion::new(
                TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            crate::flow::layout_term_frame(
                engine,
                &[(&elem.body, styles)],
                typst::introspection::Locator::root(),
                styles,
                region,
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const MOVE_RULE: ShowFn<MoveElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::transforms::layout_move(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const SCALE_RULE: ShowFn<ScaleElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::transforms::layout_scale(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const ROTATE_RULE: ShowFn<RotateElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::transforms::layout_rotate(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const SKEW_RULE: ShowFn<SkewElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::transforms::layout_skew(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const REPEAT_RULE: ShowFn<RepeatElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::repeat::layout_repeat(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const HIDE_RULE: ShowFn<HideElem> = |elem, _, _| {
    // Hidden elements produce an empty frame.
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |_elem, _engine, _config, _styles| {
            Ok(TermFrame::new(TermSize::ZERO))
        },
    ))
    .pack()
    .spanned(elem.span()))
};

const LAYOUT_RULE: ShowFn<LayoutElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let width_i64: i64 = lynchpin_library_ng::resolve_page_size(styles).cols.get() as i64;
            let height_i64: i64 = lynchpin_library_ng::resolve_page_size(styles).rows.get() as i64;
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
            let region = TermRegion::new(
                TermSize::new(lynchpin_library_ng::resolve_page_size(styles).cols, TermScalar::INFINITY),
                typst::layout::Axes::new(false, false),
            );
            crate::flow::layout_term_frame(
                engine,
                &[(&result, styles)],
                typst::introspection::Locator::root(),
                styles,
                region,
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

// ── Visualize rules ──────────────────────────────────────────────────────────

const IMAGE_RULE: ShowFn<ImageElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::image::layout_image(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const LINE_RULE: ShowFn<LineElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::shapes::layout_line(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const RECT_RULE: ShowFn<RectElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::shapes::layout_rect(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const SQUARE_RULE: ShowFn<SquareElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::shapes::layout_square(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const ELLIPSE_RULE: ShowFn<EllipseElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::shapes::layout_ellipse(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const CIRCLE_RULE: ShowFn<CircleElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::shapes::layout_circle(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const POLYGON_RULE: ShowFn<PolygonElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::shapes::layout_polygon(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const CURVE_RULE: ShowFn<CurveElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::shapes::layout_curve(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

const PATH_RULE: ShowFn<PathElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::shapes::layout_path(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

// ── Math rules ───────────────────────────────────────────────────────────────

const EQUATION_RULE: ShowFn<EquationElem> = |elem, _, styles| {
    let block = elem.block.get(styles);
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        move |elem, engine, config, styles| {
            if block {
                crate::math::layout_equation_block(elem, engine, config, styles)
            } else {
                crate::math::layout_equation_inline(elem, engine, config, styles)
            }
        },
    ))
    .pack()
    .spanned(elem.span()))
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
    let max_cols: Col = frames.iter().map(|f| f.size().cols).max().unwrap_or(TermScalar::ZERO);
    let total_rows: Row = frames.iter().map(|f| f.size().rows.max(TermScalar::ONE)).sum();
    let mut out = TermFrame::new(TermSize::new(max_cols, total_rows.max(TermScalar::ONE)));
    let mut y: Row = TermScalar::ZERO;
    for frame in frames {
        let h = frame.size().rows.max(TermScalar::ONE);
        out.push_frame(TermPoint::new(TermScalar::ZERO, y), frame);
        y = y + h;
    }
    out
}
