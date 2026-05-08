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
use typst::layout::{Abs, GridElem, LayoutElem, PadElem, PlaceElem, StackElem};
use typst::model::TableElem;
use comemo::Track;

use lynchpin_library::{TermBlockCallback, TermBlockElem, frame::TermFrame};

use crate::grid::{layout_grid, layout_table};
use crate::stack::compose_vertical;

/// Register terminal show rules into `rules`.
pub fn register(rules: &mut NativeRuleMap) {
    use typst::foundations::Target;

    rules.register(Target::Paged, STACK_RULE);
    rules.register(Target::Paged, GRID_RULE);
    rules.register(Target::Paged, TABLE_RULE);
    rules.register(Target::Paged, PLACE_RULE);
    rules.register(Target::Paged, LAYOUT_RULE);
    rules.register(Target::Paged, PAD_RULE);
}

// ── Stack ────────────────────────────────────────────────────────────────────

const STACK_RULE: ShowFn<StackElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let dir = elem.dir.get(styles);
            let mut frames: Vec<TermFrame> = Vec::new();
            for c in &elem.children {
                use typst::layout::StackChild;
                match c {
                    StackChild::Block(content) => {
                        // Layout child content as a paragraph.
                        let frame = crate::inline::layout_paragraph(
                            engine, content, config, styles,
                            crossterm::style::ContentStyle::default(),
                        )?;
                        if !frame.size().is_empty() {
                            frames.push(frame);
                        }
                    }
                    StackChild::Spacing(_) => {
                        if !frames.is_empty() {
                            frames.push(TermFrame::new(lynchpin_library::frame::TermSize::new(0, 1)));
                        }
                    }
                }
            }
            if frames.is_empty() {
                return Ok(TermFrame::new(lynchpin_library::frame::TermSize::ZERO));
            }
            let horiz = dir.axis() == typst::layout::Axis::X;
            if horiz {
                Ok(crate::stack::compose_horizontal(frames, 1))
            } else {
                Ok(compose_vertical(frames, 1, 0))
            }
        },
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

// ── Place ────────────────────────────────────────────────────────────────────

const PLACE_RULE: ShowFn<PlaceElem> = |elem, _, _| {
    Ok(TermBlockElem::new(TermBlockCallback::new(
        elem.clone(),
        |elem, engine, config, styles| {
            let body_frame = crate::inline::layout_paragraph(
                engine, &elem.body, config, styles,
                crossterm::style::ContentStyle::default(),
            )?;
            let max_w = config.width.map(|w| w as i32).unwrap_or(body_frame.cols() as i32);
            let cx = ((max_w - body_frame.cols()).max(0)) / 2;
            let mut frame = TermFrame::new(lynchpin_library::frame::TermSize::new(
                max_w.max(0),
                body_frame.rows(),
            ));
            frame.push_frame(
                lynchpin_library::frame::TermPoint::new(cx, 0),
                body_frame,
            );
            Ok(frame)
        },
    ))
    .pack()
    .spanned(elem.span()))
};

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
