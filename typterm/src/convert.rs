use crate::term::{TermElement, TermText, style::TermStyle};
use crossterm::style::{Attribute, Color, ContentStyle};
use typst::{
    __warning,
    diag::SourceResult,
    ecow::{EcoString, EcoVec},
    engine::Engine,
    foundations::{Content, SequenceElem, Smart, StyleChain, StyledElem},
    introspection::{SplitLocator, Tag, TagElem},
    layout::{BlockBody, BlockElem, BoxElem, HElem, PagebreakElem, VElem},
    model::{
        EmphElem, EnumElem, EnumItem, HeadingElem, LinkElem, ListElem, ListItem, ParElem,
        ParbreakElem, QuoteElem, StrongElem, TermItem, TermsElem,
    },
    routines::Pair,
    syntax::Span,
    text::{
        DecoLine, HighlightElem, LinebreakElem, OverlineElem, RawContent, RawElem, RawLine,
        SmartQuoteElem, SpaceElem, StrikeElem, SubElem, SuperElem, TextElem, UnderlineElem,
    },
};

// ── 公开入口 ────────────────────────────────────────────────────────────────

pub fn convert_to_nodes<'a>(
    engine: &mut Engine,
    _locator: &mut SplitLocator,
    children: impl IntoIterator<Item = Pair<'a>>,
) -> SourceResult<EcoVec<TermElement>> {
    // 收集到 Vec 以便向前查看（lookahead）
    let seq: Vec<Pair<'a>> = children.into_iter().collect();

    let mut cv = Converter {
        engine,
        output: EcoVec::new(),
        current_style: ContentStyle::default(),
        pending_heading: None,
        pending_first_item: None,
        enum_counter: 1,
    };

    for (idx, &(child, styles)) in seq.iter().enumerate() {
        let next = seq.get(idx + 1).map(|&(c, _)| c);
        handle_top(&mut cv, child, styles, next)?;
    }

    Ok(cv.finish())
}

// ── 顶层处理器（含向前查看）────────────────────────────────────────────────
//
// 顶层序列是 realize 产生的扁平 (Content, StyleChain) 流。
// 特殊点：列表/枚举/术语项的第一个 Tag::Start(item) 紧接在其
// 父元素 Tag::Start(list/enum/terms) 前面，后续项排在父元素的
// Tag::End 之后。因此需要向前查看一步来判断是否要缓冲第一项。

fn handle_top(
    cv: &mut Converter,
    child: &Content,
    styles: StyleChain,
    next: Option<&Content>,
) -> SourceResult<()> {
    if let Some(tag_elem) = child.to_packed::<TagElem>() {
        match &tag_elem.tag {
            Tag::Start(orig, _) => {
                // ── 标题：记录级别，由后续 BlockElem 消费 ──────────────────
                if let Some(h) = orig.to_packed::<HeadingElem>() {
                    cv.pending_heading = Some(h.resolve_level(styles).get() as usize);
                    return Ok(());
                }

                // ── 列表 / 枚举 / 术语：父元素 tag，冲刷第一项缓冲 ────────
                // 两种情况：
                //   markup 语法（- item / + item / / term: desc）：
                //     第一项已缓冲在 pending_first_item，后续项稍后以独立 T_start(item) 出现
                //   函数调用语法（#list[...] / #enum(start:5)[...][...] / #terms[...]）：
                //     无单独的 T_start(item)，需直接从元素的 children 渲染
                if let Some(le) = orig.to_packed::<ListElem>() {
                    if let Some(PendingItem::List(body)) = cv.pending_first_item.take() {
                        render_list_item(cv, &body, child.span(), styles)?;
                    } else {
                        for item in &le.children {
                            render_list_item(cv, &item.body, child.span(), styles)?;
                        }
                    }
                    return Ok(());
                }
                if let Some(ee) = orig.to_packed::<EnumElem>() {
                    cv.enum_counter = ee.start.get(styles).unwrap_or(1);
                    if let Some(PendingItem::Enum(num, body)) = cv.pending_first_item.take() {
                        render_enum_item(cv, num, &body, child.span(), styles)?;
                    } else {
                        for item in &ee.children {
                            render_enum_item(
                                cv,
                                item.number.get(styles),
                                &item.body,
                                child.span(),
                                styles,
                            )?;
                        }
                    }
                    return Ok(());
                }
                if let Some(te) = orig.to_packed::<TermsElem>() {
                    if let Some(PendingItem::Term(term, desc)) = cv.pending_first_item.take() {
                        render_term_item(cv, &term, &desc, child.span(), styles)?;
                    } else {
                        for item in &te.children {
                            render_term_item(
                                cv,
                                &item.term,
                                &item.description,
                                child.span(),
                                styles,
                            )?;
                        }
                    }
                    return Ok(());
                }

                // ── 列表项：判断是否为第一项（紧接父 tag）────────────────
                let next_is_parent = next
                    .and_then(|n| n.to_packed::<TagElem>())
                    .and_then(|t| {
                        if let Tag::Start(o, _) = &t.tag { Some(o) } else { None }
                    })
                    .map(|o| {
                        o.is::<ListElem>() || o.is::<EnumElem>() || o.is::<TermsElem>()
                    })
                    .unwrap_or(false);

                if next_is_parent {
                    // 第一项：缓冲，等待父 tag 触发渲染
                    if let Some(item) = orig.to_packed::<ListItem>() {
                        cv.pending_first_item =
                            Some(PendingItem::List(item.body.clone()));
                    } else if let Some(item) = orig.to_packed::<EnumItem>() {
                        cv.pending_first_item = Some(PendingItem::Enum(
                            item.number.get(styles),
                            item.body.clone(),
                        ));
                    } else if let Some(item) = orig.to_packed::<TermItem>() {
                        cv.pending_first_item = Some(PendingItem::Term(
                            item.term.clone(),
                            item.description.clone(),
                        ));
                    }
                    return Ok(());
                }

                // 后续项：直接渲染
                if let Some(item) = orig.to_packed::<ListItem>() {
                    render_list_item(cv, &item.body, child.span(), styles)?;
                } else if let Some(item) = orig.to_packed::<EnumItem>() {
                    render_enum_item(cv, item.number.get(styles), &item.body, child.span(), styles)?;
                } else if let Some(item) = orig.to_packed::<TermItem>() {
                    render_term_item(cv, &item.term, &item.description, child.span(), styles)?;
                }
                // 其他 Tag::Start（par、strong、link 等）：静默忽略
            }
            Tag::End(_, _, _) => {
                // 结束 tag 无需处理
            }
        }
        return Ok(());
    }

    // 非 tag 元素交给通用 handle
    handle(cv, child, styles)
}

// ── 渲染辅助 ────────────────────────────────────────────────────────────────

fn render_list_item(
    cv: &mut Converter,
    body: &Content,
    span: Span,
    styles: StyleChain,
) -> SourceResult<()> {
    cv.with_style(
        |s| s.foreground_color = Some(Color::Cyan),
        |cv, _| {
            cv.push_text("• ", span);
            Ok(())
        },
        styles,
    )?;
    handle(cv, body, styles)?;
    cv.push_text('\n', span);
    Ok(())
}

fn render_enum_item(
    cv: &mut Converter,
    num: Smart<u64>,
    body: &Content,
    span: Span,
    styles: StyleChain,
) -> SourceResult<()> {
    let n = match num {
        Smart::Custom(n) => {
            cv.enum_counter = n + 1;
            n
        }
        Smart::Auto => {
            let n = cv.enum_counter;
            cv.enum_counter += 1;
            n
        }
    };
    cv.with_style(
        |s| s.foreground_color = Some(Color::Cyan),
        |cv, _| {
            cv.push_text(EcoString::from(format!("{n}. ")), span);
            Ok(())
        },
        styles,
    )?;
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

// ── 递归内容处理器 ──────────────────────────────────────────────────────────

fn handle(cv: &mut Converter, child: &Content, styles: StyleChain) -> SourceResult<()> {
    // ── 透明包装器 ─────────────────────────────────────────────────────────
    if let Some(seq) = child.to_packed::<SequenceElem>() {
        for c in &seq.children {
            handle(cv, c, styles)?;
        }
    } else if let Some(s) = child.to_packed::<StyledElem>() {
        handle(cv, &s.child, styles.chain(&s.styles))?;

    // ── 内省 tag：递归上下文中静默忽略 ────────────────────────────────────
    } else if child.is::<TagElem>() {
        // 已在 handle_top 处理，此处不重复输出

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
        // realize 后 link 通过 StyleChain 中的 LinkElem::current 标识
        if styles.get_cloned(LinkElem::current).is_some() {
            style.attributes.set(Attribute::Underlined);
            style.foreground_color = Some(Color::Cyan);
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

    // ── 行内样式（StrongElem / EmphElem 等在 realize 前出现时处理）─────────
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

    // ── 标题（realize 前出现时仍处理）─────────────────────────────────────
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

    // ── 代码行（RawLine：realize 后 block/inline raw 都以此形式出现）────────
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

    // ── 代码块（RawElem：realize 前出现或 inline）─────────────────────────
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
        if let Some(level) = cv.pending_heading.take() {
            // 此 block 来自标题（HEADING_RULE 产生 BlockBody::Content）
            if let Some(BlockBody::Content(body)) = elem.body.get_ref(styles) {
                let span = child.span();
                cv.push_text('\n', span);
                cv.with_style(
                    |s| {
                        s.attributes.set(Attribute::Bold);
                        s.foreground_color = Some(heading_color(level));
                    },
                    |cv, st| handle(cv, body, st),
                    styles,
                )?;
                cv.push_text('\n', span);
            }
        } else if let Some(BlockBody::Content(body)) = elem.body.get_ref(styles) {
            // 普通内容块
            handle(cv, body, styles)?;
            cv.push_text('\n', child.span());
        }
        // MultiLayouter / SingleLayouter（列表/枚举/术语块）：静默跳过
        // 它们的内容已通过 TagElem 渲染

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

// ── 辅助函数 ────────────────────────────────────────────────────────────────

fn heading_color(level: usize) -> Color {
    match level {
        1 => Color::Yellow,
        2 => Color::Cyan,
        3 => Color::Green,
        _ => Color::Blue,
    }
}

// ── Converter 结构体 ────────────────────────────────────────────────────────

/// 缓冲的第一个列表/枚举/术语项（出现在父元素 tag 之前）
#[derive(Clone)]
enum PendingItem {
    List(Content),
    Enum(Smart<u64>, Content),
    Term(Content, Content), // (term, description)
}

pub struct Converter<'a, 'b> {
    pub engine: &'a mut Engine<'b>,
    pub output: EcoVec<TermElement>,
    pub current_style: ContentStyle,
    /// 来自 Tag::Start(HeadingElem) 的级别；由下一个 BlockElem 消费
    pending_heading: Option<usize>,
    /// 第一个列表/枚举/术语项（等待父 tag 以确定计数器起始值）
    pending_first_item: Option<PendingItem>,
    /// 当前枚举计数器
    enum_counter: u64,
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
