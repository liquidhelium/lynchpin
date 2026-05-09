# lynchpin-layout-ng Integration Workspace

## Agents and their files

- **Agent B** (flow + pages): `src/flow/*.rs`, `src/pages/*.rs`
- **Agent C** (inline): `src/inline/*.rs`
- **Agent D** (grid): `src/grid/*.rs`
- **Agent E** (math): `src/math/*.rs`
- **Agent F** (standalone): `src/lib.rs`, `src/image.rs`, `src/lists.rs`, `src/modifiers.rs`, `src/pad.rs`, `src/repeat.rs`, `src/rules.rs`, `src/shapes.rs`, `src/stack.rs`, `src/transforms.rs`

## Issue format

Each agent should add issues under their section:

```
### Agent X

**File: path/to/file.rs**
- [ ] Issue description
```

## Agent A (Library) — Round 3 fixes

### ✅ Done:

1. **`TermBlockCallback` now supports closures** (not just fn pointers).
   Removed the `callback!` macro. Now uses `Arc<dyn Fn(...)>`.
   `TermBlockCallback::new()` accepts any `F: Fn(...) + Send + Sync + 'static`.
   This fixes Agent F's rules.rs closure-capture issues.

2. **`TermFrameItem::Tag(Tag)` variant added.** Import `Tag` from `typst::introspection::Tag`.

3. **`TermFrame::push_tag(pos, Tag)` added.**

4. **`TermFrame::push(pos, TermFrameItem)` — generic push added.**

5. **`TermFrame::label(Label)` added** (no-op for now, tags handle introspection).

6. **`TermFrame::set_parent(Location, bool)` added** (no-op for now).

### ⚠️ Known:

- `TermBlockElem::pack()` — should work via `#[typst_macros::elem]`. If you get "no method named `pack`", check that `TermBlockCallback` implements `Clone + Hash + PartialEq` (it does now).

---

## Issues
<!-- Agents: add your concerns below -->

### Agent B (flow + pages)

**File: src/flow/distribute.rs**
- [x] Fixed (Round 3): Removed unnecessary `mut` from `distributor` binding —
  `run()` takes `mut self` by value, so the local binding doesn't need `mut`.

**File: src/pages/mod.rs**
- [x] Fixed (Round 3): Prefixed unused `tag` variable with underscore (`_tag`).
  Proper tag support requires `TermFrame::push_tag()` from `lynchpin-library-ng`.

**File: src/flow/collect.rs**
- [x] Fixed: `TermScalar` and `TermConfig` were not imported from `lynchpin_library_ng`.
  These types exist publicly in `lynchpin_library_ng` but were shadowed by
  private types in `crate::inline::prepare`. Import from `lynchpin_library_ng`.
- [x] Fixed: `Locator::location()` does not exist in typst 0.14. Changed
  `PlacedChild` to store `Location` directly, computed via
  `SplitLocator::next_location()` during collection.
- [x] Fixed: `Abs` is not an iterator - `PlaceElem.clearance.resolve(styles)`
  returns `Abs` directly (not `Option`). Converted to `TermScalar` via
  `abs_to_cols`.
- [x] Fixed: `Locator` intentionally does not implement `Clone`. Added manual
  `Clone` impls for `SingleChild` and `MultiChild` using `Locator::relayout()`.

**~~Note for Agent C (inline)~~** (resolved in Round 2/3)
- [x] `crate::inline::prepare::TermConfig` and `TermScalar` — these types do not
  exist in inline/prepare.rs. The note was stale. `TermConfig` is imported from
  `lynchpin_library_ng::config::TermConfig` by other agents.
- [x] `inline/box.rs` call to `unbreakable_pod` — already updated to 3-argument
  signature in Round 2.

### Agent C (inline)

**Round 3 fixes:**
- [x] `box.rs:pad::grow` type mismatch: changed `&inset_unresolved` (which was
  `Sides<Rel<Length>>`) to `&inset` (`Sides<Rel<Abs>>` from `.resolve()`).
  Removed the unnecessary `inset_unresolved` intermediate.

**Remaining cross-agent dependencies (all require Agent F or Agent A):**

**File: src/inline/box.rs**
- [ ] `crate::layout_frame(engine, body, locator, styles, pod)` — expected from
  Agent B (flow) or Agent F. Currently stubbed: box body layout returns an empty
  `TermFrame`.
- [ ] `crate::shapes::clip_rect` / `crate::shapes::fill_and_stroke` — expected
  from Agent F. Clipping falls back to `frame.clip(size)` (no-op in library).
  Fill/stroke is skipped.
- [ ] `TermFrame::label(label)` method — **Library (Agent A).** `TermFrame` has
  no `label()` method.

**File: src/inline/collect.rs**
- [ ] `crate::modifiers::FrameModifiers` (type), `FrameModify` (trait),
  `layout_and_modify` (fn) — expected from Agent F. Currently calls
  `layout_box` directly instead of `layout_and_modify`.
- [ ] `InlineElem::layout()` returns paged `Frame`; conversion to `TermFrame`
  is a stub (creates empty `TermFrame` with correct size, discards items).
  A proper `Frame→TermFrame` converter is needed from **Agent F**.

**File: src/inline/line.rs**
- [ ] `crate::modifiers::layout_and_modify` — expected from Agent F. Currently
  calls `layout_box` directly (bypasses modifier pipeline).
- [ ] `TermFrame::push_tag(pos, tag)` method — **Library (Agent A).**
  `TermFrame` has no tag mechanism. Tags are currently ignored (empty frames
  pushed as placeholders).

**File: src/inline/deco.rs**
- [ ] `crate::shapes::styled_rect(size, radius, fill, stroke)` — expected from
  Agent F. Highlight (`DecoLine::Highlight`) decoration is skipped entirely.
  Underline/overline/strikethrough work via `TermFrameItem::Shape` with line geometry.

**File: src/inline/shaping.rs**
- [ ] `crate::modifiers::FrameModifyText` (trait, for `frame.modify_text()`) —
  expected from Agent F.

**General note:**
- `TermScalar::raw()` is `pub(crate)` and cannot be accessed from this crate.
  All conversions use `TermScalar::get() as f64` to get the integer value, then
  `Abs::raw()` to construct typst `Abs`. This loses fractional precision but is
  correct for terminal cell coordinates.
- The port from paged typst to terminal uses `Abs` for intermediate inline
  calculations (line widths, justification, etc.) and `TermScalar` for terminal
  grid coordinates (`TermFrame`, `TermPoint`, `TermSize`). Conversions happen at
  the boundaries using `TermScalar::from_f64(abs.to_raw())` and
  `Abs::raw(term_scalar.get() as f64)`.

**Library (Agent A) requests:** (ALL DONE Round 3)
- [x] `TermFrame::label(Label)`
- [x] `TermFrame::push_tag(pos, Tag)`
- [x] `TermFrameItem::Tag(Tag)` variant
- [x] `TermFrame::push(pos, TermFrameItem)`
- [x] `TermFrame::set_parent(Location, bool)`

### Agent D (grid)

**Fixed in Round 2:**
- [x] `rowspans.rs:16`: Changed `use super::{self as grid_mod, layout_cell}` to `use crate::grid::layout_cell`.
- [x] `rowspans.rs:15`: Removed unused imports `RowPiece`, `RowState`.
- [x] `rowspans.rs:951`: Removed unused lifetime `'a` from `RowspanSimulator`.
- [x] `mod.rs:20`: Removed unused import `Fragment`.
- [x] `mod.rs:26`: Removed unused import `RowPiece`.
- [x] `mod.rs:90`: Changed `fragment.into_frames()` to `let mut frames: Vec<TermFrame> = fragment_into_frames(fragment)`.
- [x] `mod.rs:172`: Fixed `collect()` on backlog by using `Vec::leak` for `&'static [Abs]`.
- [x] `layouter.rs:652`: Changed `resolve_rel(v, ...)` to `resolve_rel(&v, ...)` (pass reference).
- [x] `layouter.rs:311`: Fixed use-after-move of `regions` by extracting values before the struct construction.
- [x] `layouter.rs:547`: Fixed borrow conflict by precomputing `finished_len` before the loop.
- [x] `layouter.rs:693`: Renamed `available` to `_available` (unused parameter).
- [x] `layouter.rs:778`: Added `use typst_utils::Numeric;` for `Fr::is_zero()`.

**Fixed in Round 3:**
- [x] `mod.rs:17-19`: Removed unused imports `FrameItem`, `FrameParent`, `Inherit`, `Point`.
- [x] `mod.rs:101`: Changed `Point::zero()` → `TermPoint::ZERO`, `FrameItem::Tag(...)` → `TermFrameItem::Tag(...)`.
- [x] `mod.rs:102`: Changed `Point::zero()` → `TermPoint::ZERO`, `FrameItem::Tag(...)` → `TermFrameItem::Tag(...)`. Uses library's generic `push()` method (added R3).
- [x] `mod.rs:112`: Changed `frame.set_parent(FrameParent::new(loc, Inherit::Yes))` → `frame.set_parent(loc, true)` matching new library signature `(Location, bool)`.
- [x] `mod.rs:114-117`: Changed `prepend_multiple` tuple types from `(Point, FrameItem)` to `(TermPoint, TermFrameItem)`.

**Remaining issues (cross-agent dependencies):**

**File: src/grid/mod.rs**
- [ ] Line 89: `crate::layout_fragment(engine, &cell.body, locator, styles, paged_regions)` — the function `layout_fragment` does not exist in the crate root. `layout_term_fragment` in `flow/mod.rs` takes `&[Pair<'_>]`, not `&Content`. A function is needed that takes `&Content` and returns `TermFragment`. **Expected from Agent B (flow).**
- [ ] Lines 137, 150: `layout_grid` / `layout_table` return `SourceResult<TermFragment>` but callers in `rules.rs` (Agent F) expect `Result<TermFrame, ...>` and pass wrong arguments (4 instead of 5, wrong types). **Callers in `rules.rs` need to be updated by Agent F.**
- [x] ~~`SetMax` trait: `stack.rs` already added `use crate::grid::SetMax;` in Round 2.~~ Resolved.

### Agent E (math)

**Fixed in Round 2:**
- [x] All files: `push_text` now takes `EcoString` instead of `String`. Changed all `.to_string()` and `String` arguments to `EcoString::from(...)` or `.into()`.
- [x] `fragment.rs`: Removed duplicate characters from `math_class()` match arms (unreachable patterns). `'∧'`, `'∨'`, `'⊏'`, `'⊐'`, `'⊑'`, `'⊒'`, `'≠'`, `'≡'`, `'≢'`, `'∶'`, `'∷'` appeared in multiple arms.
- [x] `fragment.rs`: Added `impl From<unicode_math_class::MathClass> for MathClass` conversion.
- [x] `mat.rs:125,127`: Fixed `TermScalar * i32` → `TermScalar * f64` (multiplying by `as f64` instead of `as i32`).
- [x] `run.rs:242,345,385`: Same `TermScalar * i32` fix.
- [x] `mod.rs:430`: Use `class.into()` to convert `unicode_math_class::MathClass` to local `MathClass`.
- [x] `mod.rs:41`: Removed unused re-exports `MathClass`, `TermLimits`.
- [x] `lr.rs:12`: Removed unused imports `TermFrame`, `TermPoint`, `TermSize`.
- [x] `text.rs:3`: Removed unused import `TermFrame`.
- [x] `Cargo.toml`: Added `unicode-math-class = "0.1"` dependency.

**Cross-agent notes:**
- [ ] `lynchpin-library-ng/src/frame.rs`: `push_text` takes `EcoString` directly (not `impl Into<EcoString>` as in the old library). All callers in math files now use explicit `EcoString::from()` or `.into()`. Other agents with `push_text` calls may need the same fix.

### Agent F (standalone)

**Round 3 fixes applied (ALL DONE):**

**File: src/rules.rs**
- [x] Added `use typst::foundations::NativeElement;` for `.pack()` method.
- [x] Added `use comemo::Track;` for `context.track()`.
- [x] Fixed `HEADING_RULE`: `elem.body.clone()` (body is Content, not Settable).
- [x] Fixed `FIGURE_CAPTION_RULE`: call `realize()` in outer scope, pass result into callback.
- [x] Fixed `FOOTNOTE_RULE`: `realize()` returns `(Destination, Content)`, use `?` not `unwrap_or_default()`; `TermFrame::text` needs 4 args.
- [x] Fixed `FOOTNOTE_ENTRY_RULE`: `realize()` returns `(Content, Content)` (prefix is Content not Option); capture sizes before move.
- [x] Fixed `REF_RULE`: `realize()` returns `Content` (not tuple).
- [x] Fixed `TABLE_RULE`: pass Locator + TermRegions, convert fragment → frame via `compose_frames()`.
- [x] Fixed `GRID_RULE`: same as TABLE_RULE.
- [x] Fixed `RAW_RULE`: `RawContent` enum matching, type annotations, `ecow::EcoString`.
- [x] Fixed `PAD_RULE`: `elem.*.resolve(styles)` returns `Rel<Abs>` directly (match `pad::grow` signature).
- [x] Fixed `LAYOUT_RULE`: `context.track()`, `TermScalar` → `i64` for dict, `Context::new` takes `Option<Location>`.
- [x] Fixed all `Locator::default()` → `Locator::root()`.
- [x] Fixed all `&[(body.clone(), st)]` → `&[(&body, st)]` borrow patterns.
- [x] Added `compose_frames()` helper.
- [x] Removed unused imports.

**File: src/shapes.rs**
- [x] Fixed `layout_line`: `Settable` access via `.get(styles)`, resolve axes individually, `Rel::relative_to()` not `.at()`.
- [x] Fixed `layout_term_shape` signature: `stroke: bool`, `width: Smart<Rel<Length>>`, `height: Sizing`, `radius: Corners<Option<Rel<Length>>>`.
- [x] Fixed inset/outset: use `.get(styles)` (not `.resolve()`) to keep `Sides<Option<Rel<Length>>>`.
- [x] Fixed polygon: `p.x.relative_to(font_size)` for `Rel<Abs>`.
- [x] Simplified stroke parameter to `bool` (terminal shapes are approximations).
- [x] Removed unused imports.

**File: src/lists.rs**
- [x] Fixed `elem.indent`: `Length::at(font_size)` for conversion.
- [x] Fixed `Locator::default()` → `Locator::root()`.
- [x] Fixed borrow-after-move for `term_frame`/`sep_frame` in `layout_terms`.

**File: src/modifiers.rs**
- [x] Fixed `Locator::default()` → `Locator::root()`.

**File: src/repeat.rs**
- [x] Fixed `Locator::default()` → `Locator::root()`.
- [x] Fixed unreachable pattern warning.

**File: src/transforms.rs**
- [x] Fixed `Locator::default()` → `Locator::root()` (4 occurrences).

**File: src/stack.rs**
- [x] Fixed `Locator::default()` → `Locator::root()`.
- [x] Fixed `TermRegions` → `TermRegion` conversion for `layout_term_frame`.
- [x] Fixed `FixedAlignment::position`: convert `TermScalar` → `Abs` → back via `Abs::raw()`.
- [x] Fixed `GenericSize<Abs>::to_point` by converting to `TermScalar`.
- [x] Removed unused imports/mut.

**File: src/pad.rs**
- [x] No changes needed (already uses `&Sides<Rel<Abs>>`).

**File: src/image.rs**
- [x] No changes needed (no `Locator` used, placeholder only).

**Remaining cross-agent dependency (NOT Agent F):**

**File: src/grid/mod.rs**
- [ ] Line 89: `crate::layout_fragment` → should be `crate::flow::layout_term_fragment`. **Expected from Agent D.** This is the only remaining error in `lynchpin-layout-ng`.

## Agent A replies (Round 3)

### To Agent D:
- Tag/push/set_parent: All DONE in library R3. Use TermFrameItem::Tag, push_tag(), push(), set_parent().
- layout_fragment not found: Check crate::flow::layout_term_fragment in flow/mod.rs.
- layout_grid/layout_table return mismatch: Agent F problem. Leave it.

### To Agent F:
- TermBlockCallback closures: DONE. new() now accepts any closure.
- TermBlockElem::pack(): Should work now (Clone+Hash+PartialEq implemented).
- Locator::default() does not exist. Use Locator::root() or Locator::link().
- SetMax trait: Agent D provides it. Add use crate::grid::SetMax in stack.rs.

### To Agent E: math is clean. No action.

### To Agent C: All library requests fulfilled. Replace stubs with real library methods.

---

## Review: paged vs term gaps

> Generated by independent review agent comparing
> `lynchpin-layout/src/` (47 files, paged source of truth)
> vs `lynchpin-layout-ng/src/` (46 files, new terminal implementation).

### Overall size comparison

| Module  | Paged (lines) | Term (lines) | Coverage |
|---------|--------------|--------------|----------|
| flow    | 3,105        | 1,299        | 42%      |
| inline  | 4,602        | 4,045        | 88%      |
| grid    | 5,468        | 4,036        | 74%      |
| math    | ~4,630       | ~4,498       | 97%      |
| pages   | 686          | 433          | 63%      |

**Critical gap**: `flow/` is at 42% — the biggest loss of logic.

---

### A. Missing files (in paged, not in term)

| File | Lines | Impact |
|------|-------|--------|
| `math/shared.rs` | 183 | Shared math helpers: `style_cramped()`, `style_flac()`, `style_dtls()`, `style_for_subscript()`, `style_for_superscript()`, `style_for_numerator()`, `style_for_denominator()`, `families()`, `stack()`, `alignments()` — **all missing**. |
| `math/stretch.rs` | 84 | `layout_stretch()`, `stretch_fragment()` — glyph stretching for delimiters. **Missing entirely.** |

**Extra file** (in term, not in paged): `math/operators.rs` (256 lines) — terminal-specific big-operator builders (`build_sum_operator`, `build_prod_operator`, `build_integral_operator`).

---

### B. Missing functions (paged has, term does not)

#### B1. `pad.rs` — Missing `layout_pad()`

- **Paged**: `layout_pad(elem, engine, locator, styles, regions) → Fragment` — the main orchestrator that shrinks regions, layouts body, then grows frames.
- **Term**: Only has `shrink()`, `shrink_multiple()`, `grow()` — the low-level helpers — but **no `layout_pad` to tie them together**. Callers (rules.rs) must manually replicate the shrink→layout→grow pipeline.

#### B2. `flow/mod.rs` — Missing `layout_columns()`

- **Paged**: `layout_columns(engine, body, locator, styles, regions) → Fragment` — multi-column layout.
- **Term**: No equivalent. Multi-column content will not work.

#### B3. `shapes.rs` — Missing `clip_rect()`, `fill_and_stroke()`, `styled_rect()`

- **Paged**: Three critical helpers used by `block.rs` and `inline/box.rs` for clipping content to rounded rectangles with stroke, and adding fill/stroke decoration.
- **Term**: All three **missing**. This forces stubs in:
  - `inline/box.rs`: clip falls back to simple `frame.clip(size)` (no-op in library), fill/stroke **skipped entirely**.
  - `inline/deco.rs`: `DecoLine::Highlight` **skipped entirely** (needs `styled_rect`).
  - `flow/block.rs`: fill/stroke/clip on blocks is **missing**.

#### B4. `modifiers.rs` — Architectural rewrite

- **Paged**: Trait-based system:
  - `FrameModifiers` — struct holding modifier flags
  - `FrameModify` — trait for modifying frames
  - `FrameModifyText` — trait for modifying text in frames
  - `layout_and_modify()` — orchestrator function
- **Term**: Replaced with individual functions (`layout_strong`, `layout_emph`, `layout_sub`, `layout_super`, `layout_underline`, `layout_overline`, `layout_strike`, `layout_highlight`, `layout_smallcaps`). These are simpler but **break callers** that expect the trait-based pipeline:
  - `inline/collect.rs` calls `FrameModifiers::get_in()` and `frame.modify()` — **broken**
  - `inline/line.rs` calls `layout_and_modify()` — **broken**
  - `inline/shaping.rs` calls `FrameModifyText` trait — **broken**

---

### C. Stubbed / no-op functions (term has function but major logic dropped)

#### C1. `flow/block.rs` — `layout_single_block()` and `layout_multi_block()` (430→119 lines)

- **`layout_single_block`**: Term version **ignores all parameters** (`let _ = (engine, body, styles)`) and returns an empty `TermFrame::new(region.size)`. Paged version handles:
  - Width/height sizing resolution
  - Three body types: `None`, `BlockBody::Content`, `BlockBody::SingleLayouter`, `BlockBody::MultiLayouter`
  - Fill and stroke application
  - Clip rect with radius
  - Inset growth
  - Frame kind (`Hard`)
  - Label assignment
- **`layout_multi_block`**: Term version **ignores all parameters** and returns `vec![TermFrame::new(TermSize::ZERO)]`. Paged version handles breakable pods, width consistency checks, per-frame post-processing, skip-first logic for empty frames.

#### C2. `transforms.rs` — `layout_rotate()`, `layout_scale()`, `layout_skew()` (all no-ops)

- **`layout_rotate`**: Body laid out as-is; rotation angle **ignored**.
- **`layout_scale`**: Body laid out as-is; scale factors **ignored**. Missing `resolve_scale()` with auto/aspect-ratio logic.
- **`layout_skew`**: Body laid out as-is; skew angles **ignored**. Missing `measure_and_layout()` with reflow support, `compute_bounding_box()`.
- Only `layout_move()` is functional (translates the frame).

#### C3. `shapes.rs` — `layout_curve()` and `layout_path()`

- Both marked as **stubs**. `layout_curve` returns empty 10×1 frame. `layout_path` returns empty 10×1 frame. Paged versions render actual Bezier curves and SVG paths.

#### C4. `shapes.rs` — `layout_term_shape()` (internal helper)

- Ignores `body`, `inset`, `outset`, `radius` parameters (`let _ = (engine, body, inset, outset, radius)`). This means shapes with child content (e.g., rect with text) don't render their body.

#### C5. `inline/box.rs` — `layout_box()` body layout

- When body is `Some(_)`, returns an **empty `TermFrame`** instead of laying out the body. Comment says "`layout_frame` is expected from Agent F / B" but not yet available.
- Fill/stroke **skipped** (needs `fill_and_stroke` from shapes.rs).
- Label assignment **commented out** (needs `TermFrame::label()` from library).

---

### D. TODO / FIXME markers requiring follow-up

| File | Line | Marker | Description |
|------|------|--------|-------------|
| `grid/repeated.rs` | 43 | `TODO(subfooters)` | Non-repeating footer detection incomplete |
| `grid/repeated.rs` | 254 | `TODO(layout model)` | Re-calculate header/footer heights after region skip |
| `grid/repeated.rs` | 395 | `TODO(subfooters)` | Footer right after skip handling |
| `grid/repeated.rs` | 501 | `TODO(subfooters)` | Reset header height after footer skip |
| `grid/rowspans.rs` | 301 | `TODO(subfooters)` | Unnecessary logic to remove later |
| `grid/lines.rs` | 523 | `FIXME` | Robustness check for footers at arbitrary positions |
| `inline/deco.rs` | 40 | `TODO` | Highlight decoration blocked on `styled_rect` |
| `inline/shaping.rs` | 799 | `unimplemented!()` | **Vertical text layout panics at runtime** |
| `inline/collect.rs` | 243 | `TODO` | Frame→TermFrame item copy not implemented |
| `inline/box.rs` | 106 | `TODO(Agent F)` | `clip_rect` replacement needed |
| `inline/box.rs` | 113 | `TODO(Agent F)` | `fill_and_stroke` call needed |
| `inline/box.rs` | 118 | `TODO(Agent A)` | `TermFrame::label()` needed |
| `transforms.rs` | 1,9 | doc | File self-describes as "stubs" |
| `flow/block.rs` | 6 | doc | File self-describes as "stub" |

---

### E. Runtime panics (active crash risks)

| File | Line | Expression | Trigger |
|------|------|------------|---------|
| `inline/shaping.rs` | 799 | `unimplemented!("vertical text layout")` | Any vertical text |
| `inline/shaping.rs` | 1092 | `panic!("one or more glyphs ... fell out of range")` | Rare shaping edge case |
| `inline/shaping.rs` | 1112 | `panic!(...)` | Font has no cmap table |
| `inline/linebreak.rs` | 265 | `panic!("bounded inline layout is incomplete")` | Complex inline layout edge case |

---

### F. Cross-agent dependency gaps (blocking integration)

These are places where one agent's code calls a function expected from another agent, but the function doesn't exist or has the wrong signature:

| Caller | Expected from | Missing/Wrong |
|--------|---------------|---------------|
| `inline/box.rs` | Agent B/F | `crate::layout_frame()` — box body layout returns empty frame |
| `inline/box.rs` | Agent F | `crate::shapes::clip_rect()` — missing |
| `inline/box.rs` | Agent F | `crate::shapes::fill_and_stroke()` — missing |
| `inline/collect.rs` | Agent F | `FrameModifiers`, `FrameModify`, `layout_and_modify` — architecture changed |
| `inline/collect.rs` | Agent F | Frame→TermFrame item conversion — stub |
| `inline/line.rs` | Agent F | `layout_and_modify()` — calls `layout_box` directly instead |
| `inline/deco.rs` | Agent F | `styled_rect()` — missing, highlight skipped |
| `inline/shaping.rs` | Agent F | `FrameModifyText` trait — not available |
| `grid/mod.rs:89` | Agent B | `crate::layout_fragment()` — should be `flow::layout_term_fragment` but signature differs |
| `grid/mod.rs:137+` | Agent F | `layout_grid`/`layout_table` return `TermFragment` but callers expect `TermFrame` |

---

### G. Summary by severity

**🔴 Critical (missing functionality that breaks layout):**
1. `flow/block.rs` — `layout_single_block`/`layout_multi_block` are stubs → all block elements broken
2. `flow/mod.rs` — `layout_columns` missing → multi-column layout broken
3. `shapes.rs` — `clip_rect`/`fill_and_stroke`/`styled_rect` missing → shapes and decoration broken
4. `math/shared.rs` + `math/stretch.rs` — missing files → math style and stretch broken
5. `modifiers.rs` — trait system removed → inline modifier pipeline broken for all callers
6. `inline/shaping.rs:799` — `unimplemented!()` panic for vertical text

**🟠 High (stubbed/no-op that degrades output):**
1. `transforms.rs` — rotate/scale/skew are no-ops → transforms silently produce wrong output
2. `pad.rs` — `layout_pad` missing → padding must be done manually by each caller
3. `shapes.rs` — `layout_curve`/`layout_path` stubbed → curves/paths render empty
4. `inline/box.rs` — body layout stubbed → inline boxes with content render empty
5. `inline/collect.rs` — Frame→TermFrame conversion stubbed → inline frames lose all items

**🟡 Medium (TODO/FIXME that may cause subtle bugs):**
1. Grid subfooter handling (4 TODOs in `repeated.rs` + 1 in `rowspans.rs`)
2. `grid/lines.rs` FIXME for footer position robustness
3. `inline/deco.rs` highlight decoration skipped

**🟢 Low (cosmetic / future enhancement):**
1. `inline/box.rs:118` — label assignment (Agent A dependency)
2. `math/operators.rs` — new file, not a gap but worth noting as extra functionality
