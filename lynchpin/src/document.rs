use crate::term::{TermDocument, TermElement};
use comemo::{Tracked, TrackedMut};
use rustc_hash::FxHashSet;
use std::num::NonZeroUsize;
use typst::{
    World,
    diag::SourceResult,
    engine::{Engine, Route, Sink, Traced},
    foundations::StyleChain,
    introspection::{IntrospectorBuilder, Location},
    layout::{Position, Transform},
    model::DocumentInfo,
    routines::Arenas,
    utils::NonZeroExt,
};

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
    let children = crate::realize::realize_term(
        &mut engine,
        &arenas,
        &mut info,
        content,
        styles,
    )?;

    let nodes = crate::convert::convert_to_nodes(
        &mut engine,
        children.iter().copied(),
    )?;

    let mut link_targets = FxHashSet::default();
    let introspector = introspect_term(&nodes, &mut link_targets);

    Ok(TermDocument { flow: nodes, info, introspector })
}

fn introspect_term(
    output: &[TermElement],
    link_targets: &mut FxHashSet<Location>,
) -> typst::introspection::Introspector {
    fn discover(
        builder: &mut IntrospectorBuilder,
        sink: &mut Vec<(typst::foundations::Content, Position)>,
        _link_targets: &mut FxHashSet<Location>,
        nodes: &[TermElement],
    ) {
        for node in nodes {
            match node {
                TermElement::Text(_) => {}
                TermElement::Frame(frame) => {
                    builder.discover_in_frame(
                        sink,
                        &frame.frame,
                        NonZeroUsize::ONE,
                        Transform::identity(),
                    );
                }
            }
        }
    }

    let mut elems = Vec::new();
    let mut builder = IntrospectorBuilder::new();
    discover(&mut builder, &mut elems, link_targets, output);
    builder.finalize(elems)
}
