use std::num::NonZeroUsize;

use crate::term::{TermDocument, TermElement};
use comemo::{Tracked, TrackedMut};
use rustc_hash::FxHashSet;
use typst::{
    World,
    diag::SourceResult,
    engine::{Engine, Route, Sink, Traced},
    foundations::{Content, StyleChain},
    introspection::{Introspector, IntrospectorBuilder, Location, Locator},
    layout::{Position, Transform},
    model::DocumentInfo,
    routines::{Arenas, RealizationKind, Routines},
    utils::NonZeroExt,
};

pub fn term_document(
    engine: &mut Engine,
    content: &Content,
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
    routines: &Routines,
    world: Tracked<dyn World + '_>,
    introspector: Tracked<Introspector>,
    traced: Tracked<Traced>,
    sink: TrackedMut<Sink>,
    route: Tracked<Route>,
    content: &Content,
    styles: StyleChain,
) -> SourceResult<TermDocument> {
    let mut locator = Locator::root().split();
    let mut engine = Engine {
        routines,
        world,
        introspector,
        traced,
        sink,
        route: Route::extend(route).unnested(),
    };

    // Create this upfront to make it as stable as possible.
    // let footnote_locator = locator.next(&());

    // Mark the external styles as "outside" so that they are valid at the
    // document level.
    let styles = styles.to_map().outside();
    let styles = StyleChain::new(&styles);

    let arenas = Arenas::default();
    let mut info = DocumentInfo::default();
    let children = (engine.routines.realize)(
        RealizationKind::LayoutDocument { info: &mut info },
        &mut engine,
        &mut locator,
        &arenas,
        content,
        styles,
    )?;

    let nodes =
        crate::convert::convert_to_nodes(&mut engine, &mut locator, children.iter().copied())?;
    // let introspectibles =  {
    //     // Add a footnote container at the end, but only if the user did not
    //     // provide their own `<html>` or `<body>` element.
    //     let notes = crate::fragment::html_block_fragment(
    //         &mut engine,
    //         FootnoteContainer::shared(),
    //         footnote_locator,
    //         StyleChain::new(&Styles::root(&children, styles)),
    //         Whitespace::Normal,
    //     )?;
    //     nodes.extend(notes);
    //     nodes
    // };

    let mut link_targets = FxHashSet::default();
    let introspector = introspect_term(&nodes, &mut link_targets);
    // let mut root = root_element(output, &info);
    // crate::link::identify_link_targets(&mut root, &mut introspector, link_targets);

    Ok(TermDocument {
        flow: nodes,
        info,
        introspector,
    })
    // todo!()
}

// Introspects Terminal nodes.
// #[typst_macros::time(name = "introspect html")]
fn introspect_term(output: &[TermElement], link_targets: &mut FxHashSet<Location>) -> Introspector {
    fn discover(
        builder: &mut IntrospectorBuilder,
        sink: &mut Vec<(Content, Position)>,
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
                // HtmlNode::Tag(tag) => {
                //     builder.discover_in_tag(
                //         sink,
                //         tag,
                //         Position { page: NonZeroUsize::ONE, point: Point::zero() },
                //     );
                // }
                // HtmlNode::Text(_, _) => {}
                // HtmlNode::Element(elem) => {
                //     if let Some(parent) = elem.parent {
                //         let mut nested = vec![];
                //         discover(builder, &mut nested, link_targets, &elem.children);
                //         builder.register_insertion(parent, nested);
                //     } else {
                //         discover(builder, sink, link_targets, &elem.children)
                //     }
                // }
                // HtmlNode::Frame(frame) => {
                //     builder.discover_in_frame(
                //         sink,
                //         &frame.inner,
                //         NonZeroUsize::ONE,
                //         Transform::identity(),
                //     );
                //     crate::link::introspect_frame_links(&frame.inner, link_targets);
                // }
                _ => todo!(),
            }
        }
    }

    let mut elems = Vec::new();
    let mut builder = IntrospectorBuilder::new();
    discover(&mut builder, &mut elems, link_targets, output);
    builder.finalize(elems)
}
