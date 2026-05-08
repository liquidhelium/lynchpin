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

pub mod fragment;
pub mod run;
pub mod text;
pub mod operators;
pub mod frac;
pub mod root;
pub mod attach;
pub mod lr;
pub mod underover;
pub mod cancel;
pub mod accent;
pub mod mat;

use crossterm::style::ContentStyle;
use ecow::EcoString;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, ContextElem, Packed, SequenceElem, StyleChain, StyledElem, SymbolElem};
use typst::layout::HideElem;
use typst::layout::{BoxElem, HElem};
use typst::math::{
    AccentElem, AlignPointElem, AttachElem, BinomElem, CancelElem, CasesElem, ClassElem,
    EquationElem, FracElem, LimitsElem, LrElem, MatElem, MathSize, MidElem, OpElem,
    OverbraceElem, OverbracketElem, OverlineElem, OverparenElem, OvershellElem, PrimesElem,
    RootElem, ScriptsElem, StretchElem, UnderbraceElem, UnderbracketElem, UnderlineElem,
    UnderparenElem, UndershellElem, VecElem,
};
use typst::text::{LinebreakElem, SpaceElem};
use crate::config::TermConfig;
use typst::routines::Arenas;
use typst::model::DocumentInfo;
use crate::frame::{TermFrame, TermSize};

pub use self::fragment::{TermLimits, TermMathFragment, TermMathFrameFragment};
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
}

impl<'cfg, 'eng, 'e> TermMathContext<'cfg, 'eng, 'e> {
    /// Create a new math context.
    pub fn new(
        engine: &'eng mut Engine<'e>,
        config: &'cfg TermConfig,
        is_display: bool,
    ) -> Self {
        Self { engine, config, is_display, fragments: Vec::new() }
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
        Ok(TermMathRun::new(self.layout_into_fragments(content, styles)?))
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

    /// Realize content with TermRealizationKind::Math, then dispatch.
    ///
    /// This properly handles show rules, location assignment (needed for
    /// ContextElem), and element preparation inside math formulas.
    pub fn layout_into_self(
        &mut self,
        content: &Content,
        styles: StyleChain,
    ) -> SourceResult<()> {
        let arenas = Arenas::default();
        let mut info = DocumentInfo::default();
        let pairs = lynchpin_term_realize::realize_term(
            self.engine, &arenas, &mut info, content, styles,
            lynchpin_term_realize::TermRealizationKind::Math,
        )?;
        for (elem, pair_styles) in pairs {
            self.dispatch_element(elem, pair_styles)?;
        }
        Ok(())
    }

    /// Walk the content tree directly (fallback, unused).
    pub fn layout_into_self_direct(
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
    fn dispatch_element(
        &mut self,
        content: &Content,
        styles: StyleChain,
    ) -> SourceResult<()> {

        // ── Whitespace & structural ───────────────────────────────────────────

        if content.is::<SpaceElem>() {
            self.push(TermMathFragment::Spacing(1, true));
            return Ok(());
        }

        if content.is::<LinebreakElem>() {
            self.push(TermMathFragment::Linebreak);
            return Ok(());
        }

        if let Some(elem) = content.to_packed::<HElem>() {
            if !elem.amount.is_zero() {
                self.push(TermMathFragment::Spacing(1, false));
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

        // Cherry-picked: HideElem -> skip; ContextElem -> evaluate via show rule
        if content.is::<HideElem>() { return Ok(()); }
        if content.is::<ContextElem>() {
            return Ok(()); // skip — requires full realization for location assignment
        }

        // ── Unknown: try to recurse, then emit placeholder ──────────────────
        if let Some(seq) = content.to_packed::<SequenceElem>() {
            for ch in &seq.children { self.dispatch_element(ch, styles)?; }
            return Ok(());
        }
        if let Some(s) = content.to_packed::<StyledElem>() {
            self.dispatch_element(&s.child, styles.chain(&s.styles))?;
            return Ok(());
        }
        if let Some(eq) = content.to_packed::<EquationElem>() {
            let nested = eq.block.get(styles);
            let size_style = if nested { EquationElem::size.set(MathSize::Display).wrap() }
                                 else { EquationElem::size.set(MathSize::Text).wrap() };
            self.dispatch_element(&eq.body, styles.chain(&size_style))?;
            return Ok(());
        }
        // Still unknown — show placeholder
        let name = content.elem().name();
        let placeholder = EcoString::from(format!("[{name}]"));
        let frame = TermFrame::text(placeholder, ContentStyle::default());
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
        frag.set_class(class);
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

    if run.is_multiline() {
        Ok(run.multiline_frame())
    } else {
        let frame = run.into_frame();
        // Pad block equations with 1 blank column on each side for readability.
        if frame.cols() > 0 {
            let mut padded =
                TermFrame::new(TermSize::new(frame.cols() + 2, frame.rows().max(1)));
            padded.set_baseline(frame.baseline());
            padded.push_frame(crate::frame::TermPoint::new(1, 0), frame);
            Ok(padded)
        } else {
            Ok(frame)
        }
    }
}
