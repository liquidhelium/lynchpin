
pub mod compile_ng {
    use comemo::{Track, Tracked};
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
        _locator: &mut typst::introspection::SplitLocator,
        arenas: &'a typst::routines::Arenas,
        content: &'a typst::foundations::Content,
        styles: typst::foundations::StyleChain<'a>,
    ) -> typst::diag::SourceResult<Vec<typst::routines::Pair<'a>>> {
        use lynchpin_term_realize::TermRealizationKind;
        match kind {
            typst::routines::RealizationKind::LayoutDocument { .. } => {
                lynchpin_term_realize::realize_term(
                    engine,
                    arenas,
                    &mut typst::model::DocumentInfo::default(),
                    content,
                    styles,
                    TermRealizationKind::Document,
                )
            }
            _ => {
                // Use Document mode for fragment realizations so that inline
                // content (TextElem, etc.) gets grouped into ParElem by the PAR
                // grouping rule. The flow collector expects ParElem, not raw
                // inline elements, and skips raw inline elements.
                lynchpin_term_realize::realize_term(
                    engine,
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
        Warned {
            output,
            warnings: sink.warnings(),
        }
    }

    fn compile_impl(
        world: Tracked<dyn World + '_>,
        traced: Tracked<Traced>,
        sink: &mut Sink,
    ) -> SourceResult<TermDocument> {
        let library = world.library();
        let base = StyleChain::new(&library.styles);
        let target = TargetElem::target.set(Target::Paged).wrap();
        let styles = base.chain(&target);
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

        let mut introspector = &empty_introspector;
        let document;

        loop {
            let mut subsink = Sink::new();
            let constraint = comemo::Constraint::new();
            let mut engine = Engine {
                world,
                introspector: introspector.track_with(&constraint),
                traced,
                sink: subsink.track_mut(),
                route: Route::default(),
                routines: &NG_ROUTINES,
            };

            use typst::model::DocumentInfo;
            use typst::routines::Arenas;
            let arenas = Arenas::default();
            let mut locator = Locator::root().split();
            let mut info = DocumentInfo::default();

            let mut children = lynchpin_term_realize::realize_term(
                &mut engine,
                &arenas,
                &mut info,
                &content,
                styles,
                lynchpin_term_realize::TermRealizationKind::Document,
            )?;
            tracing::debug!("realize produced {} children", children.len());
            for (child, _) in &children {
                tracing::debug!("  child: {}", child.elem().name());
            }

            document = lynchpin_layout_ng::pages::layout_term_document(
                &mut engine,
                &mut children,
                &mut locator,
                styles,
            )?;
            for (_i, _page) in document.iter().enumerate() {}

            introspector = &empty_introspector;
            if constraint.validate(introspector) {
                break;
            }
            break;
        }

        Ok(document)
    }
}
