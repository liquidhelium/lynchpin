use crate::term::{TermElement, TermText, style::TermStyle};
use crossterm::style::{Attribute, Color, ContentStyle};
use typst::{
    __warning,
    diag::SourceResult,
    ecow::{EcoString, EcoVec},
    engine::Engine,
    foundations::{Content, SequenceElem, Smart, StyleChain, StyledElem},
    introspection::SplitLocator,
    layout::{BlockBody, BlockElem, BoxElem, HElem, PagebreakElem, VElem},
    model::
        {EmphElem, EnumElem, HeadingElem, LinkElem, ListElem, ParElem, ParbreakElem, QuoteElem,
        StrongElem, TermsElem},
    routines::Pair,
    syntax::Span,
    text::{
        DecoLine, HighlightElem, LinebreakElem, OverlineElem, RawContent, RawElem,
        SmartQuoteElem, SpaceElem, StrikeElem, SubElem, SuperElem, TextElem, UnderlineElem,
    },
};

pub fn convert_to_nodes<'a>(
    engine: &mut Engine,
    _locator: &mut SplitLocator,
    children: impl IntoIterator<Item = Pair<'a>>,
) -> SourceResult<EcoVec<TermElement>> {
    let mut converter = Converter {
        engine,
        output: EcoVec::new(),
        current_style: ContentStyle::default(),
    };
    for (child, styles) in children {
        handle(&mut converter, child, styles)?;
    }
    Ok(converter.finish())
}

fn handle(cv: &mut Converter, child: &Content, styles: StyleChain) -> SourceResult<()> {
    // ── 透明包装器 ─────────────────────────────────────────────────────────
    if let Some(seq) = child.to_packed::<SequenceElem>() {
        for c in &seq.children {
            handle(cv, c, styles)?;
        }
    } else if let Some(s) = child.to_packed::<StyledElem>() {
        handle(cv, &s.child, styles.chain(&s.styles))?;

    // ── 空白 / 换行 ────────────────────────────────────────────────────────
    } else if child.is::<SpaceElem>() {
        cv.push_text(' ', child.span());
    } else if child.is::<LinebreakElem>() {
        cv.push_text('\n', child.span());
    } else if child.is::<ParbreakElem>() {
        cv.push_text("\n\n", child.span());

    // ── 纯文本 ─────────────────────────────────────────────────────────────
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
        // realize(Target::Paged) 将 UnderlineElem/StrikeElem/OverlineElem/HighlightElem
        // 转化为 StyledElem，装饰信息通过 TextElem::deco 传递到 StyleChain。
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
        cv.push(TermElement::text_with_style(text, child.span(), style));

    // ── 段落 ───────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<ParElem>() {
        handle(cv, &elem.body, styles)?;
        cv.push_text('\n', child.span());

    // ── 智能引号 ───────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<SmartQuoteElem>() {
        cv.push_text(
            if elem.double.get(styles) { '"' } else { '\'' },
            child.span(),
        );

    // ── 行内样式 ───────────────────────────────────────────────────────────
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
        handle(cv, &elem.body, styles)?;
    } else if let Some(elem) = child.to_packed::<SuperElem>() {
        handle(cv, &elem.body, styles)?;

    // ── 标题 ───────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<HeadingElem>() {
        let level = elem.resolve_level(styles).get();
        let span = child.span();
        cv.push_text('\n', span);
        cv.with_style(
            |s| {
                s.attributes.set(Attribute::Bold);
                s.foreground_color = Some(match level {
                    1 => Color::Yellow,
                    2 => Color::Cyan,
                    3 => Color::Green,
                    _ => Color::Blue,
                });
            },
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;
        cv.push_text('\n', span);

    // ── 无序列表 ───────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<ListElem>() {
        let span = child.span();
        for item in &elem.children {
            cv.with_style(
                |s| s.foreground_color = Some(Color::Cyan),
                |cv, _| {
                    cv.push_text("• ", span);
                    Ok(())
                },
                styles,
            )?;
            handle(cv, &item.body, styles)?;
            cv.push_text('\n', span);
        }

    // ── 有序列表 ───────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<EnumElem>() {
        let span = child.span();
        let start: u64 = elem.start.get(styles).custom().unwrap_or(1);
        let mut counter = start;
        for item in &elem.children {
            let num = match item.number.get(styles) {
                Smart::Custom(n) => {
                    counter = n + 1;
                    n
                }
                Smart::Auto => {
                    let n = counter;
                    counter += 1;
                    n
                }
            };
            cv.with_style(
                |s| s.foreground_color = Some(Color::Cyan),
                |cv, _| {
                    cv.push_text(EcoString::from(format!("{num}. ")), span);
                    Ok(())
                },
                styles,
            )?;
            handle(cv, &item.body, styles)?;
            cv.push_text('\n', span);
        }

    // ── 术语列表 ───────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<TermsElem>() {
        let span = child.span();
        for item in &elem.children {
            cv.with_style(
                |s| s.attributes.set(Attribute::Bold),
                |cv, st| handle(cv, &item.term, st),
                styles,
            )?;
            cv.push_text(": ", span);
            handle(cv, &item.description, styles)?;
            cv.push_text('\n', span);
        }

    // ── 链接 ───────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<LinkElem>() {
        cv.with_style(
            |s| {
                s.attributes.set(Attribute::Underlined);
                s.foreground_color = Some(Color::Cyan);
            },
            |cv, st| handle(cv, &elem.body, st),
            styles,
        )?;

    // ── 引用块 ─────────────────────────────────────────────────────────────
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

    // ── 代码 / Raw ─────────────────────────────────────────────────────────
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

    // ── 布局容器 ───────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<BoxElem>() {
        if let Some(body) = elem.body.get_ref(styles) {
            handle(cv, body, styles)?;
        }
    } else if let Some(elem) = child.to_packed::<BlockElem>() {
        if let Some(BlockBody::Content(body)) = elem.body.get_ref(styles) {
            handle(cv, body, styles)?;
            cv.push_text('\n', child.span());
        }

    // ── 间距 ───────────────────────────────────────────────────────────────
    } else if let Some(elem) = child.to_packed::<HElem>() {
        if !elem.amount.is_zero() {
            cv.push_text(' ', child.span());
        }
    } else if child.is::<VElem>() {
        cv.push_text('\n', child.span());
    } else if child.is::<PagebreakElem>() {
        cv.push_text("\n\n", child.span());

    // ── 未识别元素 ─────────────────────────────────────────────────────────
    } else {
        cv.engine.sink.warn(__warning!(
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
    pub current_style: ContentStyle,
}

impl Converter<'_, '_> {
    fn push_text(&mut self, text: impl Into<EcoString>, span: Span) {
        self.output.push(TermElement::Text(TermText {
            text: text.into(),
            span,
            style: TermStyle {
                style: self.current_style,
                sizing: None,
            },
        }));
    }

    fn push(&mut self, elem: impl Into<TermElement>) {
        self.output.push(elem.into());
    }

    fn finish(self) -> EcoVec<TermElement> {
        self.output
    }

    /// 临时修改样式，执行闭包，然后恢复。
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
