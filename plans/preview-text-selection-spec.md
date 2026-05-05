# Preview Text Selection — Spec

**Status**: draft, awaiting approval
**Owner**: amux preview subsystem
**Target release**: next preview iteration after Phase 1 LineMeta
**Depends on**: `gpui::StyledText` (public API), existing `PreviewState` / `PreviewElement` tree

## 1. Objective

Add native character-level text selection + clipboard copy to amux's markdown preview, without vendoring gpui-component internals or introducing a separate "Reader mode." A single render path continues to support heading navigation (`[` / `]`), TOC overlay (`o` / `:`), search (`/`), auto-reload, and the existing `Y` / `c` shortcuts, while gaining mouse-drag selection and `Cmd/Ctrl+C` copy.

**Non-goals this round:**
- Code-file previews (pure-code `uniform_list` path) — deferred to a follow-up. The row-per-line render has different coordinate semantics and warrants its own scope.
- Cmd/Ctrl+A select-all — deferred.
- Inline per-character search highlight — Tranche B leftover, not blocked by this.
- Rich-format copy (HTML/markdown fidelity). Plain text only, with block boundaries as `\n`.

## 2. Target users

- Users previewing markdown files who need to copy arbitrary ranges mid-paragraph, mid-list, or across block boundaries — not just "whole file" (`Y`) or "first code block" (`c`).
- Common scenario: pasting a quoted sentence from a README into a chat / terminal / commit message.

## 3. User-visible behavior

### Input model (mirrors gpui-component)
- **Mouse down inside preview body** → begin selection at cursor position.
- **Mouse drag** → extend selection end to current cursor position.
- **Mouse up** → stop extending; selection stays highlighted.
- **Mouse down outside preview body** (including header, footer strip, outside the panel) → clear selection.
- **Single click with no drag** → clears any prior selection; if click landed on a link, follow the link (same precedence as gpui-component: drag ⇒ selection wins, pure click ⇒ link wins).
- **`Cmd+C` / `Ctrl+C`** when preview is focused and selection non-empty → copy selected plain text to clipboard.

### Visual
- Selected characters show a translucent background rect behind them (theme `selection` color).
- Highlight survives scroll: selection stays anchored to text content, not to viewport pixels.
- Cross-block selections (e.g. last word of paragraph + first word of next heading) render one continuous highlight across both.

### Copy format (plain text)
- Characters within the selection range, extracted in document order.
- Block boundaries contribute `\n` between adjacent blocks (paragraph → `\n` → heading text).
- Inline marks (bold/italic/code/links) lose their markup in the copied text — the rendered glyphs are what the user sees, so the rendered glyphs are what they get.
- Trimmed: leading and trailing whitespace-only content at both ends of the selection is discarded (matches gpui-component `on_action_copy`).

### Invariants that stay working
- `[` / `]` heading jump
- `o` / `:` TOC overlay (arrow keys / Enter / Esc / typing filter)
- `/` search (code path only; unchanged scope)
- `n` / `N` search navigation
- `Y` copy full document, `c` copy first code block
- Auto-reload via notify (file change → re-parse)
- Header button row (`TOC`, `Find`, `Copy`)
- Persistent footer hint bar
- Scroll position preservation when switching preview tabs

### Auto-reload interaction
- On `PreviewState::load` swap, **clear selection**. Rationale: element indices and byte offsets shift; preserving selection risks copying wrong text.
- The render loop detects swap via `PreviewState` identity change; selection state stores a `path` and `generation` counter. Mismatch → drop.

## 4. Architecture

### Layered mirror of gpui-component's TextView

| Layer | gpui-component name | amux equivalent | Purpose |
|-------|---------------------|-----------------|---------|
| 1 | `GlobalState::text_view_state_stack` | **Not needed.** We pass `Option<&PreviewSelectionState>` down through `render_markdown_body` → per-element render. Our tree depth is bounded. | Let inline text nodes see selection state. |
| 2 | `TextViewState` (entity) | `PreviewSelectionState` (owned on `GpuiShellView`, non-entity) | Persistent selection state. |
| 3 | Mouse handlers in `TextView::paint` | `render_markdown_body` root div's `on_mouse_down` / `on_mouse_move` / `on_mouse_up` | Drive selection from pointer events. |
| 4 | `Inline` element + `StyledText` | New `SelectableText` element wrapping `StyledText`, replaces every text-producing div in `render_element` | Char-pixel mapping + selection paint. |
| 5 | `ParsedDocument::selected_text()` | `PreviewSelectionState::extract_selected_text(preview)` | Walk PreviewElement tree, collect bytes in range. |
| 6 | `KeyBinding("cmd-c", Copy, Some("TextView"))` | Route through existing preview keystroke handler — `Cmd+C` / `Ctrl+C` calls `copy_selection_to_clipboard()` | Clipboard write. |

Layer 1 is skipped because amux's preview has a single root render function that already threads state down via parameters (we did this for `preview_search` and `preview_scroll_handle`). No need for a global stack.

### Core data types

```rust
// apps/desktop/src/preview_selection.rs

/// Selection state for the active markdown preview. Scoped to a
/// single preview path + load generation; auto-cleared when either
/// changes.
pub struct PreviewSelectionState {
    pub path: String,
    /// Incremented on every PreviewState load into preview_tabs for
    /// `path`. Stored so we can detect reloads and invalidate stale
    /// selections without tracking every possible invalidation.
    pub generation: u64,
    /// Content-relative coordinates — (pos - bounds.origin - scroll_offset)
    /// at the moment the point was captured. `None` means no selection.
    pub anchor: Option<Point<Pixels>>,
    pub head: Option<Point<Pixels>>,
    /// Mouse button is currently held.
    pub is_selecting: bool,
    /// Window-space bounds of the markdown body container, updated
    /// every frame via on_prepaint. Needed to convert window-space
    /// mouse positions back to content space.
    pub bounds: Bounds<Pixels>,
}

/// A byte-offset range into an element's text. Produced during paint
/// by SelectableText, consumed by extract_selected_text.
pub struct InlineSelection {
    pub element_idx: usize,
    pub span_idx: usize,    // which TextSpan within the element
    pub byte_start: usize,
    pub byte_end: usize,
}
```

### Coordinate system

- **Window coordinates**: native gpui pixel space, origin at window top-left. Mouse events arrive here.
- **Content coordinates**: `window - markdown_body_bounds.origin - list_scroll_offset`. Invariant under scrolling. Selection state stores content coords.
- Conversion points:
  - Capture (mouse down/move): window → content at store time.
  - Hit test (inside `SelectableText::paint`): content → window by adding back `bounds.origin + scroll_offset`, then compare against per-character window-space bounds from `TextLayout::position_for_index`.
  - `bounds` updated via `on_prepaint` on the root markdown body div.
  - `list_scroll_offset` pulled from the existing `ListState::scroll_px_offset_for_scrollbar()` (we already have the `ListState`).

### Event flow

```
MouseDown inside body ──► clear_selection() → start_selection(window_pos)
                          (convert to content coords, store anchor=head)
                          is_selecting = true

MouseMove while is_selecting ──► update_head(window_pos)

MouseUp ──► is_selecting = false (selection stays)

MouseDown outside body ──► clear_selection()

Cmd/Ctrl+C + has_selection ──► extract_selected_text(preview) → clipboard

Render tick ──► if preview_tabs[path].generation ≠ state.generation: clear
```

### SelectableText element

A thin wrapper around `gpui::StyledText` that:
1. Renders the text with whatever style runs we produce from `TextSpan`s.
2. During paint, if `PreviewSelectionState` is Some and has a non-empty range:
   - For each character in the run, query `text_layout.position_for_index(byte_offset)` → window-space (x, y).
   - Test if (x, y) falls within the selection rect (using the same `point_in_text_selection` geometry gpui-component uses — reimplemented ~30 LOC).
   - Accumulate a contiguous byte range.
   - Paint a selection background `quad` for that range (single-line: one quad; multi-line: three quads handling first partial line, middle full lines, last partial line).
3. Stashes the computed byte range somewhere the owning `PreviewSelectionState` can read for the copy path.

### Selected-text extraction

`extract_selected_text(&PreviewState, &[InlineSelection]) -> String`:
- Sort `InlineSelection` entries by `(element_idx, span_idx, byte_start)`.
- Walk sorted entries; for each, slice the span's text by byte offsets; push to output.
- Between entries that cross an element boundary, push `\n`.
- Trim final result (matches gpui-component `on_action_copy`).

Block-type conventions for the `\n` insertion (borrowed from gpui-component):
- Heading, Paragraph, Blockquote: trailing `\n` after the block's content (if non-empty).
- ListItem: no block-level `\n` between items (items flow together; gpui-component does this too).
- CodeBlock: trailing `\n`.
- Table: row-joined cells with `" "`, rows joined with `\n`, block trailing `\n`.
- HorizontalRule: no text contribution.

## 5. Project structure

```
apps/desktop/src/
├── preview_selection.rs            (NEW, ~250 LOC)
│   ├── PreviewSelectionState
│   ├── InlineSelection
│   ├── helpers on GpuiShellView:
│   │   ├── preview_selection_start / update / end / clear
│   │   ├── copy_preview_selection (Cmd+C handler)
│   │   └── sync_preview_selection_generation (auto-reload invalidation)
│   └── extract_selected_text(preview, inline_selections) -> String
├── gpui_preview.rs                 (edited, ~100 LOC diff)
│   ├── SelectableText element (new, replaces raw div text nodes)
│   └── render_element: every text fragment now routes through SelectableText
├── gpui_entry.rs                   (edited, ~20 LOC diff)
│   └── Add `preview_selection: Option<PreviewSelectionState>` field
├── gpui_input_handler.rs           (edited, ~30 LOC diff)
│   └── Cmd/Ctrl+C while preview active + has_selection → copy
└── main.rs                         (edited, 1 LOC)
    └── mod preview_selection;
```

### Module responsibilities

- **`preview_selection.rs`**: selection state, coord math, text extraction, clipboard action. No rendering.
- **`gpui_preview.rs`**: only rendering. Owns `SelectableText` because it's a thin render wrapper.
- **`gpui_entry.rs`**: field declaration + constructor init.
- **`gpui_input_handler.rs`**: keystroke routing only.

## 6. Commands / keystrokes

No new user-typed commands. Keystroke additions:

| Keystroke | Context | Action |
|-----------|---------|--------|
| `Cmd/Ctrl+C` | preview active tab, has selection | copy selection to clipboard; existing terminal-selection Cmd+C path unchanged |
| mouse down/move/up | markdown body only | selection lifecycle |

All other preview keystrokes (`[`, `]`, `o`, `:`, `/`, `n`, `N`, `Y`, `c`, Enter, Esc, arrows) keep their current meaning. `Cmd/Ctrl+C` with a terminal pane focused continues to route to the terminal copy path — preview Cmd+C only fires when active tab is Preview AND `preview_selection` has a non-empty range.

## 7. Testing strategy

### Unit tests (in `preview_selection.rs`)

- **Coord conversion**
  - `window_to_content(p, bounds, scroll) == p - bounds.origin - scroll`
  - `content_to_window(p, bounds, scroll) == p + bounds.origin + scroll`
  - Round-trip identity.
- **Selection rect geometry**
  - `point_in_text_selection` — single-line case, multi-line case, reverse-selection case (end before start).
  - Rectangular selection: a point on line 2 between start-x and end-x is included.
- **Byte-range merging**
  - Two adjacent `InlineSelection`s in the same span merge to one range.
  - Two in different spans of same element produce two entries with no `\n` insert.
  - Two in different elements produce `\n` separator.
- **`extract_selected_text` matrix**
  - Single-span paragraph selection.
  - Cross-span (bold run + normal run) → bold marks stripped, text contiguous.
  - Cross-element (end of heading + start of paragraph) → `\n` between.
  - Table row selection → cells joined with `" "`, rows with `\n`.
  - CodeBlock selection of lines 2–4 → `line2\nline3\nline4`.
  - Whole-document selection matches `Y` output byte-for-byte.
  - Empty selection → empty string.
  - Whitespace-only selection → trimmed to empty.

### Integration (manual, documented in test plan)
- Open README.md preview, drag across a paragraph, `Cmd+C`, paste into shell — matches visible text.
- Scroll mid-selection: highlight stays on the right text.
- Start selection, trigger auto-reload (save the file from an editor): selection disappears, no crash.
- Start selection in pane A, switch to pane B terminal, `Cmd+C` — terminal copy fires, not preview (verify scoping).
- Click a link that's inside the current selection: selection clears, link opens.
- Cross-block selection: select last word of paragraph through first word of next heading, paste — both words present, newline between.
- Selection on CJK + emoji content: byte offsets align to character boundaries (no mid-codepoint slicing).

### Regression locks
- All 9 `extract_command_after_prompt` tests stay green.
- All 6 `preview_search` tests stay green.
- All 4 `build_headings_index` tests stay green.
- `cargo test -p amux-desktop --features gpui` → 140+ pass.

## 8. Code style

Match existing amux conventions:
- Rust 2024 edition.
- No per-line "what" comments on obvious code; document *why* for non-obvious decisions (coord direction conventions, atomic-save invalidation reasoning, etc.).
- Structured failure handling: empty selection → `Option::None`, not a sentinel empty string.
- Public types get rustdoc explaining non-obvious invariants (e.g. `generation` counter rationale).
- No `unsafe`. `proc_pidinfo` usage already isolated in platform layer — stays there.
- Module file size cap: if `preview_selection.rs` pushes past ~400 LOC, split into `state.rs` + `paint.rs` + `extract.rs` submodules.
- Tests colocated (`#[cfg(test)] mod tests`) unless integration-flavored.

## 9. Boundaries

### Always
- Preserve every existing preview feature listed in §3's "Invariants that stay working."
- Keep `PreviewElement` enum shape stable — no variant changes. `TextSpan` gains an optional `byte_range_in_element` field for extraction; no semantic changes to existing fields.
- Every new public type has a doc comment explaining *why* it exists.
- `cargo clippy -p amux-desktop --features gpui` stays clean on new code (not responsible for pre-existing warnings).
- Selection state cleared on: auto-reload, tab switch away from preview, Escape pressed with no other modal open, click outside markdown body.

### Ask first
- Before changing `PreviewElement` or `TextSpan` structural fields (not comments, not adding optional aux fields).
- Before touching `render_code_block_fullscreen` — code files are out of scope this round.
- Before introducing any new feature-gated dep in `Cargo.toml`.
- Before changing `Cmd/Ctrl+C` behavior outside the preview path (terminal-selection copy must not regress).
- Before making `SelectableText` or `PreviewSelectionState` `pub` beyond the crate (keep them `pub(crate)` unless external access proves necessary).

### Never
- Never vendor gpui-component internals (`Inline`, `InlineState`, `point_in_text_selection` impl) into amux. Re-implement using public `gpui::StyledText` + `TextLayout` APIs only.
- Never add a "Reader mode" switch that duplicates the render tree.
- Never block the render thread on clipboard write — `cx.write_to_clipboard` is synchronous and cheap, but if that changes, move to `cx.spawn`.
- Never use `unwrap()` in paint path. Selection extraction failure renders no selection, writes no clipboard, logs nothing visible.
- Never persist selection across app restart. Selection is ephemeral per session.
- Never synthesize fake text (markdown markup back in) in the copied output — rendered glyphs only.

## 10. Risk register

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| `StyledText::position_for_index` returns wrong values for CJK / emoji / ligatures | medium | Integration test on CJK+emoji document; lock byte-offset alignment test. |
| Scroll-during-drag causes selection to "jump" visually | medium | Use content coords throughout; integration test by scrolling mid-drag. |
| Cmd+C in a terminal pane leaks to preview | low | Active-tab gate — already the pattern for every preview shortcut. |
| List virtualization drops `SelectableText` elements out of view, losing their per-frame byte-range info | medium | Selection state is the source of truth; per-paint byte ranges are re-derived from geometry each frame. An item scrolled out simply doesn't paint, which is correct. |
| Byte offset math breaks at span boundaries on edits during auto-reload mid-drag | low | Generation counter invalidation catches this — selection clears on reload. |
| Selection on very long paragraphs (>10k chars) runs slow due to per-char hit test | low | First pass: walk linearly. If profiling shows hot, cache row boundaries from `TextLayout`. |

## 11. Implementation plan (not a commitment — for scoping only)

Suggested ordering, each step independently verifiable:

1. **Data types + constructor wiring** (day 0.5)
   - Add `PreviewSelectionState` + field on `GpuiShellView`.
   - Stub coord conversion helpers.
   - Unit tests for coord round-trip.

2. **SelectableText rendering** (day 1)
   - New element wrapping `StyledText`.
   - No selection yet — just proves the text layout machinery works in our context.
   - Replace `render_element`'s text-producing divs with SelectableText.
   - Visual regression: markdown should look identical.

3. **Mouse-driven selection state** (day 1)
   - Root-div mouse handlers populate `PreviewSelectionState`.
   - No paint yet — verify via eprintln that content coords are sane under scrolling.

4. **Selection paint** (day 1)
   - `SelectableText::paint` draws the selection background.
   - Visual QA: drag across single paragraph, cross paragraphs, with scroll.

5. **Text extraction + Cmd+C** (day 0.5)
   - `extract_selected_text` walker.
   - Keystroke handler in `gpui_input_handler.rs`.
   - Unit tests for extraction matrix.

6. **Auto-reload invalidation** (day 0.25)
   - Generation counter on `PreviewState`.
   - Render-tick check clears stale selection.

7. **Polish + regression pass** (day 0.5)
   - Click-outside clears.
   - Link vs selection precedence.
   - Full integration test pass (manual).
   - `cargo test` green.

Total: ~4 days of focused work. Splittable if any step hits unexpected complexity.

## 12. Open questions

- None at spec-approval time. If any surface during implementation, they come back here before branching from the plan.
