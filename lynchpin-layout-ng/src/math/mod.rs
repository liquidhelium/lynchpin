//! Terminal math equation layout.
//!
//! Entry points:
//! - [`layout_equation_inline`] – inline (within text) equation rendering
//! - [`layout_equation_block`] – display / block equation rendering
//!
//! Math is dispatched via [`TermMathContext::layout_into_self`], which
//! walks the content tree and calls the appropriate sub-module for each
//! math element type, accumulating [`TermMathFragment`]s that are then
//! composed into a [`TermFrame`] by [`TermMathRun`].

pub mod accent;
pub mod attach;
pub mod cancel;
pub mod frac;
pub mod fragment;
pub mod lr;
pub mod mat;
pub mod operators;
pub mod root;
pub mod run;
pub mod text;
pub mod underover;

use std::borrow::Cow;

use lynchpin_library_ng::config::TermConfig;
use lynchpin_library_ng::frame::{Col, TermFrame, TermSize};
use tracing::debug;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{
    Content, ContextElem, Packed, SequenceElem, StyleChain, StyledElem, SymbolElem, TargetElem,
};
use typst::introspection::Locatable;
use typst::layout::HideElem;
use typst::layout::{BoxElem, HElem};
use typst::math::{
    AccentElem, AlignPointElem, AttachElem, BinomElem, CancelElem, CasesElem, ClassElem,
    EquationElem, FracElem, LimitsElem, LrElem, MatElem, MathSize, MidElem, OpElem, OverbraceElem,
    OverbracketElem, OverlineElem, OverparenElem, OvershellElem, PrimesElem, RootElem, ScriptsElem,
    StretchElem, UnderbraceElem, UnderbracketElem, UnderlineElem, UnderparenElem, UndershellElem,
    VecElem,
};
use typst::text::{LinebreakElem, SpaceElem};

pub use self::fragment::{make_text_frame, TermMathFragment, TermMathFrameFragment};
pub use self::run::TermMathRun;

// ── TermMathContext ───────────────────────────────────────────────────────────

/// Terminal math layout context.
///
/// Analogous to `typst-layout`'s `MathContext`.  All math sub-modules receive
/// a `&mut TermMathContext` and push [`TermMathFragment`]s via [`push`].
///
/// # Lifetime parameters
///
/// * `'cfg` – lifetime of the [`TermConfig`] reference.
/// * `'eng` – lifetime of the `Engine` borrow.
/// * `'e`   – lifetime parameter of `Engine` itself.
pub struct TermMathContext<'cfg, 'eng, 'e> {
    /// Typst engine (used for warnings in the dispatcher).
    pub engine: &'eng mut Engine<'e>,
    /// Render configuration (ASCII vs. Unicode).
    pub config: &'cfg TermConfig,
    /// `true` when inside a display/block equation.
    pub is_display: bool,
    /// Accumulated fragments (private – mutated only through push/extend).
    fragments: Vec<TermMathFragment>,
    /// Location counter for ContextElem preparation inside math.
    loc_counter: u64,
}

impl<'cfg, 'eng, 'e> TermMathContext<'cfg, 'eng, 'e> {
    /// Create a new math context.
    pub fn new(engine: &'eng mut Engine<'e>, config: &'cfg TermConfig, is_display: bool) -> Self {
        Self {
            engine,
            config,
            is_display,
            fragments: Vec::new(),
            loc_counter: 0,
        }
    }

    // ── Fragment accumulation ─────────────────────────────────────────────────

    /// Push a single fragment.
    pub fn push(&mut self, frag: impl Into<TermMathFragment>) {
        self.fragments.push(frag.into());
    }

    /// Push multiple fragments.
    pub fn extend(&mut self, frags: impl IntoIterator<Item = TermMathFragment>) {
        self.fragments.extend(frags);
    }

    // ── Composite layout ──────────────────────────────────────────────────────

    /// Layout `content`, returning the result as a [`TermMathRun`].
    pub fn layout_into_run(
        &mut self,
        content: &Content,
        styles: StyleChain,
    ) -> SourceResult<TermMathRun> {
        Ok(TermMathRun::new(
            self.layout_into_fragments(content, styles)?,
        ))
    }

    /// Layout `content`, returning the raw fragment list.
    pub fn layout_into_fragments(
        &mut self,
        content: &Content,
        styles: StyleChain,
    ) -> SourceResult<Vec<TermMathFragment>> {
        let saved = std::mem::take(&mut self.fragments);
        self.layout_into_self(content, styles)?;
        Ok(std::mem::replace(&mut self.fragments, saved))
    }

    /// Layout `content`, returning a single unified [`TermMathFragment`].
    ///
    /// If the content produces multiple fragments they are composed into a
    /// single [`TermFrame`] fragment.
    pub fn layout_into_fragment(
        &mut self,
        content: &Content,
        styles: StyleChain,
    ) -> SourceResult<TermMathFragment> {
        Ok(self.layout_into_run(content, styles)?.into_fragment())
    }

    /// Layout `content`, returning the composed [`TermFrame`].
    pub fn layout_into_frame(
        &mut self,
        content: &Content,
        styles: StyleChain,
    ) -> SourceResult<TermFrame> {
        Ok(self.layout_into_fragment(content, styles)?.into_frame())
    }

    // ── Content dispatcher ────────────────────────────────────────────────────

    /// Walk the content tree directly, dispatching each element to the
    /// appropriate layout handler.
    ///
    /// Unlike the term-layout version, this does not use
    /// `lynchpin_term_realize::realize_term` (which is not available in
    /// `lynchpin-layout-ng`).  Instead it walks the content tree directly,
    /// handling transparent wrapper elements (Sequence, Styled, Equation)
    /// before dispatching to element-specific handlers.
    pub fn layout_into_self(
        &mut self,
        content: &Content,
        styles: StyleChain,
    ) -> SourceResult<()> {
        // ── Transparent containers ──────────────────────────────────────────
        if let Some(seq) = content.to_packed::<SequenceElem>() {
            for child in &seq.children {
                self.layout_into_self(child, styles)?;
            }
            return Ok(());
        }

        if let Some(s) = content.to_packed::<StyledElem>() {
            self.layout_into_self(&s.child, styles.chain(&s.styles))?;
            return Ok(());
        }

        if let Some(eq) = content.to_packed::<EquationElem>() {
            let nested_display = eq.block.get(styles);
            let size_style = if nested_display {
                EquationElem::size.set(MathSize::Display).wrap()
            } else {
                EquationElem::size.set(MathSize::Text).wrap()
            };
            self.layout_into_self(&eq.body, styles.chain(&size_style))?;
            return Ok(());
        }

        // ── BoxElem ─────────────────────────────────────────────────────────
        if let Some(elem) = content.to_packed::<BoxElem>() {
            if let Some(body) = elem.body.get_ref(styles) {
                self.layout_into_self(body, styles)?;
            }
            return Ok(());
        }

        self.dispatch_element(content, styles)
    }

    /// Dispatch a single element to the appropriate layout handler.
    fn dispatch_element(&mut self, content: &Content, styles: StyleChain) -> SourceResult<()> {
        // ── Whitespace & structural ───────────────────────────────────────────

        if content.is::<SpaceElem>() {
            self.push(TermMathFragment::Spacing(Col::new(1), true));
            return Ok(());
        }

        if content.is::<LinebreakElem>() {
            self.push(TermMathFragment::Linebreak);
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<HElem>() {
            if !elem.amount.is_zero() {
                self.push(TermMathFragment::Spacing(Col::new(1), false));
            }
            return Ok(());
        }

        if content.is::<AlignPointElem>() {
            self.push(TermMathFragment::Align);
            return Ok(());
        }

        // ── Text & symbols ────────────────────────────────────────────────────

        if let Some(elem) = content.to_packed::<typst::text::TextElem>() {
            text::layout_text(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<SymbolElem>() {
            text::layout_symbol(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<OpElem>() {
            text::layout_op(elem, self, styles)?;
            return Ok(());
        }

        // ── Fractions & binomials ─────────────────────────────────────────────

        if let Some(elem) = content.to_packed::<FracElem>() {
            frac::layout_frac(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<BinomElem>() {
            frac::layout_binom(elem, self, styles)?;
            return Ok(());
        }

        // ── Attachments (scripts / limits) ────────────────────────────────────

        if let Some(elem) = content.to_packed::<AttachElem>() {
            attach::layout_attach(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<PrimesElem>() {
            attach::layout_primes(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<ScriptsElem>() {
            attach::layout_scripts(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<LimitsElem>() {
            attach::layout_limits(elem, self, styles)?;
            return Ok(());
        }

        // ── Left–right delimiters ─────────────────────────────────────────────

        if let Some(elem) = content.to_packed::<LrElem>() {
            lr::layout_lr(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<MidElem>() {
            lr::layout_mid(elem, self, styles)?;
            return Ok(());
        }

        // ── StretchElem ───────────────────────────────────────────────────────

        if let Some(elem) = content.to_packed::<StretchElem>() {
            lr::layout_stretch(elem, self, styles)?;
            return Ok(());
        }

        // ── Root ──────────────────────────────────────────────────────────────

        if let Some(elem) = content.to_packed::<RootElem>() {
            root::layout_root(elem, self, styles)?;
            return Ok(());
        }

        // ── Cancel ───────────────────────────────────────────────────────────

        if let Some(elem) = content.to_packed::<CancelElem>() {
            cancel::layout_cancel(elem, self, styles)?;
            return Ok(());
        }

        // ── Accent ───────────────────────────────────────────────────────────

        if let Some(elem) = content.to_packed::<AccentElem>() {
            accent::layout_accent(elem, self, styles)?;
            return Ok(());
        }

        // ── Matrices, vectors, cases ─────────────────────────────────────────

        if let Some(elem) = content.to_packed::<MatElem>() {
            mat::layout_mat(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<VecElem>() {
            mat::layout_vec(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<CasesElem>() {
            mat::layout_cases(elem, self, styles)?;
            return Ok(());
        }

        // ── Under/over decorators ─────────────────────────────────────────────

        if let Some(elem) = content.to_packed::<UnderlineElem>() {
            underover::layout_underline(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<OverlineElem>() {
            underover::layout_overline(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<UnderbraceElem>() {
            underover::layout_underbrace(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<OverbraceElem>() {
            underover::layout_overbrace(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<UnderbracketElem>() {
            underover::layout_underbracket(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<OverbracketElem>() {
            underover::layout_overbracket(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<UnderparenElem>() {
            underover::layout_underparen(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<OverparenElem>() {
            underover::layout_overparen(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<UndershellElem>() {
            underover::layout_undershell(elem, self, styles)?;
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<OvershellElem>() {
            underover::layout_overshell(elem, self, styles)?;
            return Ok(());
        }

        // ── ClassElem ─────────────────────────────────────────────────────────

        if let Some(elem) = content.to_packed::<ClassElem>() {
            layout_class(elem, self, styles)?;
            return Ok(());
        }

        // HideElem -> measure body size and push an empty frame of the same
        // dimensions so the hidden content still occupies the right space.
        if let Some(elem) = content.to_packed::<HideElem>() {
            let frame = self.layout_into_frame(&elem.body, styles)?;
            if !frame.size().is_empty() {
                let blank = TermFrame::new(frame.size());
                self.push(TermMathFrameFragment::new(blank));
            }
            return Ok(());
        }

        // ── Context: prepare and evaluate via CONTEXT_RULE ──────────────────
        // Math bypasses the standard realization pipeline, so we mirror what
        // visit_show_rules does: set a location and apply the built-in rule.
        if content.is::<ContextElem>() {
            let mut output = Cow::Borrowed(content);
            if !output.is_prepared() {
                if output.can::<dyn Locatable>() && output.location().is_none() {
                    self.loc_counter += 1;
                    let loc = typst::introspection::Location::new(self.loc_counter as u128);
                    output.to_mut().set_location(loc);
                }
                output.to_mut().mark_prepared();
            }
            let target = styles.get(TargetElem::target);
            if let Some(rule) = self.engine.routines.rules.get(target, &output) {
                let result = rule.apply(&output, self.engine, styles)?;
                self.dispatch_element(&result, styles)?;
            }
            return Ok(());
        }

        // ── Unknown: try to recurse, then emit placeholder ──────────────────
        if let Some(seq) = content.to_packed::<SequenceElem>() {
            for ch in &seq.children {
                self.dispatch_element(ch, styles)?;
            }
            return Ok(());
        }
        if let Some(s) = content.to_packed::<StyledElem>() {
            self.dispatch_element(&s.child, styles.chain(&s.styles))?;
            return Ok(());
        }
        if let Some(eq) = content.to_packed::<EquationElem>() {
            let nested = eq.block.get(styles);
            let size_style = if nested {
                EquationElem::size.set(MathSize::Display).wrap()
            } else {
                EquationElem::size.set(MathSize::Text).wrap()
            };
            self.dispatch_element(&eq.body, styles.chain(&size_style))?;
            return Ok(());
        }
        // Still unknown — show placeholder
        let name = content.elem().name();
        let placeholder = format!("[{name}]");
        let frame = make_text_frame(&placeholder);
        self.push(TermMathFrameFragment::new(frame));

        Ok(())
    }
}

// ── ClassElem handler ─────────────────────────────────────────────────────────

fn layout_class(
    elem: &Packed<ClassElem>,
    ctx: &mut TermMathContext,
    styles: StyleChain,
) -> SourceResult<()> {
    let mut frags = ctx.layout_into_fragments(&elem.body, styles)?;
    let class = elem.class;
    for frag in &mut frags {
        frag.set_class(class.into());
    }
    ctx.extend(frags);
    Ok(())
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Layout an inline (non-block) equation, returning a [`TermFrame`].
///
/// The frame has baseline set to the math axis (vertically centred on a
/// single terminal row for simple expressions).
pub fn layout_equation_inline(
    elem: &Packed<EquationElem>,
    engine: &mut Engine<'_>,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    // Force Text (inline) math size.
    let size_style = EquationElem::size.set(MathSize::Text).wrap();
    let styles = styles.chain(&size_style);

    let mut ctx = TermMathContext::new(engine, config, false);
    let run = ctx.layout_into_run(&elem.body, styles)?;

    if run.is_multiline() {
        Ok(run.multiline_frame())
    } else {
        Ok(run.into_frame())
    }
}

/// Layout a block/display equation, returning a [`TermFrame`].
///
/// The frame is horizontally centred when placed inside a flow page.
pub fn layout_equation_block(
    elem: &Packed<EquationElem>,
    engine: &mut Engine<'_>,
    config: &TermConfig,
    styles: StyleChain,
) -> SourceResult<TermFrame> {
    // Force Display math size so large operators use limits layout etc.
    let size_style = EquationElem::size.set(MathSize::Display).wrap();
    let styles = styles.chain(&size_style);

    let mut ctx = TermMathContext::new(engine, config, true);
    let run = ctx.layout_into_run(&elem.body, styles)?;

    debug!("layout_equation_block: is_multiline={}", run.is_multiline());

    if run.is_multiline() {
        Ok(run.multiline_frame())
    } else {
        let frame = run.into_frame();
        debug!("layout_equation_block: frame cols={:?} rows={:?} baseline={:?}",
            frame.cols(), frame.rows(), frame.baseline());
        // Pad block equations with 1 blank column on each side for readability.
        if frame.cols() > Col::ZERO {
            let mut padded = TermFrame::new(TermSize::new(
                frame.cols() + Col::new(2),
                frame.rows().max(lynchpin_library_ng::frame::Row::new(1)),
            ));
            padded.set_baseline(frame.baseline());
            padded.push_frame(lynchpin_library_ng::frame::TermPoint::new(Col::new(1), lynchpin_library_ng::frame::Row::ZERO), frame);
            debug!("layout_equation_block: padded to cols={:?} rows={:?}", padded.cols(), padded.rows());
            Ok(padded)
        } else {
            Ok(frame)
        }
    }
}
