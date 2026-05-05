# Terminal rendering invariants

The terminal is the whole product. Bugs in how cells are laid out hit
every interaction — the prompt, editing, TUIs, agent output — and are
hard to diagnose because they look like "text just looks weird."
This document locks down the rules the renderer must never break.

If you're about to change `gpui_terminal.rs`, `prepaint_terminal`,
`collect_render_data`, the glyph cache, or the cursor overlay, read
this first.

## Invariant 1: Grid-locked cell origins

> **Every cell is painted at `col * cell_w`. No exceptions.**

Not "usually." Not "except for ligatures." Every cell, every frame.

A "cell" here means the alacritty grid cell. Wide characters occupy
two cells (a head cell plus a `wide_continuation` placeholder);
narrow characters occupy one. Whether the font's glyph for a given
code point happens to advance by `cell_w`, `0`, or `2 * cell_w` is
**irrelevant** to where the next cell's glyph begins — that's still
`(col + 1) * cell_w`.

This is the only way to guarantee that what alacritty's grid says and
what the user sees agree. Any renderer that walks a shaped line and
lets the shaper's advance metrics drive the pen position is wrong —
it will silently desync any time the font lies (missing glyph in
primary font, fallback font with different metrics, ambiguous EAW
chars, broken ligature advances). And it will desync in ways that
*look right most of the time*, which is the worst kind of bug.

### What this means in practice

Two paint strategies coexist in `prepaint_terminal`:

**Fast path — bulk shape, trusted advances.**
A "narrow run" is a sequence of consecutive narrow cells that share
style (fg, bold, italic, underline, strikethrough, hidden). The run
is shaped as one `gpui::ShapedLine`. If — and only if — the shaped
total width matches the grid expectation

```
|shaped.width() - narrow_cells * cell_w| < 0.5 px
```

the line is painted in bulk at its run origin `narrow_start *
cell_w`. This preserves ligatures (FiraCode `=>`, JetBrains Mono
`!=`, etc.) in the overwhelmingly common case: the shaper's advances
happen to equal `cell_w` per cell because the font was designed for
monospace rendering.

**Fallback path — per-char shape, grid-locked.**
If the drift check fails, the same run is re-shaped one character at
a time, and each single-char shaped line is painted at `cell_col *
cell_w`. Ligatures lose their rendering in this run — a correct
tradeoff, because a misaligned ligature is worse than no ligature —
and the drift is confined to the single cell that caused it.
Everything around it stays exactly on grid.

This is the *automatic* fix for the Powerline / Nerd Font icon class
of bugs (missing glyph in Menlo, fallback font returns an advance ≠
`cell_w`). It is also the automatic fix for any future font /
Unicode / shaper bug of the same shape, without per-range special
cases that have to be maintained.

### What this means for the cursor

The cursor is a cell. Its paint x is `cursor_col * cell_w`. Always.

Do not walk text runs to "find" the cursor's x. Do not shape a
prefix to match the cursor to text position. The grid computation
is authoritative — if the text path needs to fall back to per-char
to stay on grid (as described above), the cursor is already right
where it should be.

Historically `prepaint_terminal` had a `shape_cursor_x` helper that
shaped a prefix of the narrow run and added `ps.width()` to
`narrow_start * cell_w`. This worked by coincidence for pure ASCII
runs — the shaped width happened to equal the grid width — and
broke the instant anything else showed up. It's gone. Don't bring
it back.

### What this means for wide chars

A wide char head cell is shaped and painted on its own at
`col * cell_w`. The next cell is the `wide_continuation` placeholder,
which `collect_render_data` treats as part of the same logical
character (its background is painted along with the head cell, its
own character slot is skipped by the phase 2 walk).

If the shaped glyph for a wide char overflows into the third cell
(`col + 2`), that cell's own glyph — painted later at its own grid
origin `(col + 2) * cell_w` — will overlap whatever overflowed, and
whoever paints last wins. In practice fonts with correct metrics
don't do this. If a font does, the fix is the same as for PUA
glyphs: the drift check catches it the moment it happens in a
narrow run, and per-char fallback confines the damage to the
offending cell.

## Invariant 2: Cache keys match shape semantics

The glyph cache is keyed on `(text, style, fg)`. When the fast path
and fallback path are both active for the same row, they cache
entries at different granularity:

- Fast path: one entry per run (`"Brc20BatchMint "`, …)
- Fallback path: one entry per character (`"\u{e0a0}"`, `" "`, …)

That's fine — the cache has room for both, and the fallback path's
entries are reused aggressively across rows and frames (a single
space or `'m'` hits the same entry no matter which run it's in).
But any future change that introduces a third shaping strategy
must add a cache key that distinguishes it, or the cache will
return the wrong shaped line.

## Invariant 3: alacritty is authoritative for widths

`collect_render_data` walks an `alacritty_terminal::Term` and builds
our `RenderData`. Do not second-guess alacritty on whether a cell is
wide or narrow. If alacritty said "narrow," paint it as narrow, even
if the char's Unicode East Asian Width or the font's glyph metrics
suggest otherwise. The drift detection above is the safety net for
disagreements; it is not a license for the renderer to reclassify
cells.

In particular: do NOT add "is the Unicode code point in a CJK
range?" or "is the font glyph wide?" checks to Phase 2. Alacritty
already decided, and the shell wrote bytes to the PTY based on
alacritty's decision. Disagreeing creates a different bug class
(cells off by one from where the shell thinks they are) that is
much worse than a font glyph overflowing a cell.

## Testing the invariants

Pure position arithmetic (nothing is "computed" other than
`col * cell_w`) isn't interesting to test on its own. What's
interesting — and what's broken historically — is the path from
"bytes written to the PTY" to "cell at column N has character C."

The regression test for this class of bugs lives in
`crates/amux-platform/tests/pty_smoke.rs` and
`crates/amux-platform/tests/render_grid_layout.rs`. The second file
specifically asserts that writing a Powerline prompt containing
`\u{e0a0}` to an alacritty `Term` produces cells at the expected
columns, exercising the same `collect_render_data` contract the live
renderer consumes.

If you're fixing a rendering bug, add a case to
`render_grid_layout.rs` that captures the failing input before you
touch the renderer. The test is the only tool we have to keep
future "optimizations" from re-breaking this surface.
