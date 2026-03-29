# AMUX Canvas Rendering Refactor - Handoff Document

## Current Session Summary (2026-03-29)

This session covered extensive development of the AMUX terminal multiplexer. The app is functional but the terminal rendering has a fundamental architectural problem that needs to be solved in the next session.

---

## Architecture Overview

```
apps/desktop/                    # GPUI desktop app
  src/gpui_entry.rs              # Main entry, keyboard/mouse, workspace management (~1600 lines)
  src/gpui_terminal.rs           # Terminal renderer (THE FILE TO REFACTOR)
  src/gpui_status_bar.rs         # Bottom status bar
  src/gpui_workspace_sidebar.rs  # Left sidebar

crates/amux-platform/
  src/terminal/
    alacritty_view.rs            # AlacrittyTerminal wrapper (NEW - this session)
    manager.rs                   # TerminalManager - per-workspace pane/tab/split layout
    emulator.rs                  # OLD custom emulator (still exists, no longer used by desktop)
    view.rs                      # OLD TerminalView (still exists, no longer used by desktop)
    backend.rs                   # OLD PTY backend via portable-pty (still exists)
```

## What Was Accomplished

### Major Features Implemented
1. **Alacritty terminal backend** - Replaced custom emulator with `alacritty_terminal` v0.25
2. **Per-workspace terminal managers** - Each workspace has independent split layouts
3. **Split pane resize** - Mouse drag to resize with pixel-based layout
4. **Workspace management** - Create, rename (double-click), switch, persist
5. **Layout persistence** - Split layouts saved to `~/.amux/layouts.json`
6. **Session persistence** - Workspace list saved to `~/.amux/session.json`
7. **IME Chinese input** - Via `EntityInputHandler` + canvas paint registration
8. **Mouse scroll** - Forwards to PTY when mouse mode active, else scrollback
9. **Context menu** - Right-click with dismiss overlay
10. **Block character rendering** - U+2580-U+2593 rendered as colored rectangles
11. **Box-drawing character rendering** - U+2500-U+256C rendered as pixel lines
12. **Shell detection** - Auto-detects pwsh.exe > powershell.exe, $SHELL on Linux
13. **PowerShell PSStyle fix** - Removes background colors from directory listings
14. **LS_COLORS** - Set for WSL to remove background colors
15. **Alacritty Tomorrow Night color scheme** - Full 16-color + dim variants

### Key Files Changed
- `apps/desktop/Cargo.toml` - Added `alacritty_terminal`, `serde_json`
- `crates/amux-platform/Cargo.toml` - Added `alacritty_terminal`, `serde`, `serde_json`
- `crates/amux-platform/src/terminal/alacritty_view.rs` - **NEW** AlacrittyTerminal wrapper
- `crates/amux-platform/src/terminal/manager.rs` - Rewritten for AlacrittyTerminal + serialization
- `apps/desktop/src/gpui_terminal.rs` - Rewritten for alacritty `renderable_content()` API
- `apps/desktop/src/gpui_entry.rs` - Extensive changes for new terminal API

---

## THE CORE PROBLEM: Terminal Rendering Architecture

### Current Approach (div + text runs)
```
gpui_terminal.rs: render_grid()
  -> For each row: absolute-positioned div at y = row * CELL_HEIGHT
    -> For each style run: div with .w(px(char_count * CELL_WIDTH)).child(text)
      -> Background: explicit width fills cell grid
      -> Text: rendered by GPUI font engine at font's natural width
```

### Why It Breaks
**CELL_WIDTH is hardcoded (currently 7.2) but the font's actual character advance width varies by platform/DPI/font.**

| CELL_WIDTH | Effect |
|---|---|
| Too large (8.4) | Characters spaced too far apart, cursor far from prompt |
| Too small (5.1) | Text gets clipped by div width (overflow_hidden) |
| No width set | Background colors don't fill cells, dark stripe artifacts |
| Current (7.2, no overflow_hidden) | Text may overflow into adjacent cells, cursor misaligned |

This single constant causes ALL these issues:
- Cursor position doesn't match text position
- `\amux>` characters disappearing (overflow_hidden clipping)
- Dark stripes on right side (no width = bg doesn't fill)
- Spacing too wide or too narrow
- Block/box characters misaligned with text

### The Solution: Canvas-Based Rendering (like Zed)

Zed's terminal renders each character individually at exact pixel positions using GPUI's text shaping system:

```rust
// Pseudocode for canvas-based approach:
fn render_terminal(term: &AlacrittyTerminal, window: &Window) {
    // 1. Measure font to get actual cell dimensions
    let text_system = window.text_system();
    let font = resolve_monospace_font("Cascadia Code", 14.0);
    let cell_width = measure_char_advance(text_system, font, 'M');
    let cell_height = font.line_height();

    // 2. Use canvas element for pixel-perfect rendering
    canvas(
        |bounds, window, cx| {
            // Prepaint: compute layout
            (bounds, cell_width, cell_height)
        },
        |bounds, (_, cw, ch), window, cx| {
            // Paint phase: render each cell at exact position
            for row in 0..rows {
                for col in 0..cols {
                    let x = bounds.origin.x + col * cw;
                    let y = bounds.origin.y + row * ch;

                    // Paint background rectangle
                    window.paint_quad(Quad {
                        bounds: Bounds::new(point(x, y), size(cw, ch)),
                        background: cell.bg,
                        ..Default::default()
                    });

                    // Paint character glyph at exact position
                    let shaped = text_system.shape_line(&cell.ch.to_string(), font, font_size);
                    shaped.paint(point(x, y + ascent), window, cx);
                }
            }
        },
    )
}
```

### Key GPUI APIs for Canvas Rendering

```rust
// Text system (available via window.text_system())
window.text_system() -> Arc<WindowTextSystem>
text_system.layout_line(text, font_size, runs, force_width) -> Arc<LineLayout>
LineLayout { width: Pixels, ascent: Pixels, ... }

// Canvas element
gpui::canvas(prepaint_fn, paint_fn) -> Canvas<T>

// Paint primitives (available in paint phase)
window.paint_quad(quad)           // Fill rectangle with color
window.paint_glyph(glyph_id, ...)  // Paint single glyph
ShapedLine::paint(origin, window, cx)  // Paint shaped text

// Font resolution
gpui::Font { family, weight, style, ... }
text_system.resolve_font(&font) -> FontId  (might need different API)
```

### Reference: How Zed Does It

Zed's terminal renderer: `crates/terminal_view/src/terminal_element.rs`
- Uses `Element` trait with custom `prepaint()` and `paint()` phases
- Measures font in `prepaint()` to get cell dimensions
- Paints backgrounds as `window.paint_quad()` in `paint()`
- Paints text using `ShapedLine::paint()` at exact positions
- Handles cursor, selection, hyperlinks all in paint phase

Key Zed terminal files to reference:
- `crates/terminal_view/src/terminal_element.rs` (~1500 lines, the main renderer)
- `crates/terminal_view/src/terminal_view.rs` (view wrapper)

---

## Current State of Key Components

### AlacrittyTerminal (`alacritty_view.rs`)
- Working: PTY spawn, input, resize, scroll, title
- `with_term(|t| ...)` callback for reading Term state
- `send_input(bytes)` for keyboard/paste
- `scroll_up/down/to_bottom()` for scrollback

### TerminalManager (`manager.rs`)
- Working: split, close, tab management, layout persistence
- `PaneLayout` serializable (Serialize/Deserialize)
- `save_layout() -> String` / `restore_layout(json) -> Option<Self>`

### Input Handling (`gpui_entry.rs`)
- `on_key_down` handles special keys (Enter, Tab, arrows, Ctrl+X)
- `EntityInputHandler::replace_text_in_range` handles all character input (EN + CN)
- IME registered via `canvas()` paint callback + `window.handle_input()`
- Workspace rename input routed through `replace_text_in_range`

### Rendering (`gpui_terminal.rs`)
- `render_alacritty_terminal()` reads from `term.with_term()`
- Collects cells into `RenderData` struct
- `render_grid()` creates div-based layout (TO BE REPLACED)
- `render_block_char()` handles U+2580-U+2593 block characters
- `render_box_drawing()` handles U+2500-U+256C line characters
- Color scheme: Alacritty Tomorrow Night

---

## What the Next Session Should Do

### Phase 1: Canvas Renderer
1. Create a custom GPUI `Element` in `gpui_terminal.rs`
2. In `prepaint()`: measure font, compute cell dimensions, collect render data from alacritty
3. In `paint()`:
   - Paint background rectangles with `window.paint_quad()`
   - Paint text glyphs with `ShapedLine::paint()` at exact grid positions
   - Paint cursor at exact position
4. Block and box-drawing chars: paint as colored quads (already have the logic)

### Phase 2: Dynamic Cell Size
1. Remove hardcoded `CELL_WIDTH` / `CELL_HEIGHT` constants
2. Measure from actual font: `layout_line("M", size, runs)` → get width
3. Use measured dimensions for: resize calculation, cursor, block chars, box-drawing

### Phase 3: Selection (currently disabled)
1. Wire mouse events to alacritty's selection API
2. Render selection highlight in paint phase
3. Copy selected text to clipboard

### What NOT to Change
- `alacritty_view.rs` — working fine
- `manager.rs` — working fine (layout/split/tabs)
- `gpui_entry.rs` keyboard handling — working fine
- IME input — working fine
- Layout persistence — working fine

---

## Build & Test Commands

```bash
# Compile (WSL/Linux)
cargo check -p amux-desktop --features gpui

# Run (WSL with X11/Wayland)
cargo run -p amux-desktop --features gpui

# Run (Windows)
cargo run -p amux-desktop --features gpui -- --real

# Tests
cargo test -p amux-platform -p amux-core

# Key dependencies
alacritty_terminal = "0.25"
gpui (from zed git repo)
```

## Known Issues to Fix During Refactor
1. CELL_WIDTH mismatch (root cause of most rendering bugs) → SOLVED by canvas
2. Cursor position drift → SOLVED by canvas
3. Text selection disabled → Re-implement with alacritty selection API
4. `measured_cell_width` field exists but unused → Remove or use in canvas
5. Old emulator.rs still in codebase → Can delete after canvas refactor verified
6. Old view.rs/backend.rs still in codebase → Can delete after verified
