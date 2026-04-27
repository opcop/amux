//! Keyboard and IME input handling for the GPUI shell view.
//!
//! This module contains the `EntityInputHandler` implementation (for CJK/IME input)
//! and the `on_global_key_down` handler extracted from `gpui_entry.rs`.

#[cfg(feature = "gpui")]
use gpui::{Context, Window, Bounds, Pixels, UTF16Selection};

#[cfg(feature = "gpui")]
use amux_platform::terminal::manager::SplitDirection;

#[cfg(feature = "gpui")]
use crate::gpui_entry::GpuiShellView;

/// IME input handler — enables Chinese/Japanese/Korean input
#[cfg(feature = "gpui")]
impl gpui::EntityInputHandler for GpuiShellView {
    fn text_for_range(
        &mut self, _range: std::ops::Range<usize>, _adjusted: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window, _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self, _ignore: bool, _window: &mut Window, _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        None // Don't report selection — prevents GPUI from drawing a stray text caret
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<std::ops::Range<usize>> {
        // Return a valid range when IME composition is active so GPUI
        // knows not to dispatch regular key events for the composing
        // keystrokes. Without this, GPUI treats every keystroke as
        // "not in composition" and fires both the IME callback AND
        // the regular key_down event.
        self.ime_preedit.as_ref().map(|text| 0..text.len())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Called when IME composition is canceled (Escape) or when the
        // committed text has been sent via replace_text_in_range and
        // the system wants to clear the marked state. Without clearing
        // ime_preedit here, the preedit overlay persists on screen
        // after the user cancels Chinese/Japanese/Korean input.
        self.ime_preedit = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self, _range: Option<std::ops::Range<usize>>, text: &str,
        _window: &mut Window, cx: &mut Context<Self>,
    ) {
        if text.is_empty() { return; }
        // Start the input-latency stopwatch. Paired with
        // `metrics::consume_input_latency()` at the top of
        // `Render::render`.
        crate::metrics::mark_input();

        // If browser URL Input is focused, don't intercept text
        if let Some((_, entry)) = self.active_browser_entry() {
            use gpui::Focusable;
            if entry.url_input.read(cx).focus_handle(cx).is_focused(_window) {
                return;
            }
        }

        // Rename Input owns its own IME routing; bail if it's focused.
        if let Some((_, ref input)) = self.renaming_workspace {
            use gpui::Focusable;
            if input.read(cx).focus_handle(cx).is_focused(_window) {
                return;
            }
        }
        if let Some((_, _, ref input)) = self.renaming_tab {
            use gpui::Focusable;
            if input.read(cx).focus_handle(cx).is_focused(_window) {
                return;
            }
        }
        // If file picker is open, send text to search query
        if let Some(ref mut picker) = self.file_picker {
            let new_query = format!("{}{}", picker.query, text);
            picker.update_query(&new_query);
            cx.notify();
            return;
        }
        // If searching, append to search query and rebuild matches.
        // Rebuild (not navigate) so the new query's match list is
        // fresh — navigate only cycles within an existing list.
        if let Some(ref mut state) = self.search_state {
            state.query.push_str(text);
            self.search_rebuild();
            cx.notify();
            return;
        }

        // Preview-tab keyboard shortcuts. When the focused pane's
        // active tab is a Preview, plain characters intercept as
        // shortcuts instead of leaking through to the terminal PTY
        // that's hiding behind the preview. Without this guard,
        // typing "Y" to copy the doc would also send "Y" to the
        // background shell.
        if self.active_preview_path().is_some() {
            // TOC overlay input mode: every character appends to the
            // filter query. Arrow keys / Enter / Escape / Backspace
            // are handled in `on_global_key_down` above.
            if self.preview_toc.is_some() {
                self.preview_toc_input(text, cx);
                return;
            }
            // Preview search input mode takes priority: while the
            // user is typing into the `/` search bar, every
            // character appends to the query instead of running a
            // shortcut. Exit via Enter (commit) or Escape (close),
            // both handled in `on_global_key_down`.
            if let Some(state) = self.preview_search.as_ref()
                && state.input_active
            {
                self.preview_search_input(text, cx);
                return;
            }
            match text {
                "/" => {
                    self.preview_search_open(cx);
                    return;
                }
                "o" | ":" => {
                    // Both keys open the same TOC overlay. mdterm
                    // splits them into plain-TOC and fuzzy-heading
                    // with separate UIs; we collapse to one overlay
                    // that always accepts typing. Colon is easier to
                    // reach on non-US layouts via shift+semicolon —
                    // keeping both bindings mirrors muscle memory
                    // from mdterm.
                    self.preview_toc_open(cx);
                    return;
                }
                "[" => {
                    self.preview_jump_heading_prev(cx);
                    return;
                }
                "]" => {
                    self.preview_jump_heading_next(cx);
                    return;
                }
                "n" => {
                    if self.preview_search.is_some() {
                        self.preview_search_next(cx);
                        return;
                    }
                }
                "N" => {
                    if self.preview_search.is_some() {
                        self.preview_search_prev(cx);
                        return;
                    }
                }
                "Y" => {
                    self.preview_copy_full_document(cx);
                    cx.notify();
                    return;
                }
                "c" => {
                    self.preview_copy_first_code_block(cx);
                    cx.notify();
                    return;
                }
                _ => {}
            }
            // Any other text input while preview is active: swallow so
            // typing doesn't dribble into the hidden terminal behind
            // the preview.
            return;
        }

        // Send to terminal PTY and ensure we're viewing the latest output
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            term.scroll_to_bottom();
            term.send_input(text.as_bytes());
        }
        // Clear IME preedit (composition committed)
        self.ime_preedit = None;
        // Reset cursor blink so cursor is visible immediately after typing
        self.cursor_blink_frame = 0;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self, _range: Option<std::ops::Range<usize>>, new_text: &str,
        _selected: Option<std::ops::Range<usize>>, _window: &mut Window, cx: &mut Context<Self>,
    ) {
        // IME composition in progress — show preedit text overlay
        let trimmed = new_text.trim();
        if trimmed.is_empty() {
            self.ime_preedit = None;
        } else {
            self.ime_preedit = Some(new_text.to_string());
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self, _range: std::ops::Range<usize>, _element_bounds: Bounds<Pixels>,
        _window: &mut Window, _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // Always return the terminal cursor's screen position so that:
        //   1. GPUI positions its built-in IME composition box ("方框")
        //      right at the cursor instead of at the hidden 1×1 canvas.
        //   2. macOS positions the candidate/suggestion window (the
        //      floating bar with character choices) near the cursor.
        //
        // Previously this only returned bounds when `ime_preedit` was
        // active, but GPUI queries bounds_for_range BEFORE the first
        // replace_and_mark call, so the first keystroke's candidate
        // window defaulted to the wrong location.
        let metrics = self.cell_metrics.as_ref()?;
        let active_pid = self.terminal_manager().active_pane_id()?.clone();
        let (cursor_col, cursor_row) = self.terminal_manager().active_terminal_ref()
            .map(|t| t.with_term(|term| {
                let c = term.renderable_content().cursor;
                let display_offset = term.grid().display_offset() as i32;
                let viewport_row = (c.point.line.0 + display_offset).max(0) as usize;
                (c.point.column.0, viewport_row)
            }))?;
        let &(origin_x, origin_y, _, _) = self.pane_bounds.get(&active_pid.0)?;
        let pad = crate::gpui_terminal::TERMINAL_LEFT_PADDING;
        let x = origin_x + pad + cursor_col as f32 * metrics.width;
        // Position the candidate window right below the cursor line.
        // The +1 row puts it below the preedit text (same line as cursor),
        // matching macOS Terminal.app where the candidate window floats
        // directly under the composition text.
        // Note: bounds_for_range returns in WINDOW coordinates (GPUI
        // converts them via get_frame in gpui_macos), so no titlebar
        // inset is needed here — pane_bounds are in content coords and
        // GPUI's first_rect_for_character_range adds the frame origin.
        let y = origin_y + (cursor_row + 1) as f32 * metrics.height + 4.0;
        Some(Bounds {
            origin: gpui::point(gpui::px(x), gpui::px(y)),
            size: gpui::size(gpui::px(metrics.width), gpui::px(metrics.height)),
        })
    }

    fn character_index_for_point(
        &mut self, _point: gpui::Point<Pixels>, _window: &mut Window, _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

#[cfg(feature = "gpui")]
impl GpuiShellView {
    pub(crate) fn on_global_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Start the input-latency stopwatch for keys handled here
        // (shortcuts + special keys). Text input from IME composition
        // is timed in `replace_text_in_range` instead.
        crate::metrics::mark_input();
        // IME composition guard: when the user is in the middle of
        // composing a CJK character (Chinese pinyin, Japanese romaji,
        // Korean hangul), do NOT forward keystrokes to the PTY.
        // All input during composition flows through the
        // EntityInputHandler trait (replace_and_mark_text_in_range for
        // preedit updates, replace_text_in_range for commits,
        // unmark_text for cancels). Without this guard, every preedit
        // keystroke ("n", "i", "h", "a", "o") also gets sent to the
        // shell as raw ASCII, producing phantom characters that
        // persist after the IME composition is canceled.
        if self.ime_preedit.is_some() {
            return;
        }

        // If a gpui-component Input has focus, let it handle keys.
        // Only intercept Escape (return focus to terminal).
        if let Some((_, entry)) = self.active_browser_entry() {
            use gpui::Focusable;
            let input_focused = entry.url_input.read(cx).focus_handle(cx).is_focused(window);
            if input_focused {
                if event.keystroke.key == "escape" {
                    self.focus_handle.focus(window, cx);
                    entry.browser.focus_parent();
                    cx.notify();
                }
                return;
            }
        }

        let keystroke = &event.keystroke;
        // Cross-platform "app modifier" normalization.
        //
        // amux's keyboard shortcuts (Open Workspace, split, copy, paste,
        // command palette, ...) are written as `ctrl+shift+X` historically
        // because the project started Windows-first. On macOS the platform
        // convention is Cmd (modifiers.platform), and Ctrl is reserved for
        // shell control characters (Ctrl+C interrupts, Ctrl+D EOF, etc).
        //
        // Rather than rewriting every match arm with a per-platform
        // string, we normalize at the source: on macOS, treat the
        // platform (Cmd) modifier as if it were Control, AND drop the
        // real Ctrl from the modifier string entirely so that real
        // Ctrl+letter keystrokes fall through to the PTY as the user
        // expects (Ctrl+C must reach the shell on macOS too). On
        // Windows / Linux nothing changes — the platform key is still
        // treated as a separate modifier.
        let shift = keystroke.modifiers.shift;
        let alt = keystroke.modifiers.alt;
        #[cfg(target_os = "macos")]
        let ctrl = keystroke.modifiers.platform;
        #[cfg(not(target_os = "macos"))]
        let ctrl = keystroke.modifiers.control;

        let key = &keystroke.key;

        let modifier = if ctrl && shift {
            "ctrl+shift"
        } else if ctrl {
            "ctrl"
        } else if shift && alt {
            "shift+alt"
        } else {
            ""
        };

        let full_keystroke = if modifier.is_empty() {
            key.clone()
        } else {
            format!("{}+{}", modifier, key)
        };

        let keystr = full_keystroke.to_lowercase();

        // Close context menu on any key
        if self.context_menu.is_some() {
            self.context_menu = None;
            cx.notify();
            if keystr == "escape" {
                return;
            }
        }

        // F12: toggle Web Inspector for the active browser pane.
        //
        // Pressing F12 a second time closes the inspector instead of
        // racing another open call against the existing window — this
        // matches Chrome / Safari / Firefox behaviour. The inspector
        // window itself is managed by WebKit (separate NSWindow on
        // macOS, separate WebView2 dev tools window on Windows); see
        // BrowserPaneState::toggle_devtools for the size/dock caveats.
        if keystr == "f12" {
            if let Some((_, entry)) = self.active_browser_entry() {
                entry.browser.toggle_devtools();
                cx.notify();
                return;
            }
        }


        // (Legacy standalone preview_state removed — preview is now tab-based)

        // Swallow keys so rename doesn't leak to the terminal.
        // Enter / Escape handled via the Input's `InputEvent`
        // subscription, not here.
        if let Some((_, ref input)) = self.renaming_workspace {
            use gpui::Focusable;
            if input.read(cx).focus_handle(cx).is_focused(window) {
                return;
            }
        }
        if let Some((_, _, ref input)) = self.renaming_tab {
            use gpui::Focusable;
            if input.read(cx).focus_handle(cx).is_focused(window) {
                return;
            }
        }

        // Preview TOC overlay — highest priority among preview-
        // scoped modals because the overlay paints on top of
        // everything inside the preview panel. Handled before
        // preview_search so keys routed at the TOC hit it first.
        if self.preview_toc.is_some() {
            match keystr.as_str() {
                "escape" => {
                    self.preview_toc_close(cx);
                    return;
                }
                "enter" => {
                    self.preview_toc_commit(cx);
                    return;
                }
                "up" | "arrowup" => {
                    self.preview_toc_prev(cx);
                    return;
                }
                "down" | "arrowdown" => {
                    self.preview_toc_next(cx);
                    return;
                }
                "backspace" => {
                    self.preview_toc_backspace(cx);
                    return;
                }
                _ => {}
            }
        }

        // Preview text-selection escape: when the active tab is a
        // preview, no other modal is open, and a selection exists,
        // Escape clears it. Mirrors the Finder/editor convention
        // where ESC dismisses a selection without taking other
        // action. Placed after the TOC guard (which handles its own
        // Escape) but before preview_search — selection has lower
        // priority than the search bar because the search UI is
        // visually the foreground element when both exist.
        if keystr == "escape"
            && self.preview_toc.is_none()
            && self.preview_search.is_none()
            && self.active_preview_path().is_some()
            && self
                .preview_selection
                .as_ref()
                .is_some_and(|s| s.has_nonempty_selection())
        {
            self.preview_selection_clear(cx);
            return;
        }

        // Preview search handling — highest priority among modal
        // states because its input bar is visually on top of the
        // preview panel. Handled before file_picker / terminal search
        // so Enter/Escape hit the right state machine.
        if self.preview_search.is_some() {
            match keystr.as_str() {
                "escape" => {
                    self.preview_search_close(cx);
                    return;
                }
                "enter" => {
                    // In input mode: commit query (Enter = "search
                    // for this"). After commit: Enter acts like `n`
                    // to advance to the next match.
                    let input_active = self
                        .preview_search
                        .as_ref()
                        .map(|s| s.input_active)
                        .unwrap_or(false);
                    if input_active {
                        self.preview_search_commit(cx);
                    } else {
                        self.preview_search_next(cx);
                    }
                    return;
                }
                "backspace" => {
                    if self
                        .preview_search
                        .as_ref()
                        .is_some_and(|s| s.input_active)
                    {
                        self.preview_search_backspace(cx);
                    }
                    return;
                }
                _ => {}
            }
        }

        // Terminal search handling
        if self.search_state.is_some() {
            match keystr.as_str() {
                "escape" | "ctrl+f" => {
                    // Clear selection and close search
                    if let Some(term) = self.terminal_manager_mut().active_terminal() {
                        term.with_term_mut(|t| { t.selection = None; });
                    }
                    self.search_state = None;
                    cx.notify();
                    return;
                }
                "enter" => {
                    self.search_navigate(true);
                    cx.notify();
                    return;
                }
                "shift+enter" => {
                    self.search_navigate(false);
                    cx.notify();
                    return;
                }
                "tab" => {
                    // Cycle Literal → Regex → Fuzzy → Literal and
                    // rebuild against the current query.
                    if let Some(state) = self.search_state.as_mut() {
                        state.mode = state.mode.cycle();
                    }
                    self.search_rebuild();
                    cx.notify();
                    return;
                }
                "backspace" => {
                    if let Some(state) = self.search_state.as_mut() {
                        state.query.pop();
                    }
                    self.search_rebuild();
                    cx.notify();
                    return;
                }
                _ => {
                    // Character input handled by IME handler
                    return;
                }
            }
        }

        // NOTE: the command palette keystroke gate used to live here,
        // routing Enter / Backspace / arrows to the palette state
        // machine in `amux_ui` when `command_palette_open` was true.
        // Removed along with the Cmd+Shift+P handler because the
        // palette UI has never been mounted in the render tree
        // (see the comment on the removed shortcut above). Keeping
        // the gate without a visible palette turned the terminal
        // into an invisible trap — keystrokes got captured and
        // nothing rendered. When the palette is properly wired in a
        // future change, restore both the shortcut and this gate
        // together.

        // File picker handling (Ctrl+P)
        if self.file_picker.is_some() {
            match keystr.as_str() {
                "escape" => {
                    self.file_picker = None;
                }
                "enter" => {
                    let idx = self.file_picker.as_ref().map(|p| p.selected_index).unwrap_or(0);
                    crate::preview_open::open_preview_from_picker(self, cx, idx);
                }
                "up" | "arrowup" => {
                    if let Some(ref mut p) = self.file_picker {
                        if p.selected_index > 0 { p.selected_index -= 1; }
                    }
                }
                "down" | "arrowdown" => {
                    if let Some(ref mut p) = self.file_picker {
                        if p.selected_index + 1 < p.matches.len() { p.selected_index += 1; }
                    }
                }
                "backspace" => {
                    if let Some(ref mut p) = self.file_picker {
                        let mut q = p.query.clone();
                        q.pop();
                        p.update_query(&q);
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        // Agent picker handling (Launch Agent)
        if self.agent_picker.is_some() {
            match keystr.as_str() {
                "escape" => {
                    self.agent_picker = None;
                }
                "enter" => {
                    self.execute_agent_picker();
                }
                "up" | "arrowup" => {
                    if let Some(ref mut p) = self.agent_picker {
                        if p.selected_index > 0 { p.selected_index -= 1; }
                    }
                }
                "down" | "arrowdown" => {
                    if let Some(ref mut p) = self.agent_picker {
                        if p.selected_index + 1 < p.agents.len() { p.selected_index += 1; }
                    }
                }
                k if k.len() == 1 && k.as_bytes()[0] >= b'1' && k.as_bytes()[0] <= b'9' => {
                    let n = (k.as_bytes()[0] - b'0') as usize;
                    let len = self.agent_picker.as_ref().map(|p| p.agents.len()).unwrap_or(0);
                    if n >= 1 && n <= len {
                        if let Some(ref mut picker) = self.agent_picker {
                            picker.selected_index = n - 1;
                        }
                        self.execute_agent_picker();
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        // New-tab picker handling (+▾ dropdown)
        if self.new_tab_picker.is_some() {
            match keystr.as_str() {
                "escape" => {
                    self.new_tab_picker = None;
                }
                "enter" => {
                    self.execute_new_tab_picker(window, cx);
                }
                "up" | "arrowup" => {
                    if let Some(ref mut p) = self.new_tab_picker {
                        if p.selected_index > 0 { p.selected_index -= 1; }
                    }
                }
                "down" | "arrowdown" => {
                    if let Some(ref mut p) = self.new_tab_picker {
                        if p.selected_index + 1 < p.items.len() { p.selected_index += 1; }
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        // Template picker handling (Apply Layout)
        if self.template_picker.is_some() {
            match keystr.as_str() {
                "escape" => {
                    self.template_picker = None;
                }
                "enter" => {
                    self.execute_template_picker();
                }
                "delete" | "backspace" => {
                    self.delete_selected_template();
                }
                "up" | "arrowup" => {
                    if let Some(ref mut p) = self.template_picker {
                        if p.selected_index > 0 { p.selected_index -= 1; }
                    }
                }
                "down" | "arrowdown" => {
                    if let Some(ref mut p) = self.template_picker {
                        if p.selected_index + 1 < p.templates.len() { p.selected_index += 1; }
                    }
                }
                k if k.len() == 1 && k.as_bytes()[0] >= b'1' && k.as_bytes()[0] <= b'9' => {
                    let n = (k.as_bytes()[0] - b'0') as usize;
                    let len = self.template_picker.as_ref().map(|p| p.templates.len()).unwrap_or(0);
                    if n >= 1 && n <= len {
                        if let Some(ref mut picker) = self.template_picker {
                            picker.selected_index = n - 1;
                        }
                        self.execute_template_picker();
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        // Pane picker handling (Send to Pane)
        if self.pane_picker.is_some() {
            match keystr.as_str() {
                "escape" => {
                    self.pane_picker = None;
                }
                "enter" => {
                    self.execute_pane_picker();
                }
                "up" | "arrowup" => {
                    if let Some(ref mut p) = self.pane_picker {
                        if p.selected_index > 0 { p.selected_index -= 1; }
                    }
                }
                "down" | "arrowdown" => {
                    if let Some(ref mut p) = self.pane_picker {
                        if p.selected_index + 1 < p.targets.len() { p.selected_index += 1; }
                    }
                }
                k if k.len() == 1 && k.as_bytes()[0] >= b'1' && k.as_bytes()[0] <= b'9' => {
                    let n = (k.as_bytes()[0] - b'0') as usize;
                    let len = self.pane_picker.as_ref().map(|p| p.targets.len()).unwrap_or(0);
                    if n >= 1 && n <= len {
                        if let Some(ref mut picker) = self.pane_picker {
                            picker.selected_index = n - 1;
                        }
                        self.execute_pane_picker();
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        // Ctrl+Shift shortcuts — UI operations that don't conflict with shell readline
        if ctrl && shift {
            match keystr.as_str() {
                "ctrl+shift+c" => {
                    self.copy_selection(cx);
                    cx.notify();
                    return;
                }
                "ctrl+shift+v" => {
                    self.smart_paste(cx);
                    cx.notify();
                    return;
                }
                "ctrl+shift+\\" => {
                    let env = self.capture_active_env();
                    self.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
                    self.spawn_with_captured_env(&env);
                    cx.notify();
                    return;
                }
                "ctrl+shift+d" => {
                    let env = self.capture_active_env();
                    self.terminal_manager_mut().split_active_pane(SplitDirection::Vertical);
                    self.spawn_with_captured_env(&env);
                    cx.notify();
                    return;
                }
                "ctrl+shift+t" => {
                    let env = self.capture_active_env();
                    self.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                    self.spawn_with_captured_env(&env);
                    cx.notify();
                    return;
                }
                "ctrl+shift+w" => {
                    self.cleanup_pane_tab_entries();
                    if self.terminal_manager_mut().close_active_pane() {
                        cx.notify();
                    }
                    return;
                }
                "ctrl+shift+f" => {
                    self.toggle_zoom();
                    cx.notify();
                    return;
                }
                "ctrl+shift+e" => {
                    self.terminal_manager_mut().equalize_splits();
                    cx.notify();
                    return;
                }
                "ctrl+shift+m" => {
                    self.sidebar_state.collapsed = !self.sidebar_state.collapsed;
                    cx.notify();
                    return;
                }
                "ctrl+shift+a" => {
                    // Toggle sidebar mode between Workspaces and Agents
                    use crate::gpui_workspace_sidebar::SidebarMode;
                    self.sidebar_state.mode = match self.sidebar_state.mode {
                        SidebarMode::Workspaces => SidebarMode::Agents,
                        SidebarMode::Agents => SidebarMode::Workspaces,
                    };
                    // Ensure sidebar is visible when toggling mode
                    if self.sidebar_state.collapsed {
                        self.sidebar_state.collapsed = false;
                    }
                    cx.notify();
                    return;
                }
                "ctrl+shift+b" => {
                    // Open a new browser tab in the active pane
                    if !self.model.browser_supported {
                        cx.notify();
                        return;
                    }
                    self.open_browser("", window, cx);
                    cx.notify();
                    return;
                }
                // NOTE: "ctrl+shift+p" (Cmd+Shift+P on macOS) used to
                // toggle `command_palette_open` to open the VSCode-
                // style command palette, but the palette UI was never
                // mounted in the render tree — the `render_command_
                // palette` function in gpui_command_palette.rs has
                // been dead code since the first commit. Flipping the
                // flag without a visible UI turned the terminal into
                // an invisible trap: Enter / Backspace / arrows got
                // captured by the input handler's palette gate (also
                // removed in this fix) and never reached the PTY.
                // Handler removed until the palette is actually wired.
                // Apply Layout / Open Workspace — the most common
                // palette destinations — are now in the right-click
                // menu instead.
                "ctrl+shift+s" => {
                    // Open terminal search
                    self.search_state = Some(crate::gpui_entry::SearchState::new());
                    cx.notify();
                    return;
                }
                "ctrl+shift+n" => {
                    self.prompt_open_local_workspace(cx);
                    cx.notify();
                    return;
                }
                "ctrl+shift+left" => {
                    let _ = self.app.run_command("pane resize-left");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+shift+right" => {
                    let _ = self.app.run_command("pane resize-right");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+shift+enter" => {
                    self.start_send_to_pane(cx);
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }

        // Ctrl shortcuts — only intercept keys that don't conflict with shell/readline
        if ctrl && !shift {
            match keystr.as_str() {
                "ctrl+c" => {
                    // Preview text selection takes precedence over the
                    // terminal copy path. If the focused pane's active
                    // tab is a Preview AND the last paint recorded any
                    // byte ranges, copy those to the clipboard and
                    // skip both the terminal selection path and the
                    // SIGINT forward. Empty preview selection falls
                    // through to the terminal's existing logic so the
                    // user doesn't lose Ctrl+C = SIGINT when the
                    // preview tab is focused but no text is selected.
                    if self.active_preview_path().is_some()
                        && !self.preview_selection_ranges.borrow().is_empty()
                    {
                        self.copy_preview_selection(cx);
                        cx.notify();
                        return;
                    }
                    // If there's a non-empty text selection, copy it (like modern terminals).
                    // Otherwise forward to PTY as SIGINT (readline interrupt).
                    // Must check `selection_to_string()` not just `is_some()` —
                    // clicking the terminal creates a zero-length Selection which
                    // would otherwise block Ctrl+C from reaching the shell.
                    let has_selection = self
                        .terminal_manager()
                        .active_terminal_ref()
                        .map(|t| t.with_term(|term| {
                            term.selection_to_string().map_or(false, |s| !s.is_empty())
                        }))
                        .unwrap_or(false);
                    if has_selection {
                        self.copy_selection(cx);
                    } else {
                        self.handle_terminal_input(key, ctrl, shift, alt, window, cx);
                    }
                    cx.notify();
                    return;
                }
                "ctrl+v" => {
                    self.paste_clipboard(cx);
                    cx.notify();
                    return;
                }
                // Pane navigation
                "ctrl+left" => {
                    let _ = self.app.run_command("switch pane prev");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+right" => {
                    let _ = self.app.run_command("switch pane next");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                // Navigation history (browser-like back/forward)
                "ctrl+-" => {
                    {
                        let ws_id = self.active_workspace_id.clone();
                        if let Some(tm) = self.workspace_terminals.get_mut(&ws_id) {
                            if tm.nav_back() {
                                cx.notify();
                            }
                        }
                    }
                    return;
                }
                "ctrl+=" => {
                    {
                        let ws_id = self.active_workspace_id.clone();
                        if let Some(tm) = self.workspace_terminals.get_mut(&ws_id) {
                            if tm.nav_forward() {
                                cx.notify();
                            }
                        }
                    }
                    return;
                }
                "ctrl+q" => {
                    cx.quit();
                    return;
                }
                // Font size
                "ctrl+=" | "ctrl++" => {
                    let _ = self.app.run_command("font increase");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+-" => {
                    let _ = self.app.run_command("font decrease");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+0" => {
                    let _ = self.app.run_command("font reset");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                // macOS-standard shortcuts (Cmd+K/T/W/N on macOS,
                // Ctrl+K/T/W/N on Windows/Linux). These match the
                // muscle memory from Terminal.app / iTerm2.
                "ctrl+k" => {
                    // Clear scrollback + visible screen (Cmd+K on macOS)
                    if let Some(term) = self.terminal_manager_mut().active_terminal() {
                        term.with_term_mut(|t| {
                            use alacritty_terminal::vte::ansi::Handler;
                            t.clear_screen(alacritty_terminal::vte::ansi::ClearMode::Saved);
                            t.clear_screen(alacritty_terminal::vte::ansi::ClearMode::All);
                        });
                    }
                    cx.notify();
                    return;
                }
                "ctrl+t" => {
                    // New tab (Cmd+T on macOS)
                    let env = self.capture_active_env();
                    self.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                    self.spawn_with_captured_env(&env);
                    cx.notify();
                    return;
                }
                "ctrl+w" => {
                    // Forward to PTY for readline delete-previous-word.
                    // bash/zsh/fish all use Ctrl+W to delete the previous word.
                    // Closing panes/tabs is handled exclusively by Ctrl+Shift+W
                    // above, which doesn't conflict with any shell readline.
                    self.handle_terminal_input(key, ctrl, shift, alt, window, cx);
                    cx.notify();
                    return;
                }
                "ctrl+n" => {
                    // Open workspace (Cmd+N on macOS)
                    self.prompt_open_local_workspace(cx);
                    cx.notify();
                    return;
                }
                // Tab/workspace switching
                "ctrl+pageup" => {
                    let _ = self.app.run_command("switch tab prev");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+pagedown" => {
                    let _ = self.app.run_command("switch tab next");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+1" => {
                    let _ = self.app.run_command("switch workspace 1");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+2" => {
                    let _ = self.app.run_command("switch workspace 2");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+3" => {
                    let _ = self.app.run_command("switch workspace 3");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+4" => {
                    let _ = self.app.run_command("switch workspace 4");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+5" => {
                    let _ = self.app.run_command("switch workspace 5");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                // All other Ctrl+key → forward to PTY (readline: Ctrl+A/E/B/F/D/U/K/W/P/N/etc.)
                _ => {
                    self.handle_terminal_input(key, ctrl, shift, alt, window, cx);
                    cx.notify();
                    return;
                }
            }
        }

        // Alt+key → forward to PTY (readline word navigation: Alt+B/F/D, Alt+Backspace, etc.)
        if alt && !ctrl {
            self.handle_terminal_input(key, ctrl, shift, alt, window, cx);
            cx.notify();
            return;
        }

        // On macOS, real Ctrl (not Cmd) must be forwarded to the PTY with
        // the ctrl flag so that Ctrl+C → \x03 (SIGINT), Ctrl+D → \x04 (EOF),
        // etc. The normalization above maps Cmd → ctrl for app shortcuts,
        // but real Ctrl was being dropped entirely — causing Ctrl+C to be
        // sent as a plain "c" character instead of the control byte.
        #[cfg(target_os = "macos")]
        if keystroke.modifiers.control && !keystroke.modifiers.platform {
            self.handle_terminal_input(key, true, shift, alt, window, cx);
            cx.notify();
            return;
        }

        // Terminal special keys (non-modifier or with any modifier)
        match keystr.as_str() {
            "enter" | "tab" | "backspace" | "escape" => {
                self.handle_terminal_input(key, ctrl, shift, alt, window, cx);
                cx.notify();
                return;
            }
            s if s == "up" || s == "down" || s == "left" || s == "right"
                || s.starts_with("arrow") || s.starts_with("f1")
                || s.starts_with("f2") || s.starts_with("f3") || s.starts_with("f4")
                || s.starts_with("f5") || s.starts_with("f6") || s.starts_with("f7")
                || s.starts_with("f8") || s.starts_with("f9") || s.starts_with("f10")
                || s.starts_with("f11") || s.starts_with("f12") || s.starts_with("page")
                || s.starts_with("home") || s.starts_with("end") || s.starts_with("insert")
                || s.starts_with("delete") => {
                self.handle_terminal_input(key, ctrl, shift, alt, window, cx);
                cx.notify();
                return;
            }
            _ => {}
        }

        // Regular character input is handled by EntityInputHandler::replace_text_in_range
        // (both English and Chinese/IME input go through that path to avoid double-sending)
    }
}
