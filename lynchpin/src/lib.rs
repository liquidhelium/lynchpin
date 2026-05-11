
pub mod compile_ng {
    use comemo::{Track, Tracked, TrackedMut};
    use lynchpin_library_ng::TermDocument;
    use std::sync::LazyLock;
    use typst::{
        ROUTINES, World,
        diag::{SourceDiagnostic, SourceResult, Warned},
        ecow::{EcoString, eco_vec},
        engine::{Engine, Route, Sink, Traced},
        foundations::{NativeRuleMap, StyleChain, Target, TargetElem},
        introspection::{Introspector, Locator},
        routines::Routines,
        syntax::Span,
    };

    fn term_realize<'a>(
        kind: typst::routines::RealizationKind,
        engine: &mut typst::engine::Engine,
        locator: &mut typst::introspection::SplitLocator,
        arenas: &'a typst::routines::Arenas,
        content: &'a typst::foundations::Content,
        styles: typst::foundations::StyleChain<'a>,
    ) -> typst::diag::SourceResult<Vec<typst::routines::Pair<'a>>> {
        use lynchpin_term_realize::TermRealizationKind;
        match kind {
            typst::routines::RealizationKind::LayoutDocument { .. } => {
                lynchpin_term_realize::realize_term(
                    engine,
                    locator,
                    arenas,
                    &mut typst::model::DocumentInfo::default(),
                    content,
                    styles,
                    TermRealizationKind::Document,
                )
            }
            _ => {
                lynchpin_term_realize::realize_term(
                    engine,
                    locator,
                    arenas,
                    &mut typst::model::DocumentInfo::default(),
                    content,
                    styles,
                    TermRealizationKind::Document,
                )
            }
        }
    }

    static NG_ROUTINES: LazyLock<Routines> = LazyLock::new(|| {
        let mut rules = NativeRuleMap::new();
        lynchpin_layout_ng::rules::register(&mut rules);
        Routines {
            rules,
            eval_string: ROUTINES.eval_string,
            eval_closure: ROUTINES.eval_closure,
            realize: term_realize,
            layout_frame: ROUTINES.layout_frame,
            html_module: ROUTINES.html_module,
            html_span_filled: ROUTINES.html_span_filled,
        }
    });

    pub fn compile(world: &dyn World) -> Warned<SourceResult<TermDocument>> {
        let mut sink = Sink::new();
        let output = compile_impl(world.track(), Traced::default().track(), &mut sink);
        let delayed = sink.delayed();
        let mut warnings = sink.warnings();
        warnings.extend(delayed);
        Warned {
            output,
            warnings,
        }
    }

    fn compile_impl(
        world: Tracked<dyn World + '_>,
        traced: Tracked<Traced>,
        sink: &mut Sink,
    ) -> SourceResult<TermDocument> {
        let empty_introspector = Introspector::default();

        let main = world.main();
        let main = world.source(main).map_err(|err| {
            eco_vec![SourceDiagnostic::error(
                Span::detached(),
                EcoString::from(err)
            )]
        })?;

        let content = typst_eval::eval(
            &ROUTINES,
            world,
            traced,
            sink.track_mut(),
            Route::default().track(),
            &main,
        )?
        .content();

        let mut introspector = empty_introspector;
        let mut document = TermDocument::default();

        // Only keep the diagnostics from the final stable iteration.
        // Intermediate iterations produce expected delayed errors (e.g. label
        // not found on iter 1) that get resolved on later iterations; merging
        // them into the main sink would wrongly surface them as real errors.
        let mut last_subsink = Sink::new();

        for _iter in 0..5usize {
            let constraint = comemo::Constraint::new();
            let mut subsink = Sink::new();
            document = layout_term_impl(
                &NG_ROUTINES,
                world,
                introspector.track_with(&constraint),
                traced,
                subsink.track_mut(),
                &content,
            )?;
            let new_introspector = build_introspector(&document, world);
            let stable = constraint.validate(&new_introspector);
            introspector = new_introspector;
            last_subsink = subsink;
            if stable {
                break;
            }
        }
        // Merge only the final iteration's diagnostics.
        sink.extend_from_sink(last_subsink);

        Ok(document)
    }

    /// Memoized realization + layout.  Mirrors paged's `layout_document_impl` —
    /// the `introspector` Tracked parameter ensures comemo tracks all
    /// introspector reads made within this function (e.g. `RefElem::realize()`).
    #[comemo::memoize]
    fn layout_term_impl(
        routines: &'static Routines,
        world: Tracked<dyn World + '_>,
        introspector: Tracked<Introspector>,
        traced: Tracked<Traced>,
        sink: TrackedMut<Sink>,
        content: &typst::foundations::Content,
    ) -> SourceResult<TermDocument> {
        use typst::model::DocumentInfo;
        use typst::routines::Arenas;

        let library = world.library();
        let base = StyleChain::new(&library.styles);
        let target = TargetElem::target.set(Target::Paged).wrap();
        let styles = base.chain(&target);

        let mut engine = Engine {
            world,
            introspector,
            traced,
            sink,
            route: Route::default(),
            routines,
        };

        let arenas = Arenas::default();
        let mut locator = Locator::root().split();
        let mut info = DocumentInfo::default();

        let mut children = lynchpin_term_realize::realize_term(
            &mut engine,
            &mut locator,
            &arenas,
            &mut info,
            content,
            styles,
            lynchpin_term_realize::TermRealizationKind::Document,
        )?;

        lynchpin_layout_ng::pages::layout_term_document(
            &mut engine,
            &mut children,
            &mut locator,
            styles,
        )
    }

    fn build_introspector(
        document: &TermDocument,
        world: Tracked<dyn World + '_>,
    ) -> Introspector {
        use typst::introspection::IntrospectorBuilder;
        use std::num::NonZeroUsize;
        use lynchpin_library_ng::{TermFrameItem, TermPoint};
        use typst::foundations::Resolve;

        let library = world.library();
        let styles = StyleChain::new(&library.styles);
        let font_size: typst::layout::Abs = styles
            .get(typst::text::TextElem::size)
            .resolve(styles);
        let row_to_pt = font_size.to_pt();
        let col_to_pt = row_to_pt;

        let mut builder = IntrospectorBuilder::new();
        builder.pages = document.len();
        let mut elems = Vec::new();

        fn discover_in_frame(
            builder: &mut IntrospectorBuilder,
            elems: &mut Vec<(typst::foundations::Content, typst::layout::Position)>,
            frame: &lynchpin_library_ng::TermFrame,
            page: NonZeroUsize,
            offset: TermPoint,
            col_to_pt: f64,
            row_to_pt: f64,
        ) {
            for (pos, item) in frame.items() {
                let abs = *pos + offset;
                match item {
                    TermFrameItem::Tag(tag) => {
                        let position = typst::layout::Position {
                            page,
                            point: typst::layout::Point::new(
                                typst::layout::Abs::pt(abs.col.get() as f64 * col_to_pt),
                                typst::layout::Abs::pt(abs.row.get() as f64 * row_to_pt),
                            ),
                        };
                        builder.discover_in_tag(elems, tag, position);
                    }
                    TermFrameItem::Frame(sub) => {
                        discover_in_frame(builder, elems, sub, page, abs, col_to_pt, row_to_pt);
                    }
                    _ => {}
                }
            }
        }

        for (i, page) in document.iter().enumerate() {
            discover_in_frame(
                &mut builder,
                &mut elems,
                &page.inner,
                NonZeroUsize::new(1 + i).unwrap(),
                TermPoint::ZERO,
                col_to_pt,
                row_to_pt,
            );
        }
        builder.finalize(elems)
    }
}
