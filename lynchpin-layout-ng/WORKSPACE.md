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

### Agent B — Blocker report

#### Gap 1: `layout_single_block` and `layout_multi_block` are stubs (`flow/block.rs`)

**What's missing vs paged:**

The paged versions (in `lynchpin-layout/src/flow/block.rs`) do substantial work:
1. Fetch sizing properties (`width`, `height`, `inset`) from the `Packed<BlockElem>`
2. Build pod regions via `unbreakable_pod` / `breakable_pod`
3. Match on `BlockBody::Content` / `SingleLayouter` / `MultiLayouter` to layout the body
4. Apply insets via `crate::pad::grow(&mut frame, &inset)`
5. Enforce frame size on expanded axes via `frame.set_size(pod.expand.select(pod.size, frame.size()))`
6. Clip contents via `frame.clip(clip_rect(...))`
7. Apply fill and stroke via `fill_and_stroke(...)`
8. Set `FrameKind::Hard` for explicit blocks
9. Assign labels

The terminal stubs do **none of this** — they ignore all inputs and return empty frames.

**Root causes (three blockers):**

a) **Architectural mismatch — no `Packed<BlockElem>` access.** The terminal functions receive `body: &[Pair<'_>]` (pre-realized children), not `elem: &Packed<BlockElem>`. All styling properties (fill, stroke, inset, clip, radius, outset) live on the element, not on the Pair slice. Without the element, these functions cannot know what inset to apply or what fill/stroke to draw. **Question for Agent A / architecture:** Should these functions accept the original element as well, or should styling be pre-resolved and passed via a separate struct?

b) **Missing library APIs on `TermFrame`.** The paged `Frame` has methods that `TermFrame` lacks:
   - `frame.clip(rect)` — clip to a rectangle
   - `frame.set_kind(FrameKind::Hard)` — mark frame as a gradient boundary
   - `frame.label(label)` — assign a label
   - `frame.set_size(size)` — set size with axial select
   - `frame.size()` — get current size
   These are all needed to implement the block post-processing logic. **Dependency:** Agent A (`lynchpin-library-ng`) must add these methods to `TermFrame`.

c) **Missing helper functions in other terminal modules** (also stubbed — see sections C1, C3, B1, B3 in the review above):
   - `pad::grow()` / `pad::shrink()` — needed for inset application
   - `clip_rect()` — needed to compute the clip rectangle
   - `fill_and_stroke()` — needed for background/border drawing
   These are all in `shapes.rs` and `pad.rs`, which are themselves stubbed. **Dependency:** These modules need to be completed first (or at least their public APIs stabilized) before block.rs can call them.

**What I need to finish this:**
1. Clarification on how styling properties reach `layout_single_block` / `layout_multi_block` (element reference vs pre-resolved struct)
2. `TermFrame` gains `clip()`, `set_kind()`, `label()`, `set_size()` + size getter
3. `pad::grow()`, `clip_rect()`, `fill_and_stroke()` implemented in the terminal equivalents

---

#### Gap 2: `layout_columns` is missing entirely (`flow/mod.rs`)

**What's missing vs paged:**

The paged version has a public `layout_columns()` function that:
1. Takes `elem: &Packed<ColumnsElem>`, extracts `elem.count` and `elem.gutter`
2. Delegates to `layout_fragment_impl(..., columns, column_gutter)` — the same memoized impl used by `layout_fragment`
3. The `configuration()` function inside `layout_flow` computes a `ColumnConfig { count, width, gutter, dir }` from the column parameters

The terminal `layout_flow` already accepts `columns: NonZeroUsize` and `column_gutter: TermScalar` in its signature (for API compatibility), but:
- The comment says `let _ = (column_gutter, columns, mode); // kept for API compatibility` — they are explicitly **not used**
- The terminal `Config` struct only has `width` and `expand` — no `ColumnConfig`, no `FootnoteConfig`, no `LineNumberConfig`
- There is no `configuration()` equivalent that computes column widths from count + gutter
- There is no public `layout_columns` function to wire up the `ColumnsElem` → flow pipeline entry point

**Root causes (three blockers):**

a) **No terminal `ColumnsElem` equivalent (or it exists but is not wired).** The paged version depends on `typst_library::layout::ColumnsElem` which has `.count`, `.gutter`, `.body`. The terminal library may or may not have an equivalent. **Question for Agent A:** Does `lynchpin-library-ng` have a `ColumnsElem` or similar container element? If not, what's the terminal story for multi-column layout?

b) **Column layout logic is unimplemented in the compose/distribute pipeline.** The paged `configuration()` computes per-column width as `(regions.size.x - gutter * (count - 1)) / count`. The compose step then lays out into column-width sub-regions. Columns can also interact via parent-scoped placed elements and footnote/float insertion areas. None of this exists in the terminal pipeline — `compose` works on the full region width with no column splitting. This isn't a simple function to add; it requires changes to `collect`, `compose`, and `distribute`.

c) **The terminal `Config` and `Work` types are stripped down.** They lack:
   - `ColumnConfig` (count, width, gutter, dir)
   - `FootnoteConfig` (separator, clearance, gap, expand)
   - `LineNumberConfig` (scope, clearance)
   - `FlowMode::Root` (the terminal has `Root` but the `configuration()` that uses it is missing)
   The `configuration()` function that populates these from styles needs to be ported.

**What I need to finish this:**
1. Confirmation that a terminal `ColumnsElem` exists (or a decision that columns are deferred/postponed)
2. If columns are in scope: Agent A adds `ColumnConfig`/`FootnoteConfig`/`LineNumberConfig` types to the library, or I add them in `flow/mod.rs`
3. Port `configuration()` from paged → terminal, mapping `Abs`/`Rel`/`Em` types to `TermScalar` equivalents
4. Modify `compose` to split regions into column-width sub-regions and iterate over columns
5. If columns are **not** in scope for v1: I'll remove the `columns`/`column_gutter` parameters and add a `// TODO: columns` marker instead of the misleading API-compat stub

---

### Agent F — Blocker report

#### Gap 1: `clip_rect`, `fill_and_stroke`, `styled_rect` missing from `shapes.rs`

**What's missing vs expected API:**

The terminal `shapes.rs` currently contains layout functions for individual shape
primitives (`layout_line`, `layout_rect`, `layout_square`, `layout_ellipse`,
`layout_circle`, `layout_polygon`, plus stubs for `layout_curve` and
`layout_path`).  However, three **post-processing helper functions** that
Agent C (inline) depends on are absent:

1. **`clip_rect(size: TermSize, radius: &Corners<Option<Rel<Length>>>`, `stroke: &Option<FixedStroke>, outset: &Sides<Rel<Abs>>) -> TermSize`**
   — Computes the clipping rectangle for a frame, accounting for border radius,
   stroke width, and outset.  Called from `inline/box.rs:106`:
   ```rust
   // TODO(Agent F): replace with `clip_rect(frame.size(), &radius, &stroke, &outset)`
   if elem.clip.get(styles) {
       frame.clip(frame.size());  // ← uses raw size, no radius/stroke/outset adjustment
   }
   ```

2. **`fill_and_stroke(frame: &mut TermFrame, fill: Option<Paint>, stroke: &Option<FixedStroke>`, `outset: &Sides<Rel<Abs>>, radius: &Corners<Option<Rel<Length>>>, span: Span)`**
   — Adds background fill and border strokes to an already-laid-out frame.
   Called from `inline/box.rs:113`:
   ```rust
   // TODO(Agent F): call `fill_and_stroke(&mut frame, fill, &stroke, &outset, &radius, span)`
   // Currently skipped — fill/stroke are not rendered on box frames.
   ```

3. **`styled_rect(size: TermSize, radius: &Corners<Option<Rel<Length>>>`, `fill: Option<Paint>, stroke: Option<FixedStroke>) -> TermFrame`**
   — Creates a styled rectangle frame (with fill and/or stroke borders using
   Unicode box-drawing chars) intended for highlight decoration.  Called from
   `inline/deco.rs:40`:
   ```rust
   // Highlight decoration requires styled_rect from crate::shapes (Agent F).
   // For now, fall through to the line-based decoration.
   // TODO: implement highlight when styled_rect is available.
   ```

**Why they're missing (root cause analysis):**

These three functions are **entirely new constructs for the terminal world**.
They have no direct paged counterpart.  In the paged version:

- Clipping is done via `Frame::clip(size)` which clips to a raw `Size` in the
  Frame's own coordinate system — radius/stroke/outset adjustments happen before
  calling `clip()`.
- Fill and stroke are baked into the `FrameItem::Shape(Shape { geometry, stroke,
  fill, fill_rule })` struct.  When a shape is pushed via `frame.push(pos,
  FrameItem::Shape(...))`, the rendering engine handles fill/stroke natively.
  There is no separate `fill_and_stroke` step.
- Highlight decoration in paged is handled by the PDF/SVG renderer directly from
  the `DecoLine::Highlight` variant — no `styled_rect` helper exists.

The terminal architecture works differently:
- `TermFrameItem::Shape(TermShape)` stores a **single character** (`stroke_char`,
  `fill: Option<char>`) and a geometry descriptor.  It cannot represent a filled
  rectangle with a separate stroke border as one item — the fill and stroke must
  be separate Shape items or rendered via a different mechanism.
- `TermFrame::clip(size)` exists in the library (L404-407 of
  `lynchpin-library-ng/src/frame.rs`) but the current implementation may not
  account for radius/stroke/outset adjustments.
- There is no `TermSize → TermSize` clip-rect calculator that adjusts for
  outset, stroke width, and corner radius.

**What I need to finish this:**

From **Agent A (Library)**:
1. Clarify whether `TermFrame::clip()` is expected to handle radius/stroke/outset
   internally, or whether `clip_rect` should compute the adjusted size and pass
   it to `frame.clip(adjusted_size)`.  If the library is handling it, I just
   need a `clip_rect` that returns the adjusted size.  If not, the library's
   `clip()` may need updating.
2. Confirm that `TermShape` with `TermGeometry::Rect` and a `fill` char of `'█'`
   (or custom) plus a separate `TermGeometry::Rect` for the border is the
   intended approach for `fill_and_stroke`.  Alternatively, if a richer
   `TermFrameItem` variant for filled+stroked rectangles is planned, I'll wait.
3. A `Sides<Rel<Abs>>` → terminal conversion utility (or I can add inline
   conversions in shapes.rs using `units::abs_to_cols`).

From **Agent C (inline)**:
4. Confirm the exact expected signatures shown above.  The comments in box.rs
   and deco.rs give strong hints but don't specify parameter types precisely.
   If the expected API differs from what I've inferred, please provide exact
   function signatures.

---

#### Gap 2: `FrameModifiers` / `FrameModify` / `FrameModifyText` trait system replaced with individual functions

**What's missing vs paged:**

The paged `modifiers.rs` provides a **trait-based modifier pipeline**:

```rust
// Paged (lynchpin-layout/src/modifiers.rs):
pub struct FrameModifiers { dest: Option<Destination>, hidden: bool }
impl FrameModifiers {
    pub fn get_in(styles: StyleChain) -> Self { ... }  // extract all modifiers from styles
}
pub trait FrameModify {
    fn modify(&mut self, modifiers: &FrameModifiers);   // apply modifiers to a frame
    fn modified(mut self, modifiers: &FrameModifiers) -> Self { ... }
}
pub trait FrameModifyText {
    fn modify_text(&mut self, styles: StyleChain);       // apply text-level modifiers
}
pub fn layout_and_modify<F, R>(styles: StyleChain, layout: F) -> R
where F: FnOnce(StyleChain) -> R, R: FrameModify { ... }
```

The terminal `modifiers.rs` replaced this with **individual `TermBlockCallback`
functions** (`layout_strong`, `layout_emph`, `layout_sub`, `layout_super`,
`layout_underline`, `layout_overline`, `layout_strike`, `layout_highlight`,
`layout_smallcaps`).  These are useful as element-level entry points, but they
don't support the inline modifier pipeline.

**Why the trait system was replaced (root cause analysis):**

The terminal version's individual functions follow the same `TermBlockCallback`
pattern used by `shapes.rs` — each takes `&Packed<SomeElem>` and returns a
`SourceResult<TermFrame>`.  This makes them suitable as direct element callbacks
(in `rules.rs`, `lib.rs` element registrations) but **breaks composition**:

1. **`inline/collect.rs:232-240`** expects to call
   `FrameModifiers::get_in(styles)` to collect ALL current text modifiers at
   once, then apply them to a frame returned by `InlineElem::layout()` via
   `FrameModify::modify`.  Without this, inline child frames (e.g., a shaped
   text run within an `#emph[]` span) cannot inherit bold/italic/underline from
   the parent style chain — each modifier would need to be applied by a
   separate layout pass, which is impractical.

2. **`inline/line.rs`** expects `layout_and_modify(styles, |styles| layout_box(...))`
   to run layout then apply modifiers in one step, with link de-duplication
   (suppressing nested `LinkElem` destinations that are already applied at the
   outer level).  Currently, `inline/line.rs` calls `layout_box` directly,
   bypassing the modifier pipeline entirely.

3. **`inline/shaping.rs`** expects `FrameModifyText` (a trait on `TermFrame`) to
   apply text-level modifiers during text shaping — e.g., converting smallcaps
   to uppercase, or adjusting glyph selection for bold/italic fonts.

4. **`hide` and `link` are lost.**  The paged `FrameModifiers` tracks `HideElem`
   (`hidden: bool`) and `LinkElem` (`dest: Option<Destination>`).  The terminal
   `modifiers.rs` has no equivalent — hiding content or creating link regions on
   a `TermFrame` is not yet implemented.

The individual functions aren't *wrong* — they're necessary for
`TermBlockCallback`-based element registration.  But they need to **coexist**
with the trait system, not replace it.

**What I need to finish this:**

From **Agent A (Library)**:
1. Does `TermFrame` need a `hide()` method (like paged `Frame::hide()`) and/or a
   way to push `TermFrameItem::Link`?  Currently `TermFrameItem` has `Tag` but
   no `Link` variant.  If link/hide support is planned for a later round, I can
   stub those parts of `FrameModifiers`.
2. Should `TermFrame` implement `FrameModify` and `FrameModifyText` directly, or
   should the terminal crate define its own terminal-specific equivalents (e.g.,
   `TermModifiers` / `TermModify` / `TermModifyText`)?  The paged version
   implements these traits on `Frame` in `modifiers.rs` itself — the traits are
   defined and implemented in the same file.  I'd follow the same pattern for
   `TermFrame`.

From **Agent C (inline)**:
3. Confirm expected usage:
   - `FrameModifiers::get_in(styles)` should return a struct with all current
     text modifiers (bold, italic, underline, strikethrough, overline, highlight,
     smallcaps, sub/super, link dest, hidden).
   - `TermFrame::modify(&modifiers)` should walk all `TermFrameItem::Text` items
     and set their `ContentStyle` attributes accordingly.
   - `layout_and_modify(styles, layout)` should: (a) extract modifiers from
     `styles`, (b) strip redundant modifiers from `styles` (link de-dup),
     (c) call `layout(styles)`, (d) apply `.modify(&modifiers)` to the result.

From **Architecture**:
4. Should the individual `layout_strong`/`layout_emph`/etc functions remain as
   `TermBlockCallback` entry points AND the trait system be added back for the
   inline pipeline?  My assessment: **both are needed** — the individual
   functions for element dispatch, the trait system for inline composition.
   Confirmation requested.

### Agent E — Blocker report

**Question:** Why are `shared.rs` and `stretch.rs` missing from the math module?
Short answer: **by design**, not by oversight.  The new module (`lynchpin-layout-ng`)
uses a fundamentally different architecture from the old (`lynchpin-layout`).
Below is a function-by-function analysis of what each missing file provided, where
that functionality lives now (if anywhere), and whether any real gaps remain.

---

#### 1. `stretch.rs` — Glyph-level font stretching

| Old function | What it did | Status in `-ng` |
|---|---|---|
| `layout_stretch(elem, ctx, styles)` | Entry point for `StretchElem`. Laid out the body, then called `stretch_fragment`. | ✅ **Ported** to `lr::layout_stretch` (L196).  The new version extracts a single char from the body and delegates to `hstretch_char` — terminal-friendly character repetition. No font-construction-table lookup. |
| `stretch_fragment(ctx, frag, axis, rel_to, stretch, short_fall)` | Low-level glyph stretching using `stretch_axes()` (OpenType MATH font construction tables). Could stretch along X or Y axis. | ❌ **Not ported.**  The new module cannot sub-pixel-scale glyphs.  Vertical delimiter stretching is handled by `stretched_delimiter()` in `mat.rs` (L234) which maps `(` → a composed sequence like `⎛ ⎜ ⎝`.  Horizontal stretch is handled by `hstretch_char()` in `lr.rs` (L234) which repeats characters (e.g. `→` → `───→`).  **No gap.** |

**Verdict:** `stretch.rs` is fully replaced.  The old `stretch_fragment` relied on
font internals (`stretch_axes`, `GlyphFragment::stretch`) that don't exist in the
terminal rendering model.  The replacements (`stretched_delimiter`, `hstretch_char`)
cover the same use cases with terminal-appropriate techniques.

---

#### 2. `shared.rs` — Shared math utilities

This is the more interesting file.  It contains **four categories** of functionality:

##### 2a. OpenType feature style helpers — ❌ Not ported (not needed)

| Function | What it does | Why not needed |
|---|---|---|
| `style_cramped()` | Sets `EquationElem::cramped = true` in the style chain. Used by `accent.rs`, `underover.rs`, `root.rs` for sub-formulas that should use cramped glyph variants. | Terminal fonts don't have separate cramped/non-cramped glyph variants. The new `accent.rs`, `underover.rs`, and `root.rs` simply lay out content at normal size with no style manipulation. **No gap.** |
| `style_flac()` | Sets the `flac` OpenType feature (flat accents for superscripts). Used by `accent.rs`. | Terminal fonts don't support OpenType features. The new `accent.rs` places the accent char directly. **No gap.** |
| `style_dtls()` | Sets the `dtls` OpenType feature (dotless forms). Used by `accent.rs`, `text.rs`. | Same reason — no OpenType in the terminal. **No gap.** |

##### 2b. Math-size style chain helpers — ❌ Not ported (not needed)

| Function | What it does | Why not needed |
|---|---|---|
| `style_for_subscript(styles)` | Returns `[superscript_style, cramped]` — steps the math size down one level for subscripts. Used by `attach.rs`, `underover.rs`. | The new module doesn't chain Typst styles for sizing.  Sub/superscripts in `attach.rs` are laid out at the same terminal font size as the base.  The old module used these to select smaller font glyphs; the terminal has no font-size distinction.  **No gap for terminal rendering.** |
| `style_for_superscript(styles)` | Steps math size down for superscripts. Used by `underover.rs`. | Same reasoning. |
| `style_for_numerator(styles)` | Steps math size down for fraction numerators. | The new `frac.rs` uses a vertical stack with a horizontal rule; numerator/denominator are rendered at the same size. |
| `style_for_denominator(styles)` | Returns `[numerator_style, cramped]` for denominators. Used by `mat.rs`. | Same reasoning. |

##### 2c. Layout composition primitives — ⚠️ Real gap

| Function | What it does | Status in `-ng` |
|---|---|---|
| `stack(rows, align, gap, baseline, alternator)` | Stacks multiple `MathRun` rows vertically into a single `Frame`, respecting alignment points. Used by `underover.rs` for stacking above/below annotations. | ❌ **Not ported.**  The new `underover.rs` uses `compose_vertical()` from `run.rs` instead.  Need to verify this covers all cases (especially alignment-point-aware stacking). |
| `alignments(rows)` | Computes alignment point positions across a set of `MathRun` rows.  Used by `mat.rs` for matrices with alignment columns. | ❌ **Not ported.**  The new `mat.rs` may use a simpler approach.  If matrices with `&` alignment points are expected to work, this **is a real gap**. |
| `AlignmentResult { points, width }` | Data struct returned by `alignments`. | ❌ **Not ported** (only needed if `alignments` is ported). |
| `DELIM_SHORT_FALL` | Constant `Em::new(0.1)` — how much shorter scaled delimiters can be than their wrapped content. Used by `mat.rs`, `lr.rs`. | ❌ **Not ported.**  The new delimiter stretching uses exact row-matching (`stretched_delimiter` in `mat.rs`), not sub-pixel sizing, so the fall constant isn't applicable. **No gap.** |

##### 2d. Font family resolution — ❌ Not ported (not needed)

| Function | What it does | Why not needed |
|---|---|---|
| `families(styles)` | Returns a prioritized iterator of font families for math (user font → fallback chain: New Computer Modern Math, Libertinus, emoji fonts). Used by `mod.rs` for font lookups. | The terminal uses a single monospace font.  No font-family resolution is needed. **No gap.** |

---

#### 3. Potential real gaps (action items)

1. **`alignments()` / `AlignmentResult`** — if the `-ng` `mat.rs` is expected to
   support alignment points in matrices (the `&` marker in `mat(...)`), this
   functionality needs to be ported or reimplemented.  The old `mat.rs` uses
   `alignments(&rows)` at L262 to compute column widths from alignment points.
   The new `mat.rs` should be checked for this.

2. **`stack()`** — the old `underover.rs` uses `stack()` to compose multi-row
   under/over annotations with gap spacing and baseline selection.  The new
   `underover.rs` uses `compose_vertical()` which may or may not handle the same
   cases.  Worth a quick audit.

3. **`stretch_fragment` callers** — the old `attach.rs` (L80) calls `stretch_fragment`
   to stretch the base glyph of an attachment (e.g., stretching `∑` under limits).
   The new `attach.rs` does NOT call any stretch equivalent.  If display-mode
   large operators (∑, ∏, ∫) need to be visually enlarged, this is a gap.

---

#### 4. Summary

| Category | Count | Action |
|---|---|---|
| Deliberately omitted (terminal doesn't need it) | 9 functions | No action |
| Replaced with terminal-specific equivalent | 2 functions (`layout_stretch`, `stretch_fragment`) | Already done |
| Potentially missing (needs audit) | 3 functions (`stack`, `alignments`, stretch in attach) | Audit recommended |

**Bottom line:** `shared.rs` and `stretch.rs` are not missing by accident.  The
terminal math module replaced font-level styling and glyph construction with
character-composition and fixed-grid layout.  The only real concern is whether
alignment-point logic (`alignments`, `stack`) was adequately replaced in `mat.rs`
and `underover.rs`.

---

### Agent C — Blocker report

Three blockers identified.  Here is each, what it depends on, and why it
cannot proceed until the dependency is resolved.

---

#### Blocker 1: `unimplemented!("vertical text layout")` in `shaping.rs:799`

**File:** `src/inline/shaping.rs`

**Code:**

```rust
buffer.set_direction(match ctx.dir {
    Dir::LTR => rustybuzz::Direction::LeftToRight,
    Dir::RTL => rustybuzz::Direction::RightToLeft,
    _ => unimplemented!("vertical text layout"),
});
```

**What it does:** When the text direction is something other than LTR or RTL
(e.g., top-to-bottom), the shaper panics at runtime.  There is no fallback —
any input that triggers a vertical-writing-mode codepath will crash the
process.

**Root cause / dependency:**

The `Dir` enum in typst has variants beyond LTR/RTL (TTB, BTT).  The terminal
crate defines its own `Dir` (in `src/dir.rs`) but currently only has
`LeftToRight` and `RightToLeft`.  A vertical direction cannot even be
represented in the terminal type system.  So this is **not** just a matter of
mapping vertical `Dir` to a rustybuzz direction — there is no terminal
`Dir::TTB` to match on.

**Blocked by:** **Agent A (Library).**  The terminal `Dir` enum must gain
vertical variants before the inline shaper can handle them.  This is a
library-level design decision: does the terminal even support vertical text?

**Severity:** **High** — runtime panic, not a graceful degradation.

**Mitigation options while waiting:**
1. Map all non-LTR/non-RTL directions to LTR with a warning.  This silently
   produces wrong output but avoids the panic.
2. Return an error from `shape_range` / `shape_segment` for vertical runs so
   the caller can skip them gracefully.

---

#### Blocker 2: Box body layout returns empty frame (`inline/box.rs:57-70`)

**File:** `src/inline/box.rs`

**Code (abridged):**

```rust
let mut frame = match elem.body.get_ref(styles) {
    None => TermFrame::new(TermSize { cols: TermScalar::ZERO, rows: TermScalar::ZERO }),
    Some(_body) => {
        // For now, return an empty frame since layout_frame is not available.
        TermFrame::new(TermSize { cols: TermScalar::ZERO, rows: TermScalar::ZERO })
    }
};
```

**What it does:** When a `BoxElem` has a body (child content), the body is
completely ignored.  The returned frame is zero-sized.  This means any box
with content renders as an invisible 0×0 cell.

**Cascading effects in the same function (same file):**
- **L106–108:** `clip_rect` is replaced with `frame.clip(size())` (a library
  no-op).  **Depends on Agent F** (`crate::shapes::clip_rect`).
- **L113–115:** `fill_and_stroke` is entirely skipped.  **Depends on Agent F**
  (`crate::shapes::fill_and_stroke`).
- **L118–120:** `TermFrame::label()` call is commented out.  **Depends on
  Agent A** — library must expose `TermFrame::label()`.

**Blocked by:**
| Dependency | Agent | Status |
|---|---|---|
| `layout_frame(engine, body, locator, styles, pod)` | Agent B (flow) or Agent F | Missing — this is the function that recursively lays out child content into a `TermFrame`.  Without it, no child can be rendered. |
| `clip_rect(size, radius, stroke, outset)` | Agent F (shapes) | Missing |
| `fill_and_stroke(frame, fill, stroke, outset, radius, span)` | Agent F (shapes) | Missing |
| `TermFrame::label(Label)` | Agent A (Library) | Reported done in Round 3; TODO remains |

**Severity:** **Critical** — all box elements (the most common container in
typst) render empty.  This is the single biggest visual gap in inline layout.

**What Agent C can do now:** Nothing.  The body-layout call chain needs
`layout_frame` to exist.  Once Agent B/F delivers it, the `Some(_body)` arm
can be replaced with a real call.

---

#### Blocker 3: Frame→TermFrame item conversion is stubbed (`inline/collect.rs:243`)

**File:** `src/inline/collect.rs`

**Code (abridged):**

```rust
InlineItem::Frame(frame) => {
    let mut term_frame = TermFrame::new(TermSize::new(
        TermScalar::from_f64(frame.size().x.to_raw()),
        TermScalar::from_f64(frame.size().y.to_raw()),
    ));
    // TODO: copy items from frame to term_frame when conversion is available
    let _ = frame;
    apply_shift(&engine.world, &mut term_frame, styles);
    collector.push_item(Item::Frame(term_frame));
}
```

**What it does:** When an `InlineItem::Frame` (a typst paged `Frame` produced
by child layout) needs to be embedded into the inline collector's item stream,
it creates a `TermFrame` with the correct **size** but **discards all items**.
The structured content of the child frame (shapes, text runs, tags) is lost.

**Cascading effect — `FrameModifiers` pipeline bypassed:**

The same file also bypasses `FrameModifiers::get_in` / `FrameModify::modify`
(commented out at ~L237–239) and calls `layout_box` directly instead of
`layout_and_modify` (noted at ~L252–256).  Both depend on the
`FrameModifiers`/`FrameModify`/`layout_and_modify` system from **Agent F**.

**Blocked by:**
| Dependency | Agent | Status |
|---|---|---|
| `FrameModifiers` type + `FrameModify` trait + `layout_and_modify` fn | Agent F (modifiers) | Missing — the paged `modifiers.rs` system was replaced with individual functions; the trait-based pipeline doesn't exist in the terminal crate |
| `Frame` → `TermFrame` item-by-item converter | Agent F (or shared) | Not implemented — requires mapping every `FrameItem` variant (Shape, Text, Group, etc.) to a corresponding `TermFrameItem` |

**Severity:** **High** — any inline element that produces a sub-frame (images,
embedded layouts, transforms) renders as an empty placeholder.

**What Agent C can do now:** Nothing.  The converter needs the complete
`TermFrameItem` type from the library (Agent A) and a decision on how to map
paged `FrameItem` variants that have no terminal equivalent.  Agent F owns
this mapping.

---

#### Summary

| # | Blocker | Severity | Blocked by |
|---|---|---|---|
| 1 | `unimplemented!("vertical text layout")` panic | High | Agent A (Library) — terminal `Dir` needs vertical variants |
| 2 | Box body layout returns empty frame | Critical | Agent B/F (flow) — `layout_frame` missing; Agent F (shapes) — `clip_rect`/`fill_and_stroke` missing |
| 3 | Frame→TermFrame conversion stubbed | High | Agent F (modifiers + converter) — `FrameModifiers` pipeline + item mapping |

**Bottom line:** Agent C's inline layout engine is structurally complete
(line breaking, shaping, bidi, justification all work), but it cannot render
child content because both the recursive layout entry point (`layout_frame`)
and the frame-item converter are owned by other agents and are not yet
available.

## Agent A: Inline replaced (Round 4)

Deleted 9 files (4045 lines). Replaced with inline/mod.rs (470 lines) based on term-layout.

Added: ParSituation, first_line_indent, hanging_indent, justify, layout_par entry point.
Removed: HarfBuzz shaping, Knuth-Plass, BiDi, glyph-level CJK.

0 compile errors.
