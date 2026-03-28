#[cfg(feature = "gpui")]
use amux_ui::{DesktopApp, GpuiWindowModel};
#[cfg(feature = "gpui")]
use gpui::{
    rgb, App, AppContext, Context, FontWeight, IntoElement, Render, Window,
    WindowOptions, px, div, prelude::*,
};
#[cfg(feature = "gpui")]
use gpui_platform::application;
#[cfg(feature = "gpui")]
use amux_platform::terminal::manager::{TerminalManager, SplitDirection};
#[cfg(feature = "gpui")]
use crate::gpui_status_bar::{render_status_bar, StatusBarData};
#[cfg(feature = "gpui")]
use crate::gpui_workspace_sidebar::WorkspaceSidebarState;


#[cfg(feature = "gpui")]
pub(crate) struct GpuiShellView {
    app: DesktopApp,
    model: GpuiWindowModel,
    sidebar_state: WorkspaceSidebarState,
    /// Per-workspace terminal managers
    workspace_terminals: std::collections::HashMap<String, TerminalManager>,
    /// Current active workspace ID for terminal lookup
    active_workspace_id: String,
    focus_handle: gpui::FocusHandle,
    /// Mouse drag state for text selection
    selecting: bool,
    /// Context menu state
    context_menu: Option<ContextMenuState>,
    /// Drag state for resizing split panes
    resize_drag: Option<ResizeDragState>,
    /// Cursor blink frame counter (toggled by 60fps timer)
    cursor_blink_frame: u32,
    /// Workspace rename state: (workspace_id, current_text)
    renaming_workspace: Option<(String, String)>,
}

/// Right-click context menu
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
struct ContextMenuState {
    position: gpui::Point<gpui::Pixels>,
}

/// Drag state for resizing split panes
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
struct ResizeDragState {
    /// First pane ID in the left/top child (identifies which split)
    split_first_pane: String,
    /// true = horizontal split (drag left/right), false = vertical (drag up/down)
    is_horizontal: bool,
    /// Mouse position at drag start (x for horizontal, y for vertical)
    start_mouse_pos: f32,
    /// Ratio at drag start
    start_ratio: f32,
    /// Estimated container size in the drag axis (pixels)
    container_length: f32,
}

/// Context menu item definition
#[cfg(feature = "gpui")]
#[derive(Clone)]
struct ContextMenuItem {
    label: &'static str,
    shortcut: Option<&'static str>,
    enabled: bool,
    separator_after: bool,
}

#[cfg(feature = "gpui")]
impl ContextMenuItem {
    fn action(label: &'static str, shortcut: Option<&'static str>, enabled: bool) -> Self {
        Self { label, shortcut, enabled, separator_after: false }
    }
    fn separator(mut self) -> Self {
        self.separator_after = true;
        self
    }
}

#[cfg(feature = "gpui")]
impl GpuiShellView {
    /// Create a new shell view with terminal manager
    pub fn new(app: DesktopApp, model: GpuiWindowModel, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // Get the active workspace ID
        let active_ws_id = model.workspace_items.iter()
            .find(|w| w.is_active)
            .map(|w| w.id.clone())
            .unwrap_or_else(|| "default".to_string());

        // Create terminal manager for the initial workspace
        let mut workspace_terminals = std::collections::HashMap::new();
        let mut tm = TerminalManager::new();
        let _ = tm.spawn_in_active(Self::default_profile());
        workspace_terminals.insert(active_ws_id.clone(), tm);

        Self {
            app,
            model,
            sidebar_state: WorkspaceSidebarState::default(),
            workspace_terminals,
            active_workspace_id: active_ws_id,
            focus_handle,
            selecting: false,
            context_menu: None,
            resize_drag: None,
            cursor_blink_frame: 0,
            renaming_workspace: None,
        }
    }

    /// Get the terminal manager for the active workspace (immutable)
    fn terminal_manager(&self) -> &TerminalManager {
        self.workspace_terminals.get(&self.active_workspace_id)
            .expect("active workspace must have a terminal manager")
    }

    /// Get the terminal manager for the active workspace (mutable)
    fn terminal_manager_mut(&mut self) -> &mut TerminalManager {
        self.workspace_terminals.get_mut(&self.active_workspace_id)
            .expect("active workspace must have a terminal manager")
    }

    /// Ensure a workspace has a terminal manager, creating one if needed
    fn ensure_workspace_terminal(&mut self, workspace_id: &str) {
        if !self.workspace_terminals.contains_key(workspace_id) {
            let mut tm = TerminalManager::new();
            let _ = tm.spawn_in_active(Self::default_profile());
            self.workspace_terminals.insert(workspace_id.to_string(), tm);
        }
    }

    /// Switch the active workspace terminal
    fn switch_workspace_terminal(&mut self, workspace_id: &str) {
        self.ensure_workspace_terminal(workspace_id);
        self.active_workspace_id = workspace_id.to_string();
    }

    /// Copy selected text to clipboard
    fn copy_selection(&self, cx: &mut Context<Self>) {
        if let Some(term) = self.terminal_manager().active_terminal_ref() {
            let em = term.emulator();
            let text = em.selection().get_selected_text(em.grid());
            if !text.is_empty() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
        }
    }

    /// Paste from clipboard into terminal
    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let text = cx.read_from_clipboard()
            .and_then(|item| item.text().map(|s| s.to_string()));
        if let Some(text) = text {
            if let Some(term) = self.terminal_manager_mut().active_terminal() {
                let _ = term.send_input(text.as_bytes());
            }
        }
    }

    /// Convert pixel position to terminal cell coordinates.
    /// Accounts for sidebar width. Uses px() arithmetic.
    fn pixel_to_cell(pos: gpui::Point<gpui::Pixels>, sidebar_width: f32) -> (usize, usize) {
        let sidebar_px = px(sidebar_width);
        let cell_w = px(crate::gpui_terminal::CELL_WIDTH);
        let cell_h = px(crate::gpui_terminal::CELL_HEIGHT);
        // Subtract sidebar, clamp to zero, divide by cell size
        let adj_x = if pos.x > sidebar_px { pos.x - sidebar_px } else { px(0.0) };
        let col = (adj_x / cell_w) as usize;
        let row = (pos.y / cell_h) as usize;
        (col, row)
    }

    /// Build context menu items based on current state
    fn build_context_menu_items(&self) -> Vec<ContextMenuItem> {
        let has_selection = self.terminal_manager().active_terminal_ref()
            .map(|t| !t.emulator().selection().is_empty())
            .unwrap_or(false);

        let mut items = vec![
            ContextMenuItem::action("Copy", Some("Ctrl+C"), has_selection),
            ContextMenuItem::action("Paste", Some("Ctrl+V"), true).separator(),
            ContextMenuItem::action("Split Right", Some("Ctrl+D"), true),
            ContextMenuItem::action("Split Down", Some("Ctrl+Shift+D"), true).separator(),
            ContextMenuItem::action("New Tab", Some("Ctrl+T"), true),
            ContextMenuItem::action("Close Pane", Some("Ctrl+W"), self.terminal_manager().total_panes() > 1).separator(),
            ContextMenuItem::action("Clear", Some("Ctrl+K"), true).separator(),
        ];

        // AI Agent launchers
        for agent in &self.model.agent_items {
            if agent.status == "installed" || agent.supported {
                let label: &'static str = match agent.id.as_str() {
                    "claude" => "Launch Claude",
                    "codex" => "Launch Codex",
                    "opencode" => "Launch OpenCode",
                    "aider" => "Launch Aider",
                    _ => continue,
                };
                items.push(ContextMenuItem::action(label, None, true));
            }
        }

        items
    }

    /// Execute a context menu action by label
    fn execute_context_menu_action(&mut self, label: &str, cx: &mut Context<Self>) {
        match label {
            "Copy" => {
                self.copy_selection(cx);
                if let Some(term) = self.terminal_manager_mut().active_terminal() {
                    term.emulator_mut().selection_mut().clear();
                }
            }
            "Paste" => {
                self.paste_clipboard(cx);
            }
            "Split Right" => {
                self.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
                let _ = self.terminal_manager_mut().spawn_in_active(Self::default_profile());
            }
            "Split Down" => {
                self.terminal_manager_mut().split_active_pane(SplitDirection::Vertical);
                let _ = self.terminal_manager_mut().spawn_in_active(Self::default_profile());
            }
            "New Tab" => {
                self.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                let _ = self.terminal_manager_mut().spawn_in_active(Self::default_profile());
            }
            "Close Pane" => {
                self.terminal_manager_mut().close_active_pane();
            }
            "Clear" => {
                if let Some(term) = self.terminal_manager_mut().active_terminal() {
                    // Send Ctrl+L to PTY — shell clears screen and redraws prompt
                    let _ = term.send_input(&[0x0c]); // 0x0c = Form Feed = Ctrl+L
                    term.clear_scrollback();
                }
            }
            "Launch Claude" => {
                let _ = self.app.run_command("agent claude");
                self.refresh_model();
            }
            "Launch Codex" => {
                let _ = self.app.run_command("agent codex");
                self.refresh_model();
            }
            "Launch OpenCode" => {
                let _ = self.app.run_command("agent opencode");
                self.refresh_model();
            }
            "Launch Aider" => {
                let _ = self.app.run_command("agent aider");
                self.refresh_model();
            }
            _ => {}
        }
        self.context_menu = None;
        cx.notify();
    }

    /// Build a default terminal launch profile for the current platform
    fn default_profile() -> amux_core::TerminalLaunchProfile {
        let (target, shell, cwd) = if cfg!(target_os = "windows") {
            (
                amux_core::WorkspaceTarget::WindowsPath {
                    path: std::env::current_dir().unwrap_or_default(),
                },
                amux_core::ShellKind::PowerShell,
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .ok(),
            )
        } else {
            (
                amux_core::WorkspaceTarget::WindowsPath {
                    path: std::env::current_dir().unwrap_or_default(),
                },
                amux_core::ShellKind::PowerShell, // On Linux, build_pty_command ignores this and uses $SHELL
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .ok(),
            )
        };
        amux_core::TerminalLaunchProfile {
            target,
            shell,
            cwd,
            env: std::collections::BTreeMap::new(),
            title: Some("Terminal".to_string()),
        }
    }

    /// Handle key input for the terminal
    pub fn handle_terminal_input(&mut self, key: &str, ctrl: bool, shift: bool, alt: bool) {
        use amux_platform::terminal::keys;
        
        // GPUI sends lowercase keys but to_pty expects title case
        let normalized_key = match key {
            "enter" => "Enter",
            "tab" => "Tab",
            "escape" => "Escape",
            "backspace" => "Backspace",
            "arrowup" => "ArrowUp",
            "arrowdown" => "ArrowDown",
            "arrowleft" => "ArrowLeft",
            "arrowright" => "ArrowRight",
            "home" => "Home",
            "end" => "End",
            "pageup" => "PageUp",
            "pagedown" => "PageDown",
            "insert" => "Insert",
            "delete" => "Delete",
            "f1" => "F1",
            "f2" => "F2",
            "f3" => "F3",
            "f4" => "F4",
            "f5" => "F5",
            "f6" => "F6",
            "f7" => "F7",
            "f8" => "F8",
            "f9" => "F9",
            "f10" => "F10",
            "f11" => "F11",
            "f12" => "F12",
            "space" => "Space",
            _ => key,
        };
        
        let input = keys::to_pty(normalized_key, ctrl, shift, alt);
        
        // Only send to PTY - PTY will echo back, no local echo needed
        if let Some(terminal) = self.terminal_manager_mut().active_terminal() {
            if terminal.is_active() {
                let _ = terminal.send_input(&input);
            }
        }
        
        // Don't request re-render here - the 60fps polling loop will trigger re-render when PTY output arrives
    }
}

#[cfg(feature = "gpui")]
impl Render for GpuiShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Ensure we have keyboard focus
        if !self.focus_handle.is_focused(window) {
            self.focus_handle.focus(window, cx);
        }

        let sidebar_visible = !self.sidebar_state.collapsed;
        let workspaces = self.model.workspace_items.clone();
        let model_ref = &self.model;

        // Poll ALL workspace terminal managers for PTY output
        let mut had_output = false;
        for tm in self.workspace_terminals.values_mut() {
            had_output |= tm.poll_all();
        }

        // Resize terminals — skip during drag to avoid content loss
        if self.resize_drag.is_none() {
            let sidebar_w = if self.sidebar_state.collapsed { 28.0 } else { 220.0 };
            let vp = window.viewport_size();
            let content_w = vp.width.as_f32() - sidebar_w;
            let status_bar_h = 28.0_f32;
            let content_h = vp.height.as_f32() - status_bar_h;
            self.terminal_manager_mut().resize_all_panes(
                content_w, content_h,
                crate::gpui_terminal::CELL_WIDTH,
                crate::gpui_terminal::CELL_HEIGHT,
            );
        }


        
        // Main layout - limux/mori style dark theme
        div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xffffff))
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.on_global_key_down(event, window, cx);
            }))
            // Mouse: start selection on left button down (also closes context menu)
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                // Cancel workspace rename on click elsewhere
                if this.renaming_workspace.is_some() {
                    this.renaming_workspace = None;
                    cx.notify();
                }
                // Don't close context menu here — it's handled by the overlay dismiss layer
                // Don't start text selection if a resize drag is active
                if this.resize_drag.is_some() {
                    return;
                }
                let sidebar_w = if this.sidebar_state.collapsed { 28.0 } else { 220.0 };
                let (col, row) = Self::pixel_to_cell(event.position, sidebar_w);
                if let Some(term) = this.terminal_manager_mut().active_terminal() {
                    term.emulator_mut().set_selection_start(col, row);
                }
                this.selecting = true;
                cx.notify();
            }))
            // Mouse: extend selection or resize drag
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _window, cx| {
                // Handle split resize drag (don't cx.notify here — 60fps timer handles re-render)
                if let Some(ref drag) = this.resize_drag.clone() {
                    let current_pos = if drag.is_horizontal {
                        event.position.x.as_f32()
                    } else {
                        event.position.y.as_f32()
                    };
                    let delta = current_pos - drag.start_mouse_pos;
                    let new_ratio = (drag.start_ratio + delta / drag.container_length).clamp(0.1, 0.9);
                    let pane_id = amux_platform::terminal::manager::PaneId(drag.split_first_pane.clone());
                    this.terminal_manager_mut().update_split_ratio(&pane_id, new_ratio);
                    return;
                }
                // Handle text selection
                if !this.selecting { return; }
                let sidebar_w = if this.sidebar_state.collapsed { 28.0 } else { 220.0 };
                let (col, row) = Self::pixel_to_cell(event.position, sidebar_w);
                if let Some(term) = this.terminal_manager_mut().active_terminal() {
                    term.emulator_mut().set_selection_end(col, row);
                }
                cx.notify();
            }))
            // Mouse: end selection or resize drag on button up
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _event: &gpui::MouseUpEvent, _window, cx| {
                this.selecting = false;
                this.resize_drag = None;
                cx.notify();
            }))
            // Mouse wheel: scroll terminal history
            .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _window, cx| {
                let lines = match event.delta {
                    gpui::ScrollDelta::Lines(pt) => -pt.y,
                    gpui::ScrollDelta::Pixels(pt) => -pt.y.as_f32() / crate::gpui_terminal::CELL_HEIGHT,
                };
                if let Some(term) = this.terminal_manager_mut().active_terminal() {
                    if lines > 0.0 {
                        term.emulator_mut().scroll_up(lines.ceil() as usize);
                    } else if lines < 0.0 {
                        term.emulator_mut().scroll_down((-lines).ceil() as usize);
                    }
                }
                cx.notify();
            }))
            // Right-click: show context menu
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                this.context_menu = Some(ContextMenuState {
                    position: event.position,
                });
                cx.notify();
            }))
            // Main content
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    // Sidebar
                    .child({
                        if sidebar_visible {
                            div()
                                .id("sidebar-expanded")
                                .w(px(220.0))
                                .bg(rgb(0x181818))
                                .flex()
                                .flex_col()
                                .border_r_1()
                                .border_color(rgb(0x2a2a2a))
                                // Header: title + collapse button
                                .child(
                                    div()
                                        .flex()
                                        .justify_between()
                                        .items_center()
                                        .px_3()
                                        .py_2()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(0x585b70))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child("WORKSPACES"),
                                        )
                                        .child(
                                            div()
                                                .id("sidebar-collapse-btn")
                                                .px(px(5.0))
                                                .py(px(2.0))
                                                .rounded(px(3.0))
                                                .text_xs()
                                                .text_color(rgb(0x585b70))
                                                .hover(|d| d.bg(rgb(0x313244)).text_color(rgb(0xcdd6f4)))
                                                .child("◀")
                                                .on_click(cx.listener(|this, _e, _w, cx| {
                                                    this.sidebar_state.collapsed = true;
                                                    cx.notify();
                                                })),
                                        ),
                                )
                                // Workspace list
                                .child(
                                    div()
                                        .flex_col()
                                        .flex_1()
                                        .overflow_hidden()
                                        .children(workspaces.iter().map(|item| {
                                            let is_active = item.is_active;
                                            let bg_color = if is_active { rgb(0x252530) } else { rgb(0x181818) };
                                            let text_color = if is_active { rgb(0xcdd6f4) } else { rgb(0x7f849c) };
                                            let ws_id = item.id.clone();
                                            let ws_id_dbl = item.id.clone();
                                            let ws_name = item.name.clone();
                                            let is_renaming = self.renaming_workspace.as_ref()
                                                .map(|(id, _)| id == &item.id)
                                                .unwrap_or(false);

                                            div()
                                                .id(gpui::ElementId::Name(format!("ws-{}", item.id).into()))
                                                .flex()
                                                .items_center()
                                                .px_3()
                                                .py(px(6.0))
                                                .mx_1()
                                                .my_px()
                                                .rounded(px(4.0))
                                                .bg(bg_color)
                                                .cursor_pointer()
                                                .hover(|d| d.bg(rgb(0x252530)))
                                                .when(is_active, |d| d.border_l_2().border_color(rgb(0x89b4fa)))
                                                // Click: switch workspace; double-click: rename
                                                .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                                                    if event.click_count() >= 2 {
                                                        // Double click: start inline rename
                                                        this.renaming_workspace = Some((ws_id_dbl.clone(), ws_name.clone()));
                                                        cx.notify();
                                                    } else if this.renaming_workspace.is_none() {
                                                        // Single click: switch workspace
                                                        let _ = this.app.activate_workspace(&ws_id);
                                                        this.switch_workspace_terminal(&ws_id);
                                                        this.model = this.app.render_with(&amux_ui::GpuiRenderer);
                                                        cx.notify();
                                                    }
                                                }))
                                                .child(if is_renaming {
                                                    // Inline rename input
                                                    let rename_text = self.renaming_workspace.as_ref()
                                                        .map(|(_, t)| t.clone())
                                                        .unwrap_or_default();
                                                    div()
                                                        .text_sm()
                                                        .text_color(rgb(0xcdd6f4))
                                                        .px_1()
                                                        .bg(rgb(0x313244))
                                                        .rounded(px(2.0))
                                                        .border_1()
                                                        .border_color(rgb(0x89b4fa))
                                                        .child(if rename_text.is_empty() { "▎".to_string() } else { format!("{}▎", rename_text) })
                                                        .into_any_element()
                                                } else {
                                                    div()
                                                        .text_sm()
                                                        .text_color(text_color)
                                                        .when(is_active, |d| d.font_weight(FontWeight::MEDIUM))
                                                        .child(item.name.clone())
                                                        .into_any_element()
                                                })
                                        })),
                                )
                                // Bottom: + New Workspace
                                .child(
                                    div()
                                        .id("sidebar-new-ws")
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .px_3()
                                        .py_2()
                                        .mx_1()
                                        .mb_1()
                                        .rounded(px(4.0))
                                        .text_xs()
                                        .text_color(rgb(0x585b70))
                                        .cursor_pointer()
                                        .hover(|d| d.bg(rgb(0x252530)).text_color(rgb(0xcdd6f4)))
                                        .child("+  New Workspace")
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            let cwd = std::env::current_dir().unwrap_or_default();
                                            let _ = this.app.dispatch(
                                                amux_ui::UiAction::OpenWindowsWorkspace(cwd)
                                            );
                                            this.model = this.app.render_with(&amux_ui::GpuiRenderer);
                                            // Create terminal for the new workspace and switch to it
                                            if let Some(new_ws) = this.model.workspace_items.iter().find(|w| w.is_active) {
                                                this.switch_workspace_terminal(&new_ws.id.clone());
                                            }
                                            cx.notify();
                                        })),
                                )
                        } else {
                            // Collapsed sidebar: narrow strip with expand button
                            div()
                                .id("sidebar-expand")
                                .w(px(28.0))
                                .bg(rgb(0x181818))
                                .flex()
                                .flex_col()
                                .items_center()
                                .border_r_1()
                                .border_color(rgb(0x2a2a2a))
                                .child(
                                    div()
                                        .id("sidebar-expand-btn")
                                        .mt_2()
                                        .px(px(5.0))
                                        .py(px(4.0))
                                        .rounded(px(3.0))
                                        .text_xs()
                                        .text_color(rgb(0x585b70))
                                        .hover(|d| d.bg(rgb(0x313244)).text_color(rgb(0xcdd6f4)))
                                        .child("▶")
                                        .on_click(cx.listener(|this, _e, _w, cx| {
                                            this.sidebar_state.collapsed = false;
                                            cx.notify();
                                        })),
                                )
                        }
                    })
                    // Main content area
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            // Terminal pane(s) — renders split layout recursively
                            .child({
                                let active_pane_id = self.terminal_manager_mut().active_pane_id().cloned();
                                let sidebar_w = if self.sidebar_state.collapsed { 28.0 } else { 220.0 };
                                let vp = window.viewport_size();
                                let content_w = vp.width.as_f32() - sidebar_w;
                                let status_bar_h = 28.0_f32;
                                let content_h = vp.height.as_f32() - status_bar_h;
                                let cursor_blink_on = (self.cursor_blink_frame / 30) % 2 == 0;
                                if let Some(layout) = self.terminal_manager_mut().active_layout().cloned() {
                                    render_layout(&layout, self.terminal_manager(), active_pane_id.as_ref(), content_w, content_h, cursor_blink_on, cx)
                                } else {
                                    div().flex_1().bg(rgb(0x1e1e2e)).child("No terminal").into_any_element()
                                }
                            })
                    ),
            )
            // Status bar
            .child(render_status_bar(&StatusBarData {
                workspace_name: self.model.active_workspace_name
                    .clone()
                    .unwrap_or_else(|| "No workspace".into()),
                pane_count: self.terminal_manager().total_panes(),
                tab_count: self.terminal_manager().total_tabs(),
                shell_name: if cfg!(target_os = "windows") { "pwsh".into() } else {
                    std::env::var("SHELL").unwrap_or_else(|_| "bash".into())
                        .rsplit('/').next().unwrap_or("bash").to_string()
                },
            }))
            // Context menu: dismiss overlay + menu
            .when_some(self.context_menu.clone(), |this, menu| {
                let items = self.build_context_menu_items();
                let vp = window.viewport_size();
                this
                    // Full-screen transparent overlay to catch clicks outside menu
                    .child(
                        div()
                            .id("context-menu-dismiss")
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.context_menu = None;
                                cx.notify();
                            }))
                    )
                    // The actual menu (rendered on top of the overlay)
                    .child(render_context_menu(menu.position, items, vp.width, vp.height, cx))
            })
    }
}

#[cfg(feature = "gpui")]
impl GpuiShellView {
    fn refresh_model(&mut self) {
        self.model = self.app.render_with(&amux_ui::GpuiRenderer);
    }

    fn on_global_key_down(
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
                    // Type characters into rename field
                    if !ctrl && !alt && key.len() == 1 {
                        text.push_str(key);
                        cx.notify();
                        return;
                    } else if keystr == "space" {
                        text.push(' ');
                        cx.notify();
                        return;
                    }
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

        // Ctrl shortcuts - these need to be checked FIRST before character input
        if ctrl {
            match keystr.as_str() {
                // Copy / Paste
                "ctrl+shift+c" => {
                    self.copy_selection(cx);
                    cx.notify();
                    return;
                }
                "ctrl+shift+v" => {
                    self.paste_clipboard(cx);
                    cx.notify();
                    return;
                }
                "ctrl+c" => {
                    // If there's a selection, copy it; otherwise send Ctrl+C to PTY
                    let has_selection = self.terminal_manager_mut().active_terminal_ref()
                        .map(|t| !t.emulator().selection().is_empty())
                        .unwrap_or(false);
                    if has_selection {
                        self.copy_selection(cx);
                        // Clear selection after copy
                        if let Some(term) = self.terminal_manager_mut().active_terminal() {
                            term.emulator_mut().selection_mut().clear();
                        }
                        cx.notify();
                        return;
                    }
                    // No selection → fall through to send Ctrl+C to PTY
                }
                "ctrl+v" => {
                    self.paste_clipboard(cx);
                    cx.notify();
                    return;
                }
                // Terminal pane operations
                "ctrl+d" => {
                    self.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
                    let _ = self.terminal_manager_mut().spawn_in_active(Self::default_profile());
                    cx.notify();
                    return;
                }
                "ctrl+shift+d" => {
                    self.terminal_manager_mut().split_active_pane(SplitDirection::Vertical);
                    let _ = self.terminal_manager_mut().spawn_in_active(Self::default_profile());
                    cx.notify();
                    return;
                }
                "ctrl+t" => {
                    self.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                    let _ = self.terminal_manager_mut().spawn_in_active(Self::default_profile());
                    cx.notify();
                    return;
                }
                "ctrl+w" => {
                    if self.terminal_manager_mut().close_active_pane() {
                        cx.notify();
                    }
                    return;
                }
                "ctrl+m" => {
                    self.sidebar_state.collapsed = !self.sidebar_state.collapsed;
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
                // Split resize
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
                // Terminal operations
                "ctrl+k" => {
                    if let Some(terminal) = self.terminal_manager_mut().active_terminal() {
                        let _ = terminal.send_input(&[0x0c]); // Ctrl+L to shell
                        terminal.clear_scrollback();
                        cx.notify();
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
                // Workspace
                "ctrl+shift+n" => {
                    let _ = self.app.run_command("new workspace");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+shift+left" | "ctrl+pageup" => {
                    let _ = self.app.run_command("switch tab prev");
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                "ctrl+shift+right" | "ctrl+pagedown" => {
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
                "ctrl+p" => {
                    let _ = self.app.dispatch(amux_ui::UiAction::ToggleCommandPalette);
                    self.refresh_model();
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }

        // Terminal input keys (no Ctrl modifier)
        match keystr.as_str() {
            "enter" | "tab" | "backspace" | "escape" | "space" => {
                self.handle_terminal_input(key, ctrl, shift, alt);
                cx.notify();
                return;
            }
            s if s.starts_with("arrow") || s.starts_with("f1") 
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

        // Regular character input — send to PTY
        if key.len() == 1 {
            self.handle_terminal_input(key, ctrl, shift, alt);
            cx.notify();
            return;
        }
    }
}

/// Render the right-click context menu
#[cfg(feature = "gpui")]
fn render_context_menu(
    pos: gpui::Point<gpui::Pixels>,
    items: Vec<ContextMenuItem>,
    viewport_w: gpui::Pixels,
    viewport_h: gpui::Pixels,
    cx: &mut Context<GpuiShellView>,
) -> impl IntoElement {
    let menu_w = 220.0_f32;
    let menu_h = (items.len() as f32) * 28.0 + 16.0; // approximate menu height

    // Adjust position to keep menu within viewport
    let mut x = pos.x.as_f32();
    let mut y = pos.y.as_f32();
    if x + menu_w > viewport_w.as_f32() {
        x = (viewport_w.as_f32() - menu_w).max(0.0);
    }
    if y + menu_h > viewport_h.as_f32() {
        y = (viewport_h.as_f32() - menu_h).max(0.0);
    }

    let mut menu = div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(menu_w))
        .rounded(px(8.0))
        .bg(rgb(0x1e1e2e))
        .border_1()
        .border_color(rgb(0x313244))
        .shadow_lg()
        .py_1()
        .flex()
        .flex_col();

    for item in items {
        let label = item.label;
        let enabled = item.enabled;

        let text_color = if enabled { rgb(0xcdd6f4) } else { rgb(0x45475a) };

        let row = div()
            .id(gpui::ElementId::Name(label.into()))
            .px_3()
            .py(px(6.0))
            .mx_1()
            .rounded(px(4.0))
            .flex()
            .justify_between()
            .items_center()
            .when(enabled, |d| d.hover(|d| d.bg(rgb(0x313244))))
            .when(enabled, |d| {
                d.on_click(cx.listener(move |this, _event, _window, cx| {
                    this.execute_context_menu_action(label, cx);
                }))
            })
            .child(
                div()
                    .text_sm()
                    .text_color(text_color)
                    .child(label),
            )
            .children(item.shortcut.map(|kb| {
                div()
                    .text_xs()
                    .text_color(rgb(0x585b70))
                    .child(kb)
            }));

        menu = menu.child(row);

        if item.separator_after {
            menu = menu.child(
                div()
                    .mx_2()
                    .my_1()
                    .h(px(1.0))
                    .bg(rgb(0x313244)),
            );
        }
    }

    menu
}

/// Recursively render the tab layout tree (split panes)
/// Get the first pane ID from a layout subtree (for identifying splits)
#[cfg(feature = "gpui")]
fn first_pane_in_layout(layout: &amux_platform::terminal::manager::TabLayout) -> Option<amux_platform::terminal::manager::PaneId> {
    use amux_platform::terminal::manager::TabLayout;
    match layout {
        TabLayout::Single(id) => Some(id.clone()),
        TabLayout::Horizontal { left, .. } => first_pane_in_layout(left),
        TabLayout::Vertical { top, .. } => first_pane_in_layout(top),
    }
}

#[cfg(feature = "gpui")]
fn render_layout(
    layout: &amux_platform::terminal::manager::TabLayout,
    manager: &TerminalManager,
    active_pane_id: Option<&amux_platform::terminal::manager::PaneId>,
    avail_w: f32,
    avail_h: f32,
    cursor_blink_on: bool,
    cx: &mut Context<GpuiShellView>,
) -> gpui::AnyElement {
    use amux_platform::terminal::manager::{PaneId, TabLayout};

    match layout {
        TabLayout::Single(pane_id) => {
            let is_active = active_pane_id == Some(pane_id);

            // Build per-pane tab strip + terminal content
            let (tab_strip, content) = if let Some(pane) = manager.get_pane(pane_id) {
                let tabs = pane.tab_titles();
                let pid_for_tabs = pane_id.clone();
                let has_multiple_panes = manager.total_panes() > 1;

                // Left side: tab buttons
                let tabs_row = div()
                    .flex()
                    .flex_row()
                    .gap_px()
                    .flex_1()
                    .overflow_hidden()
                    .children(tabs.into_iter().map(|(idx, title, is_tab_active)| {
                        let pid_click = pid_for_tabs.clone();
                        div()
                            .id(gpui::ElementId::Name(
                                format!("{}-tab-{}", pid_for_tabs.0, idx).into(),
                            ))
                            .px_3()
                            .py(px(4.0))
                            .text_xs()
                            .text_color(if is_tab_active { rgb(0xcdd6f4) } else { rgb(0x6c7086) })
                            .bg(if is_tab_active { rgb(0x313244) } else { rgb(0x1e1e2e) })
                            .border_b_2()
                            .border_color(if is_tab_active { rgb(0x89b4fa) } else { rgb(0x1e1e2e) })
                            .hover(|d| d.bg(rgb(0x313244)))
                            .child(title)
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.terminal_manager_mut().set_active_pane(&pid_click);
                                this.terminal_manager_mut().set_active_tab_in_pane(idx);
                                cx.notify();
                            }))
                    }));

                // Right side: action buttons
                let pid_new = pane_id.clone();
                let pid_sr = pane_id.clone();
                let pid_sd = pane_id.clone();
                let pid_close = pane_id.clone();

                let actions_row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .px_1()
                    // + New Tab
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("{}-btn-add", pane_id.0).into()))
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(3.0))
                            .text_xs()
                            .text_color(rgb(0x6c7086))
                            .hover(|d| d.bg(rgb(0x313244)).text_color(rgb(0xcdd6f4)))
                            .child("+")
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.terminal_manager_mut().set_active_pane(&pid_new);
                                this.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                                let _ = this.terminal_manager_mut().spawn_in_active(GpuiShellView::default_profile());
                                cx.notify();
                            })),
                    )
                    // Split Right ⬕
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("{}-btn-sr", pane_id.0).into()))
                            .px(px(5.0))
                            .py(px(2.0))
                            .rounded(px(3.0))
                            .text_xs()
                            .text_color(rgb(0x6c7086))
                            .hover(|d| d.bg(rgb(0x313244)).text_color(rgb(0xcdd6f4)))
                            .child("⬕")
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.terminal_manager_mut().set_active_pane(&pid_sr);
                                this.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
                                let _ = this.terminal_manager_mut().spawn_in_active(GpuiShellView::default_profile());
                                cx.notify();
                            })),
                    )
                    // Split Down ⬓
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("{}-btn-sd", pane_id.0).into()))
                            .px(px(5.0))
                            .py(px(2.0))
                            .rounded(px(3.0))
                            .text_xs()
                            .text_color(rgb(0x6c7086))
                            .hover(|d| d.bg(rgb(0x313244)).text_color(rgb(0xcdd6f4)))
                            .child("⬓")
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.terminal_manager_mut().set_active_pane(&pid_sd);
                                this.terminal_manager_mut().split_active_pane(SplitDirection::Vertical);
                                let _ = this.terminal_manager_mut().spawn_in_active(GpuiShellView::default_profile());
                                cx.notify();
                            })),
                    )
                    // Close ✕
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("{}-btn-close", pane_id.0).into()))
                            .px(px(5.0))
                            .py(px(2.0))
                            .rounded(px(3.0))
                            .text_xs()
                            .text_color(if has_multiple_panes { rgb(0x6c7086) } else { rgb(0x313244) })
                            .when(has_multiple_panes, |d| {
                                d.hover(|d| d.bg(rgb(0x45475a)).text_color(rgb(0xf38ba8)))
                            })
                            .child("✕")
                            .when(has_multiple_panes, |d| {
                                d.on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.terminal_manager_mut().set_active_pane(&pid_close);
                                    this.terminal_manager_mut().close_active_pane();
                                    cx.notify();
                                }))
                            }),
                    );

                // Combine into tab strip
                let tab_strip = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .bg(rgb(0x1e1e2e))
                    .border_b_1()
                    .border_color(rgb(0x313244))
                    .child(tabs_row)
                    .child(actions_row)
                    .into_any_element();

                let term = pane.active_terminal_ref();
                let em = term.emulator();
                let cur = term.cursor();
                let content = crate::gpui_terminal::render_terminal(em, cur, cursor_blink_on).into_any_element();
                (tab_strip, content)
            } else {
                (
                    div().into_any_element(),
                    div().flex_1().bg(rgb(0x1e1e2e)).child("Empty pane").into_any_element(),
                )
            };

            let pid = pane_id.clone();
            div()
                .id(gpui::ElementId::Name(pane_id.0.clone().into()))
                .flex_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                .bg(rgb(0x1e1e2e))
                // Active pane: subtle top accent line only
                .when(is_active, |d| d.border_t_2().border_color(rgb(0x89b4fa)))
                .when(!is_active, |d| d.border_t_1().border_color(rgb(0x252530)))
                // Tab strip at top (limux style)
                .child(tab_strip)
                // Terminal content
                .child(content)
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.terminal_manager_mut().set_active_pane(&pid);
                    cx.notify();
                }))
                .into_any_element()
        }
        TabLayout::Horizontal { left, right, ratio } => {
            let r = *ratio;
            let handle_px = 6.0_f32;
            let usable = (avail_w - handle_px).max(0.0);
            let left_w = usable * r;
            let right_w = usable * (1.0 - r);

            let split_id = first_pane_in_layout(right)
                .map(|p| p.0.clone())
                .unwrap_or_default();
            let split_id_clone = split_id.clone();

            let left_div = div()
                .id(gpui::ElementId::Name(format!("split-l-{}", split_id).into()))
                .w(px(left_w))
                .h_full()
                .overflow_hidden()
                .child(render_layout(left, manager, active_pane_id, left_w, avail_h, cursor_blink_on, cx));

            let handle = div()
                .id(gpui::ElementId::Name(format!("resize-h-{}", split_id).into()))
                .w(px(handle_px))
                .flex_shrink_0()
                .cursor_col_resize()
                .child(
                    div()
                        .w(px(1.0))
                        .h_full()
                        .mx_auto()
                        .bg(rgb(0x313244))
                )
                .hover(|d| d.bg(rgb(0x313244)))
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, event: &gpui::MouseDownEvent, _window, _cx| {
                    this.resize_drag = Some(ResizeDragState {
                        split_first_pane: split_id_clone.clone(),
                        is_horizontal: true,
                        start_mouse_pos: event.position.x.as_f32(),
                        start_ratio: r,
                        container_length: usable,
                    });
                }));

            let right_div = div()
                .id(gpui::ElementId::Name(format!("split-r-{}", split_id).into()))
                .w(px(right_w))
                .h_full()
                .overflow_hidden()
                .child(render_layout(right, manager, active_pane_id, right_w, avail_h, cursor_blink_on, cx));

            div()
                .w(px(avail_w))
                .h(px(avail_h))
                .flex()
                .flex_row()
                .overflow_hidden()
                .child(left_div)
                .child(handle)
                .child(right_div)
                .into_any_element()
        }
        TabLayout::Vertical { top, bottom, ratio } => {
            let r = *ratio;
            let handle_px = 6.0_f32;
            let usable = (avail_h - handle_px).max(0.0);
            let top_h = usable * r;
            let bottom_h = usable * (1.0 - r);

            let split_id = first_pane_in_layout(bottom)
                .map(|p| p.0.clone())
                .unwrap_or_default();
            let split_id_clone = split_id.clone();

            let top_div = div()
                .id(gpui::ElementId::Name(format!("split-t-{}", split_id).into()))
                .w_full()
                .h(px(top_h))
                .overflow_hidden()
                .child(render_layout(top, manager, active_pane_id, avail_w, top_h, cursor_blink_on, cx));

            let handle = div()
                .id(gpui::ElementId::Name(format!("resize-v-{}", split_id).into()))
                .h(px(handle_px))
                .flex_shrink_0()
                .cursor_ns_resize()
                .child(
                    div()
                        .h(px(1.0))
                        .w_full()
                        .my_auto()
                        .bg(rgb(0x313244))
                )
                .hover(|d| d.bg(rgb(0x313244)))
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, event: &gpui::MouseDownEvent, _window, _cx| {
                    this.resize_drag = Some(ResizeDragState {
                        split_first_pane: split_id_clone.clone(),
                        is_horizontal: false,
                        start_mouse_pos: event.position.y.as_f32(),
                        start_ratio: r,
                        container_length: usable,
                    });
                }));

            let bottom_div = div()
                .id(gpui::ElementId::Name(format!("split-b-{}", split_id).into()))
                .w_full()
                .h(px(bottom_h))
                .overflow_hidden()
                .child(render_layout(bottom, manager, active_pane_id, avail_w, bottom_h, cursor_blink_on, cx));

            div()
                .w(px(avail_w))
                .h(px(avail_h))
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(top_div)
                .child(handle)
                .child(bottom_div)
                .into_any_element()
        }
    }
}

#[cfg(feature = "gpui")]
pub fn run(app: &amux_ui::DesktopApp) {
    use amux_ui::GpuiRenderer;
    use smol::Timer;

    eprintln!("Starting GPUI application...");
    
    let mut app = app.clone();
    let model = app.render_with(&GpuiRenderer);

    application().run(move |cx: &mut App| {
        eprintln!("GPUI application started, opening window...");
        let model = model.clone();
        let app = app.clone();
        
        let window_opts = WindowOptions {
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("AMUX".into()),
                appears_transparent: false,
                ..Default::default()
            }),
            ..Default::default()
        };
        let window_result = cx.open_window(window_opts, |_, cx| {
            eprintln!("Creating window content...");
            cx.new(|cx| {
                // Start a ~60fps polling timer to drain PTY output into the emulator
                cx.spawn(async move |this, cx| {
                    loop {
                        Timer::after(std::time::Duration::from_millis(16)).await;
                        let result = this.update(cx, |this: &mut GpuiShellView, cx: &mut Context<GpuiShellView>| {
                            let mut has_pty = false;
                            for tm in this.workspace_terminals.values_mut() {
                                has_pty |= tm.poll_all();
                            }
                            let has_drag = this.resize_drag.is_some();
                            // Cursor blink: toggle every ~30 frames (500ms at 60fps)
                            this.cursor_blink_frame = this.cursor_blink_frame.wrapping_add(1);
                            let blink_notify = this.cursor_blink_frame % 30 == 0;
                            if has_pty || has_drag || blink_notify {
                                cx.notify();
                            }
                        });
                        if result.is_err() {
                            break;
                        }
                    }
                })
                .detach();

                GpuiShellView::new(app, model, cx)
            })
        });
        
        match window_result {
            Ok(_) => {
                eprintln!("Window opened successfully!");
                cx.activate(true);
            }
            Err(e) => {
                eprintln!("ERROR: Failed to open window: {:?}", e);
            }
        }
    });
}

#[cfg(not(feature = "gpui"))]
pub fn run(_: &amux_ui::DesktopApp) {}
