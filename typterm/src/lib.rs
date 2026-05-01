use typst::layout::PagedDocument;

pub mod size_protocol;

pub mod document {
    use crate::term::TermDocument;
    use comemo::{Tracked, TrackedMut};
    use typst::{
        World,
        diag::SourceResult,
        engine::{Engine, Route, Sink, Traced},
        foundations::{Content, StyleChain},
        introspection::{Introspector, Locator},
        model::DocumentInfo,
        routines::{Arenas, RealizationKind, Routines},
    };

    pub fn term_document(
        engine: &mut Engine,
        content: &Content,
        styles: StyleChain,
    ) -> SourceResult<TermDocument> {
        html_document_impl(
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
    fn html_document_impl(
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
        let footnote_locator = locator.next(&());

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

        // let nodes = crate::convert::convert_to_nodes(
        //     &mut engine,
        //     &mut locator,
        //     children.iter().copied(),
        //     ConversionLevel::Block,
        //     Whitespace::Normal,
        // )?;

        // let mut output = classify_output(nodes.clone())?;
        // let introspectibles = if let OutputKind::Leaves(leaves) = &mut output {
        //     // Add a footnote container at the end, but only if the user did not
        //     // provide their own `<html>` or `<body>` element.
        //     let notes = crate::fragment::html_block_fragment(
        //         &mut engine,
        //         FootnoteContainer::shared(),
        //         footnote_locator,
        //         StyleChain::new(&Styles::root(&children, styles)),
        //         Whitespace::Normal,
        //     )?;
        //     leaves.extend(notes);
        //     leaves
        // } else {
        //     FootnoteContainer::unsupported_with_custom_dom(&engine)?;
        //     &nodes
        // };

        // let mut link_targets = FxHashSet::default();
        // let mut introspector = introspect_html(introspectibles, &mut link_targets);
        // let mut root = root_element(output, &info);
        // crate::link::identify_link_targets(&mut root, &mut introspector, link_targets);

        // Ok(HtmlDocument { info, root, introspector })
        todo!()
    }
}

pub mod term {
    pub mod style {
        use std::process::Command;

        use crossterm::style::*;

        pub struct TermStyle {
            style: ContentStyle,
            pub sizing: Option<crate::size_protocol::EncodeParams>,
        }

        impl AsRef<ContentStyle> for TermStyle {
            fn as_ref(&self) -> &ContentStyle {
                &self.style
            }
        }
    }
    use typst::{
        ecow::{EcoString, EcoVec},
        introspection::Introspector,
        layout::Frame,
        model::DocumentInfo,
        syntax::Span,
    };

    use crate::term::style::TermStyle;

    #[derive(Debug, Clone)]
    pub struct TermDocument {
        pub flow: EcoVec<TermElement>,
        pub info: DocumentInfo,
        pub introspector: Introspector,
    }

    #[derive(Debug, Clone)]
    pub enum TermElement {
        Text(EcoString, Span),
        Frame(TermFrame),
    }

    #[derive(Debug, Clone)]
    pub struct TermFrame {
        pub frame: Frame,
        pub span: Span,
    }
    pub struct TermText {
        pub text: EcoString,
        pub span: Span,
        pub style: TermStyle,
    }
}

pub fn transform(document: &PagedDocument) -> String {
    let _ = document;
    todo!()
}
