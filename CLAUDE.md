# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Amux

Amux is an AI-focused terminal multiplexer built in Rust. It uses GPUI (Zed's UI framework) for rendering and alacritty_terminal 0.25 for terminal emulation. Primary target is Windows (with WSL support); Linux support via `gpui-linux` feature.

## Build & Run

```bash
# Build (Windows, default)
cargo build -p amux-desktop --features gpui

# Build (Linux)
cargo build -p amux-desktop --features gpui-linux

# Run
cargo run -p amux-desktop --features gpui

# Run tests (whole workspace)
cargo test --workspace

# Run tests for a specific crate
cargo test -p amux-platform

# Run a single test
cargo test -p amux-desktop --features gpui -- test_name
```

The `gpui` feature is required for the GUI — without it, only a basic text TUI runs. The feature gates most modules in `apps/desktop/src/main.rs`.

## Architecture

**Workspace crates** (in `crates/`):
- `amux-core`: Domain model — workspace, layout (splits/tabs), sessions, surfaces, commands, events, IDs
- `amux-platform`: OS abstraction — terminal backend (`alacritty_terminal`), filesystem, shell, WSL detection/fs ops, path mapping
- `amux-ui`: App state, controller logic, rendering trait (`AppRenderer`), command palette. `DesktopApp` is the root object
- `amux-workspace`, `amux-session`, `amux-agent`: Thin wrappers (workspace/session/agent data types)

**Desktop app** (`apps/desktop/src/`):
- `gpui_entry.rs`: Main GPUI view (`GpuiShellView`) — owns all state, input routing, rendering orchestration. This is the largest file (~100KB) and the central hub.
- `gpui_terminal.rs`: Canvas-based terminal rendering — glyph shaping, cell painting, cursor, selection, scrollbar. Uses thread-local glyph cache with generation-based eviction.
- `gpui_layout_renderer.rs`: Renders workspace layout (splits, tabs, context menus, pickers)
- `gpui_input_handler.rs`: GPUI `InputHandler` impl — keyboard, IME, file picker input
- `gpui_preview.rs`: File preview panel — Markdown (pulldown-cmark), syntax highlighting (14 languages), file picker (Ctrl+P)
- `gpui_workspace_sidebar.rs`: Workspace list sidebar (resizable)
- `gpui_workspace_persistence.rs`: Save/load layouts as JSON, startup scripts with per-pane env (Win/WSL)
- `gpui_vibe_tools.rs`: AI tool integration (Claude, Cursor, etc.), path conversion helpers
- `gpui_config.rs`: Config from `~/.amux/config.toml` (font, theme, scrollback)

**Terminal management** (`crates/amux-platform/src/terminal/`):
- `manager.rs`: `TerminalManager` — pane tree (splits), tab management, resize. Owns `PaneTab`, `CommandPhase`, agent status detection.
- `alacritty_view.rs`: `AlacrittyTerminal` wrapping `alacritty_terminal::Term` — the live emulator. On Unix, PTY reads are routed through `FilterPty` → `OscInterceptor` before hitting the VTE parser.
- `osc_intercept.rs`: Byte-level state machine extracting OSC 7 (cwd) and OSC 133 (shell integration) from the PTY stream. See "OSC integration" below.
- `backend.rs`: PTY spawn + reader thread (`portable-pty`)
- `keys.rs`: Key-event → PTY-byte translation

## OSC integration

The PTY output stream passes through `OscInterceptor` before alacritty sees it. Extracts:

- **OSC 7** (`ESC ] 7 ; file://host/path ST`) — working directory. Updates `PaneTab.shell_reported_cwd`, which `active_tab_live_cwd` checks first (before the title-change cache, before the process cwd syscall, before the saved spawn cwd).
- **OSC 133** (shell integration lifecycle) — drives `PaneTab.shell_integration_phase`:
  - `133;A` prompt start → `CommandPhase::PromptReady` → `AgentStatus::Waiting`
  - `133;B` / `133;C` command executing → `CommandPhase::Executing` → regex still runs (agents like claude hide inside long-running commands; regex refines Thinking vs Waiting)
  - `133;D[;exit]` command finished → `CommandPhase::FinishedOk` or `FinishedErr(exit)` → `AgentStatus::Done` / `Error`

Every other OSC (0/2 title, 4/10/11 palette, 52 clipboard, 9/777 notifications that we don't yet handle) passes through byte-for-byte so alacritty's existing handling stays untouched.

**Fallbacks preserved**: shells without OSC 7 keep using the title-change → `proc_pidinfo` chain; shells without OSC 133 keep using the last-5-lines regex for agent status. The integration layer is purely additive.

**Platform**: Unix only for now. Windows falls through to raw `tty::Pty` — adding OSC support on Windows needs an equivalent `FilterPty` wrapper on the Windows PTY type.

**Testing**: `crates/amux-platform/src/terminal/osc_intercept.rs` has 29 unit tests covering OSC parsing invariants. `crates/amux-platform/tests/osc_smoke.rs` spawns a real `/bin/sh` that emits OSC sequences via `printf` and asserts the events reach `AlacrittyTerminal::take_osc_events()`.

**Third-party** (`third_party/`): Vendored GPUI component library and limux (not actively used).

## Key Patterns

- **Rust 2024 edition**: `gen` is reserved. Use `cur_gen` or similar. `ref mut` in patterns has restrictions.
- **GPUI rendering**: Two-phase model (prepaint/paint). Canvas drawing for terminal cells. Scrollable divs require an `ElementId`. No `overflow_x_scroll` — use `overflow_hidden`.
- **Terminal rendering**: All terminal content is painted on a GPUI canvas (not DOM elements). The glyph cache uses `u64` hash keys for zero-allocation lookup.
- **Theme**: Tomorrow Night is the default. All colors should use the Tomorrow Night palette (gray-tinted, not blue-tinted like Catppuccin). Key colors: bg `0x1d1f21`, fg `0xc5c8c6`, blue `0x81a2be`, red `0xcc6666`, green `0xb5bd68`.
- **Config path**: `~/.amux/` for config.toml, layouts.json, workspaces.
- **WSL support**: Windows builds detect WSL, support per-pane Win/WSL env, auto-convert paths between formats.

## Approach

Behavior rules live in @rules/approach.md.
