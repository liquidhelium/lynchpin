use crate::term::{TermElement, TermText, style::TermStyle};
use crossterm::style::{Attribute, Color, ContentStyle};
use typst::{
    __warning,
    diag::SourceResult,
    ecow::{EcoString, EcoVec},
    engine::Engine,
    foundations::{Content, SequenceElem, Smart, StyleChain, StyledElem},
    layout::{BlockBody, BlockElem, BoxElem, HElem, PagebreakElem, VElem},
    model::{
        EmphElem, EnumElem, HeadingElem, LinkElem, ListElem, ParElem, ParbreakElem, QuoteElem,
        StrongElem, TermsElem,
    },
    routines::Pair,
    syntax::Span,
    text::{
        DecoLine, HighlightElem, LinebreakElem, OverlineElem, RawContent, RawElem, RawLine,
        SmartQuoteElem, SpaceElem, StrikeElem, SubElem, SuperElem, TextElem, UnderlineElem,
    },
    visualize::Paint,
};

// ── Public entry ─────────────────────────────────────────────────────────────

/// Convert a stream of realized `Pair`s into `TermElement`s.
///
/// After `realize_term`, the stream is clean:
/// - No `TagElem` elements.
/// - Inline content is already grouped in `ParElem`.
/// - Lists/enums/terms are `ListElem`/`EnumElem`/`TermsElem` with children.
/// - `HeadingElem` appears directly (not wrapped in `BlockElem`).
/// - `StrongElem`/`EmphElem` have been rewritten to styled body content by
///   `realize_term`; they never appear here.
pub fn convert_to_nodes<'a>(
    engine: &mut Engine,
    children: impl IntoIterator<Item = Pair<'a>>,
) -> SourceResult<EcoVec<TermElement>> {
    let mut cv = Converter {
        engine,
        output: EcoVec::new(),
        current_style: ContentStyle::default(),
        enum_counter: 1,
    };

    for (child, styles) in children {
        handle(&mut cv, child, styles)?;
    }

    Ok(cv.finish())
}

// ── Main handler ──────────────────────────────────────────────────────────────

fn handle(cv: &mut Converter, child: &Content, styles: StyleChain) -> SourceResult<()> {
    // ── Transparent wrappers ──────────────────────────────────────────────────
    if let Some(seq) = child.to_packed::<SequenceElem>() {
        for c in &seq.children {
            handle(cv, c, styles)?;
        }
    } else if let Some(s) = child.to_packed::<StyledElem>() {
        handle(cv, &s.child, styles.chain(&s.styles))?;

    // ── Whitespace / line breaks ──────────────────────────────────────────────
    } else if child.is::<SpaceElem>() {
        cv.push_text(' ', child.span());
    } else if child.is::<LinebreakElem>() {
        cv.push_text('\n', child.span());
    } else if child.is::<ParbreakElem>() {
        cv.push_text("\n\n", child.span());

    // ── Plain text ────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<TextElem>() {
        let text: EcoString = if let Some(case) = styles.get(TextElem::case) {
            case.apply(&elem.text).into()
        } else {
            elem.text.clone()
        };
        let mut style = cv.current_style;
        let typst::text::WeightDelta(delta) = styles.get(TextElem::delta);
        if delta > 0 {
            style.attributes.set(Attribute::Bold);
        }
        if styles.get(TextElem::emph).0 {
            style.attributes.set(Attribute::Italic);
        }
        // `TextElem::deco` is set by the paged built-in show rules for
        // UnderlineElem/StrikeElem/OverlineElem/HighlightElem. In our pipeline
        // those elements are NOT rewritten via show rules (deco construction is
        // too verbose); they are handled as explicit branches further below.
        // We still check deco here to cover content that arrived through a user
        // show rule that went through the paged path (e.g. inside a `context`).
        for deco in styles.get_cloned(TextElem::deco).iter() {
            match &deco.line {
                DecoLine::Underline { .. } => {
                    style.attributes.set(Attribute::Underlined);
                }
                DecoLine::Strikethrough { .. } => {
                    style.attributes.set(Attribute::CrossedOut);
                }
                DecoLine::Overline { .. } => {
                    style.attributes.set(Attribute::OverLined);
                }
                DecoLine::Highlight { .. } => {
                    style.background_color = Some(Color::Yellow);
                }
            }
        }
        // Link styling: set via StyleChain when `LinkElem::current` is present.
        // This is populated by our direct `LinkElem` handler below, which sets
        // the style on the body before recursing.
        if styles.get_cloned(LinkElem::current).is_some() {
            style.attributes.set(Attribute::Underlined);
            style.foreground_color = Some(Color::Cyan);
        }
        let f = styles.get_cloned(TextElem::fill);
        if f != typst::visualize::Color::BLACK.into() {
            match f {
                Paint::Gradient(_) => cv.engine.sink.warn(__warning!(
                    child.span(),
                    "Gradient fill was ignored during Terminal export",
                )),
                Paint::Tiling(_) => cv.engine.sink.warn(__warning!(
                    child.span(),
                    "Tiling fill was ignored during Terminal export",
                )),
                Paint::Solid(c) => {
                    let r = c.to_linear_rgb();
                    style.foreground_color = Some(Color::Rgb {
                        r: (r.red * 256.0) as u8,
                        g: (r.green * 256.0) as u8,
                        b: (r.alpha * 256.0) as u8,
                    })
                }
            }
        }
        cv.push(TermElement::Text(TermText {
            text,
            span: child.span(),
            style: TermStyle { style },
        }));

    // ── Paragraph ─────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<ParElem>() {
        handle(cv, &elem.body, styles)?;
        cv.push_text('\n', child.span());

    // ── Heading ───────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<HeadingElem>() {
        let level = elem.resolve_level(styles).get() as usize;
        let span = child.span();
        cv.push_text('\n', span);
        cv.with_style(
            |s| {
                s.attributes.set(Attribute::Bold);
                s.foreground_color = Some(heading_color(level));
            },
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;
        cv.push_text('\n', span);

    // ── List ──────────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<ListElem>() {
        for item in &elem.children {
            render_list_item(cv, &item.body, child.span(), styles)?;
        }

    // ── Enum ──────────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<EnumElem>() {
        cv.enum_counter = elem.start.get(styles).unwrap_or(1);
        for item in &elem.children {
            render_enum_item(
                cv,
                item.number.get(styles),
                &item.body,
                child.span(),
                styles,
            )?;
        }

    // ── Term list ─────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<TermsElem>() {
        for item in &elem.children {
            render_term_item(cv, &item.term, &item.description, child.span(), styles)?;
        }

    // ── Smart quotes ──────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<SmartQuoteElem>() {
        cv.push_text(
            if elem.double.get(styles) { '"' } else { '\'' },
            child.span(),
        );

    // ── Inline styling ────────────────────────────────────────────────────────
    // `StrongElem` and `EmphElem` are handled at realization time by
    // `realize.rs::visit_term_rules` for top-level document content: they are
    // rewritten to their body with `TextElem::delta`/`TextElem::emph` set, so
    // the `TextElem` branch above picks up the styling via the StyleChain.
    //
    // However, bodies of other elements (ListItem, UnderlineElem, BoxElem, …)
    // are NOT revisited by the realization pass, so `StrongElem`/`EmphElem` can
    // still appear here when nested. The branches below handle that fallback.
    } else if let Some(elem) = child.to_packed::<StrongElem>() {
        cv.with_style(
            |s| s.attributes.set(Attribute::Bold),
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;
    } else if let Some(elem) = child.to_packed::<EmphElem>() {
        cv.with_style(
            |s| s.attributes.set(Attribute::Italic),
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;
    } else if let Some(elem) = child.to_packed::<UnderlineElem>() {
        cv.with_style(
            |s| s.attributes.set(Attribute::Underlined),
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;
    } else if let Some(elem) = child.to_packed::<StrikeElem>() {
        cv.with_style(
            |s| s.attributes.set(Attribute::CrossedOut),
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;
    } else if let Some(elem) = child.to_packed::<OverlineElem>() {
        cv.with_style(
            |s| s.attributes.set(Attribute::OverLined),
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;
    } else if let Some(elem) = child.to_packed::<HighlightElem>() {
        cv.with_style(
            |s| s.background_color = Some(Color::Yellow),
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;
    } else if let Some(elem) = child.to_packed::<SubElem>() {
        // Subscript: render in dim colour as a fallback (no Kitty sizing).
        cv.with_style(
            |s| s.foreground_color = Some(Color::DarkGrey),
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;
    } else if let Some(elem) = child.to_packed::<SuperElem>() {
        // Superscript: render in dim colour as a fallback (no Kitty sizing).
        cv.with_style(
            |s| s.foreground_color = Some(Color::DarkGrey),
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;

    // ── Links ─────────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<LinkElem>() {
        cv.with_style(
            |s| {
                s.attributes.set(Attribute::Underlined);
                s.foreground_color = Some(Color::Cyan);
            },
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;

    // ── Block quotes ──────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<QuoteElem>() {
        let span = child.span();
        cv.with_style(
            |s| {
                s.attributes.set(Attribute::Italic);
                s.foreground_color = Some(Color::DarkGrey);
            },
            |cv, st| {
                cv.push_text('"', span);
                handle(cv, &elem.body, st)?;
                cv.push_text('"', span);
                Ok(())
            },
            styles,
        )?;

    // ── Code (after realize: RawLine for block/inline; RawElem pre-realize) ───
    } else if let Some(elem) = child.to_packed::<RawLine>() {
        let span = child.span();
        cv.with_style(
            |s| s.foreground_color = Some(Color::Green),
            |cv, _| {
                cv.push_text(elem.text.clone(), span);
                Ok(())
            },
            styles,
        )?;
    } else if let Some(elem) = child.to_packed::<RawElem>() {
        let is_block = elem.block.get(styles);
        let text: EcoString = match &elem.text {
            RawContent::Text(t) => t.clone(),
            RawContent::Lines(lines) => lines
                .iter()
                .map(|(s, _)| s.as_str())
                .collect::<Vec<_>>()
                .join("\n")
                .into(),
        };
        let span = child.span();
        if is_block {
            cv.push_text('\n', span);
        }
        cv.with_style(
            |s| s.foreground_color = Some(Color::Green),
            |cv, _| {
                cv.push_text(text.clone(), span);
                Ok(())
            },
            styles,
        )?;
        if is_block {
            cv.push_text('\n', span);
        }

    // ── Layout containers ─────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<BoxElem>() {
        if let Some(body) = elem.body.get_ref(styles) {
            handle(cv, body, styles)?;
        }
    } else if let Some(elem) = child.to_packed::<BlockElem>() {
        // BlockElem can appear when a user writes `#block[...]` directly.
        // With our realize, HeadingElem no longer generates a BlockElem wrapper,
        // so there is no `pending_heading` to check here.
        if let Some(BlockBody::Content(body)) = elem.body.get_ref(styles) {
            handle(cv, body, styles)?;
            cv.push_text('\n', child.span());
        }
        // BlockBody::MultiLayouter / SingleLayouter → silently skip
        // (list/enum/term blocks whose content was already emitted via tags).

        // ── Spacing ───────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<HElem>() {
        if !elem.amount.is_zero() {
            cv.push_text(' ', child.span());
        }
    } else if child.is::<VElem>() {
        cv.push_text('\n', child.span());
    } else if child.is::<PagebreakElem>() {
        cv.push_text("\n\n", child.span());

    // ── Unknown element ───────────────────────────────────────────────────────
    } else {
        cv.engine.sink.warn(__warning!(
            child.span(),
            "{} was ignored during Terminal export",
            child.elem().name()
        ));
    }
    Ok(())
}

// ── List / enum / term renderers ──────────────────────────────────────────────

fn render_list_item(
    cv: &mut Converter,
    body: &Content,
    span: Span,
    styles: StyleChain,
) -> SourceResult<()> {
    cv.push_text("• ", span);
    handle(cv, body, styles)?;
    cv.push_text('\n', span);
    Ok(())
}

fn render_enum_item(
    cv: &mut Converter,
    number: Smart<u64>,
    body: &Content,
    span: Span,
    styles: StyleChain,
) -> SourceResult<()> {
    let n = match number {
        Smart::Auto => {
            let n = cv.enum_counter;
            cv.enum_counter += 1;
            n
        }
        Smart::Custom(n) => {
            // Explicit number: advance the counter past this item so that the
            // next Auto-numbered item follows on from here.
            cv.enum_counter = n + 1;
            n
        }
    };
    cv.push_text(format!("{}. ", n), span);
    handle(cv, body, styles)?;
    cv.push_text('\n', span);
    Ok(())
}

fn render_term_item(
    cv: &mut Converter,
    term: &Content,
    desc: &Content,
    span: Span,
    styles: StyleChain,
) -> SourceResult<()> {
    cv.with_style(
        |s| s.attributes.set(Attribute::Bold),
        |cv, st| handle(cv, term, st),
        styles,
    )?;
    cv.push_text(": ", span);
    handle(cv, desc, styles)?;
    cv.push_text('\n', span);
    Ok(())
}

// ── Heading colour ────────────────────────────────────────────────────────────

fn heading_color(level: usize) -> Color {
    match level {
        1 => Color::Yellow,
        2 => Color::Cyan,
        3 => Color::Green,
        _ => Color::Blue,
    }
}

// ── Converter struct ────────────────────────────────────────────────────────

struct Converter<'a, 'b> {
    engine: &'a mut Engine<'b>,
    output: EcoVec<TermElement>,
    current_style: ContentStyle,
    /// Current enumeration counter (reset at the start of each `EnumElem`).
    enum_counter: u64,
}

impl Converter<'_, '_> {
    fn push_text(&mut self, text: impl Into<EcoString>, span: Span) {
        let text = text.into();
        self.output.push(TermElement::Text(TermText {
            text,
            span,
            style: TermStyle {
                style: self.current_style,
            },
        }));
    }

    fn push(&mut self, elem: impl Into<TermElement>) {
        self.output.push(elem.into());
    }

    fn finish(self) -> EcoVec<TermElement> {
        self.output
    }

    /// Temporarily modifies the current style, runs a closure, then restores.
    fn with_style<F>(
        &mut self,
        modify: impl FnOnce(&mut ContentStyle),
        f: F,
        styles: StyleChain,
    ) -> SourceResult<()>
    where
        F: FnOnce(&mut Converter, StyleChain) -> SourceResult<()>,
    {
        let saved = self.current_style;
        modify(&mut self.current_style);
        let result = f(self, styles);
        self.current_style = saved;
        result
    }
}
