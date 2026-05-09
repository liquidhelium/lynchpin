//! Terminal-specific show rules.
//!
//! These rules mirror the paged rules in `lynchpin-layout/src/rules.rs`,
//! but produce [`TermBlockElem`] with terminal-specific callbacks instead
//! of `BlockElem::multi_layouter`.
//!
//! Each rule wraps the original element in a [`TermBlockElem`] whose
//! callback calls the corresponding terminal layout function.

use typst::foundations::{NativeElement, NativeRuleMap, ShowFn};
use typst::foundations::{Context, dict};
use typst::layout::{Abs, GridElem, LayoutElem, PadElem, StackElem};
use typst::model::TableElem;
use comemo::Track;

use lynchpin_library::{TermBlockCallback, TermBlockElem, frame::TermFrame};

use crate::grid::{layout_grid, layout_table};

/// Register terminal show rules into `rules`.
pub fn register(rules: &mut NativeRuleMap) {
    use typst::foundations::Target;

    rules.register(Target::Paged, STACK_RULE);
    rules.register(Target::Paged, GRID_RULE);
    rules.register(Target::Paged, TABLE_RULE);
    rules.register(Target::Paged, LAYOUT_RULE);
    rules.register(Target::Paged, PAD_RULE);
}

// ── Stack ────────────────────────────────────────────────────────────────────

const STACK_RULE: ShowFn<StackElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| crate::stack::layout_stack(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

// ── Grid ─────────────────────────────────────────────────────────────────────

const GRID_RULE: ShowFn<GridElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| layout_grid(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

// ── Table ────────────────────────────────────────────────────────────────────

const TABLE_RULE: ShowFn<TableElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| layout_table(elem, engine, config, styles),
    ))
    .pack()
    .spanned(elem.span()))
};

// ── Place — handled directly in handle_block (flow.rs), no show rule.

// ── Layout ────────────────────────────────────────────────────────────────────

const LAYOUT_RULE: ShowFn<LayoutElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let width_cols = config.width.unwrap_or(80) as f64;
            let width = Abs::pt(width_cols * 8.4);
            let height = Abs::pt(1000.0);
            let loc = elem.location();
            let context = Context::new(loc, Some(styles));
            let result = elem.func.call(
                engine,
                context.track(),
                [dict! { "width" => width, "height" => height }],
            )?.display();
            // Layout the returned content as a paragraph.
            crate::inline::layout_paragraph(
                engine, &result, config, styles,
                crossterm::style::ContentStyle::default(),
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};

// ── Pad ──────────────────────────────────────────────────────────────────────

const PAD_RULE: ShowFn<PadElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            // For now, just layout the body; padding is not yet implemented.
            crate::inline::layout_paragraph(
                engine, &elem.body, config, styles,
                crossterm::style::ContentStyle::default(),
            )
        },
    ))
    .pack()
    .spanned(elem.span()))
};
