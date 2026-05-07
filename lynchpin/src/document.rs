use crate::term::TermDocument;
use comemo::{Tracked, TrackedMut};
use typst::{
    World,
    diag::SourceResult,
    engine::{Engine, Route, Sink, Traced},
    foundations::StyleChain,
    introspection::Introspector,
    model::DocumentInfo,
    routines::Arenas,
};

use lynchpin_term_layout::config::TermConfig;
use lynchpin_term_layout::flow::layout_document;

pub fn term_document(
    engine: &mut Engine,
    content: &typst::foundations::Content,
    styles: StyleChain,
) -> SourceResult<TermDocument> {
    term_document_impl(
        engine.routines,
        engine.world,
        engine.introspector,
        engine.traced,
        TrackedMut::reborrow_mut(&mut engine.sink),
        engine.route.track(),
        content,
        styles,
    )
}

#[comemo::memoize]
#[allow(clippy::too_many_arguments)]
fn term_document_impl(
    routines: &typst::routines::Routines,
    world: Tracked<dyn World + '_>,
    introspector: Tracked<typst::introspection::Introspector>,
    traced: Tracked<Traced>,
    sink: TrackedMut<Sink>,
    route: Tracked<Route>,
    content: &typst::foundations::Content,
    styles: StyleChain,
) -> SourceResult<TermDocument> {
    let mut engine = Engine {
        routines,
        world,
        introspector,
        traced,
        sink,
        route: Route::extend(route).unnested(),
    };

    // Mark the external styles as "outside" so that document-level set rules
    // are recognized correctly.
    let styles = styles.to_map().outside();
    let styles = StyleChain::new(&styles);

    let arenas = Arenas::default();
    let mut info = DocumentInfo::default();

    // Use our own terminal realize instead of the paged/HTML realize.
    let children = lynchpin_term_realize::realize_term(
        &mut engine,
        &arenas,
        &mut info,
        content,
        styles,
    )?;

    let config = TermConfig::default();
    let pages = layout_document(
        &mut engine,
        children.iter().copied(),
        &config,
        styles,
    )?;

    // Terminal rendering doesn't need introspection; an empty Introspector
    // makes the convergence loop in compile.rs always validate on the first
    // pass (no tracked accesses ⇒ constraint is trivially satisfied).
    let introspector = Introspector::default();

    Ok(TermDocument { pages, info, introspector, config })
}
