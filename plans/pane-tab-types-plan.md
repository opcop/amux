# Pane Tab Types: Browser & Preview as Tabs

## Goal

把浏览器和文件预览从独立的覆盖面板，变成 pane 内的 tab 类型（学习 limux 架构）。每个 pane 可以有终端 tab、浏览器 tab、预览 tab 混合存在。

## Architecture

### Current (overlay panels)
```
┌─────────┬──────────┬─────────────┐
│ Sidebar │ Terminal  │ Browser     │  ← 独立面板，覆盖终端 pane
│         │ Panes    │ (overlay)   │
│         │          │             │
└─────────┴──────────┴─────────────┘
```

### Target (tab types)
```
┌─────────┬──────────────┬──────────────┐
│ Sidebar │ Pane A       │ Pane B       │
│         │ [Term1][🌐]  │ [Term1][📄]  │
│         │ ┌──────────┐ │ ┌──────────┐ │
│         │ │ Browser  │ │ │ Preview  │ │
│         │ │ content  │ │ │ content  │ │
│         │ └──────────┘ │ └──────────┘ │
└─────────┴──────────────┴──────────────┘
```

## Waves

### Wave 1: Data Model (amux-platform)

**Files:** `crates/amux-platform/src/terminal/manager.rs`

1. Add `TabKind` enum:
   ```rust
   pub enum TabKind {
       Terminal,
       Browser { url: String },
       Preview { path: String },
   }
   ```

2. Add `kind` field to `PaneTab`:
   ```rust
   pub struct PaneTab {
       pub title: String,
       pub custom_title: bool,
       pub kind: TabKind,           // NEW
       pub terminal: Option<AlacrittyTerminal>,  // only for TabKind::Terminal
       // ... existing fields
   }
   ```

3. Add methods to `TerminalPane`:
   - `add_browser_tab(url: &str) -> usize`
   - `add_preview_tab(path: &str) -> usize`
   - `active_tab_kind() -> &TabKind`

4. Update persistence (`SavedTab`, `save_layout`, `restore_layout`) to handle new kinds.

### Wave 2: Browser Tab Rendering (apps/desktop)

**Files:** `apps/desktop/src/gpui_entry.rs`, `apps/desktop/src/gpui_browser.rs`, `apps/desktop/src/gpui_layout_renderer.rs`

1. Move `BrowserPaneState` ownership from `GpuiShellView` (single global) to per-tab storage:
   - `GpuiShellView.browser_tabs: HashMap<(PaneId, usize), BrowserPaneState>`
   - Each browser tab has its own WebView2 + URL Input entity

2. Update `gpui_layout_renderer.rs` — when rendering a pane's content area:
   - If active tab is `TabKind::Terminal` → render terminal (existing code)
   - If active tab is `TabKind::Browser` → render browser content (URL bar + WebView2 canvas)
   - If active tab is `TabKind::Preview` → render preview content

3. Update tab bar rendering to show icons per tab type:
   - Terminal: existing terminal icon/title
   - Browser: 🌐 + page title or URL
   - Preview: 📄 + filename

4. Remove the old standalone browser panel code (the `.when(browser_pane.is_some(), ...)` block in render()).

### Wave 3: Input & Focus Adaptation

**Files:** `apps/desktop/src/gpui_input_handler.rs`, `apps/desktop/src/gpui_entry.rs`

1. Update `on_global_key_down`:
   - Check if active tab in active pane is a browser tab → route keys to browser
   - Check if active tab is preview → route keys to preview
   - Otherwise → route to terminal (existing)

2. Update `replace_text_in_range` (IME handler) similarly.

3. Update mouse event handlers:
   - Click within a pane that has browser active tab → handle WebView2 focus
   - `focus_parent()` calls when switching from browser tab to terminal tab

4. Update `Ctrl+Shift+B` shortcut:
   - Opens a new browser tab in the current active pane (instead of toggling global panel)

### Wave 4: Preview Tab Migration

**Files:** `apps/desktop/src/gpui_preview.rs`, `apps/desktop/src/gpui_entry.rs`

1. Move preview state from `GpuiShellView.preview_state` to per-tab storage.
2. `Ctrl+P` → file picker opens preview tab in current pane.
3. Ctrl+Click on terminal path → opens preview tab in current pane.
4. Remove old standalone preview panel.

### Wave 5: Polish & Edge Cases

1. Tab close behavior: closing a browser/preview tab switches to adjacent tab.
2. Persistence: save/restore browser URLs and preview paths across sessions.
3. WebView2 lifecycle: destroy WebView2 when browser tab is closed.
4. WebView2 bounds sync: only sync for the visible (active) browser tab.
5. Multiple browser tabs: handle multiple WebView2 instances correctly.

## Risk Mitigations

- **Focus issues**: Keep the `focus_parent()` pattern that works. Each pane with an active browser tab calls focus_parent on terminal click.
- **WebView2 bounds**: Only the active browser tab in the active pane gets sync_bounds. Hidden browser tabs have WebView2 set_visible(false).
- **Incremental**: Each wave is independently testable. Wave 1-2 can ship without Wave 3-4.

## Success Criteria

- [ ] Browser opens as a tab inside current pane
- [ ] Terminal tabs and browser tabs coexist in same pane
- [ ] Switching tabs between terminal and browser works
- [ ] Closing browser tab doesn't affect terminal tabs
- [ ] Other panes are never covered/affected by browser
- [ ] Preview opens as a tab inside current pane
- [ ] Session persistence saves/restores browser and preview tabs
