//! Terminal attachment layout (superscripts, subscripts, limits, primes).
//!
//! Handles [`AttachElem`], [`PrimesElem`], [`ScriptsElem`], and
//! [`LimitsElem`].

use crossterm::style::ContentStyle;
use ecow::EcoString;
use lynchpin_library_ng::frame::{Col, Row, TermFrame, TermPoint, TermSize, char_cols};
use typst::diag::SourceResult;
use typst::foundations::{Content, Packed, Resolve, StyleChain, SymbolElem};
use typst::math::{AttachElem, LimitsElem, PrimesElem, ScriptsElem, StretchElem};

use super::fragment::{make_text_frame, TermLimits, TermMathFragment, TermMathFrameFragment};
use super::run::{compose_horizontal, compose_vertical, pad_h};
use super::unicode_scripts;
use super::TermMathContext;

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
    let t_content = elem.t.get_cloned(styles);
    let b_content = elem.b.get_cloned(styles);
    let tr_content = elem.tr.get_cloned(styles);
    let br_content = elem.br.get_cloned(styles);
    let tl_content = elem.tl.get_cloned(styles);
    let bl_content = elem.bl.get_cloned(styles);

    // Decide what goes above / top-right and below / bottom-right.
    let (above, top_right) = if use_limits {
        (t_content, tr_content)
    } else {
        let combined_tr = match (t_content, tr_content) {
            (Some(t), Some(tr)) => Some(Content::sequence([tr, t])),
            (Some(t), None) => Some(t),
            (None, tr) => tr,
        };
        (None, combined_tr)
    };

    let (below, bot_right) = if use_limits {
        (b_content, br_content)
    } else {
        let combined_br = match (b_content, br_content) {
            (Some(b), Some(br)) => Some(Content::sequence([br, b])),
            (Some(b), None) => Some(b),
            (None, br) => br,
        };
        (None, combined_br)
    };

    // ── Horizontal stretch for stretch(=) inside limits ────────────────────
    let mut base_frag = base_frag;
    if let Some(stretch) = stretch_base(&elem.base, styles) {
        let above_frame = above
            .as_ref()
            .map(|c| ctx.layout_into_frame(c, styles))
            .transpose()
            .ok()
            .flatten();
        let below_frame = below
            .as_ref()
            .map(|c| ctx.layout_into_frame(c, styles))
            .transpose()
            .ok()
            .flatten();
        let rel_w_cols = above_frame
            .as_ref()
            .map(|f| f.cols())
            .max(below_frame.as_ref().map(|f| f.cols()))
            .unwrap_or(Col::new(1));
        use typst::foundations::Resolve;
        use typst::layout::Abs;
        use typst::text::TextElem;
        let font_size: Abs = styles.get(TextElem::size).resolve(styles);
        let rel_w_abs = font_size * rel_w_cols.get() as f64;
        let target_abs = stretch.relative_to(rel_w_abs);
        let cols_val = (target_abs.to_pt() / font_size.to_pt()).ceil().max(1.0) as i32;
        let cols = Col::new(cols_val).max(rel_w_cols);
        if let Some(ch) = extract_stretch_base_char(&elem.base) {
            use super::lr::hstretch_char;
            let s = hstretch_char(ch, cols, ctx.config.mode);
            let frame = make_text_frame(&s);
            base_frag = TermMathFrameFragment::new(frame).into();
        }
    }

    // ── Layout ───────────────────────────────────────────────────────────────

    if use_limits {
        layout_limits_mode(
            ctx,
            styles,
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
            ctx,
            styles,
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
#[allow(clippy::too_many_arguments)]
fn layout_limits_mode(
    ctx: &mut TermMathContext,
    styles: StyleChain,
    base_frag: TermMathFragment,
    above: Option<&Content>,
    below: Option<&Content>,
    tl: Option<&Content>,
    bl: Option<&Content>,
    tr: Option<&Content>,
    br: Option<&Content>,
) -> SourceResult<()> {
    // Layout optional limit fragments.
    let above_frame = above
        .map(|c| ctx.layout_into_frame(c, styles))
        .transpose()?;
    let below_frame = below
        .map(|c| ctx.layout_into_frame(c, styles))
        .transpose()?;

    let base_frame = base_frag.into_frame();

    // Center above/below relative to base width.
    let limit_width = above_frame
        .as_ref()
        .map(|f| f.cols())
        .max(below_frame.as_ref().map(|f| f.cols()))
        .unwrap_or(Col::ZERO)
        .max(base_frame.cols());

    // Determine the baseline index for compose_vertical.
    let mut frames: Vec<TermFrame> = Vec::new();
    let mut baseline_idx: usize = 0;

    if let Some(af) = above_frame {
        let ac = af.cols();
        let pad = Col::new((limit_width - ac).get().max(0) / 2);
        frames.push(if pad > Col::ZERO {
            pad_h(af, pad, limit_width - ac - pad)
        } else {
            af
        });
        baseline_idx = 1;
    }
    let bc = base_frame.cols();
    let pad = Col::new((limit_width - bc).get().max(0) / 2);
    frames.push(if pad > Col::ZERO {
        pad_h(base_frame, pad, limit_width - bc - pad)
    } else {
        base_frame
    });
    if let Some(bf) = below_frame {
        let bfc = bf.cols();
        let pad = Col::new((limit_width - bfc).get().max(0) / 2);
        frames.push(if pad > Col::ZERO {
            pad_h(bf, pad, limit_width - bfc - pad)
        } else {
            bf
        });
    }

    let stack = compose_vertical(frames, /*gap=*/ Row::ZERO, baseline_idx);

    // Handle pre-scripts (tl, bl) to the left.
    let pre_frame = build_pre_scripts(
        ctx,
        styles,
        stack.cols(),
        stack.rows(),
        stack.baseline(),
        tl,
        bl,
    )?;

    // Handle post-scripts (tr, br) to the right if also present.
    let post_frame = build_post_scripts(
        ctx,
        styles,
        stack.cols(),
        stack.rows(),
        stack.baseline(),
        tr,
        br,
    )?;

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
    tr: Option<&Content>,
    br: Option<&Content>,
    tl: Option<&Content>,
    bl: Option<&Content>,
) -> SourceResult<()> {
    let italics = base_frag.italics_correction();
    let base_frame = base_frag.into_frame();

    let pre_frame = build_pre_scripts(
        ctx,
        styles,
        base_frame.cols(),
        base_frame.rows(),
        base_frame.baseline(),
        tl,
        bl,
    )?;
    let post_frame = build_post_scripts_with_ic(
        ctx,
        styles,
        base_frame.cols(),
        base_frame.rows(),
        base_frame.baseline(),
        tr,
        br,
        italics,
    )?;

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
    tl: Option<&Content>,
    bl: Option<&Content>,
) -> SourceResult<Option<TermFrame>> {
    if tl.is_none() && bl.is_none() {
        return Ok(None);
    }
    let tl_frame = tl.map(|c| ctx.layout_into_frame(c, styles)).transpose()?;
    let bl_frame = bl.map(|c| ctx.layout_into_frame(c, styles)).transpose()?;

    // ── Unicode inline conversion (same conditions as post-scripts) ─────────
    if ctx.config.mode.is_unicode() {
        let has_both = tl_frame.is_some() && bl_frame.is_some();
        let both_allowed = base_rows >= Row::new(2);

        if !has_both || both_allowed {
            let super_str = tl_frame.as_ref().and_then(|tf| {
                if tf.rows() == Row::new(1) {
                    unicode_scripts::frame_to_superscript(tf)
                } else {
                    None
                }
            });
            let sub_str = bl_frame.as_ref().and_then(|bf| {
                if bf.rows() == Row::new(1) {
                    unicode_scripts::frame_to_subscript(bf)
                } else {
                    None
                }
            });

            let tl_ok = tl_frame.is_none() || super_str.is_some();
            let bl_ok = bl_frame.is_none() || sub_str.is_some();

            if tl_ok && bl_ok && (super_str.is_some() || sub_str.is_some()) {
                return Ok(Some(build_inline_script_frame(
                    super_str.as_deref(),
                    sub_str.as_deref(),
                    base_rows,
                    base_baseline,
                )));
            }
        }
    }
    // ───────────────────────────────────────────────────────────────────

    let script_cols: Col = tl_frame
        .as_ref()
        .map(|f| f.cols())
        .max(bl_frame.as_ref().map(|f| f.cols()))
        .unwrap_or(Col::ZERO);

    // Add rows above/below base for pre-script placement.
    let top_extra = if tl_frame.is_some() { Row::new(1) } else { Row::ZERO };
    let bot_extra = if bl_frame.is_some() { Row::new(1) } else { Row::ZERO };
    let total_rows = base_rows + top_extra + bot_extra;
    let new_baseline = base_baseline + top_extra;
    let mut frame = TermFrame::new(TermSize::new(script_cols, total_rows));
    frame.set_baseline(new_baseline);

    // tl sits above baseline, bl below.
    if let Some(tf) = tl_frame {
        let x = (script_cols - tf.cols()).max(Col::ZERO); // right-align
        frame.push_frame(TermPoint::new(x, Row::ZERO), tf);
    }
    if let Some(bf) = bl_frame {
        let x = (script_cols - bf.cols()).max(Col::ZERO); // right-align
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
    tr: Option<&Content>,
    br: Option<&Content>,
) -> SourceResult<Option<TermFrame>> {
    build_post_scripts_with_ic(ctx, styles, base_cols, base_rows, base_baseline, tr, br, Col::ZERO)
}

/// Like [`build_post_scripts`] but offset the superscript by `italics_correction`.
fn build_post_scripts_with_ic(
    ctx: &mut TermMathContext,
    styles: StyleChain,
    _base_cols: Col,
    base_rows: Row,
    base_baseline: Row,
    tr: Option<&Content>,
    br: Option<&Content>,
    _italics_correction: Col,
) -> SourceResult<Option<TermFrame>> {
    if tr.is_none() && br.is_none() {
        return Ok(None);
    }
    let tr_frame = tr.map(|c| ctx.layout_into_frame(c, styles)).transpose()?;
    let br_frame = br.map(|c| ctx.layout_into_frame(c, styles)).transpose()?;

    // ── Unicode inline conversion ─────────────────────────────────────────
    // Conditions (all must hold):
    //  1. Left/right script — already guaranteed here (not limits mode)
    //  2. If both tr and br exist: base must be ≥2 rows tall
    //     If only one side exists: any base height is fine
    //  3. Each side’s frame must be exactly 1 row
    //  4. Unicode rendering mode
    //  5. Every character on that side has a Unicode super/subscript form
    //  6. ALL existing sides must succeed (no partial inline conversion)
    if ctx.config.mode.is_unicode() {
        let has_both = tr_frame.is_some() && br_frame.is_some();
        let both_allowed = base_rows >= Row::new(2);

        if !has_both || both_allowed {
            let super_str = tr_frame.as_ref().and_then(|tf| {
                if tf.rows() == Row::new(1) {
                    unicode_scripts::frame_to_superscript(tf)
                } else {
                    None
                }
            });
            let sub_str = br_frame.as_ref().and_then(|bf| {
                if bf.rows() == Row::new(1) {
                    unicode_scripts::frame_to_subscript(bf)
                } else {
                    None
                }
            });

            // Require every present side to have converted successfully.
            let tr_ok = tr_frame.is_none() || super_str.is_some();
            let br_ok = br_frame.is_none() || sub_str.is_some();

            if tr_ok && br_ok && (super_str.is_some() || sub_str.is_some()) {
                return Ok(Some(build_inline_script_frame(
                    super_str.as_deref(),
                    sub_str.as_deref(),
                    base_rows,
                    base_baseline,
                )));
            }
        }
    }
    // ─────────────────────────────────────────────────────────────────────

    let script_cols: Col = tr_frame
        .as_ref()
        .map(|f| f.cols())
        .max(br_frame.as_ref().map(|f| f.cols()))
        .unwrap_or(Col::ZERO);

    // Add rows above/below base for superscript/subscript placement.
    let top_extra = if tr_frame.is_some() { Row::new(1) } else { Row::ZERO };
    let bot_extra = if br_frame.is_some() { Row::new(1) } else { Row::ZERO };
    let total_rows = base_rows + top_extra + bot_extra;
    let new_baseline = base_baseline + top_extra;

    let mut frame = TermFrame::new(TermSize::new(script_cols, total_rows));
    frame.set_baseline(new_baseline);

    // Superscript right-aligned above baseline.
    if let Some(tf) = tr_frame {
        let x = (script_cols - tf.cols()).max(Col::ZERO);
        frame.push_frame(TermPoint::new(x, Row::ZERO), tf);
    }
    // Subscript right-aligned below baseline.
    if let Some(bf) = br_frame {
        let x = (script_cols - bf.cols()).max(Col::ZERO);
        let y = top_extra + base_rows;
        frame.push_frame(TermPoint::new(x, y), bf);
    }

    Ok(Some(frame))
}

// ── Assembly ──────────────────────────────────────────────────────────────────

/// Assemble pre-frame + base + post-frame into a single frame, baseline-aligned.
fn assemble_with_pre_post(
    pre: Option<TermFrame>,
    mut base: TermFrame,
    post: Option<TermFrame>,
) -> TermFrame {
    // Shift base content to align baseline with pre/post scripts.
    let target_bl = pre
        .as_ref()
        .map(|f| f.baseline())
        .max(post.as_ref().map(|f| f.baseline()))
        .unwrap_or(Row::ZERO);
    let shift = target_bl - base.baseline();
    if shift > Row::ZERO {
        base.translate(TermPoint::new(Col::ZERO, shift));
        base.set_baseline(target_bl);
    }

    let mut parts: Vec<TermFrame> = Vec::new();
    if let Some(p) = pre {
        parts.push(p);
    }
    parts.push(base);
    if let Some(p) = post {
        parts.push(p);
    }

    if parts.len() == 1 {
        return parts.into_iter().next().unwrap();
    }

    compose_horizontal(parts, Col::ZERO)
}

// ── Inline Unicode script frame ─────────────────────────────────────────────

/// Build an inline Unicode script frame.
///
/// The frame keeps the base height (`base_rows`) and baseline unchanged so
/// that [`assemble_with_pre_post`] places it without shifting the base.
/// Within that frame the converted text is positioned at:
///
/// - superscript: `clamp(base_baseline − 1, 0, base_rows−1)`
/// - subscript:   `clamp(base_baseline + 1, 0, base_rows−1)`
///
/// For a 1-row base both clamp to row 0 → script appears on the same line.
/// For taller bases the text ends up near the top/bottom of the base.
fn build_inline_script_frame(
    super_text: Option<&str>,
    sub_text: Option<&str>,
    base_rows: Row,
    base_baseline: Row,
) -> TermFrame {
    fn str_cols(s: &str) -> Col {
        s.chars().map(char_cols).fold(Col::ZERO, |a, b| a + b)
    }
    let cols = super_text
        .map(str_cols)
        .unwrap_or(Col::ZERO)
        .max(sub_text.map(str_cols).unwrap_or(Col::ZERO))
        .max(Col::ZERO);

    let rows = base_rows.max(Row::new(1));
    let max_row = rows - Row::new(1); // last valid row index

    // super: one row above baseline, clamped to [0, max_row]
    let super_row = (base_baseline - Row::new(1))
        .max(Row::ZERO)
        .min(max_row);
    // sub: one row below baseline, clamped to [0, max_row]
    let sub_row = (base_baseline + Row::new(1))
        .max(Row::ZERO)
        .min(max_row);

    let mut frame = TermFrame::new(TermSize::new(cols, rows));
    frame.set_baseline(base_baseline);

    if let Some(s) = super_text {
        frame.push_text(
            TermPoint::new(Col::ZERO, super_row),
            EcoString::from(s),
            ContentStyle::default(),
        );
    }
    if let Some(s) = sub_text {
        frame.push_text(
            TermPoint::new(Col::ZERO, sub_row),
            EcoString::from(s),
            ContentStyle::default(),
        );
    }

    frame
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
                let frag = ctx
                    .layout_into_fragment(&SymbolElem::packed('′').spanned(elem.span()), styles)?;
                ctx.push(frag);
            }
            return Ok(());
        }
    };

    let frag = ctx.layout_into_fragment(&SymbolElem::packed(ch).spanned(elem.span()), styles)?;
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

// ── Horizontal stretch helpers ────────────────────────────────────────────────

fn stretch_base(
    base: &Content,
    styles: StyleChain,
) -> Option<typst::layout::Rel<typst::layout::Abs>> {
    let mut b = base;
    loop {
        if let Some(eq) = b.to_packed::<typst::math::EquationElem>() {
            b = &eq.body;
        } else if let Some(lim) = b.to_packed::<LimitsElem>() {
            b = &lim.body;
        } else if let Some(scr) = b.to_packed::<ScriptsElem>() {
            b = &scr.body;
        } else {
            break;
        }
    }
    b.to_packed::<StretchElem>()
        .map(|s| s.size.get_cloned(styles).resolve(styles))
}

fn extract_stretch_base_char(base: &Content) -> Option<char> {
    let mut b = base;
    loop {
        if let Some(eq) = b.to_packed::<typst::math::EquationElem>() {
            b = &eq.body;
        } else if let Some(lim) = b.to_packed::<LimitsElem>() {
            b = &lim.body;
        } else if let Some(scr) = b.to_packed::<ScriptsElem>() {
            b = &scr.body;
        } else {
            break;
        }
    }
    b.to_packed::<StretchElem>()
        .and_then(|s| s.body.to_packed::<SymbolElem>())
        .and_then(|sym| {
            let mut cs = sym.text.chars();
            let c = cs.next()?;
            cs.next().is_none().then_some(c)
        })
}
