//! Terminal attachment layout (superscripts, subscripts, limits, primes).
//!
//! Handles [`AttachElem`], [`PrimesElem`], [`ScriptsElem`], and
//! [`LimitsElem`].

use typst::diag::SourceResult;
use typst::foundations::{Packed, StyleChain, SymbolElem};
use typst::math::{AttachElem, LimitsElem, PrimesElem, ScriptsElem};

use crate::frame::{Col, Row, TermFrame, TermPoint, TermSize};
use crate::stack::{compose_horizontal, compose_vertical};

use super::{TermLimits, TermMathContext, TermMathFragment, TermMathFrameFragment};
use crate::pad;

// ── layout_attach ─────────────────────────────────────────────────────────────

/// Layout an [`AttachElem`].
///
/// Supports:
/// * **limits mode** – top/bottom annotations stacked above/below the base
///   (used for large operators like ∑ in display equations).
/// * **scripts mode** – superscripts placed top-right, subscripts bottom-right.
/// * **pre-scripts** (tl, bl) – placed to the left of the base.
pub fn layout_attach(
    elem: &Packed<AttachElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    // Merge nested AttachElems where possible (mirrors typst-layout behaviour).
    let merged = elem.merge_base();
    let elem = merged.as_ref().unwrap_or(elem);

    // ── Layout base ───────────────────────────────────────────────────────────
    let base_frag = ctx.layout_into_fragment(&elem.base, styles)?;
    let use_limits = base_frag.limits().active(ctx.is_display);

    // ── Collect attachments ───────────────────────────────────────────────────
    // For `t` / `b` we prefer the dedicated slot over tr / br when limits are
    // active, mirroring the typst-layout logic.
    let t_content  = elem.t.get_cloned(styles);
    let b_content  = elem.b.get_cloned(styles);
    let tr_content = elem.tr.get_cloned(styles);
    let br_content = elem.br.get_cloned(styles);
    let tl_content = elem.tl.get_cloned(styles);
    let bl_content = elem.bl.get_cloned(styles);

    // Decide what goes above / top-right and below / bottom-right.
    // When in limits mode: t → above, b → below (and tr/br fall through to
    //   scripts on the right if also set).
    // When in scripts mode: t and tr merge (t → tr), b and br merge (b → br).
    let (above, top_right) = if use_limits {
        (t_content, tr_content)
    } else {
        // In scripts mode `t` is an alias for `tr`
        let combined_tr = match (t_content, tr_content) {
            (Some(t), Some(tr)) => Some(Content::sequence([tr, t])),
            (Some(t), None)     => Some(t),
            (None, tr)          => tr,
        };
        (None, combined_tr)
    };

    let (below, bot_right) = if use_limits {
        (b_content, br_content)
    } else {
        // In scripts mode `b` is an alias for `br`
        let combined_br = match (b_content, br_content) {
            (Some(b), Some(br)) => Some(Content::sequence([br, b])),
            (Some(b), None)     => Some(b),
            (None, br)          => br,
        };
        (None, combined_br)
    };

    // ── Layout ───────────────────────────────────────────────────────────────

    if use_limits {
        layout_limits_mode(
            ctx, styles,
            base_frag,
            above.as_ref(),
            below.as_ref(),
            tl_content.as_ref(),
            bl_content.as_ref(),
            top_right.as_ref(),
            bot_right.as_ref(),
        )
    } else {
        layout_scripts_mode(
            ctx, styles,
            base_frag,
            top_right.as_ref(),
            bot_right.as_ref(),
            tl_content.as_ref(),
            bl_content.as_ref(),
        )
    }
}

// ── Limits layout ─────────────────────────────────────────────────────────────

/// Stack `above`, `base`, `below` vertically, centred.
///
/// Any residual tr/br (when limits mode is active for only `t`/`b`) are placed
/// as scripts to the right of the composed stack.
#[allow(clippy::too_many_arguments)]
fn layout_limits_mode(
    ctx: &mut TermMathContext,
    styles: StyleChain,
    base_frag: TermMathFragment,
    above: Option<&typst::foundations::Content>,
    below: Option<&typst::foundations::Content>,
    tl: Option<&typst::foundations::Content>,
    bl: Option<&typst::foundations::Content>,
    tr: Option<&typst::foundations::Content>,
    br: Option<&typst::foundations::Content>,
) -> SourceResult<()> {
    // Layout optional limit fragments.
    let above_frame = above.map(|c| ctx.layout_into_frame(c, styles)).transpose()?;
    let below_frame = below.map(|c| ctx.layout_into_frame(c, styles)).transpose()?;

    let base_frame = base_frag.into_frame();

    // Center above/below relative to base width.
    let limit_width = above_frame.as_ref().map(|f| f.cols())
        .max(below_frame.as_ref().map(|f| f.cols()))
        .unwrap_or(0)
        .max(base_frame.cols());

    // Determine the baseline index for compose_vertical.
    let mut frames: Vec<TermFrame> = Vec::new();
    let mut baseline_idx: usize = 0;

    if let Some(af) = above_frame {
        let ac = af.cols();
        let pad = (limit_width - ac).max(0) / 2;
        frames.push(if pad > 0 { pad::pad_h(af, pad, limit_width - ac - pad) } else { af });
        baseline_idx = 1;
    }
    let bc = base_frame.cols();
    let pad = (limit_width - bc).max(0) / 2;
    frames.push(if pad > 0 { pad::pad_h(base_frame, pad, limit_width - bc - pad) } else { base_frame });
    if let Some(bf) = below_frame {
        let bfc = bf.cols();
        let pad = (limit_width - bfc).max(0) / 2;
        frames.push(if pad > 0 { pad::pad_h(bf, pad, limit_width - bfc - pad) } else { bf });
    }

    let stack = compose_vertical(frames, /*gap=*/0, baseline_idx);

    // Handle pre-scripts (tl, bl) to the left.
    let pre_frame = build_pre_scripts(ctx, styles, stack.cols(), stack.rows(), stack.baseline(), tl, bl)?;

    // Handle post-scripts (tr, br) to the right if also present.
    let post_frame = build_post_scripts(ctx, styles, stack.cols(), stack.rows(), stack.baseline(), tr, br)?;

    let final_frame = assemble_with_pre_post(pre_frame, stack, post_frame);
    ctx.push(TermMathFrameFragment::new(final_frame));
    Ok(())
}

// ── Scripts layout ────────────────────────────────────────────────────────────

/// Place superscripts top-right and subscripts bottom-right.
fn layout_scripts_mode(
    ctx: &mut TermMathContext,
    styles: StyleChain,
    base_frag: TermMathFragment,
    tr: Option<&typst::foundations::Content>,
    br: Option<&typst::foundations::Content>,
    tl: Option<&typst::foundations::Content>,
    bl: Option<&typst::foundations::Content>,
) -> SourceResult<()> {
    let italics = base_frag.italics_correction();
    let base_frame = base_frag.into_frame();

    let pre_frame = build_pre_scripts(ctx, styles, base_frame.cols(), base_frame.rows(), base_frame.baseline(), tl, bl)?;
    let post_frame = build_post_scripts_with_ic(ctx, styles, base_frame.cols(), base_frame.rows(), base_frame.baseline(), tr, br, italics)?;

    let final_frame = assemble_with_pre_post(pre_frame, base_frame, post_frame);
    ctx.push(TermMathFrameFragment::new(final_frame));
    Ok(())
}

// ── Pre-script builder ────────────────────────────────────────────────────────

/// Build a frame containing the pre-scripts (tl, bl) positioned relative to
/// `base_rows` / `base_baseline`.  Returns `None` if no pre-scripts exist.
fn build_pre_scripts(
    ctx: &mut TermMathContext,
    styles: StyleChain,
    _base_cols: Col,
    base_rows: Row,
    base_baseline: Row,
    tl: Option<&typst::foundations::Content>,
    bl: Option<&typst::foundations::Content>,
) -> SourceResult<Option<TermFrame>> {
    if tl.is_none() && bl.is_none() {
        return Ok(None);
    }
    let tl_frame = tl.map(|c| ctx.layout_into_frame(c, styles)).transpose()?;
    let bl_frame = bl.map(|c| ctx.layout_into_frame(c, styles)).transpose()?;

    let script_cols: Col = tl_frame.as_ref().map(|f| f.cols())
        .max(bl_frame.as_ref().map(|f| f.cols()))
        .unwrap_or(0);

    // Add rows above/below base for pre-script placement.
    let top_extra = if tl_frame.is_some() { 1 } else { 0 };
    let bot_extra = if bl_frame.is_some() { 1 } else { 0 };
    let total_rows = base_rows + top_extra + bot_extra;
    let new_baseline = base_baseline + top_extra;
    let mut frame = TermFrame::new(TermSize::new(script_cols, total_rows));
    frame.set_baseline(new_baseline);

    // tl sits above baseline, bl below.
    if let Some(tf) = tl_frame {
        let x = (script_cols - tf.cols()).max(0); // right-align
        frame.push_frame(TermPoint::new(x, 0), tf);
    }
    if let Some(bf) = bl_frame {
        let x = (script_cols - bf.cols()).max(0); // right-align
        let y = top_extra + base_rows;
        frame.push_frame(TermPoint::new(x, y), bf);
    }

    Ok(Some(frame))
}

// ── Post-script builder ───────────────────────────────────────────────────────

/// Build a frame containing post-scripts (tr, br).
fn build_post_scripts(
    ctx: &mut TermMathContext,
    styles: StyleChain,
    base_cols: Col,
    base_rows: Row,
    base_baseline: Row,
    tr: Option<&typst::foundations::Content>,
    br: Option<&typst::foundations::Content>,
) -> SourceResult<Option<TermFrame>> {
    build_post_scripts_with_ic(ctx, styles, base_cols, base_rows, base_baseline, tr, br, 0)
}

/// Like [`build_post_scripts`] but offset the superscript by `italics_correction`.
fn build_post_scripts_with_ic(
    ctx: &mut TermMathContext,
    styles: StyleChain,
    _base_cols: Col,
    base_rows: Row,
    base_baseline: Row,
    tr: Option<&typst::foundations::Content>,
    br: Option<&typst::foundations::Content>,
    _italics_correction: Col,
) -> SourceResult<Option<TermFrame>> {
    if tr.is_none() && br.is_none() {
        return Ok(None);
    }
    let tr_frame = tr.map(|c| ctx.layout_into_frame(c, styles)).transpose()?;
    let br_frame = br.map(|c| ctx.layout_into_frame(c, styles)).transpose()?;

    let script_cols: Col = tr_frame.as_ref().map(|f| f.cols())
        .max(br_frame.as_ref().map(|f| f.cols()))
        .unwrap_or(0);

    // Add rows above/below base for superscript/subscript placement.
    let top_extra = if tr_frame.is_some() { 1 } else { 0 };
    let bot_extra = if br_frame.is_some() { 1 } else { 0 };
    let total_rows = base_rows + top_extra + bot_extra;
    let new_baseline = base_baseline + top_extra;

    let mut frame = TermFrame::new(TermSize::new(script_cols, total_rows));
    frame.set_baseline(new_baseline);

    // Superscript right-aligned above baseline.
    if let Some(tf) = tr_frame {
        let x = (script_cols - tf.cols()).max(0);
        frame.push_frame(TermPoint::new(x, 0), tf);
    }
    // Subscript right-aligned below baseline.
    if let Some(bf) = br_frame {
        let x = (script_cols - bf.cols()).max(0);
        let y = top_extra + base_rows;
        frame.push_frame(TermPoint::new(x, y), bf);
    }

    Ok(Some(frame))
}

// ── Assembly ──────────────────────────────────────────────────────────────────

/// Assemble pre-frame + base + post-frame into a single frame, baseline-aligned.
///
/// Translates the base content vertically so its baseline aligns with
/// pre/post, but does NOT add empty rows to the base frame.
fn assemble_with_pre_post(
    pre: Option<TermFrame>,
    mut base: TermFrame,
    post: Option<TermFrame>,
) -> TermFrame {
    // Shift base content to align baseline with pre/post scripts.
    let target_bl = pre.as_ref().map(|f| f.baseline())
        .max(post.as_ref().map(|f| f.baseline()))
        .unwrap_or(0);
    let shift = target_bl - base.baseline();
    if shift > 0 {
        base.translate(TermPoint::new(0, shift));
        base.set_baseline(target_bl);
    }

    let mut parts: Vec<TermFrame> = Vec::new();
    if let Some(p) = pre  { parts.push(p); }
    parts.push(base);
    if let Some(p) = post { parts.push(p); }

    if parts.len() == 1 {
        return parts.into_iter().next().unwrap();
    }

    compose_horizontal(parts, 0)
}

// ── layout_primes ─────────────────────────────────────────────────────────────

/// Layout a [`PrimesElem`].
///
/// Emits 1–3 combined prime characters (′ ″ ‴), or `count` individual primes
/// for larger values.
pub fn layout_primes(
    elem: &Packed<PrimesElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let count = elem.count;
    let ch = match count {
        1 => '′',
        2 => '″',
        3 => '‴',
        _ => {
            // Emit `count` individual prime symbols.
            for _ in 0..count {
                let frag = ctx.layout_into_fragment(
                    &SymbolElem::packed('′').spanned(elem.span()),
                    styles,
                )?;
                ctx.push(frag);
            }
            return Ok(());
        }
    };

    let frag = ctx.layout_into_fragment(
        &SymbolElem::packed(ch).spanned(elem.span()),
        styles,
    )?;
    ctx.push(frag);
    Ok(())
}

// ── layout_scripts ────────────────────────────────────────────────────────────

/// Layout a [`ScriptsElem`].
///
/// Forces script (non-limits) mode on the body by setting `TermLimits::Never`.
pub fn layout_scripts(
    elem: &Packed<ScriptsElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let mut frag = ctx.layout_into_fragment(&elem.body, styles)?;
    frag.set_limits(TermLimits::Never);
    ctx.push(frag);
    Ok(())
}

// ── layout_limits ─────────────────────────────────────────────────────────────

/// Layout a [`LimitsElem`].
///
/// Forces limits mode on the body.  If `inline` is `false` the limits mode is
/// `TermLimits::Display` (only active in display equations); otherwise
/// `TermLimits::Always`.
pub fn layout_limits(
    elem: &Packed<LimitsElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let mut frag = ctx.layout_into_fragment(&elem.body, styles)?;
    let limits = if elem.inline.get(styles) {
        TermLimits::Always
    } else {
        TermLimits::Display
    };
    frag.set_limits(limits);
    ctx.push(frag);
    Ok(())
}

// ── Content sequence helper ───────────────────────────────────────────────────

use typst::foundations::Content;
