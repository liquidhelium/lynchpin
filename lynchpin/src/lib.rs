pub mod document;

pub mod term;

pub mod compile_ng {
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
    use comemo::{Track, Tracked};
    use lynchpin_library_ng::TermDocument;

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
                    engine, arenas, &mut typst::model::DocumentInfo::default(),
                    content, styles, TermRealizationKind::Document,
                )
            }
            _ => {
                // Use Document mode for fragment realizations so that inline
                // content (TextElem, etc.) gets grouped into ParElem by the PAR
                // grouping rule. The flow collector expects ParElem, not raw
                // inline elements, and skips raw inline elements.
                lynchpin_term_realize::realize_term(
                    engine, arenas, &mut typst::model::DocumentInfo::default(),
                    content, styles, TermRealizationKind::Document,
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
        Warned { output, warnings: sink.warnings() }
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
            eco_vec![SourceDiagnostic::error(Span::detached(), EcoString::from(err))]
        })?;

        let content = typst_eval::eval(
            &ROUTINES, world, traced, sink.track_mut(), Route::default().track(), &main,
        )?.content();

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

            use typst::routines::Arenas;
            use typst::model::DocumentInfo;
            let arenas = Arenas::default();
            let mut locator = Locator::root().split();
            let mut info = DocumentInfo::default();

            let mut children = lynchpin_term_realize::realize_term(
                &mut engine, &arenas, &mut info, &content, styles,
                lynchpin_term_realize::TermRealizationKind::Document,
            )?;
            tracing::debug!("realize produced {} children", children.len());
            for (child, _) in &children {
                tracing::debug!("  child: {}", child.elem().name());
            }

            document = lynchpin_layout_ng::pages::layout_term_document(
                &mut engine, &mut children, &mut locator, styles,
            )?;
            for (_i, _page) in document.iter().enumerate() {
            }

            introspector = &empty_introspector;
            if constraint.validate(introspector) { break; }
            break;
        }

        Ok(document)
    }
}

pub mod compile {
    use comemo::{Track, Tracked};
    use rustc_hash::FxHashSet;
    use std::sync::LazyLock;
    use typst::{
        __warning, ROUTINES, World,
        diag::{FileError, SourceDiagnostic, SourceResult, Warned},
        ecow::{EcoString, EcoVec, eco_format, eco_vec},
        engine::{Engine, Route, Sink, Traced},
        foundations::{NativeRuleMap, StyleChain, Styles, Target, TargetElem, Value},
        introspection::Introspector,
        routines::Routines,
        syntax::{FileId, Span},
    };

    /// Terminal-specific Routines: paged rules replaced with terminal rules.
    static TERM_ROUTINES: LazyLock<Routines> = LazyLock::new(|| {
        let mut rules = NativeRuleMap::new();
        lynchpin_term_layout::rules::register(&mut rules);
        Routines {
            rules,
            eval_string: ROUTINES.eval_string,
            eval_closure: ROUTINES.eval_closure,
            realize: ROUTINES.realize,
            layout_frame: ROUTINES.layout_frame,
            html_module: ROUTINES.html_module,
            html_span_filled: ROUTINES.html_span_filled,
        }
    });

    use crate::{document::term_document, term::TermDocument};

    pub fn compile(world: &dyn World) -> Warned<SourceResult<TermDocument>> {
        let mut sink = Sink::new();
        let output =
            compile_impl(world.track(), Traced::default().track(), &mut sink).map_err(deduplicate);
        Warned {
            output,
            warnings: sink.warnings(),
        }
    }

    /// Compiles sources and returns all values and styles observed at the given
    /// `span` during compilation.
    pub fn trace(world: &dyn World, span: Span) -> EcoVec<(Value, Option<Styles>)> {
        let mut sink = Sink::new();
        let traced = Traced::new(span);
        compile_impl(world.track(), traced.track(), &mut sink).ok();
        sink.values()
    }

    fn compile_impl(
        world: Tracked<dyn World + '_>,
        traced: Tracked<Traced>,
        sink: &mut Sink,
    ) -> SourceResult<TermDocument> {
        let library = world.library();
        let base = StyleChain::new(&library.styles);
        // Use Target::Paged so that RawElem::synthesize emits syntax-highlight
        // colors via `TextElem::fill` (not as HtmlElem CSS wrappers).
        // Our realize_term skips all paged built-in show rules, so this only
        // affects RawElem color generation and user `context target` checks.
        let target = TargetElem::target.set(Target::Paged).wrap();
        let styles = base.chain(&target);
        let empty_introspector = Introspector::default();

        // Fetch the main source file once.
        let main = world.main();
        let main = world
            .source(main)
            .map_err(|err| hint_invalid_main_file(world, err, main))?;

        // First evaluate the main source file into a module.
        let content = typst_eval::eval(
            &ROUTINES,
            world,
            traced,
            sink.track_mut(),
            Route::default().track(),
            &main,
        )?
        .content();

        let mut iter = 0;
        let mut subsink;
        let mut introspector = &empty_introspector;
        let mut document: TermDocument;

        // Relayout until all introspections stabilize.
        // Terminal realization produces no tags, so this converges in one pass.
        loop {
            subsink = Sink::new();

            let constraint = comemo::Constraint::new();
            let mut engine = Engine {
                world,
                introspector: introspector.track_with(&constraint),
                traced,
                sink: subsink.track_mut(),
                route: Route::default(),
                routines: &TERM_ROUTINES,
            };

            document = term_document(&mut engine, &content, styles)?;
            introspector = &document.introspector;
            iter += 1;

            if constraint.validate(introspector) {
                break;
            }

            if iter >= 5 {
                subsink.warn(__warning!(
                    Span::detached(), "layout did not converge within 5 attempts";
                    hint: "check if any states or queries are updating themselves"
                ));
                break;
            }
        }

        sink.extend_from_sink(subsink);

        // Promote delayed errors.
        let delayed = sink.delayed();
        if !delayed.is_empty() {
            return Err(delayed);
        }

        Ok(document)
    }

    fn deduplicate(mut diags: EcoVec<SourceDiagnostic>) -> EcoVec<SourceDiagnostic> {
        let mut unique = FxHashSet::default();
        diags.retain(|diag| {
            let hash = typst::utils::hash128(&(&diag.span, &diag.message));
            unique.insert(hash)
        });
        diags
    }

    fn hint_invalid_main_file(
        world: Tracked<dyn World + '_>,
        file_error: FileError,
        input: FileId,
    ) -> EcoVec<SourceDiagnostic> {
        let is_utf8_error = matches!(file_error, FileError::InvalidUtf8);
        let mut diagnostic = SourceDiagnostic::error(Span::detached(), EcoString::from(file_error));

        if is_utf8_error {
            let path = input.vpath();
            let extension = path.as_rootless_path().extension();
            if extension.is_some_and(|extension| extension == "typ") {
                return eco_vec![diagnostic];
            }

            match extension {
                Some(extension) => {
                    diagnostic.hint(eco_format!(
                        "a file with the `.{}` extension is not usually a Typst file",
                        extension.to_string_lossy()
                    ));
                }
                None => {
                    diagnostic.hint("a file without an extension is not usually a Typst file");
                }
            };

            if world.source(input.with_extension("typ")).is_ok() {
                diagnostic.hint("check if you meant to use the `.typ` extension instead");
            }
        }

        eco_vec![diagnostic]
    }
}
