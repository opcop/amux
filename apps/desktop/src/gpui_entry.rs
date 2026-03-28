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
use amux_platform::terminal::emulator::{TerminalEmulator, Cursor};
#[cfg(feature = "gpui")]
use crate::gpui_status_bar::render_status_bar;
#[cfg(feature = "gpui")]
use crate::gpui_workspace_sidebar::WorkspaceSidebarState;


#[cfg(feature = "gpui")]
pub(crate) struct GpuiShellView {
    app: DesktopApp,
    model: GpuiWindowModel,
    sidebar_state: WorkspaceSidebarState,
    // Terminal manager with tabs and panes
    terminal_manager: TerminalManager,
    // Focus handle for keyboard input
    focus_handle: gpui::FocusHandle,
}

#[cfg(feature = "gpui")]
impl GpuiShellView {
    /// Create a new shell view with terminal manager
    pub fn new(app: DesktopApp, model: GpuiWindowModel, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let mut terminal_manager = TerminalManager::new();

        // Detect platform and choose the right shell
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
            // Linux / macOS / WSL — use bash
            (
                amux_core::WorkspaceTarget::WindowsPath {
                    path: std::env::current_dir().unwrap_or_default(),
                },
                amux_core::ShellKind::Cmd, // We'll override the command below
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .ok(),
            )
        };

        // Try to spawn a real PTY session
        if let Some(term) = terminal_manager.active_terminal() {
            let profile = amux_core::TerminalLaunchProfile {
                target,
                shell,
                cwd,
                env: std::collections::BTreeMap::new(),
                title: Some("Terminal".to_string()),
            };

            match term.spawn(profile) {
                Ok(_) => {
                    eprintln!("Terminal spawned successfully");
                }
                Err(e) => {
                    eprintln!("Failed to spawn terminal: {}", e);
                    term.feed(b"\x1b[1;32mWelcome to AMUX Terminal\x1b[0m\r\n");
                    term.feed(b"\x1b[33mNote: Run in a real terminal environment for full functionality.\x1b[0m\r\n\r\n");
                    term.feed(b"$ ");
                }
            }
        }

        Self {
            app,
            model,
            sidebar_state: WorkspaceSidebarState::default(),
            terminal_manager,
            focus_handle,
        }
    }

    /// Handle key input for the terminal
    pub fn handle_terminal_input(&mut self, key: &str, ctrl: bool, shift: bool, alt: bool) {
        use amux_platform::terminal::keys;
        
        let input = keys::to_pty(key, ctrl, shift, alt);
        
        // Feed to active terminal emulator for local rendering
        if let Some(terminal) = self.terminal_manager.active_terminal() {
            terminal.feed(&input);
            
            // Send to PTY if session is active
            if terminal.is_active() {
                let _ = terminal.send_input(&input);
            }
        }
        
        // Request re-render
        self.model = self.app.render_with(&amux_ui::GpuiRenderer);
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

        // Poll PTY output and feed to emulator before rendering
        let had_output = self.terminal_manager.poll_active();

        // Get terminal info from manager
        let terminal_tabs = self.terminal_manager.tab_titles();
        let term_view = self.terminal_manager.active_terminal_ref();
        let emulator: Option<&TerminalEmulator> = term_view.map(|t| t.emulator());
        let cursor: Option<&Cursor> = emulator.map(|e| e.cursor());

        // Debug: log terminal state on first few renders
        {
            static RENDER_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let count = RENDER_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if count < 5 || (had_output && count < 20) {
                let has_em = emulator.is_some();
                let grid_content = emulator.map(|em| {
                    let grid = em.grid();
                    let first_row: String = if !grid.is_empty() {
                        grid[0].iter().map(|c| c.ch).collect::<String>().trim_end().to_string()
                    } else {
                        "(empty grid)".to_string()
                    };
                    let non_empty_rows = grid.iter().filter(|row| row.iter().any(|c| c.ch != ' ' && c.ch != '\0')).count();
                    format!("rows_with_content={}, first_row='{}'", non_empty_rows, &first_row[..first_row.len().min(60)])
                }).unwrap_or_else(|| "no emulator".to_string());
                eprintln!("[render#{}] has_emulator={}, had_output={}, tabs={}, {}", count, has_em, had_output, terminal_tabs.len(), grid_content);
            }
        }
        
        // Main layout - limux/mori style dark theme
        div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x171717))
            .text_color(rgb(0xffffff))
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.on_global_key_down(event, window, cx);
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
                                .w(px(220.0))
                                .bg(rgb(0x191919))
                                .flex_col()
                                .border_r_1()
                                .border_color(rgb(0x333333))
                                .child(
                                    div()
                                        .px_3()
                                        .py_2()
                                        .text_xs()
                                        .text_color(rgb(0x666666))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("WORKSPACES")
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .flex_1()
                                        .overflow_hidden()
                                        // Workspace list
                                        .children(workspaces.iter().map(|item| {
                                            let is_active = item.is_active;
                                            let bg_color = if is_active { rgb(0x2a2a2a) } else { rgb(0x191919) };
                                            let border_color = if is_active { rgb(0x0091ff) } else { rgb(0x333333) };
                                            let text_color = if is_active { rgb(0xffffff) } else { rgb(0xb3b3b3) };
                                            let font_weight = if is_active { FontWeight::SEMIBOLD } else { FontWeight::NORMAL };
                                            let name_label = if is_active { format!("● {}", item.name) } else { item.name.clone() };
                                            
                                            div()
                                                .flex()
                                                .px_3()
                                                .py_2()
                                                .mx_1()
                                                .my_1()
                                                .rounded(px(6.0))
                                                .bg(bg_color)
                                                .border_l_3()
                                                .border_color(border_color)
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(text_color)
                                                        .font_weight(font_weight)
                                                        .child(name_label)
                                                )
                                        }))
                                )
                                // New workspace hint
                                .child(
                                    div()
                                        .px_3()
                                        .py_2()
                                        .mx_1()
                                        .mb_2()
                                        .rounded(px(6.0))
                                        .bg(rgb(0x1a1a1a))
                                        .text_sm()
                                        .text_color(rgb(0xb3b3b3))
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child("+ New Workspace")
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(0x666666))
                                                .child("(Ctrl+Shift+N)")
                                        )
                                )
                                // Collapse hint
                                .child(
                                    div()
                                        .px_3()
                                        .py_2()
                                        .mx_1()
                                        .rounded(px(6.0))
                                        .bg(rgb(0x1a1a1a))
                                        .text_sm()
                                        .text_color(rgb(0xb3b3b3))
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child("← Collapse")
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(0x666666))
                                                .child("(Ctrl+M)")
                                        )
                                )
                        } else {
                            // Collapsed sidebar - thin strip
                            div()
                                .w(px(4.0))
                                .bg(rgb(0x191919))
                                .flex_col()
                        }
                    })
                    // Main content area
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .bg(rgb(0x0f172a))  // Terminal background
                            .overflow_hidden()
                            // Terminal view - the main content
                            .child({
                                if let (Some(em), Some(cur)) = (emulator, cursor) {
                                    crate::gpui_terminal::render_terminal(em, cur).into_any_element()
                                } else {
                                    div().flex_1().child("No terminal").into_any_element()
                                }
                            })
                            // Tab strip at bottom
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .bg(rgb(0x171717))
                                    .border_t_1()
                                    .border_color(rgb(0x333333))
                                    .px_2()
                                    .py_1()
                                    .gap_1()
                                    .children(terminal_tabs.iter().map(|(id, title, is_active)| {
                                        div()
                                            .px_3()
                                            .py_1()
                                            .rounded(px(4.0))
                                            .text_xs()
                                            .text_color(if *is_active { rgb(0xffffff) } else { rgb(0xb3b3b3) })
                                            .bg(if *is_active { rgb(0x262626) } else { rgb(0x1f1f1f) })
                                            .child(title.clone())
                                    }))
                            )
                    ),
            )
            // Status bar
            .child(render_status_bar(&self.model))
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

        match full_keystroke.to_lowercase().as_str() {
            // Command palette
            "escape" | "ctrl+p" => {
                let _ = self.app.dispatch(amux_ui::UiAction::ToggleCommandPalette);
                self.refresh_model();
                cx.notify();
                return;
            }
            "enter" if self.model.command_palette_open => {
                let _ = self.app.execute_selected_palette_command();
                self.refresh_model();
                cx.notify();
                return;
            }
            "up" | "arrowup" if self.model.command_palette_open => {
                self.app.select_previous_palette_item();
                self.refresh_model();
                cx.notify();
                return;
            }
            "down" | "arrowdown" if self.model.command_palette_open => {
                self.app.select_next_palette_item();
                self.refresh_model();
                cx.notify();
                return;
            }
            _ if self.model.command_palette_open => return,

            // Terminal input - send to emulator
            "enter" | "tab" | "backspace" | "escape" => {
                self.handle_terminal_input(key, ctrl, shift, false);
                cx.notify();
                return;
            }
            s if s.starts_with("arrow") || s.starts_with("f1") || s.starts_with("f2") 
                || s.starts_with("f3") || s.starts_with("f4") || s.starts_with("f5")
                || s.starts_with("f6") || s.starts_with("f7") || s.starts_with("f8")
                || s.starts_with("f9") || s.starts_with("f10") || s.starts_with("f11")
                || s.starts_with("f12") || s.starts_with("page") || s.starts_with("home")
                || s.starts_with("end") || s.starts_with("insert") || s.starts_with("delete") => {
                self.handle_terminal_input(key, ctrl, shift, false);
                cx.notify();
                return;
            }
            _ if key.len() == 1 || key == "Space" => {
                // Regular character input
                self.handle_terminal_input(key, ctrl, shift, false);
                cx.notify();
                return;
            }

            // Terminal pane operations
            "ctrl+d" => {
                // Split terminal horizontally (side by side)
                self.terminal_manager.split_active_pane(SplitDirection::Horizontal);
                cx.notify();
                return;
            }
            "ctrl+shift+d" => {
                // Split terminal vertically (top and bottom)
                self.terminal_manager.split_active_pane(SplitDirection::Vertical);
                cx.notify();
                return;
            }
            "ctrl+t" => {
                // New terminal tab
                self.terminal_manager.create_tab_auto();
                cx.notify();
                return;
            }
            "ctrl+w" => {
                // Close current pane (if more than one pane)
                if self.terminal_manager.close_active_pane() {
                    cx.notify();
                }
                return;
            }
            "ctrl+m" => {
                self.sidebar_state.collapsed = !self.sidebar_state.collapsed;
                cx.notify();
                return;
            }

            // Workspace
            "ctrl+shift+n" => {
                let _ = self.app.run_command("new workspace");
                self.refresh_model();
                cx.notify();
            }
            "ctrl+shift+left" | "ctrl+pageup" => {
                let _ = self.app.run_command("switch tab prev");
                self.refresh_model();
                cx.notify();
            }
            "ctrl+shift+right" | "ctrl+pagedown" => {
                let _ = self.app.run_command("switch tab next");
                self.refresh_model();
                cx.notify();
            }
            "ctrl+1" => {
                let _ = self.app.run_command("switch workspace 1");
                self.refresh_model();
                cx.notify();
            }
            "ctrl+2" => {
                let _ = self.app.run_command("switch workspace 2");
                self.refresh_model();
                cx.notify();
            }
            "ctrl+3" => {
                let _ = self.app.run_command("switch workspace 3");
                self.refresh_model();
                cx.notify();
            }
            "ctrl+4" => {
                let _ = self.app.run_command("switch workspace 4");
                self.refresh_model();
                cx.notify();
            }
            "ctrl+5" => {
                let _ = self.app.run_command("switch workspace 5");
                self.refresh_model();
                cx.notify();
            }
            _ => {}
        }
    }
}

#[cfg(feature = "gpui")]
pub fn run(app: &amux_ui::DesktopApp) {
    use amux_ui::GpuiRenderer;
    use smol::Timer;

    let mut app = app.clone();
    let model = app.render_with(&GpuiRenderer);

    application().run(move |cx: &mut App| {
        let model = model.clone();
        let app = app.clone();
        cx.open_window(WindowOptions::default(), |_, cx| {
            cx.new(|cx| {
                // Start a ~60fps polling timer to drain PTY output into the emulator
                cx.spawn(async move |this, cx| {
                    loop {
                        Timer::after(std::time::Duration::from_millis(16)).await;
                        let result = this.update(cx, |this: &mut GpuiShellView, cx: &mut Context<GpuiShellView>| {
                            if this.terminal_manager.poll_active() {
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
        })
        .expect("Failed to open AMUX window");
        cx.activate(true);
    });
}

#[cfg(not(feature = "gpui"))]
pub fn run(_: &amux_ui::DesktopApp) {}
