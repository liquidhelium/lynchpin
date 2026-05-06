pub mod size_protocol;

pub mod document;

pub mod term;

pub mod convert;

pub mod compile {
    use comemo::{Track, Tracked};
    use rustc_hash::FxHashSet;
    use typst::{
        __warning, ROUTINES, World, diag::{FileError, SourceDiagnostic, SourceResult, Warned}, ecow::{EcoString, EcoVec, eco_format, eco_vec}, engine::{Engine, Route, Sink, Traced}, foundations::{StyleChain, Styles, Target, TargetElem, Value}, introspection::Introspector, syntax::{FileId, Span}
    };

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

    /// The internal implementation of `compile` with a bit lower-level interface
    /// that is also used by `trace`.
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
        // If that doesn't happen within five attempts, we give up.
        loop {
            // The name of the iterations for timing scopes.
            const ITER_NAMES: &[&str] = &[
                "layout (1)",
                "layout (2)",
                "layout (3)",
                "layout (4)",
                "layout (5)",
            ];
            // let _scope = TimingScope::new(ITER_NAMES[iter]);

            subsink = Sink::new();

            let constraint = comemo::Constraint::new();
            let mut engine = Engine {
                world,
                introspector: introspector.track_with(&constraint),
                traced,
                sink: subsink.track_mut(),
                route: Route::default(),
                routines: &ROUTINES,
            };

            // Layout!
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

    /// Deduplicate diagnostics.
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

        // Attempt to provide helpful hints for UTF-8 errors. Perhaps the user
        // mistyped the filename. For example, they could have written "file.pdf"
        // instead of "file.typ".
        if is_utf8_error {
            let path = input.vpath();
            let extension = path.as_rootless_path().extension();
            if extension.is_some_and(|extension| extension == "typ") {
                // No hints if the file is already a .typ file.
                // The file is indeed just invalid.
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

// pub fn transform(document: &PagedDocument) -> String {

// }
