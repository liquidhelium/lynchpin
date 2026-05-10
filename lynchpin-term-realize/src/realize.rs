//! Terminal realization — a clean adaptation of typst's realization subsystem.
//!
//! Key differences from `typst-realize`:
//! - Single realization kind (no paged / HTML / math modes).
//! - No introspection tag generation.
//! - No TEXTUAL grouping (regex show rules) or CITES grouping.
//! Built-in show rules are **not** applied for structural elements (no paged/HTML
//!   transformations). However, a small set of "term built-in rules" *are* applied
//!   for simple inline-styling wrappers (`strong`, `emph`) that reduce cleanly to
//!   TextElem style-chain fields already read by `convert.rs`.
//! - Space collapsing is included (required for correct PAR grouping).

use std::borrow::Cow;
use std::cell::LazyCell;

use arrayvec::ArrayVec;
use comemo::Track;
use typst::diag::{At, SourceResult, bail};
use typst::engine::Engine;
use typst::foundations::{
    Content, Context, NativeElement, Packed, Recipe, RecipeIndex, SequenceElem, ShowSet,
    StyleChain, StyledElem, Styles, Synthesize, Transformation,
};
use typst::foundations::{ContextElem, TargetElem};
use typst::introspection::TagElem;
use typst::layout::{AlignElem, BoxElem, HElem, HideElem, InlineElem, VElem};
use lynchpin_library_ng::TermInlineElem;
use typst::math::{EquationElem, Mathy};
use typst::model::{
    DocumentInfo, EmphElem, EnumElem, ListElem, ListItemLike, ListLike, ParElem, ParbreakElem,
    StrongElem, TermsElem,
};
use typst::routines::{Arenas, Pair};
use typst::syntax::Span;
use typst::text::{
    HighlightElem, ItalicToggle, LinebreakElem, OverlineElem, RawElem, SmartQuoteElem, SpaceElem,
    StrikeElem, SubElem, SuperElem, TextElem, UnderlineElem, WeightDelta,
};
use typst::utils::SliceExt;

// ── RealizationKind ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermRealizationKind {
    Document,
    Math,
    Inline,
}
// ── Public entry ─────────────────────────────────────────────────────────────

/// Realize content for terminal output.
///
/// Returns a flat list of `Pair`s (content + style chain) with the following
/// guarantees:
/// - No `TagElem` elements.
/// - Inline content is grouped into `ParElem`.
/// - List/enum/term items are grouped into their container elements.
/// - User-defined show rules are applied; built-in (paged/HTML) rules are not.
pub fn realize_term<'a>(
    engine: &mut Engine,
    arenas: &'a Arenas,
    info: &mut DocumentInfo,
    content: &'a Content,
    styles: StyleChain<'a>,
    kind: TermRealizationKind,
) -> SourceResult<Vec<Pair<'a>>> {
    let mut s = State {
        engine,
        arenas,
        info,
        kind,
        sink: vec![],
        groupings: ArrayVec::new(),
        may_attach: false,
        loc_counter: 1,
    };

    visit(&mut s, content, styles)?;
    finish(&mut s)?;

    Ok(s.sink)
}

// ── State & helpers ───────────────────────────────────────────────────────────

/// Mutable realization state.
///
/// We use three separate lifetimes to avoid forcing the lifetime of `engine`
/// and `arenas` to be equal (they are invariant under `&mut`):
/// - `'a`: lifetime of the output `Pair`s (content + style chains).
/// - `'x`: lifetime of engine / arenas borrow.
/// - `'y`: inner lifetime of `Engine`.
struct State<'a, 'x, 'y> {
    engine: &'x mut Engine<'y>,
    arenas: &'a Arenas,
    info: &'x mut DocumentInfo,
    kind: TermRealizationKind,
    /// Flat output of realized `(content, styles)` pairs.
    sink: Vec<Pair<'a>>,
    /// Currently active groupings (max `MAX_GROUP_NESTING` deep).
    groupings: ArrayVec<Grouping<'x>, MAX_GROUP_NESTING>,
    /// Whether "attach" spacing following the last element can survive.
    may_attach: bool,
    loc_counter: u64,
    // (no saw_parbreak: it was set but never read — removed)
}

impl<'a> State<'a, '_, '_> {
    /// Lifetime-extends owned content into the arena with lifetime `'a`.
    fn store(&self, content: Content) -> &'a Content {
        self.arenas.content.alloc(content)
    }
}

// ── Grouping infrastructure ───────────────────────────────────────────────────

/// Describes how a class of elements shall be grouped during realization.
struct GroupingRule {
    /// Higher priority causes a nested group to start instead of joining the
    /// existing one.
    priority: u8,
    /// Returns `true` for elements that both *start* and *belong to* this
    /// grouping.
    trigger: fn(&Content) -> bool,
    /// Returns `true` for elements that may appear in the interior of the
    /// grouping but are stripped from its edges (e.g. `SpaceElem`).
    inner: fn(&Content) -> bool,
    /// Returns `true` for style-map element types that *interrupt* the
    /// grouping when encountered in a `StyledElem`.
    interrupt: fn(typst::foundations::Element) -> bool,
    /// Converts the accumulated `s.sink[start..]` slice into the grouped
    /// element and re-visits it.
    finish: fn(Grouped<'_, '_, '_, '_>) -> SourceResult<()>,
}

struct Grouping<'a> {
    /// Index into `s.sink` where this group starts.
    start: usize,
    /// Set when a style interruption has been recorded but the group has not
    /// yet been flushed (can still be promoted to inline-only).
    interrupted: bool,
    rule: &'a GroupingRule,
}

/// Provides access to the grouped slice while `finish` is executing.
struct Grouped<'a, 'x, 'y, 's> {
    s: &'s mut State<'a, 'x, 'y>,
    start: usize,
}

impl<'a, 'x, 'y, 's> Grouped<'a, 'x, 'y, 's> {
    fn get(&self) -> &[Pair<'a>] {
        &self.s.sink[self.start..]
    }
    fn get_mut(&mut self) -> (&mut Vec<Pair<'a>>, usize) {
        (&mut self.s.sink, self.start)
    }
    /// Truncates the sink back to `start` and returns the state so that the
    /// finished element can be re-visited.
    fn end(self) -> &'s mut State<'a, 'x, 'y> {
        self.s.sink.truncate(self.start);
        self.s
    }
}

// ── Show-rule verdict ─────────────────────────────────────────────────────────

struct Verdict<'a> {
    prepared: bool,
    map: Styles,
    step: Option<ShowStep<'a>>,
}

enum ShowStep<'a> {
    Recipe(&'a Recipe, RecipeIndex),
    /// Result pre-computed from a built-in show rule.
    BuiltIn(Content),
}

// ── visit: the core dispatch ──────────────────────────────────────────────────

/// Handles an arbitrary piece of content during realization.
fn visit<'a>(
    s: &mut State<'a, '_, '_>,
    content: &'a Content,
    styles: StyleChain<'a>,
) -> SourceResult<()> {
    // Tags from an outer realize pass (e.g. inside a show rule body that went
    // through typst's normal pipeline): silently discard them — we do not
    // generate or consume tags in terminal realization.
    if content.is::<TagElem>() {
        return Ok(());
    }

    // Apply user-defined show rules and run element preparation.
    if visit_show_rules(s, content, styles)? {
        return Ok(());
    }

    // Transparently flatten sequences.
    if let Some(seq) = content.to_packed::<SequenceElem>() {
        for elem in &seq.children {
            visit(s, elem, styles)?;
        }
        return Ok(());
    }

    // Transparently chain styled elements.
    if let Some(styled) = content.to_packed::<StyledElem>() {
        return visit_styled(s, &styled.child, Cow::Borrowed(&styled.styles), styles);
    }

    // Transparently wrap mathy content in an EquationElem so that user show
    // rules targeting `equation` work correctly.

    // Math mode: keep SymbolElem as-is (don't convert to TextElem).
    // Document mode: wrap Mathy in EquationElem, convert SymbolElem -> TextElem.
    if s.kind != TermRealizationKind::Math {
        if content.can::<dyn Mathy>() && !content.is::<EquationElem>() {
            let eq = EquationElem::new(content.clone())
                .pack()
                .spanned(content.span());
            visit(s, s.store(eq), styles)?;
            return Ok(());
        }
        if let Some(sym) = content.to_packed::<typst::foundations::SymbolElem>() {
            let text = TextElem::packed(sym.text.clone()).spanned(sym.span());
            visit(s, s.store(text), styles)?;
            return Ok(());
        }
    }

    // Evaluate ContextElem: apply the built-in show rule to resolve the context
    // closure.  This is handled here (after user show rules and transparent
    // unwrapping, but before grouping) so that #context works everywhere —
    // not just inside math.  If the built-in rule is absent (should never
    // happen in practice) we simply discard the element to avoid pushing an
    // unevaluated ContextElem into the layout stage.
    if content.is::<ContextElem>() {
        let target = styles.get(TargetElem::target);
        if let Some(rule) = s.engine.routines.rules.get(target, content) {
            let result = rule.apply(content, s.engine, styles)?;
            visit(s, s.store(result), styles)?;
        }
        return Ok(());
    }

    // Apply built-in terminal show rules for simple inline-styling elements.
    // These rules mirror what the paged target does: transform the element into
    // its body with the relevant TextElem style field set, so that `convert.rs`
    // only needs to handle `TextElem` for styling (it already reads `delta` and
    // `emph` from the StyleChain).  User show rules fired above take priority.
    if visit_term_rules(s, content, styles)? {
        return Ok(());
    }

    // Grouping rules (PAR, LIST, ENUM, TERMS).
    if visit_grouping_rules(s, content, styles)? {
        return Ok(());
    }

    // Filter rules: discard spaces and paragraph-breaks at the top level.
    if visit_filter_rules(s, content, styles)? {
        return Ok(());
    }

    s.sink.push((content, styles));
    Ok(())
}

// ── Show rules ────────────────────────────────────────────────────────────────

/// Tries to apply user-defined show rules and/or run element preparation.
/// Returns `true` if the element was fully handled (need not be pushed).
fn visit_show_rules<'a>(
    s: &mut State<'a, '_, '_>,
    content: &'a Content,
    styles: StyleChain<'a>,
) -> SourceResult<bool> {
    let Some(Verdict {
        prepared,
        mut map,
        step,
    }) = verdict(s.engine, content, styles)
    else {
        return Ok(false);
    };

    let mut output = Cow::Borrowed(content);

    if !prepared {
        prepare(
            s.engine,
            output.to_mut(),
            &mut map,
            styles,
            &mut s.loc_counter,
        )?;
    }

    if let Some(step) = step {
        output = match step {
            ShowStep::Recipe(recipe, guard) => {
                let chained = styles.chain(&map);
                let result = {
                    let context = Context::new(output.location(), Some(chained));
                    recipe.apply(
                        s.engine,
                        context.track(),
                        output.into_owned().guarded(guard),
                    )
                };
                Cow::Owned(s.engine.delay(result))
            }
            ShowStep::BuiltIn(content) => {
                // Built-in rule already produced the result.
                Cow::Owned(content)
            }
        };
    }

    let realized = match output {
        Cow::Borrowed(r) => r,
        Cow::Owned(r) => s.store(r),
    };

    s.engine.route.increase();
    s.engine.route.check_show_depth().at(content.span())?;
    visit_styled(s, realized, Cow::Owned(map), styles)?;
    s.engine.route.decrease();

    Ok(true)
}

/// Inspects `elem` and the current `styles` to decide whether and how to apply
/// show rules. Returns `None` if neither preparation nor a show rule is needed.
fn verdict<'a>(
    engine: &mut Engine,
    elem: &'a Content,
    styles: StyleChain<'a>,
) -> Option<Verdict<'a>> {
    let prepared = elem.is_prepared();
    let mut map = Styles::new();
    let mut step = None;

    // Pre-synthesis on a clone: lets show-set rules match synthesized fields
    // before real synthesis runs (e.g. `show figure.where(kind: table)`).
    let mut elem = elem;
    let mut slot;
    if !prepared && elem.can::<dyn Synthesize>() {
        slot = elem.clone();
        slot.with_mut::<dyn Synthesize>()
            .unwrap()
            .synthesize(engine, styles)
            .ok();
        elem = &slot;
    }

    // Lazily compute recipe chain depth (needed to build `RecipeIndex`).
    let depth = LazyCell::new(|| styles.recipes().count());

    for (r, recipe) in styles.recipes().enumerate() {
        if !recipe
            .selector()
            .is_some_and(|sel| sel.matches(elem, Some(styles)))
        {
            continue;
        }

        // Show-set rules: accumulate styles, don't count as a "step".
        if let Transformation::Style(transform) = recipe.transform() {
            if !prepared {
                map.apply(transform.clone());
            }
            continue;
        }

        if step.is_some() {
            continue;
        }

        let index = RecipeIndex(*depth - r);
        if elem.is_guarded(index) {
            continue;
        }

        step = Some(ShowStep::Recipe(recipe, index));
        if prepared {
            break;
        }
    }

    // If no user show rule matched, try built-in rules from the engine.
    // Since our Routines only contains terminal rules, this won't pick up
    // paged rules.
    if step.is_none() {
        let target = styles.get(TargetElem::target);
        if let Some(rule) = engine.routines.rules.get(target, elem) {
            let result = rule.apply(elem, engine, styles).ok()?;
            step = Some(ShowStep::BuiltIn(result));
        }
    }

    // Nothing to do?
    if step.is_none()
        && map.is_empty()
        && (prepared || {
            elem.label().is_none()
                && elem.location().is_none()
                && !elem.can::<dyn ShowSet>()
                && !elem.can::<dyn typst::introspection::Locatable>()
                && !elem.can::<dyn typst::introspection::Tagged>()
                && !elem.can::<dyn Synthesize>()
        })
    {
        return None;
    }

    Some(Verdict {
        prepared,
        map,
        step,
    })
}

/// First-time preparation of an element: applies show-set rules, synthesizes
/// fields, and materializes the style chain into the element.
/// Unlike typst-realize, we do **not** generate location/tag pairs.
fn prepare(
    engine: &mut Engine,
    elem: &mut Content,
    map: &mut Styles,
    styles: StyleChain,
    loc_counter: &mut u64,
) -> SourceResult<()> {
    if let Some(show_settable) = elem.with::<dyn ShowSet>() {
        map.apply(show_settable.show_set(styles));
    }
    if let Some(synthesizable) = elem.with_mut::<dyn Synthesize>() {
        synthesizable.synthesize(engine, styles.chain(map))?;
    }
    if elem.can::<dyn typst::introspection::Locatable>() {
        *loc_counter += 1;
        let loc = typst::introspection::Location::new(*loc_counter as u128);
        elem.set_location(loc);
    }
    elem.materialize(styles.chain(map));
    elem.mark_prepared();
    Ok(())
}

// ── visit_styled ──────────────────────────────────────────────────────────────

/// Handles a block of styles applied to a child element.
fn visit_styled<'a>(
    s: &mut State<'a, '_, '_>,
    content: &'a Content,
    local: Cow<'a, Styles>,
    outer: StyleChain<'a>,
) -> SourceResult<()> {
    if local.is_empty() {
        return visit(s, content, outer);
    }

    // Populate document metadata from document-level set rules.
    for style in local.iter() {
        let Some(elem_type) = style.element() else {
            continue;
        };
        if elem_type == typst::model::DocumentElem::ELEM {
            s.info.populate(&*local);
        } else if elem_type == TextElem::ELEM {
            s.info.populate_locale(&*local);
        }
        // PageElem set rules are silently ignored in terminal realization.
    }

    // Lifetime-extend `outer` and `local` into the arena so that the chained
    // StyleChain can outlive this stack frame (required by the `'a` lifetime
    // on `Pair`).
    let outer = s.arenas.bump.alloc(outer);
    let local = match local {
        Cow::Borrowed(map) => map,
        Cow::Owned(owned) => &*s.arenas.styles.alloc(owned),
    };

    finish_interrupted(s, local)?;
    visit(s, content, outer.chain(local))?;
    finish_interrupted(s, local)?;

    Ok(())
}

// ── Grouping rules ────────────────────────────────────────────────────────────

/// Tries to add `content` to an active grouping or start a new one.
/// Returns `true` if the element was consumed by a grouping.
fn visit_grouping_rules<'a>(
    s: &mut State<'a, '_, '_>,
    content: &'a Content,
    styles: StyleChain<'a>,
) -> SourceResult<bool> {
    let matching = rules_for(s.kind).iter().find(|r| (r.trigger)(content));

    let mut i = 0;
    while let Some(active) = s.groupings.last() {
        // Start a nested group if a higher-priority rule matches.
        if matching.is_some_and(|rule| rule.priority > active.rule.priority) {
            break;
        }

        // Add to the active grouping if it's not interrupted and the element
        // is a trigger or an inner element.
        if !active.interrupted && ((active.rule.trigger)(content) || (active.rule.inner)(content)) {
            s.sink.push((content, styles));
            return Ok(true);
        }

        finish_innermost_grouping(s)?;
        i += 1;
        if i > 512 {
            bail!(content.span(), "maximum grouping depth exceeded");
        }
    }

    if let Some(rule) = matching {
        let start = s.sink.len();
        s.groupings.push(Grouping {
            start,
            rule,
            interrupted: false,
        });
        s.sink.push((content, styles));
        return Ok(true);
    }

    Ok(false)
}

/// Filter rules: suppress elements that should not appear in the top-level
/// output stream.
fn visit_filter_rules<'a>(
    s: &mut State<'a, '_, '_>,
    content: &'a Content,
    styles: StyleChain<'a>,
) -> SourceResult<bool> {
    if s.kind == TermRealizationKind::Document && content.is::<SpaceElem>() {
        // Spaces outside paragraphs are meaningless at the document level.
        return Ok(true);
    }
    if content.is::<ParbreakElem>() {
        s.may_attach = false;
        return Ok(true);
    }
    // Suppress "attach" spacing unless immediately following a paragraph.
    if !s.may_attach
        && content
            .to_packed::<VElem>()
            .is_some_and(|e| e.attach.get(styles))
    {
        return Ok(true);
    }

    s.may_attach = content.is::<ParElem>();
    // Cherry-picked built-in: ContextElem
    if content.is::<ContextElem>() {
        let target = styles.get(TargetElem::target);
        if let Some(rule) = s.engine.routines.rules.get(target, content) {
            let result = rule.apply(content, s.engine, styles)?;
            visit(s, s.store(result), styles)?;
            return Ok(true);
        }
    }
    // Cherry-picked built-in: HideElem
    if content.is::<HideElem>() {
        return Ok(true);
    }
    Ok(false)
}

// ── Built-in terminal show rules ──────────────────────────────────────────────

/// Applies built-in show rules for inline-styling elements whose only effect is
/// to push a style field onto the `TextElem` StyleChain.
///
/// Returns `true` if the element was handled (the caller should return early).
///
/// Why only `strong` and `emph`?
/// - `strong` → sets `TextElem::delta` (weight delta). `convert.rs::TextElem`
///   already reads `styles.get(TextElem::delta)` and sets `Attribute::Bold`.
/// - `emph` → sets `TextElem::emph`. `convert.rs::TextElem` already reads
///   `styles.get(TextElem::emph)` and sets `Attribute::Italic`.
/// - Deco elements (`underline`, `strike`, `overline`, `highlight`) would need
///   `Decoration` + `SmallVec` construction — complex with no reduction in code.
///   They stay in `convert.rs` where the terminal styling is one clear branch.
/// - `sub`/`super` use custom dim-color logic (no standard typst style field).
/// - `link` requires destination resolution; stays in `convert.rs`.
fn visit_term_rules<'a>(
    s: &mut State<'a, '_, '_>,
    content: &'a Content,
    styles: StyleChain<'a>,
) -> SourceResult<bool> {
    if let Some(elem) = content.to_packed::<StrongElem>() {
        let delta = elem.delta.get(styles);
        let body = s.store(elem.body.clone().set(TextElem::delta, WeightDelta(delta)));
        visit(s, body, styles)?;
        return Ok(true);
    }

    if let Some(elem) = content.to_packed::<EmphElem>() {
        // EmphElem toggles italic; `ItalicToggle` XORs with the current state.
        let body = s.store(elem.body.clone().set(TextElem::emph, ItalicToggle(true)));
        visit(s, body, styles)?;
        return Ok(true);
    }

    Ok(false)
}

// ── Grouping finish ───────────────────────────────────────────────────────────

/// Finishes all active groupings.
fn finish(s: &mut State) -> SourceResult<()> {
    finish_grouping_while(s, |s| !s.groupings.is_empty())
}

/// Finishes any grouping that is interrupted by the given styles.
fn finish_interrupted(s: &mut State, local: &Styles) -> SourceResult<()> {
    let mut last = None;
    for elem in local.iter().filter_map(|style| style.element()) {
        if last == Some(elem) {
            continue;
        }
        finish_grouping_while(s, |s| s.groupings.iter().any(|g| (g.rule.interrupt)(elem)))?;
        last = Some(elem);
    }
    Ok(())
}

fn finish_grouping_while<F>(s: &mut State, mut f: F) -> SourceResult<()>
where
    F: FnMut(&mut State) -> bool,
{
    let mut i = 0;
    while f(s) {
        finish_innermost_grouping(s)?;
        i += 1;
        if i > 512 {
            bail!(Span::detached(), "maximum grouping depth exceeded");
        }
    }
    Ok(())
}

/// Pops and finalizes the innermost active grouping.
fn finish_innermost_grouping(s: &mut State) -> SourceResult<()> {
    let Grouping { start, rule, .. } = s.groupings.pop().unwrap();

    // Trim trailing "inner" (non-trigger) elements from the group edges.
    let trimmed = s.sink[start..].trim_end_matches(|(c, _)| !(rule.trigger)(c));
    let end = start + trimmed.len();

    // Stash elements that come after the group; we will re-visit them.
    // `Pair<'a>` is Copy, so this plain Vec is fine.
    let tail: Vec<_> = s.sink[end..].to_vec();
    s.sink.truncate(end);

    (rule.finish)(Grouped { s, start })?;

    for (content, styles) in tail {
        visit(s, content, styles)?;
    }
    Ok(())
}

// ── Static grouping rules ─────────────────────────────────────────────────────

const MAX_GROUP_NESTING: usize = 3;

static TERM_DOCUMENT_RULES: &[&GroupingRule] = &[&PAR, &LIST, &ENUM, &TERMS];
static TERM_MATH_RULES: &[&GroupingRule] = &[&LIST, &ENUM, &TERMS];
static TERM_INLINE_RULES: &[&GroupingRule] = &[];

fn rules_for(kind: TermRealizationKind) -> &'static [&'static GroupingRule] {
    match kind {
        TermRealizationKind::Document => TERM_DOCUMENT_RULES,
        TermRealizationKind::Math => TERM_MATH_RULES,
        TermRealizationKind::Inline => TERM_INLINE_RULES,
    }
}

/// Groups consecutive inline-level elements into a `ParElem`.
static PAR: GroupingRule = GroupingRule {
    priority: 1,
    trigger: |content| {
        let e = content.elem();
        e == TextElem::ELEM
            || e == HElem::ELEM
            || e == LinebreakElem::ELEM
            || e == SmartQuoteElem::ELEM
            || e == InlineElem::ELEM
            || e == TermInlineElem::ELEM
            || e == BoxElem::ELEM
            // Inline styling wrappers: no paged built-in show rules unwrap these,
            // so we include them directly as PAR members.
            || e == typst::model::StrongElem::ELEM
            || e == typst::model::EmphElem::ELEM
            || e == typst::model::LinkElem::ELEM
            || e == typst::model::QuoteElem::ELEM
            || e == UnderlineElem::ELEM
            || e == StrikeElem::ELEM
            || e == OverlineElem::ELEM
            || e == HighlightElem::ELEM
            || e == SubElem::ELEM
            || e == SuperElem::ELEM
            || e == RawElem::ELEM
            || (e == EquationElem::ELEM
                && content.to_packed::<EquationElem>()
                    .is_some_and(|eq| eq.block.as_option() != &Some(true)))
            // HideElem is treated as inline so that #hide[text] in a paragraph
            // stays inside the paragraph (preserving horizontal space).
            || e == HideElem::ELEM
    },
    inner: |content| content.elem() == SpaceElem::ELEM,
    interrupt: |elem| elem == ParElem::ELEM || elem == AlignElem::ELEM,
    finish: finish_par,
};

/// Groups `ListItem`s into a `ListElem`.
static LIST: GroupingRule = list_like_grouping::<ListElem>();

/// Groups `EnumItem`s into an `EnumElem`.
static ENUM: GroupingRule = list_like_grouping::<EnumElem>();

/// Groups `TermItem`s into a `TermsElem`.
static TERMS: GroupingRule = list_like_grouping::<TermsElem>();

const fn list_like_grouping<T: ListLike>() -> GroupingRule {
    GroupingRule {
        priority: 2,
        trigger: |content| content.elem() == T::Item::ELEM,
        inner: |content| {
            let e = content.elem();
            e == SpaceElem::ELEM || e == ParbreakElem::ELEM
        },
        interrupt: |elem| elem == T::ELEM || elem == AlignElem::ELEM,
        finish: finish_list_like::<T>,
    }
}

// ── Grouping finishers ────────────────────────────────────────────────────────

fn finish_par(mut grouped: Grouped) -> SourceResult<()> {
    let (sink, start) = grouped.get_mut();
    collapse_spaces(sink, start);

    let elems = grouped.get();
    let span = select_span(elems);
    let (body, trunk) = repack(elems);

    let s = grouped.end();
    let elem = ParElem::new(body).pack().spanned(span);
    visit(s, s.store(elem), trunk)
}

fn finish_list_like<T: ListLike>(grouped: Grouped) -> SourceResult<()> {
    let elems = grouped.get();
    let span = select_span(elems);
    let tight = !elems.iter().any(|(c, _)| c.is::<ParbreakElem>());

    let trunk = StyleChain::trunk(
        elems
            .iter()
            .filter(|(c, _)| c.elem() == T::Item::ELEM)
            .map(|&(_, s)| s),
    )
    .unwrap();
    let trunk_depth = trunk.links().count();

    let children: Vec<Packed<T::Item>> = elems
        .iter()
        .copied()
        .filter_map(|(c, s)| {
            let item = c.to_packed::<T::Item>()?.clone();
            let local = s.suffix(trunk_depth);
            Some(T::Item::styled(item, local))
        })
        .collect();

    let s = grouped.end();
    let elem = T::create(children, tight).pack().spanned(span);
    visit(s, s.store(elem), trunk)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn select_span(children: &[Pair]) -> Span {
    Span::find(children.iter().map(|(c, _)| c.span()))
}

/// Repacks a slice of `(content, styles)` pairs back into a single `Content`
/// with a trunk `StyleChain`.
fn repack<'a>(buf: &[Pair<'a>]) -> (Content, StyleChain<'a>) {
    let trunk = StyleChain::trunk_from_pairs(buf).unwrap_or_default();
    let depth = trunk.links().count();
    let mut seq = Vec::with_capacity(buf.len());

    for (chain, group) in buf.group_by_key(|&(_, s)| s) {
        let iter = group.iter().map(|&(c, _)| c.clone());
        let suffix = chain.suffix(depth);
        if suffix.is_empty() {
            seq.extend(iter);
        } else if let &[(element, _)] = group {
            seq.push(element.clone().styled_with_map(suffix));
        } else {
            seq.push(Content::sequence(iter).styled_with_map(suffix));
        }
    }

    (Content::sequence(seq), trunk)
}

// ── Space collapsing (from typst-realize/src/spaces.rs) ──────────────────────

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum SpaceState {
    /// The element is invisible for collapsing purposes.
    Invisible,
    /// Destroys adjacent spaces.
    Destructive,
    /// Normal: spaces survive only when supported on both sides.
    Supportive,
    /// A space element; adjacent ones collapse into the first.
    Space,
}

/// Collapse adjacent and edge `SpaceElem`s in `buf[start..]` in-place.
fn collapse_spaces(buf: &mut Vec<Pair>, start: usize) {
    let mut cursor = start;
    let mut prev_space = cursor;
    let mut state = SpaceState::Destructive;

    for i in start..buf.len() {
        let (content, styles) = buf[i];
        state = match collapse_state(content, styles) {
            SpaceState::Invisible => state,
            SpaceState::Destructive => {
                if state == SpaceState::Space {
                    buf.copy_within(prev_space + 1..cursor, prev_space);
                    cursor -= 1;
                }
                SpaceState::Destructive
            }
            SpaceState::Supportive => SpaceState::Supportive,
            SpaceState::Space => {
                if state != SpaceState::Supportive {
                    continue;
                }
                prev_space = cursor;
                SpaceState::Space
            }
        };
        if cursor < i {
            buf[cursor] = buf[i];
        }
        cursor += 1;
    }

    if state == SpaceState::Space {
        buf.copy_within(prev_space + 1..cursor, prev_space);
        cursor -= 1;
    }
    buf.truncate(cursor);
}

fn collapse_state(content: &Content, styles: StyleChain) -> SpaceState {
    if let Some(elem) = content.to_packed::<HElem>() {
        if elem.amount.is_fractional() || elem.weak.get(styles) {
            SpaceState::Destructive
        } else {
            SpaceState::Invisible
        }
    } else if content.is::<LinebreakElem>() {
        SpaceState::Destructive
    } else if content.is::<SpaceElem>() {
        SpaceState::Space
    } else {
        SpaceState::Supportive
    }
}
