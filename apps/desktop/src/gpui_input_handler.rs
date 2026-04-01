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
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self, _range: Option<std::ops::Range<usize>>, text: &str,
        _window: &mut Window, cx: &mut Context<Self>,
    ) {
        if text.is_empty() { return; }

        // If renaming workspace, send text to rename field
        if let Some((_, ref mut rename_text)) = self.renaming_workspace {
            rename_text.push_str(text);
            cx.notify();
            return;
        }
        // If renaming tab, send text to rename field
        if let Some((_, _, ref mut rename_text)) = self.renaming_tab {
            rename_text.push_str(text);
            cx.notify();
            return;
        }
        // If compare setup dialog is open, send text to prompt field
        if let Some(ref mut setup) = self.compare_setup {
            setup.prompt.push_str(text);
            cx.notify();
            return;
        }
        // If searching, append to search query and auto-navigate
        if let Some((ref mut query, _)) = self.search_state {
            query.push_str(text);
            let q = query.clone();
            drop(query);
            self.search_navigate(true);
            cx.notify();
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
        if new_text.is_empty() {
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
        // Position IME candidate window near the terminal cursor
        let metrics = self.cell_metrics.as_ref()?;
        let active_pid = self.terminal_manager().active_pane_id()?.clone();
        let (cursor_col, cursor_row) = self.terminal_manager().active_terminal_ref()
            .map(|t| t.with_term(|term| {
                let c = term.renderable_content().cursor;
                (c.point.column.0, c.point.line.0.max(0) as usize)
            }))?;
        let (origin_x, origin_y, _, _) = self.pane_bounds.get(&active_pid.0)?;
        let x = origin_x + cursor_col as f32 * metrics.width;
        let y = origin_y + cursor_row as f32 * metrics.height;
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
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let ctrl = keystroke.modifiers.control;
        let shift = keystroke.modifiers.shift;
        let alt = keystroke.modifiers.alt;

        let key = &keystroke.key;

        let modifier = if ctrl && shift {
            "ctrl+shift"
        } else if ctrl {
            "ctrl"
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

        // Workspace rename handling
        if let Some((ref ws_id, ref mut text)) = self.renaming_workspace {
            match keystr.as_str() {
                "enter" => {
                    let ws_id = ws_id.clone();
                    let new_name = text.clone();
                    if !new_name.is_empty() {
                        let _ = self.app.rename_workspace(&ws_id, &new_name);
                        self.refresh_model();
                    }
                    self.renaming_workspace = None;
                    cx.notify();
                    return;
                }
                "escape" => {
                    self.renaming_workspace = None;
                    cx.notify();
                    return;
                }
                "backspace" => {
                    text.pop();
                    cx.notify();
                    return;
                }
                _ => {
                    // Character input handled by replace_text_in_range (IME handler)
                    return;
                }
            }
        }

        // Tab rename handling
        if let Some((ref pane_id, tab_idx, ref mut text)) = self.renaming_tab {
            match keystr.as_str() {
                "enter" => {
                    let pid = amux_platform::terminal::manager::PaneId(pane_id.clone());
                    let new_name = text.clone();
                    if !new_name.is_empty() {
                        if let Some(pane) = self.terminal_manager_mut().get_pane_mut(&pid) {
                            if let Some(tab) = pane.tabs.get_mut(tab_idx) {
                                tab.title = new_name;
                                tab.custom_title = true;
                            }
                        }
                    }
                    self.renaming_tab = None;
                    cx.notify();
                    return;
                }
                "escape" => {
                    self.renaming_tab = None;
                    cx.notify();
                    return;
                }
                "backspace" => {
                    text.pop();
                    cx.notify();
                    return;
                }
                _ => return,
            }
        }

        // Terminal search handling
        if let Some((ref mut query, ref mut _match_idx)) = self.search_state {
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
                "backspace" => {
                    query.pop();
                    if !query.is_empty() {
                        // Auto-search on each keystroke
                        let q = query.clone();
                        drop(query);
                        self.search_navigate(true);
                    } else {
                        // Clear selection when query is empty
                        if let Some(term) = self.terminal_manager_mut().active_terminal() {
                            term.with_term_mut(|t| { t.selection = None; });
                        }
                    }
                    cx.notify();
                    return;
                }
                _ => {
                    // Character input handled by IME handler
                    return;
                }
            }
        }

        // Command palette handling
        if self.model.command_palette_open {
            match keystr.as_str() {
                "escape" | "ctrl+p" => {
                    let _ = self.app.dispatch(amux_ui::UiAction::ToggleCommandPalette);
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "enter" => {
                    // Intercept layout commands — they need gpui-layer side effects
                    let cmd = self.app.selected_palette_command_str();
                    if let Some(ref c) = cmd {
                        if c.starts_with("layout template ") {
                            let _ = self.app.dispatch(amux_ui::UiAction::ToggleCommandPalette);
                            self.open_template_picker();
                            self.refresh_model();
                            cx.notify();
                            return;
                        }
                        if c == "layout save-as-template" {
                            let _ = self.app.dispatch(amux_ui::UiAction::ToggleCommandPalette);
                            let ws_name = self.model.active_workspace_name
                                .clone().unwrap_or_else(|| self.active_workspace_id.clone());
                            self.save_current_as_template(&ws_name);
                            self.refresh_model();
                            cx.notify();
                            return;
                        }
                    }
                    let _ = self.app.execute_selected_palette_command();
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "up" | "arrowup" => {
                    self.app.select_previous_palette_item();
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "down" | "arrowdown" => {
                    self.app.select_next_palette_item();
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                _ => return,
            }
        }

        // Compare setup dialog handling
        if self.compare_setup.is_some() {
            match keystr.as_str() {
                "escape" => {
                    self.compare_setup = None;
                }
                "enter" => {
                    self.start_compare_task();
                }
                "backspace" => {
                    if let Some(ref mut setup) = self.compare_setup {
                        setup.prompt.pop();
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
                    if n <= self.agent_picker.as_ref().unwrap().agents.len() {
                        self.agent_picker.as_mut().unwrap().selected_index = n - 1;
                        self.execute_agent_picker();
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
                    if n <= self.template_picker.as_ref().unwrap().templates.len() {
                        self.template_picker.as_mut().unwrap().selected_index = n - 1;
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
                    if n <= self.pane_picker.as_ref().unwrap().targets.len() {
                        self.pane_picker.as_mut().unwrap().selected_index = n - 1;
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
                "ctrl+shift+p" => {
                    let _ = self.app.dispatch(amux_ui::UiAction::ToggleCommandPalette);
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+shift+s" => {
                    // Open terminal search
                    self.search_state = Some((String::new(), 0));
                    cx.notify();
                    return;
                }
                "ctrl+shift+n" => {
                    let _ = self.app.run_command("new workspace");
                    self.refresh_model();
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
                "ctrl+shift+t" => {
                    self.open_compare_setup();
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }

        // Ctrl shortcuts — only intercept keys that don't conflict with shell/readline
        if ctrl && !shift {
            match keystr.as_str() {
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
                    self.handle_terminal_input(key, ctrl, shift, alt);
                    cx.notify();
                    return;
                }
            }
        }

        // Alt+key → forward to PTY (readline word navigation: Alt+B/F/D, Alt+Backspace, etc.)
        if alt && !ctrl {
            self.handle_terminal_input(key, ctrl, shift, alt);
            cx.notify();
            return;
        }

        // Terminal special keys (non-modifier or with any modifier)
        match keystr.as_str() {
            "enter" | "tab" | "backspace" | "escape" => {
                self.handle_terminal_input(key, ctrl, shift, alt);
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
                self.handle_terminal_input(key, ctrl, shift, alt);
                cx.notify();
                return;
            }
            _ => {}
        }

        // Regular character input is handled by EntityInputHandler::replace_text_in_range
        // (both English and Chinese/IME input go through that path to avoid double-sending)
    }
}
