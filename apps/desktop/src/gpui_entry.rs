#[cfg(feature = "gpui")]
use amux_ui::{DesktopApp, GpuiWindowModel};
#[cfg(feature = "gpui")]
use gpui::{
    rgb, App, AppContext, Context, FontWeight, IntoElement, Render, Window,
    WindowOptions, px, div, prelude::*, Bounds, Pixels, UTF16Selection,
};
#[cfg(feature = "gpui")]
use gpui_platform::application;
#[cfg(feature = "gpui")]
use amux_platform::terminal::manager::{TerminalManager, SplitDirection};
#[cfg(feature = "gpui")]
use crate::gpui_status_bar::{render_status_bar, StatusBarData, AgentSummary};
#[cfg(feature = "gpui")]
use crate::gpui_workspace_sidebar::WorkspaceSidebarState;
#[cfg(feature = "gpui")]
use crate::gpui_layout_renderer::{render_context_menu, render_layout, render_pane_picker, render_template_picker, render_agent_picker};


#[cfg(feature = "gpui")]
const SIDEBAR_WIDTH_COLLAPSED: f32 = 28.0;
const SIDEBAR_WIDTH_MIN: f32 = 120.0;
const SIDEBAR_WIDTH_MAX: f32 = 480.0;

#[cfg(feature = "gpui")]
pub(crate) struct GpuiShellView {
    pub(crate) app: DesktopApp,
    pub(crate) model: GpuiWindowModel,
    pub(crate) sidebar_state: WorkspaceSidebarState,
    pub(crate) workspace_terminals: std::collections::HashMap<String, TerminalManager>,
    pub(crate) active_workspace_id: String,
    pub(crate) focus_handle: gpui::FocusHandle,
    pub(crate) cell_metrics: Option<crate::gpui_terminal::CellMetrics>,
    pub(crate) selecting: bool,
    pub(crate) context_menu: Option<ContextMenuState>,
    pub(crate) resize_drag: Option<ResizeDragState>,
    pub(crate) cursor_blink_frame: u32,
    pub(crate) renaming_workspace: Option<(String, String)>,
    pub(crate) renaming_tab: Option<(String, usize, String)>,
    pub(crate) search_state: Option<(String, usize)>,
    pub(crate) detected_vibe_tools: Vec<(&'static str, &'static str, &'static str)>,
    pub(crate) wsl_detected: bool,
    pub(crate) terminals_spawned: bool,
    pub(crate) tools_detected: bool,
    pub(crate) zoomed_pane: Option<amux_platform::terminal::manager::PaneId>,
    pub(crate) workspace_order: Vec<String>,
    pub(crate) pane_bounds: std::collections::HashMap<String, (f32, f32, f32, f32)>,
    pub(crate) config: crate::gpui_config::AmuxConfig,
    pub(crate) terminal_theme: crate::gpui_terminal::TerminalTheme,
    /// Toast notifications for agent status changes.
    pub(crate) toasts: Vec<ToastNotification>,
    /// Pane picker for "Send to Pane" (Ctrl+Shift+Enter)
    pub(crate) pane_picker: Option<PanePickerState>,
    /// Template picker for "Apply Layout..."
    pub(crate) template_picker: Option<TemplatePickerState>,
    /// Agent launcher picker
    pub(crate) agent_picker: Option<AgentPickerState>,
    /// IME preedit text (composition in progress)
    pub(crate) ime_preedit: Option<String>,
    /// Sidebar resize drag: (start_mouse_x, start_width)
    pub(crate) sidebar_drag_start: Option<(f32, f32)>,
    /// File preview panel (legacy standalone — kept for backward compat, will be removed)
    pub(crate) preview_state: Option<crate::gpui_preview::PreviewState>,
    /// Preview tab states keyed by file path
    pub(crate) preview_tabs: std::collections::HashMap<String, crate::gpui_preview::PreviewState>,
    /// File picker (Ctrl+P)
    pub(crate) file_picker: Option<crate::gpui_preview::FilePickerState>,
    /// Preview panel resize drag: (start_mouse_x, start_width)
    pub(crate) preview_drag_start: Option<(f32, f32)>,
    /// Browser panel resize drag: (start_mouse_x, start_width)
    pub(crate) browser_drag_start: Option<(f32, f32)>,
    /// Browser tab states keyed by browser_id (each browser tab has its own WebView2)
    pub(crate) browser_tabs: std::collections::HashMap<u64, crate::gpui_browser::BrowserTabEntry>,
    /// Next browser_id to assign
    pub(crate) next_browser_id: u64,
    /// Flag: restore terminal focus on next render (set after URL input Enter)
    pub(crate) restore_terminal_focus: bool,
    /// Flag: focus the browser URL Input on next render (deferred to avoid track_focus race)
    pub(crate) pending_url_input_focus: bool,
    /// Pending URL to sync to the address bar Input (set by timer, consumed by render)
    pub(crate) pending_url_bar_update: Option<String>,
    /// Cached raw window handle for WebView2 creation (avoids RefCell re-borrow)
    #[cfg(feature = "gpui")]
    pub(crate) cached_window_handle: Option<raw_window_handle::RawWindowHandle>,
}

/// Right-click context menu
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub(crate) struct ContextMenuState {
    position: gpui::Point<gpui::Pixels>,
}

/// Drag state for resizing split panes
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub(crate) struct ResizeDragState {
    pub(crate) split_first_pane: String,
    pub(crate) is_horizontal: bool,
    pub(crate) start_mouse_pos: f32,
    pub(crate) start_ratio: f32,
    pub(crate) container_length: f32,
}

/// Toast notification for agent status changes
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub(crate) struct ToastNotification {
    pub(crate) message: String,
    pub(crate) color: u32,
    pub(crate) frame_created: u32,
    pub(crate) pane_id: amux_platform::terminal::manager::PaneId,
    pub(crate) tab_index: usize,
}

/// Pane picker state for "Send to Pane" feature
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub(crate) struct PanePickerState {
    pub(crate) text: String,
    pub(crate) targets: Vec<(amux_platform::terminal::manager::PaneId, String)>,
    pub(crate) selected_index: usize,
}

/// Template picker state for "Apply Layout" feature
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub(crate) struct TemplatePickerState {
    pub(crate) templates: Vec<amux_platform::terminal::manager::LayoutTemplate>,
    pub(crate) selected_index: usize,
}

/// Agent launcher picker state
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub(crate) struct AgentPickerState {
    /// (tool_id, display_label, is_wsl)
    pub(crate) agents: Vec<(String, String, bool)>,
    pub(crate) selected_index: usize,
}

/// Drag data for tab drag-and-drop between panes
#[cfg(feature = "gpui")]
#[derive(Clone)]
pub(crate) struct DragTab {
    pub(crate) source_pane: amux_platform::terminal::manager::PaneId,
    pub(crate) tab_index: usize,
    pub(crate) title: String,
}

#[cfg(feature = "gpui")]
impl Render for DragTab {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py(px(4.0))
            .bg(rgb(0x282a2e))
            .border_1()
            .border_color(rgb(0x969896))
            .rounded(px(4.0))
            .text_xs()
            .text_color(rgb(0xc5c8c6))
            .shadow_md()
            .child(self.title.clone())
    }
}

/// Drag data for workspace reordering in sidebar
#[cfg(feature = "gpui")]
#[derive(Clone)]
struct DragWorkspace {
    workspace_id: String,
    name: String,
    index: usize,
}

#[cfg(feature = "gpui")]
impl Render for DragWorkspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py(px(4.0))
            .bg(rgb(0x252530))
            .border_1()
            .border_color(rgb(0x969896))
            .rounded(px(4.0))
            .text_sm()
            .text_color(rgb(0xc5c8c6))
            .shadow_md()
            .child(self.name.clone())
    }
}

/// Context menu item definition
#[cfg(feature = "gpui")]
#[derive(Clone)]
pub(crate) struct ContextMenuItem {
    pub(crate) label: &'static str,
    pub(crate) shortcut: Option<&'static str>,
    pub(crate) enabled: bool,
    pub(crate) separator_after: bool,
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
    /// Create a section header (non-clickable label)
    fn header(label: &'static str) -> Self {
        Self { label, shortcut: None, enabled: false, separator_after: false }
    }
}

/// Captured terminal environment for spawning a new pane/tab.
#[cfg(feature = "gpui")]
pub(crate) struct CapturedEnv {
    pub shell: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    /// Optional command to send as input after the shell starts (e.g. "wsl --cd /path")
    pub initial_input: Option<String>,
}

#[cfg(feature = "gpui")]
impl GpuiShellView {
    /// Create a new shell view with terminal manager
    pub fn new(app: DesktopApp, model: GpuiWindowModel, config: crate::gpui_config::AmuxConfig, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // Get the active workspace ID
        let active_ws_id = model.workspace_items.iter()
            .find(|w| w.is_active)
            .map(|w| w.id.clone())
            .unwrap_or_else(|| "default".to_string());

        // Restore layout structures (fast — no PTY spawning yet)
        let mut workspace_terminals = std::collections::HashMap::new();
        let layouts = Self::load_all_layouts();
        for ws in &model.workspace_items {
            let mut tm = if let Some(json) = layouts.get(&ws.id) {
                TerminalManager::restore_layout(json)
                    .unwrap_or_else(|| TerminalManager::with_scrollback(config.scrollback))
            } else {
                TerminalManager::with_scrollback(config.scrollback)
            };
            tm.heal_layout();
            workspace_terminals.insert(ws.id.clone(), tm);
        }
        if !workspace_terminals.contains_key(&active_ws_id) {
            workspace_terminals.insert(active_ws_id.clone(), TerminalManager::with_scrollback(config.scrollback));
        }
        // PTY processes will be spawned on the first render frame (deferred for fast startup)

        let ws_order: Vec<String> = model.workspace_items.iter().map(|w| w.id.clone()).collect();
        Self {
            app,
            model,
            sidebar_state: WorkspaceSidebarState::default(),
            workspace_terminals,
            active_workspace_id: active_ws_id,
            focus_handle,
            cell_metrics: None,
            selecting: false,
            context_menu: None,
            resize_drag: None,
            cursor_blink_frame: 0,
            renaming_workspace: None,
            renaming_tab: None,
            search_state: None,
            terminals_spawned: false,
            detected_vibe_tools: Vec::new(),
            tools_detected: false,
            zoomed_pane: None,
            workspace_order: ws_order,
            pane_bounds: std::collections::HashMap::new(),
            wsl_detected: false, // detected lazily in background
            terminal_theme: crate::gpui_terminal::TerminalTheme::by_name(&config.theme),
            config,
            toasts: Vec::new(),
            pane_picker: None,
            template_picker: None,
            agent_picker: None,
            ime_preedit: None,
            sidebar_drag_start: None,
            preview_state: None,
            preview_tabs: std::collections::HashMap::new(),
            file_picker: None,
            preview_drag_start: None,
            browser_drag_start: None,
            browser_tabs: std::collections::HashMap::new(),
            next_browser_id: 1,
            restore_terminal_focus: false,
            pending_url_input_focus: false,
            pending_url_bar_update: None,
            cached_window_handle: None,
        }
    }


    /// Get cell dimensions (width, height). Falls back to defaults if not yet measured.
    fn cell_dims(&self) -> (f32, f32) {
        match &self.cell_metrics {
            Some(m) => (m.width, m.height),
            None => (8.0, 20.0), // safe fallback before first render
        }
    }

    /// Current sidebar width in pixels.
    fn sidebar_width(&self) -> f32 {
        if self.sidebar_state.collapsed {
            SIDEBAR_WIDTH_COLLAPSED
        } else {
            self.sidebar_state.width
        }
    }

    /// Check if the active terminal has mouse reporting enabled.
    /// Returns (mouse_mode, sgr_mode).
    fn active_term_mouse_mode(&self) -> (bool, bool) {
        let mgr = self.terminal_manager();
        let pid = match mgr.active_pane_id() {
            Some(id) => id,
            None => return (false, false),
        };
        let pane = match mgr.get_pane(pid) {
            Some(p) => p,
            None => return (false, false),
        };
        let term = match pane.active_terminal_ref() {
            Some(t) => t,
            None => return (false, false),
        };
        term.with_term(|t| {
            let mode = t.mode();
            (
                mode.intersects(alacritty_terminal::term::TermMode::MOUSE_MODE),
                mode.contains(alacritty_terminal::term::TermMode::SGR_MOUSE),
            )
        })
    }

    /// Check if the active terminal is in alternate screen with alternate scroll mode.
    /// When true, scroll wheel should send arrow keys to the application instead of
    /// scrolling the (empty) scrollback buffer.
    fn active_term_alt_screen_scroll(&self) -> bool {
        let mgr = self.terminal_manager();
        let pid = match mgr.active_pane_id() {
            Some(id) => id,
            None => return false,
        };
        let pane = match mgr.get_pane(pid) {
            Some(p) => p,
            None => return false,
        };
        let term = match pane.active_terminal_ref() {
            Some(t) => t,
            None => return false,
        };
        term.with_term(|t| {
            let mode = t.mode();
            mode.contains(alacritty_terminal::term::TermMode::ALT_SCREEN)
                && mode.contains(alacritty_terminal::term::TermMode::ALTERNATE_SCROLL)
        })
    }

    /// Convert pixel position to terminal cell (col, row) for the active pane.
    /// Uses cached pane_bounds from render_layout for correct multi-pane coordinates.
    fn pixel_to_term_cell(&self, pos: gpui::Point<gpui::Pixels>) -> (usize, usize) {
        let (cw, ch) = self.cell_dims();
        let cw = cw.max(1.0);
        let ch = ch.max(1.0);

        // Look up active pane's screen bounds
        if let Some(pid) = self.terminal_manager().active_pane_id() {
            if let Some(&(px_x, px_y, _pw, _ph)) = self.pane_bounds.get(&pid.0) {
                let x = (pos.x.as_f32() - px_x).max(0.0);
                let y = (pos.y.as_f32() - px_y).max(0.0);
                return ((x / cw) as usize, (y / ch) as usize);
            }
        }

        // Fallback: assume single pane after sidebar + tab strip
        let sidebar_w = self.sidebar_width();
        let tab_strip_h = 28.0_f32;
        let x = (pos.x.as_f32() - sidebar_w).max(0.0);
        let y = (pos.y.as_f32() - tab_strip_h).max(0.0);
        ((x / cw) as usize, (y / ch) as usize)
    }

    /// Send a mouse event to the active terminal PTY.
    /// `button`: 0=left, 1=middle, 2=right, 64=scroll_up, 65=scroll_down
    /// `pressed`: true for press (M), false for release (m)
    fn send_mouse_event(&mut self, button: u8, col: usize, row: usize, pressed: bool) {
        let cx_1 = col + 1;
        let cy_1 = row + 1;
        let (_, sgr_mode) = self.active_term_mouse_mode();
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            if sgr_mode {
                let suffix = if pressed { 'M' } else { 'm' };
                let seq = format!("\x1b[<{};{};{}{}", button, cx_1, cy_1, suffix);
                term.send_input(seq.as_bytes());
            } else {
                // Legacy encoding — only supports press, release uses button 3
                let b = if pressed { button + 32 } else { 35 }; // 35 = release in legacy
                let x = (col.min(222) as u8) + 33;
                let y = (row.min(222) as u8) + 33;
                let seq = [b'\x1b', b'[', b'M', b, x, y];
                term.send_input(&seq);
            }
        }
    }

    /// Get the terminal manager for the active workspace (immutable)
    pub(crate) fn terminal_manager(&self) -> &TerminalManager {
        self.workspace_terminals.get(&self.active_workspace_id)
            .expect("active workspace must have a terminal manager")
    }

    /// Get the terminal manager for the active workspace (mutable).
    /// Auto-creates if missing (defensive against stale workspace IDs).
    pub(crate) fn terminal_manager_mut(&mut self) -> &mut TerminalManager {
        if !self.workspace_terminals.contains_key(&self.active_workspace_id) {
            self.ensure_workspace_terminal(&self.active_workspace_id.clone());
        }
        self.workspace_terminals.get_mut(&self.active_workspace_id)
            .expect("just ensured workspace exists")
    }

    /// Ensure a workspace has a terminal manager, creating one if needed.
    /// Also heals layout/pane inconsistencies for existing managers.
    fn ensure_workspace_terminal(&mut self, workspace_id: &str) {
        if !self.workspace_terminals.contains_key(workspace_id) {
            let mut tm = TerminalManager::with_scrollback(self.config.scrollback);
            let (shell, args) = Self::default_shell();
            let cwd = Self::default_cwd();
            let _ = tm.spawn_in_active(&shell, &args, cwd.as_deref());
            self.workspace_terminals.insert(workspace_id.to_string(), tm);
        } else if let Some(tm) = self.workspace_terminals.get_mut(workspace_id) {
            // Heal layout, then spawn all tabs (not just active) for restored workspaces
            tm.heal_layout();
            let (shell, args) = Self::default_shell();
            let cwd = Self::default_cwd();
            let pane_ids: Vec<_> = tm.active_layout()
                .map(|l| l.pane_ids()).unwrap_or_default();
            for pid in pane_ids {
                tm.spawn_all_tabs_in_pane(&pid, &shell, &args, cwd.as_deref());
            }
        }
    }

    /// Switch the active workspace terminal.
    /// Auto-runs startup commands if the workspace is empty and has a startup file.
    fn switch_workspace_terminal(&mut self, workspace_id: &str) {
        self.ensure_workspace_terminal(workspace_id);
        self.active_workspace_id = workspace_id.to_string();

        // Auto-run startup if workspace is empty and has a startup file
        if self.is_workspace_empty() {
            let ws_name = self.model.workspace_items.iter()
                .find(|w| w.id == workspace_id)
                .map(|w| w.name.clone())
                .unwrap_or_else(|| workspace_id.to_string());
            let path = Self::startup_file_path(&ws_name);
            if path.exists() {
                self.run_startup_commands();
            }
        }
    }

    /// Get the default shell program and args for the current platform
    pub(crate) fn default_shell() -> (String, Vec<String>) {
        if cfg!(target_os = "windows") {
            let shell = if Self::silent_command("pwsh.exe").arg("--version").output().is_ok() {
                "pwsh.exe"
            } else {
                "powershell.exe"
            };
            // -NoExit keeps shell open after running the init command
            // PSStyle fix removes background colors from directory listings
            (shell.to_string(), vec![
                "-NoLogo".to_string(),
                "-NoExit".to_string(),
                "-Command".to_string(),
                "$PSStyle.FileInfo.Directory = \"`e[34;1m\"; $PSStyle.FileInfo.Executable = \"`e[32;1m\"".to_string(),
            ])
        } else {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            (shell, vec!["-l".to_string()])
        }
    }

    pub(crate) fn default_cwd() -> Option<String> {
        std::env::current_dir().ok().map(|p| p.to_string_lossy().to_string())
    }



    /// Capture the active tab's shell + cwd before any operation that changes active pane.
    ///
    /// On Windows: detects if the user is inside a WSL session by checking the terminal
    /// title (bash sets it to "user@host:path"). If detected, spawns the same default shell
    /// but sends "wsl --cd <path>" as input, so `exit` from WSL returns to the shell normally.
    pub(crate) fn capture_active_env(&self) -> CapturedEnv {
        let title = self.terminal_manager().active_terminal_title();

        // Check terminal title for WSL session (user@host:path pattern)
        if cfg!(target_os = "windows") {
            if let Some(ref t) = title {
                if let Some(wsl_cmd) = Self::detect_wsl_from_title_str(t) {
                    let (shell, args) = Self::default_shell();
                    let wsl_cwd = Self::extract_wsl_path_from_title(t);
                    return CapturedEnv { shell, args, cwd: wsl_cwd, initial_input: Some(wsl_cmd) };
                }
            }
            // Fallback: detect WSL from prompt line (user@host:/path$)
            if let Some(prompt_line) = self.terminal_manager().active_terminal_ref()
                .map(|t| t.cursor_line_text())
            {
                if let Some(linux_path) = extract_cwd_from_prompt_line(&prompt_line) {
                    if linux_path.starts_with('/') {
                        let (shell, args) = Self::default_shell();
                        let wsl_cmd = format!("wsl --cd {}", linux_path);
                        return CapturedEnv { shell, args, cwd: Some(linux_path), initial_input: Some(wsl_cmd) };
                    }
                }
            }
        }

        let inherited = self.terminal_manager().active_shell_cmd()
            .map(|(s, a)| (s.to_string(), a.to_vec()));

        // Best-effort CWD: use the same resolve chain as file picker
        // Prompt extraction is the most reliable source on Windows —
        // PowerShell prompt always shows the real current directory.
        // sysinfo often returns the spawn-time CWD, not the live one after `cd`.
        let prompt_cwd = self.terminal_manager().active_terminal_ref()
            .map(|t| t.cursor_line_text())
            .and_then(|line| extract_cwd_from_prompt_line(&line))
            .map(|p| self.maybe_convert_wsl_path(&p));
        let process_cwd = self.terminal_manager().active_process_cwd();
        let saved_cwd = self.terminal_manager().active_saved_cwd();

        let live_cwd = prompt_cwd.filter(|p| std::path::Path::new(p).is_dir())
            .or_else(|| process_cwd.filter(|p| std::path::Path::new(p).is_dir()))
            .or_else(|| saved_cwd.filter(|p| std::path::Path::new(p).is_dir()));

        let (shell, args) = inherited.unwrap_or_else(Self::default_shell);
        let cwd = live_cwd.or_else(Self::default_cwd);
        CapturedEnv { shell, args, cwd, initial_input: None }
    }

    /// Parse a "user@host:path" or "user@host/path" terminal title.
    /// Returns the path portion if present, or empty string if the format matches but has no path.
    /// Returns None if the title doesn't match the WSL pattern at all.
    fn parse_wsl_title_path(title: &str) -> Option<&str> {
        let title = title.trim();
        let at_pos = title.find('@')?;
        if at_pos == 0 { return None; }
        let after_at = &title[at_pos + 1..];
        let path = if let Some(colon_pos) = after_at.find(':') {
            after_at[colon_pos + 1..].trim_start()
        } else if let Some(slash_pos) = after_at.find('/') {
            &after_at[slash_pos..]
        } else {
            return None;
        };
        Some(path.trim())
    }

    /// Detect WSL session from terminal title and return the wsl command to send.
    /// Returns Some("wsl --cd /path") or Some("wsl") if WSL detected.
    fn detect_wsl_from_title_str(title: &str) -> Option<String> {
        let path = Self::parse_wsl_title_path(title)?;
        if path.is_empty() {
            Some("wsl".to_string())
        } else if path.starts_with('/') {
            Some(format!("wsl --cd {}", path))
        } else {
            Some("wsl".to_string())
        }
    }

    /// Extract the WSL path from a "user@host:path" terminal title.
    fn extract_wsl_path_from_title(title: &str) -> Option<String> {
        let path = Self::parse_wsl_title_path(title)?;
        if path.starts_with('/') { Some(path.to_string()) } else { None }
    }

    /// Spawn a terminal in the active pane's active tab, inheriting env from the current tab.
    pub(crate) fn spawn_terminal_in_active(&mut self) {
        let env = self.capture_active_env();
        self.spawn_with_captured_env(&env);
    }

    /// Spawn a terminal with pre-captured environment (use after split/new-tab).
    pub(crate) fn spawn_with_captured_env(&mut self, env: &CapturedEnv) {
        // When initial_input is set (WSL scenario), the CWD is a Linux path that
        // Windows ConPTY cannot use as working_directory. Pass None to PTY and let
        // the initial_input command (e.g. "wsl --cd /path") handle directory setup.
        let pty_cwd = if env.initial_input.is_some() {
            None
        } else {
            env.cwd.as_deref()
        };
        if let Err(e) = self.terminal_manager_mut().spawn_in_active(&env.shell, &env.args, pty_cwd) {
            eprintln!("[amux] spawn_in_active failed: {} | shell={:?} args={:?} cwd={:?}", e, env.shell, env.args, pty_cwd);
        }
        // Send initial command if present (e.g. "wsl --cd /path")
        if let Some(ref cmd) = env.initial_input {
            if let Some(term) = self.terminal_manager_mut().active_terminal() {
                let input = format!("{}\r", cmd);
                term.send_input(input.as_bytes());
            }
        }
        // Record the logical CWD on the tab for future inheritance and persistence,
        // even if it wasn't passed to the PTY (e.g. WSL Linux paths on Windows).
        // Always overwrite so that live CWD from the parent pane is captured.
        if let Some(ref cwd) = env.cwd {
            if let Some(pane) = self.terminal_manager_mut().active_pane_mut() {
                if let Some(tab) = pane.tabs.get_mut(pane.active_tab) {
                    tab.cwd = Some(cwd.clone());
                }
            }
        }
    }

    /// Apply a layout template to the current workspace.
    /// Replaces all panes and spawns terminals in each.
    pub(crate) fn apply_template(&mut self, template: &amux_platform::terminal::manager::LayoutTemplate) {
        let mut tm = TerminalManager::from_template(template);
        tm.set_scrollback(self.config.scrollback);
        self.workspace_terminals.insert(self.active_workspace_id.clone(), tm);
        // Spawn terminals in all panes
        let (shell, args) = Self::default_shell();
        let cwd = Self::default_cwd();
        let pane_ids: Vec<_> = self.terminal_manager().active_layout()
            .map(|l| l.pane_ids()).unwrap_or_default();
        for pid in pane_ids {
            self.terminal_manager_mut().spawn_all_tabs_in_pane(&pid, &shell, &args, cwd.as_deref());
        }
        self.save_all_layouts();
    }

    /// Apply a template by name (searches built-in + custom).
    pub(crate) fn apply_template_by_name(&mut self, name: &str) {
        let templates = Self::all_templates();
        if let Some(t) = templates.iter().find(|t| t.name == name) {
            self.apply_template(t);
        }
    }

    /// Save current layout as a custom template with auto-generated name.
    pub(crate) fn save_current_as_template(&mut self, name: &str) {
        let desc = format!("{} panes", self.terminal_manager().total_panes());
        let template = self.terminal_manager().to_template(name, &desc);
        Self::save_template(&template);
    }

    /// Open the template picker overlay.
    pub(crate) fn open_template_picker(&mut self) {
        let templates = Self::all_templates();
        if templates.is_empty() { return; }
        self.template_picker = Some(TemplatePickerState {
            templates,
            selected_index: 0,
        });
    }

    /// Execute the template picker selection.
    pub(crate) fn execute_template_picker(&mut self) {
        if let Some(picker) = self.template_picker.take() {
            if let Some(t) = picker.templates.get(picker.selected_index) {
                self.apply_template(t);
            }
        }
    }

    /// Delete the selected custom template from the picker.
    pub(crate) fn delete_selected_template(&mut self) {
        if let Some(ref mut picker) = self.template_picker {
            if let Some(t) = picker.templates.get(picker.selected_index) {
                if t.builtin { return; } // can't delete built-in
                let name = t.name.clone();
                Self::delete_template(&name);
                picker.templates.remove(picker.selected_index);
                if picker.templates.is_empty() {
                    self.template_picker = None;
                } else if picker.selected_index >= picker.templates.len() {
                    picker.selected_index = picker.templates.len() - 1;
                }
            }
        }
    }

    /// Open the agent launcher picker.
    pub(crate) fn open_agent_picker(&mut self) {
        let mut agents: Vec<(String, String, bool)> = Vec::new();
        if self.wsl_detected {
            agents.push(("wsl".into(), "WSL Terminal".into(), true));
        }
        for &(tool_id, label, _env) in &self.detected_vibe_tools {
            agents.push((tool_id.into(), label.into(), false));
        }
        if agents.is_empty() { return; }
        // If only one option, launch directly
        if agents.len() == 1 {
            let (tool_id, _, is_wsl) = &agents[0];
            if *is_wsl {
                self.launch_wsl_shell();
            } else {
                self.launch_vibe_tool_env(tool_id, false);
            }
            return;
        }
        self.agent_picker = Some(AgentPickerState {
            agents,
            selected_index: 0,
        });
    }

    /// Execute the agent picker selection.
    pub(crate) fn execute_agent_picker(&mut self) {
        if let Some(picker) = self.agent_picker.take() {
            if let Some((tool_id, _, is_wsl)) = picker.agents.get(picker.selected_index) {
                if *is_wsl {
                    self.launch_wsl_shell();
                } else {
                    self.launch_vibe_tool_env(tool_id, false);
                }
            }
        }
    }

    /// Restart the terminal in a specific pane (used when process exits)
    pub(crate) fn restart_terminal_in_pane(&mut self, pane_id: &amux_platform::terminal::manager::PaneId) {
        self.terminal_manager_mut().set_active_pane(pane_id);
        // Restart with the same shell + cwd the tab was using
        let inherited = self.terminal_manager().active_shell_cmd()
            .map(|(s, a)| (s.to_string(), a.to_vec()));
        let saved_cwd = self.terminal_manager().active_cwd();

        let (shell, args) = inherited.unwrap_or_else(Self::default_shell);
        let cwd = saved_cwd.or_else(Self::default_cwd);
        let _ = self.terminal_manager_mut().restart_active_terminal(&shell, &args, cwd.as_deref());
    }


    /// Reorder a workspace by moving it from one index to another.
    fn reorder_workspace(&mut self, from_index: usize, to_id: &str) {
        let items = &mut self.model.workspace_items;
        let to_index = items.iter().position(|w| w.id == to_id);
        if let Some(to_index) = to_index {
            if from_index != to_index && from_index < items.len() {
                let item = items.remove(from_index);
                // After remove, adjust target: if we removed before target, target shifted left
                let adjusted = if from_index < to_index { to_index.saturating_sub(1) } else { to_index };
                let insert_at = adjusted.min(items.len());
                items.insert(insert_at, item);
                self.workspace_order = items.iter().map(|w| w.id.clone()).collect();
            }
        }
    }

    /// Refresh model from backend, then re-apply custom workspace order.
    pub(crate) fn refresh_model(&mut self) {
        self.model = self.app.render_with(&amux_ui::GpuiRenderer);
        self.apply_workspace_order();
    }

    /// Sort workspace_items to match the stored workspace_order.
    /// New workspaces (not in order list) go to the end.
    fn apply_workspace_order(&mut self) {
        let order = &self.workspace_order;
        self.model.workspace_items.sort_by(|a, b| {
            let ia = order.iter().position(|id| id == &a.id).unwrap_or(usize::MAX);
            let ib = order.iter().position(|id| id == &b.id).unwrap_or(usize::MAX);
            ia.cmp(&ib)
        });
        // Add any new workspaces to the order list
        for w in &self.model.workspace_items {
            if !self.workspace_order.contains(&w.id) {
                self.workspace_order.push(w.id.clone());
            }
        }
    }

    /// Jump to the next/previous search match in the active terminal.
    pub(crate) fn search_navigate(&mut self, forward: bool) {
        use alacritty_terminal::index::{Direction, Side, Point as APoint, Line, Column};
        use alacritty_terminal::term::search::RegexSearch;

        let query = match &self.search_state {
            Some((q, _)) if !q.is_empty() => q.clone(),
            _ => return,
        };

        // Escape regex special chars for literal search
        let escaped: String = query.chars().flat_map(|c| {
            if "\\^$.|?*+()[]{}".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        }).collect();
        let mut regex = match RegexSearch::new(&escaped) {
            Ok(r) => r,
            Err(_) => return,
        };

        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            let result = term.with_term(|t| {
                let direction = if forward { Direction::Right } else { Direction::Left };
                let origin = t.renderable_content().cursor.point;
                t.search_next(&mut regex, origin, direction, Side::Left, None)
            });
            if let Some(m) = result {
                term.with_term_mut(|t| {
                    // Scroll to bring match into view
                    let line_i32 = m.start().line.0;
                    if line_i32 < 0 {
                        let needed = (-line_i32) as usize;
                        let display_offset = t.grid().display_offset();
                        if needed > display_offset {
                            t.scroll_display(alacritty_terminal::grid::Scroll::Delta(
                                (needed - display_offset) as i32
                            ));
                        }
                    } else if t.grid().display_offset() > 0 {
                        // Match is on screen but we're scrolled up — scroll to bottom
                        t.scroll_display(alacritty_terminal::grid::Scroll::Bottom);
                    }
                    // Highlight the match via selection
                    use alacritty_terminal::selection::{Selection, SelectionType};
                    let mut sel = Selection::new(SelectionType::Simple, *m.start(), Side::Left);
                    sel.update(*m.end(), Side::Right);
                    t.selection = Some(sel);
                });
            }
        }
    }

    /// Toggle zoom on the active pane — fills the entire content area.
    /// Press again to restore the original split layout.
    pub(crate) fn toggle_zoom(&mut self) {
        if self.zoomed_pane.is_some() {
            self.zoomed_pane = None;
        } else if let Some(pid) = self.terminal_manager().active_pane_id().cloned() {
            self.zoomed_pane = Some(pid);
        }
    }


    /// Convert pixel position to terminal cell coordinates.
    /// Accounts for sidebar width.
    fn pixel_to_cell(pos: gpui::Point<gpui::Pixels>, sidebar_width: f32, cell_w: f32, cell_h: f32) -> (usize, usize) {
        let sidebar_px = px(sidebar_width);
        let cw = px(cell_w);
        let ch = px(cell_h);
        let adj_x = if pos.x > sidebar_px { pos.x - sidebar_px } else { px(0.0) };
        let col = (adj_x / cw) as usize;
        let row = (pos.y / ch) as usize;
        (col, row)
    }

    /// Build context menu items based on current state
    fn build_context_menu_items(&self) -> Vec<ContextMenuItem> {
        let has_selection = false; // TODO: integrate alacritty selection

        let mut items = vec![
            ContextMenuItem::action("Copy", Some("Ctrl+Shift+C"), has_selection),
            ContextMenuItem::action("Send to Pane", Some("Ctrl+Shift+Enter"), self.terminal_manager().total_panes() > 1),
            ContextMenuItem::action("Paste", Some("Ctrl+V"), true).separator(),
            ContextMenuItem::action("Split Right", Some("Ctrl+Shift+\\"), true),
            ContextMenuItem::action("Split Down", Some("Ctrl+Shift+D"), true).separator(),
            ContextMenuItem::action("New Tab", Some("Ctrl+Shift+T"), true),
            ContextMenuItem::action("Close Pane", Some("Ctrl+Shift+W"), self.terminal_manager().total_panes() > 1).separator(),
            if self.zoomed_pane.is_some() {
                ContextMenuItem::action("Restore Pane", Some("Ctrl+Shift+F"), true).separator()
            } else {
                ContextMenuItem::action("Zoom Pane", Some("Ctrl+Shift+F"), self.terminal_manager().total_panes() > 1).separator()
            },
        ];

        // Workspace startup commands
        {
            let ws_name = self.model.active_workspace_name
                .clone().unwrap_or_else(|| self.active_workspace_id.clone());
            let has_startup = Self::startup_file_path(&ws_name).exists();
            if has_startup {
                items.push(ContextMenuItem::action("Run Startup", None, true));
            }
            items.push(ContextMenuItem::action("Edit Startup", None, true).separator());
        }

        // Layout templates (opens picker)
        items.push(ContextMenuItem::action("Apply Layout...", None, true));
        items.push(ContextMenuItem::action("Save Layout as Template", None, true).separator());

        // File preview & browser
        items.push(ContextMenuItem::action("Preview File...", None, true));
        let browser_label = if self.has_visible_browser() { "Close Browser" } else { "Open Browser" };
        items.push(ContextMenuItem::action(browser_label, None, true).separator());

        // Terminal launchers
        let has_launchers = self.wsl_detected || !self.detected_vibe_tools.is_empty();
        if has_launchers {
            items.push(ContextMenuItem::action("Launch Agent...", None, true));
        }
        items
    }

    /// Execute a context menu action by label
    pub(crate) fn execute_context_menu_action(&mut self, label: &str, window: &mut Window, cx: &mut Context<Self>) {
        match label {
            "Copy" => {
                self.copy_selection(cx);
            }
            "Send to Pane" => {
                self.start_send_to_pane(cx);
            }
            "Paste" => {
                self.paste_clipboard(cx);
            }
            "Split Right" => {
                let env = self.capture_active_env();
                self.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
                self.spawn_with_captured_env(&env);
            }
            "Split Down" => {
                let env = self.capture_active_env();
                self.terminal_manager_mut().split_active_pane(SplitDirection::Vertical);
                self.spawn_with_captured_env(&env);
            }
            "New Tab" => {
                let env = self.capture_active_env();
                self.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                self.spawn_with_captured_env(&env);
            }
            "Close Pane" => {
                self.zoomed_pane = None; // unzoom on close
                self.terminal_manager_mut().close_active_pane();
            }
            "Zoom Pane" | "Restore Pane" => {
                self.toggle_zoom();
            }
            "WSL Terminal" | "Launch Agent..." => {
                self.open_agent_picker();
            }
            "Clear" => {
                if let Some(term) = self.terminal_manager_mut().active_terminal() {
                    let _ = term.send_input(&[0x0c]);
                }
            }
            "Run Startup" => {
                self.run_startup_commands();
            }
            "Edit Startup" => {
                self.edit_startup_file();
            }
            "Preview File..." => {
                self.open_file_picker();
            }
            "Open Browser" => {
                self.open_browser("http://localhost:3000", window, cx);
            }
            "Close Browser" => {
                self.close_browser();
            }
            "Apply Layout..." => {
                self.open_template_picker();
            }
            "Save Layout as Template" => {
                let ws_name = self.model.active_workspace_name
                    .clone().unwrap_or_else(|| self.active_workspace_id.clone());
                self.save_current_as_template(&ws_name);
            }
            l if l.starts_with("Launch Claude")   => self.launch_vibe_tool_env("claude", l.contains("WSL")),
            l if l.starts_with("Launch Codex")    => self.launch_vibe_tool_env("codex", l.contains("WSL")),
            l if l.starts_with("Launch OpenCode") => self.launch_vibe_tool_env("opencode", l.contains("WSL")),
            l if l.starts_with("Launch Aider")    => self.launch_vibe_tool_env("aider", l.contains("WSL")),
            l if l.starts_with("Launch Gemini")   => self.launch_vibe_tool_env("gemini", l.contains("WSL")),
            l if l.starts_with("Launch Copilot")  => self.launch_vibe_tool_env("copilot", l.contains("WSL")),
            _ => {}
        }
        self.context_menu = None;
        cx.notify();
    }

    // ─── File Preview ────────────────────────────────────────────

    /// Open the file picker (Ctrl+P / right-click / amux preview)
    pub(crate) fn open_file_picker(&mut self) {
        // Try prompt extraction first (most reliable for WSL), then resolve chain
        let prompt_cwd = self.terminal_manager().active_terminal_ref()
            .map(|t| t.cursor_line_text())
            .and_then(|line| extract_cwd_from_prompt_line(&line));
        let cwd = prompt_cwd.map(|p| self.maybe_convert_wsl_path(&p))
            .filter(|p| std::path::Path::new(p).is_dir())
            .or_else(|| self.resolve_active_cwd());
        self.file_picker = Some(crate::gpui_preview::FilePickerState::new(cwd));
    }

    fn open_file_picker_with_cwd(&mut self, cwd: Option<String>) {
        let cwd = cwd.map(|p| self.maybe_convert_wsl_path(&p))
            .filter(|p| std::path::Path::new(p).is_dir())
            .or_else(|| self.resolve_active_cwd());
        self.file_picker = Some(crate::gpui_preview::FilePickerState::new(cwd));
    }

    fn open_preview_file_with_cwd(&mut self, path: &str, cwd: Option<&str>) {
        let full_path = if std::path::Path::new(path).is_absolute() {
            self.maybe_convert_wsl_path(path)
        } else {
            let resolved_cwd = cwd.map(|p| self.maybe_convert_wsl_path(p))
                .filter(|p| std::path::Path::new(p).is_dir())
                .or_else(|| self.resolve_active_cwd());
            resolved_cwd
                .map(|cwd| std::path::PathBuf::from(cwd).join(path).to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string())
        };
        self.open_preview_file(&full_path);
    }

    /// Best-effort resolve the current working directory of the active pane.
    /// Tries multiple sources in order:
    /// 1. Live /proc/PID/cwd (Linux/WSL only)
    /// 2. Parse cwd from the terminal prompt line (PowerShell: "PS C:\path>", Bash: "user@host:~/dir$")
    /// 3. Saved spawn-time cwd from tab
    /// 4. GUI process cwd (last resort)
    fn resolve_active_cwd(&self) -> Option<String> {
        // 1. Parse from terminal prompt line — most reliable, always shows live cwd
        //    (sysinfo on Windows often returns stale spawn-time cwd)
        if let Some(cwd) = self.extract_cwd_from_prompt() {
            let resolved = self.maybe_convert_wsl_path(&cwd);
            if std::path::Path::new(&resolved).is_dir() {
                return Some(resolved);
            }
        }

        // 2. Live process cwd (sysinfo on Windows, /proc on Linux)
        if let Some(cwd) = self.terminal_manager().active_process_cwd() {
            let resolved = self.maybe_convert_wsl_path(&cwd);
            if std::path::Path::new(&resolved).is_dir() {
                return Some(resolved);
            }
        }

        // 3. Saved spawn-time cwd — last resort, may be stale after `cd`
        if let Some(cwd) = self.terminal_manager().active_saved_cwd() {
            let resolved = self.maybe_convert_wsl_path(&cwd);
            if std::path::Path::new(&resolved).is_dir() {
                return Some(resolved);
            }
        }

        None
    }

    /// Convert a WSL Linux path to a Windows-accessible path.
    /// Two cases:
    ///   /mnt/d/repo/...  → D:\repo\...        (WSL drive mount → native Windows path)
    ///   /home/user/...   → \\wsl$\Distro\...  (WSL-native → UNC path)
    /// On Linux builds, this is a no-op.
    fn maybe_convert_wsl_path(&self, path: &str) -> String {
        #[cfg(target_os = "windows")]
        {
            if !path.starts_with('/') {
                return path.to_string();
            }

            // Case 1: /mnt/X/... → X:\...  (drive mount)
            if path.starts_with("/mnt/") && path.len() >= 6 {
                let drive_letter = path.as_bytes()[5]; // the char after "/mnt/"
                if drive_letter.is_ascii_alphabetic()
                    && (path.len() == 6 || path.as_bytes()[6] == b'/')
                {
                    let rest = if path.len() > 6 { &path[6..] } else { "" };
                    let win_path = format!("{}:{}", (drive_letter as char).to_uppercase().next().unwrap(), rest.replace('/', "\\"));
                    return win_path;
                }
            }

            // Case 2: /home/... or other WSL-native path → \\wsl$\Distro\...
            let distro = self.detect_pane_wsl_distro()
                .or_else(|| amux_platform::get_default_distro());
            if let Some(distro) = distro {
                return amux_platform::windows::paths::wsl_unc_path(&distro, path);
            }
        }
        path.to_string()
    }

    /// Check if the active pane is running in WSL and return the distro name.
    #[cfg(target_os = "windows")]
    fn detect_pane_wsl_distro(&self) -> Option<String> {
        let (shell, args) = self.terminal_manager().active_shell_cmd()?;
        if !shell.to_lowercase().contains("wsl") {
            return None;
        }
        for (i, arg) in args.iter().enumerate() {
            if (arg == "-d" || arg == "--distribution") && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
        amux_platform::get_default_distro()
    }

    /// Extract the working directory from the terminal's current prompt line.
    /// Supports:
    ///   PowerShell: "PS C:\Users\foo\project> ..."  → "C:\Users\foo\project"
    ///   Bash/Zsh:   "user@host:~/project$ ..."      → expand ~ to home
    fn extract_cwd_from_prompt(&self) -> Option<String> {
        let term = self.terminal_manager().active_terminal_ref()?;
        let line = term.cursor_line_text();

        // PowerShell: "PS C:\path> " or "PS D:\path>"
        if let Some(ps_start) = line.find("PS ") {
            let after_ps = &line[ps_start + 3..];
            // Find the closing ">" which ends the path
            if let Some(gt) = after_ps.find('>') {
                let path = after_ps[..gt].trim();
                if !path.is_empty() {
                    return Some(path.to_string());
                }
            }
        }

        // Bash/Zsh: "user@host:~/dir$ " or "user@host:/path$ "
        if let Some(colon) = line.find(':') {
            // Check for @ before colon (indicates user@host:path pattern)
            if line[..colon].contains('@') {
                let after_colon = &line[colon + 1..];
                if let Some(dollar) = after_colon.find("$ ") {
                    let path = after_colon[..dollar].trim();
                    if !path.is_empty() {
                        // Expand ~ to home directory
                        let expanded = if path.starts_with('~') {
                            if let Some(home) = std::env::var("HOME").ok().or_else(|| std::env::var("USERPROFILE").ok()) {
                                path.replacen('~', &home, 1)
                            } else {
                                path.to_string()
                            }
                        } else {
                            path.to_string()
                        };
                        return Some(expanded);
                    }
                }
            }
        }

        None
    }

    /// Open a file for preview from the picker
    pub(crate) fn open_preview_from_picker(&mut self, index: usize) {
        let (path, base_dir) = if let Some(ref picker) = self.file_picker {
            (picker.matches.get(index).cloned(), picker.base_dir.clone())
        } else {
            (None, None)
        };
        self.file_picker = None;
        if let Some(path) = path {
            // Use the picker's base_dir to resolve the relative path —
            // this is the cwd that was captured when the picker opened,
            // guaranteed to be consistent with the file list.
            let full_path = if std::path::Path::new(&path).is_absolute() {
                path
            } else if let Some(ref base) = base_dir {
                std::path::PathBuf::from(base).join(&path).to_string_lossy().to_string()
            } else {
                path
            };
            self.open_preview_file(&full_path);
        }
    }

    /// Try to preview a file path at the given terminal cell position.
    /// Extracts the path-like text around (col, row), checks if file exists.
    /// Returns true if a preview was opened.
    pub(crate) fn try_preview_path_at(&mut self, col: usize, row: usize) -> bool {
        let path = match self.extract_path_at_cursor(col, row) {
            Some(p) => p,
            None => return false,
        };

        // Check if file exists (try relative to pane CWD, then absolute)
        let pane_cwd = self.resolve_active_cwd();
        let converted = self.maybe_convert_wsl_path(&path);
        let resolved = if std::path::Path::new(&converted).is_absolute() && std::path::Path::new(&converted).exists() {
            converted
        } else if let Some(ref cwd) = pane_cwd {
            let full = std::path::PathBuf::from(cwd).join(&path);
            if full.exists() {
                full.to_string_lossy().to_string()
            } else {
                return false;
            }
        } else {
            return false;
        };

        // Check if it's a previewable file type
        let is_previewable = matches!(
            std::path::Path::new(&resolved).extension().and_then(|e| e.to_str()),
            Some("md" | "markdown" | "txt" | "rs" | "js" | "ts" | "py" | "toml"
                | "json" | "yaml" | "yml" | "sh" | "bash" | "css" | "html"
                | "tsx" | "jsx" | "go" | "c" | "cpp" | "h" | "hpp" | "java"
                | "rb" | "php" | "swift" | "kt" | "lua" | "sql" | "xml"
                | "ini" | "cfg" | "conf" | "log" | "vim")
        );
        if !is_previewable { return false; }

        self.open_preview_file(&resolved);
        true
    }

    /// Extract a file-path-like string from the terminal grid at the given position.
    /// Scans left and right from (col, row) collecting path characters.
    fn extract_path_at_cursor(&self, col: usize, row: usize) -> Option<String> {
        let term = self.terminal_manager().active_terminal_ref()?;
        term.with_term(|t| {
            use alacritty_terminal::grid::Dimensions;
            use alacritty_terminal::index::{Line, Column};

            let grid = t.grid();
            let cols = grid.columns();
            let rows = grid.screen_lines();
            if row >= rows || col >= cols { return None; }

            let line = Line(row as i32);

            // Check if the character at cursor is a path-like character
            let ch = grid[line][Column(col)].c;
            if ch == ' ' || ch == '\0' { return None; }

            // Path characters: alphanumeric, /, \, ., -, _, :, ~
            let is_path_char = |c: char| -> bool {
                c.is_alphanumeric() || matches!(c, '/' | '\\' | '.' | '-' | '_' | ':' | '~' | '(' | ')')
            };

            // Scan left
            let mut start = col;
            while start > 0 {
                let c = grid[line][Column(start - 1)].c;
                if !is_path_char(c) { break; }
                start -= 1;
            }

            // Scan right
            let mut end = col;
            while end + 1 < cols {
                let c = grid[line][Column(end + 1)].c;
                if !is_path_char(c) { break; }
                end += 1;
            }

            // Collect the text
            let mut path = String::new();
            for c in start..=end {
                let ch = grid[line][Column(c)].c;
                if ch != '\0' {
                    path.push(ch);
                }
            }

            let path = path.trim().to_string();
            if path.len() < 3 { return None; } // too short to be a useful path

            Some(path)
        })
    }

    /// Open a file for preview by path
    pub(crate) fn open_preview_file(&mut self, path: &str) {
        // Resolve relative paths against active pane's CWD
        let full_path = if std::path::Path::new(path).is_absolute() {
            path.to_string()
        } else {
            self.resolve_active_cwd()
                .map(|cwd| std::path::PathBuf::from(cwd).join(path).to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string())
        };
        // Load preview and open as a tab in the active pane
        if let Some(state) = crate::gpui_preview::PreviewState::load(&full_path) {
            let active_pid = self.terminal_manager().active_pane_id().cloned();
            if let Some(ref pid) = active_pid {
                if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                    pane.add_preview_tab(&full_path);
                }
            }
            self.preview_tabs.insert(full_path, state);
        }
    }

    // ─── Browser Pane ────────────────────────────────────────────

    /// Open a browser tab in the active pane (limux-style).
    /// WebView2 creation is deferred via cx.spawn to avoid RefCell re-borrow.
    pub(crate) fn open_browser(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        use gpui_component::input::{InputState, InputEvent};
        use crate::gpui_browser::{BrowserPaneState, BrowserTabEntry};

        let url = if url.is_empty() { "http://localhost:3000" } else { url };
        let raw_handle = match self.cached_window_handle {
            Some(h) => h,
            None => {
                eprintln!("[amux-browser] no cached window handle yet");
                return;
            }
        };

        // Assign a unique browser_id
        let browser_id = self.next_browser_id;
        self.next_browser_id += 1;

        // Add browser tab to the active pane
        let active_pid = self.terminal_manager().active_pane_id().cloned();
        if let Some(ref pid) = active_pid {
            if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                pane.add_browser_tab(url, browser_id);
            }
        }

        // Create URL bar Input entity
        let url_owned = url.to_string();
        let url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(url_owned)
                .placeholder("Enter URL and press Enter...")
        });

        // Subscribe: Enter navigates
        let bid = browser_id;
        cx.subscribe(&url_input, move |this: &mut GpuiShellView, input_entity, event: &InputEvent, cx| {
            match event {
                InputEvent::PressEnter { .. } => {
                    let url = input_entity.read(cx).value().to_string();
                    if url.is_empty() { return; }
                    let url = if !url.contains("://") {
                        if url.starts_with("localhost") || url.contains(':') {
                            format!("http://{}", url)
                        } else {
                            format!("https://{}", url)
                        }
                    } else { url };
                    if let Some(entry) = this.browser_tabs.get_mut(&bid) {
                        entry.browser.navigate(&url);
                    }
                    this.restore_terminal_focus = true;
                    cx.notify();
                }
                InputEvent::Blur => { cx.notify(); }
                _ => {}
            }
        }).detach();

        let bounds_cell = std::rc::Rc::new(std::cell::Cell::new(None));

        // Store the browser tab entry
        self.browser_tabs.insert(browser_id, BrowserTabEntry {
            browser: BrowserPaneState::new(url),
            url_input,
            bounds_cell: bounds_cell.clone(),
        });

        // Defer WebView2 creation
        cx.spawn(async move |this, cx| {
            smol::Timer::after(std::time::Duration::from_millis(50)).await;
            let _ = this.update(cx, |this: &mut GpuiShellView, cx| {
                if let Some(entry) = this.browser_tabs.get_mut(&bid) {
                    if !entry.browser.is_initialized() {
                        entry.browser.init_webview(raw_handle);
                        if let Some(bounds) = entry.bounds_cell.get() {
                            entry.browser.sync_bounds(bounds);
                        }
                        entry.browser.focus_parent();
                    }
                }
                this.restore_terminal_focus = true;
                cx.notify();
            });
        }).detach();

        cx.notify();
    }

    /// Close the browser tab that is active in the current pane.
    pub(crate) fn close_browser(&mut self) {
        // Find the active pane's active tab — if it's a browser, close it
        let active_pid = self.terminal_manager().active_pane_id().cloned();
        if let Some(ref pid) = active_pid {
            let browser_id = self.terminal_manager().get_pane(pid)
                .and_then(|p| p.active_tab_kind())
                .and_then(|k| match k {
                    amux_platform::terminal::manager::TabKind::Browser { browser_id, .. } => Some(*browser_id),
                    _ => None,
                });
            if let Some(bid) = browser_id {
                self.browser_tabs.remove(&bid);
                // Close the tab in the pane
                if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                    let idx = pane.active_tab;
                    pane.close_tab(idx);
                }
            }
        }
        self.restore_terminal_focus = true;
    }

    /// Get the active browser tab entry (if the active pane's active tab is a browser).
    pub(crate) fn active_browser_entry(&self) -> Option<(u64, &crate::gpui_browser::BrowserTabEntry)> {
        let pid = self.terminal_manager().active_pane_id()?;
        let pane = self.terminal_manager().get_pane(pid)?;
        match pane.active_tab_kind()? {
            amux_platform::terminal::manager::TabKind::Browser { browser_id, .. } => {
                self.browser_tabs.get(browser_id).map(|e| (*browser_id, e))
            }
            _ => None,
        }
    }

    /// Check if any browser tab exists and is visible (active in its pane).
    pub(crate) fn has_visible_browser(&self) -> bool {
        self.active_browser_entry().is_some()
    }


    /// Check if the current terminal input line is an `amux` command.
    /// Returns Some(true) if intercepted, Some(false) if not an amux command, None if can't read.
    fn try_intercept_amux_command(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<bool> {
        // Read the cursor line — this is always the line the user just typed on,
        // regardless of screen position or leftover content below.
        let last_line = self.terminal_manager().active_terminal_ref()
            .map(|t| t.cursor_line_text())?;

        // Extract the command after the prompt. Look for common prompt patterns:
        // "PS C:\path> amux preview file.md"
        // "user@host:~$ amux preview file.md"
        // "> amux preview file.md"
        let cmd = extract_command_after_prompt(&last_line);
        let cmd = cmd.trim();

        if !cmd.starts_with("amux ") && cmd != "amux" {
            return Some(false);
        }

        // Extract CWD from the prompt portion of the SAME line we just read.
        // This is the most reliable source — the prompt always shows the path,
        // and we read it before any state changes (Ctrl+C, etc.).
        let prompt_cwd = extract_cwd_from_prompt_line(&last_line);

        let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
        match parts.get(1).map(|s| *s) {
            Some("preview") | Some("view") | Some("open") => {
                if let Some(path) = parts.get(2) {
                    let path = path.trim();
                    if !path.is_empty() {
                        self.open_preview_file_with_cwd(path, prompt_cwd.as_deref());
                        return Some(true);
                    }
                }
                // No file specified — open file picker
                self.open_file_picker_with_cwd(prompt_cwd);
                Some(true)
            }
            Some("browser") | Some("web") => {
                let url = parts.get(2).map(|s| s.trim()).unwrap_or("http://localhost:3000");
                self.open_browser(url, window, cx);
                Some(true)
            }
            _ => Some(false),
        }
    }

    /// Handle key input for the terminal
    pub fn handle_terminal_input(&mut self, key: &str, ctrl: bool, shift: bool, alt: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Reset cursor blink on any terminal input
        self.cursor_blink_frame = 0;
        use amux_platform::terminal::keys;
        
        // GPUI sends lowercase keys but to_pty expects title case
        let normalized_key = match key {
            "enter" => "Enter",
            "tab" => "Tab",
            "escape" => "Escape",
            "backspace" => "Backspace",
            "up" | "arrowup" => "ArrowUp",
            "down" | "arrowdown" => "ArrowDown",
            "left" | "arrowleft" => "ArrowLeft",
            "right" | "arrowright" => "ArrowRight",
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
        
        // Intercept `amux` commands on Enter before sending to PTY
        if normalized_key == "Enter" && !ctrl && !alt {
            if let Some(handled) = self.try_intercept_amux_command(window, cx) {
                if handled {
                    // Send Enter to PTY so the shell gets a blank line (command was "eaten")
                    // Then send Ctrl+C to cancel the partially typed command
                    if let Some(terminal) = self.terminal_manager_mut().active_terminal() {
                        terminal.send_input(b"\x03"); // Ctrl+C to clear the line
                    }
                    return;
                }
            }
        }

        // Check app cursor key mode from active terminal
        let app_cursor = self.terminal_manager().active_terminal_ref()
            .map(|t| t.with_term(|term| term.mode().contains(alacritty_terminal::term::TermMode::APP_CURSOR)))
            .unwrap_or(false);
        let input = keys::to_pty_with_mode(normalized_key, ctrl, shift, alt, app_cursor);

        // Scroll to bottom on input so user always sees what they type
        if let Some(terminal) = self.terminal_manager_mut().active_terminal() {
            terminal.scroll_to_bottom();
            terminal.send_input(&input);
        }
        
        // Don't request re-render here - the 60fps polling loop will trigger re-render when PTY output arrives
    }
}

/// Extract the command portion from a terminal line, stripping the shell prompt.
/// Handles: "PS C:\path> cmd", "user@host:~$ cmd", "% cmd", "> cmd"
#[cfg(feature = "gpui")]
fn extract_command_after_prompt(line: &str) -> &str {
    // PowerShell: "PS C:\foo> command"
    if let Some(pos) = line.find("> ") {
        let after = &line[pos + 2..];
        // Make sure it's actually a prompt (has PS prefix or short prefix)
        if line[..pos].contains("PS ") || line[..pos].contains("❯") || pos < 80 {
            return after;
        }
    }
    // Bash/Zsh: "user@host:~/dir$ command"
    if let Some(pos) = line.find("$ ") {
        return &line[pos + 2..];
    }
    // Zsh: "% command"
    if let Some(pos) = line.find("% ") {
        if pos < 5 {
            return &line[pos + 2..];
        }
    }
    // Fallback: if line starts with "amux ", treat entire line as command
    if line.trim_start().starts_with("amux ") {
        return line.trim_start();
    }
    line
}

/// Extract the working directory from a terminal prompt line.
/// This is a pure function that operates on a string — no terminal access needed.
///
/// Supports:
///   PowerShell:  "PS C:\Users\foo\project> amux preview"  → "C:\Users\foo\project"
///   Bash/Zsh:    "user@host:~/project$ amux preview"      → "/home/user/project"
///   Zsh:         "~/project% amux preview"                 → "/home/user/project"
#[cfg(feature = "gpui")]
fn extract_cwd_from_prompt_line(line: &str) -> Option<String> {
    // PowerShell: "PS C:\path> ..." or "PS D:\path>"
    if let Some(ps_start) = line.find("PS ") {
        let after_ps = &line[ps_start + 3..];
        if let Some(gt) = after_ps.find('>') {
            let path = after_ps[..gt].trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }

    // Bash/Zsh: "user@host:~/dir$ cmd" or "user@host:/path$" (no command typed)
    if let Some(colon) = line.find(':') {
        if line[..colon].contains('@') {
            let after_colon = &line[colon + 1..];
            // Find $ or % that ends the path — with or without trailing space
            let end = after_colon.find("$ ")
                .or_else(|| after_colon.find("% "))
                .or_else(|| {
                    // No space after $ — prompt with nothing typed, or $ at end of line
                    let trimmed = after_colon.trim_end();
                    if trimmed.ends_with('$') {
                        Some(trimmed.len() - 1)
                    } else if trimmed.ends_with('%') {
                        Some(trimmed.len() - 1)
                    } else {
                        None
                    }
                });
            if let Some(pos) = end {
                let path = after_colon[..pos].trim();
                if !path.is_empty() {
                    return Some(expand_tilde(path));
                }
            }
        }
    }

    // Simple zsh: "~/project% cmd" or "/path%" (no command)
    if let Some(pct) = line.find('%') {
        if pct < 120 {
            let path = line[..pct].trim();
            if !path.is_empty() && (path.starts_with('/') || path.starts_with('~') || path.starts_with('\\')) {
                return Some(expand_tilde(path));
            }
        }
    }

    None
}

#[cfg(feature = "gpui")]
fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Some(home) = std::env::var("HOME").ok()
            .or_else(|| std::env::var("USERPROFILE").ok())
        {
            return path.replacen('~', &home, 1);
        }
    }
    path.to_string()
}

#[cfg(feature = "gpui")]
impl Render for GpuiShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Focus management.
        // When the browser is open, trust GPUI's own focus system:
        // - Input's track_focus + prevent_default handles URL bar focus correctly
        // - Root's track_focus handles terminal focus on clicks outside Input
        // Only use explicit flags for specific transitions (Enter navigate, close browser).
        if self.restore_terminal_focus {
            self.restore_terminal_focus = false;
            self.focus_handle.focus(window, cx);
            // Reclaim OS focus from any active browser WebView2
            if let Some((_, entry)) = self.active_browser_entry() {
                entry.browser.focus_parent();
            }
        } else if self.has_visible_browser() {
            // Browser is open AND visible — do NOT aggressively grab focus.
            // WebView2 is a child HWND that takes OS focus on click, which may
            // cause GPUI to clear its internal focus state. If we force-focus root
            // here every frame, we'd fight WebView2 and break the URL Input.
            // Focus is managed entirely by click events:
            //   - Click terminal  → root's track_focus + focus_parent()
            //   - Click URL Input → Input's track_focus (with prevent_default)
            //   - Click WebView2  → WebView2 gets OS focus, GPUI does nothing
        } else {
            // No browser — safe to ensure terminal always has focus.
            if !self.focus_handle.is_focused(window) {
                self.focus_handle.focus(window, cx);
            }
        }

        // Sync URL bar when navigation changed the page address.
        // Only update when the Input is NOT focused (don't overwrite user's editing).
        if let Some(url) = self.pending_url_bar_update.take() {
            let child_input_focused = self.active_browser_entry()
                .map(|(_, e)| {
                    use gpui::Focusable;
                    e.url_input.read(cx).focus_handle(cx).is_focused(window)
                })
                .unwrap_or(false);
            if child_input_focused {
                self.pending_url_bar_update = Some(url);
            } else if let Some((_, entry)) = self.active_browser_entry() {
                let input = entry.url_input.clone();
                input.update(cx, |state, cx| {
                    state.set_value(url, window, cx);
                });
            }
        }

        // Cache native window handle on first render (needed for WebView2 creation later)
        if self.cached_window_handle.is_none() {
            use raw_window_handle::HasWindowHandle;
            if let Ok(handle) = window.window_handle() {
                self.cached_window_handle = Some(handle.as_raw());
            }
        }

        // Browser bounds sync is done in the 60fps timer, not here in render,
        // to avoid timing issues with canvas prepaint.

        let sidebar_visible = !self.sidebar_state.collapsed;
        let workspaces = self.model.workspace_items.clone();

        // Measure font metrics on first render
        let metrics = self.cell_metrics.get_or_insert_with(|| {
            crate::gpui_terminal::measure_cell_metrics(window, &self.config.font_family, self.config.font_size, self.config.line_height)
        }).clone();
        let cell_w = metrics.width.max(1.0);  // guard against zero
        let cell_h = metrics.height.max(1.0);

        // Resize terminals — skip during drag to avoid content loss
        if self.resize_drag.is_none() && self.sidebar_drag_start.is_none() && self.preview_drag_start.is_none() && self.browser_drag_start.is_none() {
            let sidebar_w = self.sidebar_width();
            let vp = window.viewport_size();
            let content_w = vp.width.as_f32() - sidebar_w;
            let status_bar_h = 28.0_f32;
            let content_h = vp.height.as_f32() - status_bar_h;
            if let Some(zpid) = self.zoomed_pane.clone() {
                // Zoom mode: give the zoomed pane the full content area
                self.terminal_manager_mut().resize_pane_terminals(
                    &zpid, content_w, content_h, cell_w, cell_h,
                );
            } else {
                self.terminal_manager_mut().resize_all_panes(
                    content_w, content_h, cell_w, cell_h,
                );
            }
        }


        
        // Always register our IME input handler. When the browser URL Input has
        // focus, our handler detects this and returns early (letting the platform
        // deliver text to the focused Input's own handler via its paint-phase
        // handle_input call which runs AFTER ours and takes precedence).
        let view_entity = cx.entity().clone();
        let focus_for_ime = self.focus_handle.clone();

        // Main layout - limux/mori style dark theme
        div()
            .track_focus(&self.focus_handle)
            .child(gpui::canvas(
                move |bounds, _window, _cx| bounds,
                move |bounds, _, window, cx| {
                    window.handle_input(
                        &focus_for_ime,
                        gpui::ElementInputHandler::new(bounds, view_entity),
                        cx,
                    );
                },
            ).w(px(1.0)).h(px(1.0)).absolute().left(px(-10.0)).top(px(-10.0)))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1d1f21))
            .text_color(rgb(0xffffff))
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.on_global_key_down(event, window, cx);
            }))
            // Mouse: left button down — forward to PTY or start selection
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                if this.resize_drag.is_some() {
                    return;
                }
                // Ignore clicks in the sidebar region — those are handled by
                // workspace/tab click handlers, not terminal selection.
                // MUST check before clearing rename state, otherwise double-click
                // rename on a workspace gets set by the workspace handler then
                // immediately cleared here via event bubbling.
                let sidebar_w = this.sidebar_width();
                if event.position.x.as_f32() < sidebar_w {
                    return;
                }
                // If any browser tab exists, reclaim OS focus from WebView2 on every
                // click in the GPUI area (terminal, URL bar, etc.). WebView2 is a
                // child HWND that steals OS keyboard focus; this ensures GPUI gets
                // keyboard events after clicking anywhere in our window.
                for entry in this.browser_tabs.values() {
                    if entry.browser.is_initialized() {
                        entry.browser.focus_parent();
                        break; // one call is enough
                    }
                }
                // Click outside sidebar: dismiss any active rename/search
                if this.renaming_workspace.is_some() {
                    this.renaming_workspace = None;
                    cx.notify();
                }
                if this.renaming_tab.is_some() {
                    this.renaming_tab = None;
                    cx.notify();
                }
                let (mouse_mode, _) = this.active_term_mouse_mode();
                let (col, row) = this.pixel_to_term_cell(event.position);

                // Ctrl+Click: try to preview file path under cursor.
                // Always takes priority, even when mouse mode is on (e.g. Claude Code).
                if event.modifiers.control {
                    if this.try_preview_path_at(col, row) {
                        cx.notify();
                        return;
                    }
                }

                if mouse_mode {
                    this.send_mouse_event(0, col, row, true);
                } else {
                    use alacritty_terminal::index::{Column, Line, Point as AlacPoint, Direction};
                    use alacritty_terminal::selection::{Selection, SelectionType};
                    let point = AlacPoint::new(Line(row as i32), Column(col));
                    let clicks = event.click_count;
                    let sel_type = if clicks >= 3 {
                        SelectionType::Lines
                    } else if clicks == 2 {
                        SelectionType::Semantic
                    } else {
                        SelectionType::Simple
                    };
                    let side = Direction::Left;
                    if let Some(term) = this.terminal_manager_mut().active_terminal() {
                        term.with_term_mut(|t| {
                            t.selection = Some(Selection::new(sel_type, point, side));
                        });
                    }
                    this.selecting = true;
                }
                cx.notify();
            }))
            // Mouse: move — forward to PTY or extend selection
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _window, cx| {
                // Handle sidebar resize drag
                if let Some((start_x, start_w)) = this.sidebar_drag_start {
                    let delta = event.position.x.as_f32() - start_x;
                    this.sidebar_state.width = (start_w + delta).clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX);
                    cx.notify();
                    return;
                }
                // Handle preview panel resize drag (drag left = wider, drag right = narrower)
                if let Some((start_x, start_w)) = this.preview_drag_start {
                    let delta = start_x - event.position.x.as_f32();
                    if let Some(ref mut state) = this.preview_state {
                        state.width = (start_w + delta).clamp(250.0, 900.0);
                    }
                    cx.notify();
                    return;
                }
                // (Browser panel resize drag removed — browser is now a pane tab)
                // Handle split resize drag
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
                let (mouse_mode, _) = this.active_term_mouse_mode();
                if mouse_mode && event.pressed_button == Some(gpui::MouseButton::Left) {
                    let (col, row) = this.pixel_to_term_cell(event.position);
                    this.send_mouse_event(32, col, row, true);
                } else if this.selecting {
                    // Extend selection
                    use alacritty_terminal::index::{Column, Line, Point as AlacPoint, Direction};
                    let (col, row) = this.pixel_to_term_cell(event.position);
                    let point = AlacPoint::new(Line(row as i32), Column(col));
                    let side = Direction::Right;
                    if let Some(term) = this.terminal_manager_mut().active_terminal() {
                        term.with_term_mut(|t| {
                            if let Some(ref mut sel) = t.selection {
                                sel.update(point, side);
                            }
                        });
                    }
                }
                cx.notify();
            }))
            // Mouse: left button up — forward to PTY or finalize selection + auto-copy
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseUpEvent, _window, cx| {
                let (mouse_mode, _) = this.active_term_mouse_mode();
                if mouse_mode {
                    let (col, row) = this.pixel_to_term_cell(event.position);
                    this.send_mouse_event(0, col, row, false);
                } else if this.selecting {
                    // Copy selected text to clipboard
                    if let Some(term) = this.terminal_manager_mut().active_terminal() {
                        let text = term.with_term(|t| t.selection_to_string());
                        if let Some(text) = text {
                            if !text.is_empty() {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                            }
                        }
                    }
                }
                this.selecting = false;
                this.resize_drag = None;
                this.sidebar_drag_start = None;
                this.preview_drag_start = None;
                this.browser_drag_start = None;
                cx.notify();
            }))
            // Mouse: right button up — forward release to PTY
            .on_mouse_up(gpui::MouseButton::Right, cx.listener(|this, event: &gpui::MouseUpEvent, _window, _cx| {
                let (mouse_mode, _) = this.active_term_mouse_mode();
                if mouse_mode {
                    let (col, row) = this.pixel_to_term_cell(event.position);
                    this.send_mouse_event(2, col, row, false);
                }
            }))
            // Mouse: middle click — paste clipboard
            .on_mouse_down(gpui::MouseButton::Middle, cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.paste_clipboard(cx);
            }))
            // Mouse wheel: scroll terminal or forward to PTY
            //
            // When an app enables mouse mode (Claude Code, vim, fzf), it expects
            // to receive scroll events so it can handle scrolling internally.
            // This matches Alacritty/kitty/WezTerm behavior: mouse mode → app
            // gets the events. Shift+scroll bypasses mouse mode to scroll our
            // scrollback buffer (for apps in primary screen with history).
            //
            // For alt screen apps without mouse mode (less with ALTERNATE_SCROLL),
            // convert scroll to arrow keys.
            .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _window, cx| {
                // GPUI on Windows: pt.y > 0 when user scrolls UP (wheel away).
                // We keep this sign: positive = scroll up = see earlier content.
                let lines = match event.delta {
                    gpui::ScrollDelta::Lines(pt) => pt.y,
                    gpui::ScrollDelta::Pixels(pt) => pt.y.as_f32() / this.cell_dims().1,
                };
                if lines == 0.0 { return; }

                let (mouse_mode, _sgr) = this.active_term_mouse_mode();
                let alt_scroll = this.active_term_alt_screen_scroll();
                let shift = event.modifiers.shift;

                if mouse_mode && !shift {
                    // Mouse mode ON: forward scroll events to the app.
                    // Apps like Claude Code handle their own scrolling.
                    let (col, row) = this.pixel_to_term_cell(event.position);
                    let count = lines.abs().ceil().max(1.0) as usize;
                    let button: u8 = if lines > 0.0 { 64 } else { 65 };
                    for _ in 0..count {
                        this.send_mouse_event(button, col, row, true);
                    }
                } else if alt_scroll && !mouse_mode && !shift {
                    // Alt screen + ALTERNATE_SCROLL, no mouse mode: send arrow keys.
                    // (e.g. `less` without mouse mode)
                    let count = lines.abs().ceil().max(1.0) as usize;
                    let arrow: &[u8] = if lines > 0.0 { b"\x1b[A" } else { b"\x1b[B" };
                    if let Some(term) = this.terminal_manager().active_terminal_ref() {
                        for _ in 0..count {
                            term.send_input(arrow);
                        }
                    }
                } else if let Some(term) = this.terminal_manager_mut().active_terminal() {
                    // No mouse mode (or Shift held): scroll scrollback buffer
                    if lines > 0.0 {
                        term.scroll_up(lines.ceil() as usize);
                    } else {
                        term.scroll_down((-lines).ceil() as usize);
                    }
                }
                cx.notify();
            }))
            // Right-click: forward to PTY if mouse mode, else show context menu
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                let (mouse_mode, _) = this.active_term_mouse_mode();
                if mouse_mode {
                    let (col, row) = this.pixel_to_term_cell(event.position);
                    this.send_mouse_event(2, col, row, true); // button 2 = right press
                } else {
                    this.context_menu = Some(ContextMenuState {
                        position: event.position,
                    });
                }
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
                            let sw = self.sidebar_state.width;
                            div()
                                .id("sidebar-expanded")
                                .w(px(sw))
                                .bg(rgb(0x181818))
                                .flex()
                                .flex_row()
                                .overflow_hidden()
                                // Sidebar content column
                                .child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .overflow_hidden()
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
                                                .text_color(rgb(0x969896))
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
                                                .text_color(rgb(0x969896))
                                                .hover(|d| d.bg(rgb(0x282a2e)).text_color(rgb(0xc5c8c6)))
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
                                        .children(workspaces.iter().enumerate().map(|(ws_idx, item)| {
                                            let is_active = item.is_active;
                                            let has_ws_activity = !is_active && self.workspace_terminals
                                                .get(&item.id)
                                                .map(|tm| tm.has_any_activity())
                                                .unwrap_or(false);
                                            let bg_color = if is_active { rgb(0x252530) } else { rgb(0x181818) };
                                            let text_color = if is_active { rgb(0xc5c8c6) } else { rgb(0x7f849c) };
                                            let ws_id = item.id.clone();
                                            let ws_id_dbl = item.id.clone();
                                            let ws_id_drop = item.id.clone();
                                            let ws_name = item.name.clone();
                                            let drag_name = item.name.clone();
                                            let drag_id = item.id.clone();
                                            let ws_id_del = item.id.clone();
                                            let can_delete = workspaces.len() > 1;
                                            let is_renaming = self.renaming_workspace.as_ref()
                                                .map(|(id, _)| id == &item.id)
                                                .unwrap_or(false);

                                            div()
                                                .id(gpui::ElementId::Name(format!("ws-{}", item.id).into()))
                                                .group(format!("ws-group-{}", item.id))
                                                .flex()
                                                .items_center()
                                                .px_3()
                                                .py(px(6.0))
                                                .mx_1()
                                                .my_px()
                                                .rounded(px(4.0))
                                                .bg(bg_color)
                                                .cursor_grab()
                                                .hover(|d| d.bg(rgb(0x252530)))
                                                .when(is_active, |d| d.border_l_2().border_color(rgb(0x81a2be)))
                                                // Drag to reorder
                                                .on_drag(
                                                    DragWorkspace { workspace_id: drag_id, name: drag_name, index: ws_idx },
                                                    |drag, _, _, cx| {
                                                        cx.stop_propagation();
                                                        cx.new(|_| drag.clone())
                                                    },
                                                )
                                                .drag_over::<DragWorkspace>(|style, _, _, _| {
                                                    style.bg(rgb(0x282a2e)).border_t_2().border_color(rgb(0x81a2be))
                                                })
                                                .on_drop(cx.listener(move |this, drag: &DragWorkspace, _window, cx| {
                                                    this.reorder_workspace(drag.index, &ws_id_drop);
                                                    cx.notify();
                                                }))
                                                // Double-click: rename; single-click: switch workspace.
                                                // Use on_mouse_down so double-click is detected BEFORE
                                                // re-render invalidates the element's click tracking.
                                                .on_mouse_down(gpui::MouseButton::Left, cx.listener(
                                                    move |this, event: &gpui::MouseDownEvent, _window, cx| {
                                                        if event.click_count >= 2 {
                                                            this.renaming_workspace = Some((ws_id_dbl.clone(), ws_name.clone()));
                                                            cx.notify();
                                                        } else if this.renaming_workspace.is_none() {
                                                            let _ = this.app.activate_workspace(&ws_id);
                                                            this.switch_workspace_terminal(&ws_id);
                                                            this.refresh_model();
                                                            cx.notify();
                                                        }
                                                    }
                                                ))
                                                .child(if is_renaming {
                                                    // Inline rename input
                                                    let rename_text = self.renaming_workspace.as_ref()
                                                        .map(|(_, t)| t.clone())
                                                        .unwrap_or_default();
                                                    div()
                                                        .text_sm()
                                                        .text_color(rgb(0xc5c8c6))
                                                        .px_1()
                                                        .bg(rgb(0x282a2e))
                                                        .rounded(px(2.0))
                                                        .border_1()
                                                        .border_color(rgb(0x81a2be))
                                                        .child(if rename_text.is_empty() { "▎".to_string() } else { format!("{}▎", rename_text) })
                                                        .into_any_element()
                                                } else {
                                                    let group_name = format!("ws-group-{}", item.id);
                                                    div()
                                                        .flex()
                                                        .flex_row()
                                                        .items_center()
                                                        .gap(px(6.0))
                                                        .flex_1()
                                                        // Activity dot for inactive workspaces
                                                        .when(has_ws_activity, |d| {
                                                            d.child(
                                                                div().w(px(6.0)).h(px(6.0)).rounded(px(3.0))
                                                                    .bg(rgb(0xa6e3a1)).flex_shrink_0()
                                                            )
                                                        })
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .overflow_hidden()
                                                                .whitespace_nowrap()
                                                                .text_sm()
                                                                .text_color(text_color)
                                                                .when(is_active, |d| d.font_weight(FontWeight::MEDIUM))
                                                                .child(item.name.clone())
                                                        )
                                                        // Delete button: only visible on hover, hidden if last workspace
                                                        .when(can_delete, |d| {
                                                            d.child(
                                                                div()
                                                                    .id(gpui::ElementId::Name(format!("ws-del-{}", ws_id_del).into()))
                                                                    .px(px(3.0))
                                                                    .rounded(px(3.0))
                                                                    .text_xs()
                                                                    .text_color(rgb(0x181818)) // invisible by default
                                                                    .group_hover(&group_name, |d| {
                                                                        d.text_color(rgb(0x969896))
                                                                    })
                                                                    .hover(|d| d.bg(rgb(0x45475a)).text_color(rgb(0xf38ba8)))
                                                                    .child("✕")
                                                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                                                        let _ = this.app.run_command(&format!("workspace close {}", ws_id_del));
                                                                        // Remove terminal manager for deleted workspace
                                                                        this.workspace_terminals.remove(&ws_id_del);
                                                                        this.workspace_order.retain(|id| id != &ws_id_del);
                                                                        this.refresh_model();
                                                                        // Switch to another workspace if we deleted the active one
                                                                        if this.active_workspace_id == ws_id_del {
                                                                            if let Some(first) = this.model.workspace_items.first() {
                                                                                let new_id = first.id.clone();
                                                                                this.switch_workspace_terminal(&new_id);
                                                                            }
                                                                        }
                                                                        cx.notify();
                                                                    }))
                                                            )
                                                        })
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
                                        .text_color(rgb(0x969896))
                                        .cursor_pointer()
                                        .hover(|d| d.bg(rgb(0x252530)).text_color(rgb(0xc5c8c6)))
                                        .child("+  New Workspace")
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            let cwd = std::env::current_dir().unwrap_or_default();
                                            let _ = this.app.dispatch(
                                                amux_ui::UiAction::OpenWindowsWorkspace(cwd)
                                            );
                                            this.refresh_model();
                                            // Create terminal for the new workspace and switch to it
                                            if let Some(new_ws) = this.model.workspace_items.iter().find(|w| w.is_active) {
                                                this.switch_workspace_terminal(&new_ws.id.clone());
                                            }
                                            cx.notify();
                                        })),
                                )
                                ) // end sidebar content column
                                // Resize handle (right edge)
                                .child(
                                    div()
                                        .id("sidebar-resize-handle")
                                        .group("sidebar-handle")
                                        .w(px(4.0))
                                        .h_full()
                                        .flex_shrink_0()
                                        .cursor_col_resize()
                                        .child(
                                            div()
                                                .w(px(1.0))
                                                .h_full()
                                                .bg(rgb(0x2a2a2a))
                                                .group_hover("sidebar-handle", |d| d.w(px(2.0)).bg(rgb(0x81a2be)))
                                        )
                                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, _w, _cx| {
                                            this.sidebar_drag_start = Some(
                                                (event.position.x.as_f32(), this.sidebar_state.width)
                                            );
                                        }))
                                )
                        } else {
                            // Collapsed sidebar: narrow strip with expand button
                            div()
                                .id("sidebar-expand")
                                .w(px(SIDEBAR_WIDTH_COLLAPSED))
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
                                        .text_color(rgb(0x969896))
                                        .hover(|d| d.bg(rgb(0x282a2e)).text_color(rgb(0xc5c8c6)))
                                        .child("▶")
                                        .on_click(cx.listener(|this, _e, _w, cx| {
                                            this.sidebar_state.collapsed = false;
                                            cx.notify();
                                        })),
                                )
                        }
                    })
                    // Main content area (terminal + optional preview panel)
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_row()
                            .overflow_hidden()
                            // Terminal column
                            .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            // Terminal pane(s) — renders split layout recursively
                            .child({
                                let active_pane_id = self.terminal_manager_mut().active_pane_id().cloned();
                                let sidebar_w = self.sidebar_width();
                                let vp = window.viewport_size();
                                let content_w = vp.width.as_f32() - sidebar_w;
                                let status_bar_h = 28.0_f32;
                                let content_h = vp.height.as_f32() - status_bar_h;
                                // Cursor blinks: visible for 30 frames, hidden for 30 frames (~500ms each at 60fps)
                                let cursor_blink_on = (self.cursor_blink_frame % 60) < 30;
                                // Compute pane bounds for mouse hit-testing
                                self.pane_bounds.clear();
                                let origin_x = sidebar_w;
                                let origin_y = 0.0_f32;
                                // Clone layout + refs before passing pane_bounds mutably
                                let zoomed = self.zoomed_pane.clone();
                                let layout_cloned = self.terminal_manager_mut().active_layout().cloned();
                                let renaming_tab = self.renaming_tab.clone();
                                // Get the manager pointer before the mutable borrow of pane_bounds.
                                // SAFETY: pane_bounds and workspace_terminals are disjoint fields.
                                let pb = &mut self.pane_bounds as *mut std::collections::HashMap<String, (f32, f32, f32, f32)>;
                                // Zoom mode: render only the zoomed pane at full size
                                if let Some(zpid) = zoomed {
                                    let single = amux_platform::terminal::manager::PaneLayout::Single(zpid.clone());
                                    render_layout(&single, self.terminal_manager(), Some(&zpid), content_w, content_h, cursor_blink_on, &metrics, true, &renaming_tab, origin_x, origin_y, unsafe { &mut *pb }, &self.config.font_family, self.config.font_size, &self.terminal_theme, &self.browser_tabs, &self.preview_tabs, cx)
                                } else if let Some(layout) = layout_cloned {
                                    render_layout(&layout, self.terminal_manager(), active_pane_id.as_ref(), content_w, content_h, cursor_blink_on, &metrics, false, &renaming_tab, origin_x, origin_y, unsafe { &mut *pb }, &self.config.font_family, self.config.font_size, &self.terminal_theme, &self.browser_tabs, &self.preview_tabs, cx)
                                } else {
                                    div().flex_1().bg(rgb(0x1d1f21)).child("No terminal").into_any_element()
                                }
                            })
                            ) // end terminal column
                            // (Preview is now rendered inside pane tabs, not as a separate column)
                            // (Browser is now rendered inside pane tabs, not as a separate column)
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
                agents: self.terminal_manager().agent_summaries()
                    .into_iter()
                    .map(|(name, icon, color)| AgentSummary {
                        name,
                        status_icon: icon,
                        color,
                    })
                    .collect(),
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
            // Search bar overlay (top-right)
            .when_some(self.search_state.clone(), |this, (query, _idx)| {
                this.child(
                    div()
                        .absolute()
                        .top(px(4.0))
                        .right(px(16.0))
                        .w(px(320.0))
                        .px_3()
                        .py(px(6.0))
                        .rounded(px(8.0))
                        .bg(rgb(0x1d1f21))
                        .border_1()
                        .border_color(rgb(0x45475a))
                        .shadow_lg()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div().text_xs().text_color(rgb(0x969896)).child("Find:")
                        )
                        .child(
                            div()
                                .flex_1()
                                .px_2()
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(rgb(0x11111b))
                                .border_1()
                                .border_color(rgb(0x282a2e))
                                .text_sm()
                                .text_color(rgb(0xc5c8c6))
                                .min_h(px(20.0))
                                .child(if query.is_empty() {
                                    div().text_color(rgb(0x969896)).child("Type to search...").into_any_element()
                                } else {
                                    div().child(format!("{}▎", query)).into_any_element()
                                })
                        )
                        .child(
                            div().text_xs().text_color(rgb(0x969896)).child("Enter/Shift+Enter  Esc close")
                        )
                )
            })
            // IME preedit overlay (near cursor)
            .when_some(self.ime_preedit.clone(), |this, preedit| {
                // Position near active terminal cursor
                let pos = self.cell_metrics.as_ref().and_then(|m| {
                    let pid = self.terminal_manager().active_pane_id()?;
                    let (ox, oy, _, _) = self.pane_bounds.get(&pid.0)?;
                    let (col, row) = self.terminal_manager().active_terminal_ref()
                        .map(|t| t.with_term(|term| {
                            let c = term.renderable_content().cursor;
                            (c.point.column.0, c.point.line.0.max(0) as usize)
                        }))?;
                    Some((ox + col as f32 * m.width, oy + row as f32 * m.height + m.height))
                });
                if let Some((x, y)) = pos {
                    this.child(
                        div()
                            .absolute()
                            .left(px(x))
                            .top(px(y))
                            .px_2()
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .bg(rgb(0x282a2e))
                            .border_1()
                            .border_color(rgb(0x81a2be))
                            .text_sm()
                            .text_color(rgb(0xc5c8c6))
                            .child(preedit)
                    )
                } else {
                    this
                }
            })
            // File picker overlay (Ctrl+P)
            .when_some(self.file_picker.clone(), |this, picker| {
                this.child(crate::gpui_preview::render_file_picker(&picker, cx))
            })
            // Agent picker overlay (Launch Agent)
            .when_some(self.agent_picker.clone(), |this, picker| {
                this.child(render_agent_picker(&picker, cx))
            })
            // Template picker overlay (Apply Layout)
            .when_some(self.template_picker.clone(), |this, picker| {
                this.child(render_template_picker(&picker, cx))
            })
            // Pane picker overlay (Send to Pane)
            .when_some(self.pane_picker.clone(), |this, picker| {
                this.child(render_pane_picker(&picker, cx))
            })
            // Agent toast notifications (bottom-right)
            .when(!self.toasts.is_empty(), |this| {
                let toast_els: Vec<_> = self.toasts.iter().enumerate().map(|(i, t)| {
                    let pane_id = t.pane_id.clone();
                    let tab_idx = t.tab_index;
                    div()
                        .id(gpui::ElementId::Name(format!("toast-{}", i).into()))
                        .px_3()
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .bg(rgb(0x1d1f21))
                        .border_1()
                        .border_color(rgb(t.color))
                        .shadow_lg()
                        .text_xs()
                        .text_color(rgb(t.color))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(0x252530)))
                        .child(t.message.clone())
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.terminal_manager_mut().set_active_pane(&pane_id);
                            this.terminal_manager_mut().set_active_tab_in_pane(tab_idx);
                            // Dismiss all toasts on click
                            this.toasts.clear();
                            cx.notify();
                        }))
                        .into_any_element()
                }).collect();
                this.child(
                    div()
                        .absolute()
                        .bottom(px(36.0))
                        .right(px(16.0))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(toast_els)
                )
            })
    }
}


// NOTE: render_context_menu, first_pane_in_layout, render_layout
// have been moved to gpui_layout_renderer.rs
#[cfg(feature = "gpui")]
pub fn run(app: &amux_ui::DesktopApp, config: crate::gpui_config::AmuxConfig) {
    use amux_ui::GpuiRenderer;
    use smol::Timer;

    // Required for WebView2 to render correctly inside GPUI's DirectComposition window.
    #[cfg(target_os = "windows")]
    unsafe { std::env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "true"); }

    let mut app = app.clone();
    let model = app.render_with(&GpuiRenderer);

    application().run(move |cx: &mut App| {
        // Initialize gpui-component (registers Input keybindings, theme, etc.)
        gpui_component::init(cx);
        // Set dark theme to match Amux's Tomorrow Night palette
        gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);

        let model = model.clone();
        let app = app.clone();
        let config = config.clone();
        
        let window_opts = WindowOptions {
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("AMUX".into()),
                appears_transparent: false,
                ..Default::default()
            }),
            app_id: Some("amux".to_string()),
            window_min_size: Some(gpui::Size { width: px(480.0), height: px(320.0) }),
            ..Default::default()
        };
        let window_result = cx.open_window(window_opts, |window, cx| {
            let view = cx.new(|cx| {
                // Start a ~60fps polling timer to drain PTY output into the emulator
                cx.spawn(async move |this, cx| {
                    loop {
                        Timer::after(std::time::Duration::from_millis(16)).await;
                        let result = this.update(cx, |this: &mut GpuiShellView, cx: &mut Context<GpuiShellView>| {
                            let has_drag = this.resize_drag.is_some();
                            // Cursor blink: toggle every ~30 frames (500ms at 60fps)
                            this.cursor_blink_frame = this.cursor_blink_frame.wrapping_add(1);
                            let cursor_blink_toggle = this.cursor_blink_frame % 30 == 0;

                            // Check if any terminal has new output (dirty flag from PTY wakeup)
                            let mut any_dirty = false;
                            'outer: for tm in this.workspace_terminals.values() {
                                for term in tm.all_terminals() {
                                    if term.take_dirty() {
                                        any_dirty = true;
                                        break 'outer;
                                    }
                                }
                            }

                            // Determine which browser_id (if any) should be visible:
                            // only the active tab in the active pane.
                            let visible_browser_id: Option<u64> = this.active_browser_entry().map(|(bid, _)| bid);

                            // Sync browser WebView2 bounds, visibility, and pending navigations.
                            for (&bid, entry) in this.browser_tabs.iter_mut() {
                                let should_show = visible_browser_id == Some(bid);
                                if should_show {
                                    if let Some(bounds) = entry.bounds_cell.get() {
                                        entry.browser.sync_bounds(bounds);
                                    }
                                    if !entry.browser.is_visible() {
                                        entry.browser.show();
                                    }
                                } else if entry.browser.is_visible() {
                                    entry.browser.hide();
                                }
                                entry.browser.process_pending_nav();
                                if let Some(url) = entry.browser.take_current_url() {
                                    this.pending_url_bar_update = Some(url);
                                    cx.notify();
                                }
                            }

                            // Only re-render when needed: new output, cursor blink, or drag
                            if any_dirty || cursor_blink_toggle || has_drag || this.selecting {
                                cx.notify();
                            }
                            // Deferred startup: spawn PTY processes on first frame
                            // Only spawn the active workspace's terminals for fast startup.
                            // Other workspaces spawn on first switch (ensure_workspace_terminal).
                            if !this.terminals_spawned {
                                this.terminals_spawned = true;
                                let (shell, args) = GpuiShellView::default_shell();
                                let default_cwd = GpuiShellView::default_cwd();
                                let active_ws = this.active_workspace_id.clone();
                                if let Some(tm) = this.workspace_terminals.get_mut(&active_ws) {
                                    let pane_ids: Vec<_> = tm.active_layout()
                                        .map(|l| l.pane_ids()).unwrap_or_default();
                                    for pid in pane_ids {
                                        tm.spawn_all_tabs_in_pane(&pid, &shell, &args, default_cwd.as_deref());
                                    }
                                }
                                cx.notify();
                            }
                            // Deferred tool detection: launch in background thread on third frame
                            if !this.tools_detected && this.cursor_blink_frame >= 3 {
                                this.tools_detected = true;
                                // Spawn detection in background so it doesn't block rendering
                                cx.spawn(async move |this, cx| {
                                    // Run blocking detection on a background thread
                                    let (tools, wsl) = smol::unblock(|| {
                                        let tools = GpuiShellView::detect_all_vibe_tools();
                                        let wsl = GpuiShellView::wsl_available();
                                        (tools, wsl)
                                    }).await;
                                    let _ = this.update(cx, |view: &mut GpuiShellView, _cx| {
                                        view.detected_vibe_tools = tools;
                                        view.wsl_detected = wsl;
                                    });
                                }).detach();
                            }
                            // Poll terminal activity for all workspaces (~15fps)
                            if this.cursor_blink_frame % 4 == 0 {
                                let frame = this.cursor_blink_frame;
                                for tm in this.workspace_terminals.values_mut() {
                                    let notifs = tm.poll_activity();
                                    for n in notifs {
                                        let msg = format!("{} {} — {}",
                                            n.new_status.icon(),
                                            n.tab_title,
                                            n.new_status.label(),
                                        );
                                        this.toasts.push(ToastNotification {
                                            message: msg,
                                            color: n.new_status.color_rgb(),
                                            frame_created: frame,
                                            pane_id: n.pane_id,
                                            tab_index: n.tab_index,
                                        });
                                    }
                                }
                                // Clear activity for the active tab since user is looking at it
                                this.terminal_manager_mut().clear_active_activity();
                                // Expire old toasts (after ~3 seconds = 180 frames at 60fps)
                                this.toasts.retain(|t| {
                                    frame.wrapping_sub(t.frame_created) < 180
                                });
                            }
                            // Auto-save layouts every ~5 seconds (300 frames at 60fps)
                            if this.cursor_blink_frame % 300 == 0 {
                                this.save_all_layouts();
                            }
                        });
                        if result.is_err() {
                            break;
                        }
                    }
                })
                .detach();

                GpuiShellView::new(app, model, config, cx)
            });
            // Wrap in gpui-component Root (required for Input component)
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        });
        
        match window_result {
            Ok(_) => {
                cx.activate(true);
            }
            Err(_) => {}
        }
    });
}

#[cfg(not(feature = "gpui"))]
pub fn run(_: &amux_ui::DesktopApp, _config: crate::gpui_config::AmuxConfig) {}
