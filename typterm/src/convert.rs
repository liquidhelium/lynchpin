use typst::{
    __warning,
    diag::SourceResult,
    ecow::EcoVec,
    engine::Engine,
    foundations::{Content, StyleChain},
    introspection::SplitLocator,
    model::{ParElem, StrongElem},
    routines::Pair,
    text::{SpaceElem, TextElem},
};

use crate::term::TermElement;

pub fn convert_to_nodes<'a>(
    engine: &mut Engine,
    _locator: &mut SplitLocator,
    children: impl IntoIterator<Item = Pair<'a>>,
) -> SourceResult<EcoVec<TermElement>> {
    // let block = matches!(level, ConversionLevel::Block);
    let mut converter = Converter {
        engine,
        // locator,
        // quoter: match level {
        //     ConversionLevel::Inline(quoter) => quoter,
        //     ConversionLevel::Block => &mut SmartQuoter::new(),
        // },
        // whitespace,
        output: EcoVec::new(),
        // trailing: None,
    };

    for (child, styles) in children {
        handle(&mut converter, child, styles)?;
    }

    let nodes = converter.finish();
    // if block && whitespace == Whitespace::Normal {
    //     protect_spaces(&mut nodes);
    // }

    Ok(nodes)
}

fn handle(converter: &mut Converter, child: &Content, styles: StyleChain) -> SourceResult<()> {
    if child.is::<SpaceElem>() {
        converter.push(TermElement::text(' ', child.span()))
    } else if let Some(elem) = child.to_packed::<StrongElem>() {
    } else if let Some(elem) = child.to_packed::<TextElem>() {
        let text = if let Some(case) = styles.get(TextElem::case) {
            case.apply(&elem.text).into()
        } else {
            elem.text.clone()
        };
        converter.push(TermElement::text(text, child.span()));
    } else if let Some(elem) = child.to_packed::<ParElem>() {
        handle(converter, &elem.body, styles)?
    } else {
        converter.engine.sink.warn(__warning!(
            child.span(),
            "{} was ignored during Terminal export",
            child.elem().name()
        ));
    }
    Ok(())
}

pub struct Converter<'a, 'b> {
    pub engine: &'a mut Engine<'b>,
    pub output: EcoVec<TermElement>,
}

impl Converter<'_, '_> {
    fn push(&mut self, elem: impl Into<TermElement>) {
        self.output.push(elem.into());
    }
    fn finish(self) -> EcoVec<TermElement> {
        self.output
    }
}
