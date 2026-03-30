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
use crate::gpui_status_bar::{render_status_bar, StatusBarData};
#[cfg(feature = "gpui")]
use crate::gpui_workspace_sidebar::WorkspaceSidebarState;


#[cfg(feature = "gpui")]
const SIDEBAR_WIDTH_EXPANDED: f32 = 220.0;
#[cfg(feature = "gpui")]
const SIDEBAR_WIDTH_COLLAPSED: f32 = 28.0;

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
    /// Cell dimensions measured from actual font metrics (None = not yet measured)
    cell_metrics: Option<crate::gpui_terminal::CellMetrics>,
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
    /// Tab rename state: (pane_id, tab_index, current_text)
    renaming_tab: Option<(String, usize, String)>,
    /// Terminal search state: (query_text, match_index)
    search_state: Option<(String, usize)>,
    /// Cached Vibe Coding tool availability: vec of (tool_id, label, env)
    detected_vibe_tools: Vec<(&'static str, &'static str, &'static str)>,
    /// Whether WSL is available (cached at startup, Windows only)
    wsl_detected: bool,
    /// Whether PTY processes have been spawned (deferred from constructor)
    terminals_spawned: bool,
    /// Whether startup tool detection has completed
    tools_detected: bool,
    /// Zoomed pane: when set, only this pane is rendered at full size
    zoomed_pane: Option<amux_platform::terminal::manager::PaneId>,
    /// Custom workspace display order (vec of workspace IDs).
    /// Applied after every model refresh to preserve user's drag ordering.
    workspace_order: Vec<String>,
    /// Per-pane screen bounds: (x, y, w, h) in window pixels.
    /// Computed during render_layout, used by mouse handlers for hit-testing.
    pane_bounds: std::collections::HashMap<String, (f32, f32, f32, f32)>,
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

/// Drag data for tab drag-and-drop between panes
#[cfg(feature = "gpui")]
#[derive(Clone)]
struct DragTab {
    source_pane: amux_platform::terminal::manager::PaneId,
    tab_index: usize,
    title: String,
}

#[cfg(feature = "gpui")]
impl Render for DragTab {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py(px(4.0))
            .bg(rgb(0x313244))
            .border_1()
            .border_color(rgb(0x585b70))
            .rounded(px(4.0))
            .text_xs()
            .text_color(rgb(0xcdd6f4))
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
            .border_color(rgb(0x585b70))
            .rounded(px(4.0))
            .text_sm()
            .text_color(rgb(0xcdd6f4))
            .shadow_md()
            .child(self.name.clone())
    }
}

/// Context menu item definition
#[cfg(feature = "gpui")]
#[derive(Clone)]
struct ContextMenuItem {
    label: &'static str,
    shortcut: Option<&'static str>,
    icon: Option<&'static str>,
    enabled: bool,
    separator_after: bool,
}

#[cfg(feature = "gpui")]
impl ContextMenuItem {
    fn action(label: &'static str, shortcut: Option<&'static str>, enabled: bool) -> Self {
        Self { label, shortcut, icon: None, enabled, separator_after: false }
    }
    fn with_icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }
    fn separator(mut self) -> Self {
        self.separator_after = true;
        self
    }
    /// Create a section header (non-clickable label)
    fn header(label: &'static str) -> Self {
        Self { label, shortcut: None, icon: None, enabled: false, separator_after: false }
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

        // Restore layout structures (fast — no PTY spawning yet)
        let mut workspace_terminals = std::collections::HashMap::new();
        let layouts = Self::load_all_layouts();
        for ws in &model.workspace_items {
            let mut tm = if let Some(json) = layouts.get(&ws.id) {
                TerminalManager::restore_layout(json).unwrap_or_else(TerminalManager::new)
            } else {
                TerminalManager::new()
            };
            tm.heal_layout();
            workspace_terminals.insert(ws.id.clone(), tm);
        }
        if !workspace_terminals.contains_key(&active_ws_id) {
            workspace_terminals.insert(active_ws_id.clone(), TerminalManager::new());
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
        }
    }

    /// Detect all Vibe Coding tools once at startup.
    /// On Windows: checks both native PATH and WSL, may add two entries per tool.
    fn detect_all_vibe_tools() -> Vec<(&'static str, &'static str, &'static str)> {
        let tool_ids: &[&str] = &["claude", "opencode", "aider", "codex", "gemini", "copilot"];
        let has_wsl = if cfg!(target_os = "windows") { Self::wsl_available() } else { false };
        let mut results = Vec::new();

        for &tool_id in tool_ids {
            let Some((linux_bin, win_bin, _, _)) = Self::vibe_tool_info(tool_id) else {
                continue;
            };

            // Check native (Windows: where xxx.cmd, Linux: bash -ilc "command -v xxx")
            let native_bin = if cfg!(target_os = "windows") { win_bin } else { linux_bin };
            let found_native = Self::native_has_tool(native_bin);

            // Check WSL (Windows only)
            let found_wsl = if cfg!(target_os = "windows") && has_wsl {
                Self::wsl_has_tool(linux_bin)
            } else {
                false
            };

            // Add native entry
            if found_native {
                let label: &'static str = match tool_id {
                    "claude"   => "Launch Claude",
                    "opencode" => "Launch OpenCode",
                    "aider"    => "Launch Aider",
                    "codex"    => "Launch Codex",
                    "gemini"   => "Launch Gemini",
                    "copilot"  => "Launch Copilot",
                    _ => continue,
                };
                results.push((tool_id, label, "native"));
            }

            // Add WSL entry (even if native also exists — user may prefer WSL)
            if found_wsl {
                let label: &'static str = match tool_id {
                    "claude"   => "Launch Claude (WSL)",
                    "opencode" => "Launch OpenCode (WSL)",
                    "aider"    => "Launch Aider (WSL)",
                    "codex"    => "Launch Codex (WSL)",
                    "gemini"   => "Launch Gemini (WSL)",
                    "copilot"  => "Launch Copilot (WSL)",
                    _ => continue,
                };
                results.push((tool_id, label, "wsl"));
            }
        }
        results
    }

    /// Get cell dimensions (width, height). Falls back to defaults if not yet measured.
    fn cell_dims(&self) -> (f32, f32) {
        match &self.cell_metrics {
            Some(m) => (m.width, m.height),
            None => (8.0, 20.0), // safe fallback before first render
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
        let sidebar_w = if self.sidebar_state.collapsed { SIDEBAR_WIDTH_COLLAPSED } else { SIDEBAR_WIDTH_EXPANDED };
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
        } else {

        }
    }

    /// Get the terminal manager for the active workspace (immutable)
    fn terminal_manager(&self) -> &TerminalManager {
        self.workspace_terminals.get(&self.active_workspace_id)
            .expect("active workspace must have a terminal manager")
    }

    /// Get the terminal manager for the active workspace (mutable).
    /// Auto-creates if missing (defensive against stale workspace IDs).
    fn terminal_manager_mut(&mut self) -> &mut TerminalManager {
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
            let mut tm = TerminalManager::new();
            let (shell, args) = Self::default_shell();
            let cwd = Self::default_cwd();
            let _ = tm.spawn_in_active(&shell, &args, cwd.as_deref());
            self.workspace_terminals.insert(workspace_id.to_string(), tm);
        } else if let Some(tm) = self.workspace_terminals.get_mut(workspace_id) {
            // Heal any layout/pane mismatches and spawn terminals for empty panes
            tm.heal_layout();
            let (shell, args) = Self::default_shell();
            let cwd = Self::default_cwd();
            let pane_ids: Vec<_> = tm.active_layout()
                .map(|l| l.pane_ids()).unwrap_or_default();
            for pid in pane_ids {
                let _ = tm.spawn_in_pane(&pid, &shell, &args, cwd.as_deref());
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
    fn default_shell() -> (String, Vec<String>) {
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

    fn default_cwd() -> Option<String> {
        std::env::current_dir().ok().map(|p| p.to_string_lossy().to_string())
    }

    // ─── WSL-aware tool detection ───────────────────────────────

    /// Create a Command that won't flash a console window on Windows.
    fn silent_command(program: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new(program);
        cmd.stdout(std::process::Stdio::null())
           .stderr(std::process::Stdio::null());
        // On Windows, prevent the subprocess from creating a visible console window
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        cmd
    }

    /// Check if WSL is available (Windows only, always false on other platforms).
    fn wsl_available() -> bool {
        if !cfg!(target_os = "windows") { return false; }
        Self::silent_command("wsl.exe")
            .arg("--status")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check if a binary exists in WSL (Windows only, always false on other platforms).
    fn wsl_has_tool(bin: &str) -> bool {
        if !cfg!(target_os = "windows") { return false; }
        Self::silent_command("wsl.exe")
            .args(["--", "which", bin])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check if a binary exists on the native platform.
    fn native_has_tool(bin: &str) -> bool {
        if cfg!(target_os = "windows") {
            Self::silent_command("where")
                .arg(bin)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        } else {
            let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            Self::silent_command(&sh)
                .args(["-ilc", &format!("command -v {} >/dev/null 2>&1", bin)])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
    }

    /// Detect where a Vibe Coding tool is available.
    /// Returns: "native", "wsl", or "" (not found).
    fn detect_tool_env(bin: &str) -> &'static str {
        // 1. Check native PATH first
        if Self::native_has_tool(bin) {
            return "native";
        }
        // 2. On Windows, also check WSL
        #[cfg(target_os = "windows")]
        if Self::wsl_available() && Self::wsl_has_tool(bin) {
            return "wsl";
        }
        ""
    }

    /// Convert a Windows path to WSL mount path.
    /// e.g. "D:\projects\myapp" → "/mnt/d/projects/myapp"
    fn windows_path_to_wsl(path: &str) -> String {
        // Handle "D:\foo\bar" or "D:/foo/bar"
        let path = path.replace('\\', "/");
        if path.len() >= 2 && path.as_bytes()[1] == b':' {
            let drive = (path.as_bytes()[0] as char).to_ascii_lowercase();
            format!("/mnt/{}{}", drive, &path[2..])
        } else {
            path
        }
    }

    /// Get the ~/.amux base directory
    fn amux_dir() -> std::path::PathBuf {
        let home = if cfg!(target_os = "windows") {
            std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into())
        } else {
            std::env::var("HOME").unwrap_or_else(|_| ".".into())
        };
        std::path::PathBuf::from(home).join(".amux")
    }

    /// Get layout storage path
    fn layout_file_path() -> std::path::PathBuf {
        Self::amux_dir().join("layouts.json")
    }

    /// Get startup file path for a workspace
    fn startup_file_path(workspace_name: &str) -> std::path::PathBuf {
        let safe_name = workspace_name.replace(['/', '\\', ':', ' '], "_");
        Self::amux_dir().join("workspaces").join(format!("{}.startup", safe_name))
    }

    /// Parse a .startup file into pane commands.
    /// Format:
    ///   [pane:1 title=My Title]
    ///   cd /some/dir
    ///   command arg1 arg2
    ///   [pane:2]
    ///   another-command
    /// Returns vec of (pane_number, custom_title, vec_of_commands)
    fn parse_startup_file(path: &std::path::Path) -> Vec<(usize, Option<String>, Vec<String>)> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut result: Vec<(usize, Option<String>, Vec<String>)> = Vec::new();
        let mut current_pane: usize = 1;
        let mut current_title: Option<String> = None;
        let mut current_cmds: Vec<String> = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // Check for [pane:N] or [pane:N title=xxx] header
            if trimmed.starts_with("[pane:") && trimmed.ends_with(']') {
                // Save previous pane
                if !current_cmds.is_empty() {
                    result.push((current_pane, current_title.take(), current_cmds.clone()));
                    current_cmds.clear();
                }
                let inner = &trimmed[6..trimmed.len() - 1]; // "1 title=xxx" or "1"
                if let Some(space_pos) = inner.find(' ') {
                    current_pane = inner[..space_pos].parse().unwrap_or(1);
                    // Parse key=value attributes
                    let attrs = &inner[space_pos + 1..];
                    if let Some(t) = attrs.strip_prefix("title=") {
                        let t = t.trim();
                        if !t.is_empty() {
                            current_title = Some(t.to_string());
                        }
                    }
                } else {
                    current_pane = inner.parse().unwrap_or(1);
                    current_title = None;
                }
            } else {
                current_cmds.push(trimmed.to_string());
            }
        }
        // Save last pane
        if !current_cmds.is_empty() {
            result.push((current_pane, current_title, current_cmds));
        }
        result
    }

    /// Check if workspace is "empty" (single pane, single tab, no splits).
    fn is_workspace_empty(&self) -> bool {
        let mgr = self.terminal_manager();
        mgr.total_panes() == 1 && mgr.total_tabs() <= 1
    }

    /// Execute startup commands for the active workspace.
    /// Creates panes as needed and sends commands to each.
    fn run_startup_commands(&mut self) {
        let ws_name = self.model.active_workspace_name
            .clone()
            .unwrap_or_else(|| self.active_workspace_id.clone());
        let path = Self::startup_file_path(&ws_name);
        let pane_cmds = Self::parse_startup_file(&path);
        if pane_cmds.is_empty() {
            return;
        }

        let (shell, shell_args) = Self::default_shell();
        let cwd = Self::default_cwd();

        for (i, (pane_num, custom_title, cmds)) in pane_cmds.iter().enumerate() {
            // First pane already exists, subsequent panes need split
            if i > 0 {
                let direction = if i % 2 == 1 {
                    SplitDirection::Horizontal
                } else {
                    SplitDirection::Vertical
                };
                self.terminal_manager_mut().split_active_pane(direction);
                self.spawn_terminal_in_active();
            }

            // Send commands to the active terminal
            if let Some(term) = self.terminal_manager_mut().active_terminal() {
                for cmd in cmds {
                    let input = format!("{}\r", cmd);
                    term.send_input(input.as_bytes());
                }
            }

            // Set tab title: custom_title > last command name > pane:N
            let active_id = self.terminal_manager().active_pane_id().cloned();
            if let Some(ref pid) = active_id {
                if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                    if let Some(tab) = pane.tabs.get_mut(pane.active_tab) {
                        let title = if let Some(t) = custom_title {
                            t.clone()
                        } else if let Some(last_cmd) = cmds.last() {
                            last_cmd.split_whitespace().next()
                                .unwrap_or("Terminal").to_string()
                        } else {
                            format!("pane:{}", pane_num)
                        };
                        tab.title = title;
                        tab.custom_title = custom_title.is_some();
                    }
                }
            }
        }

        // Equalize splits after creating all panes
        self.terminal_manager_mut().equalize_splits();
    }

    /// Open the startup file for editing in a new split pane.
    fn edit_startup_file(&mut self) {
        let ws_name = self.model.active_workspace_name
            .clone()
            .unwrap_or_else(|| self.active_workspace_id.clone());
        let path = Self::startup_file_path(&ws_name);

        // Create directory and template file if it doesn't exist
        if !path.exists() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let template = format!(
                "# Startup commands for workspace: {}\n\
                 # Each [pane:N] section creates a new terminal pane.\n\
                 # Use [pane:N title=Name] to set a custom tab title.\n\
                 # Lines are sent as commands to the shell.\n\
                 #\n\
                 # Example:\n\
                 # [pane:1 title=Build]\n\
                 # cd /my/project\n\
                 # cargo watch -x check\n\
                 #\n\
                 # [pane:2 title=AI]\n\
                 # cd /my/project\n\
                 # claude\n",
                ws_name
            );
            let _ = std::fs::write(&path, template);
        }

        if cfg!(target_os = "windows") {
            // Windows: open with GUI editor directly (no terminal pane needed)
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "notepad".to_string());
            let _ = std::process::Command::new(&editor)
                .arg(&path)
                .spawn();
            return;
        }

        // Linux/Mac: open in a split pane with terminal editor
        self.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
        let cmd = format!("{} {}", editor, path.to_string_lossy());
        let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let _ = self.terminal_manager_mut().spawn_in_active(&sh, &["-ilc".to_string(), cmd], None);

        // Rename tab
        let active_id = self.terminal_manager().active_pane_id().cloned();
        if let Some(ref pid) = active_id {
            if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                if let Some(tab) = pane.tabs.last_mut() {
                    tab.title = "Startup Config".to_string();
                    tab.custom_title = true;
                }
            }
        }
    }

    /// Save all workspace layouts to disk
    fn save_all_layouts(&self) {
        let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (ws_id, tm) in &self.workspace_terminals {
            map.insert(ws_id.clone(), tm.save_layout());
        }
        let path = Self::layout_file_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string(&map) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Load all workspace layouts from disk
    fn load_all_layouts() -> std::collections::HashMap<String, String> {
        let path = Self::layout_file_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        }
    }

    fn spawn_terminal_in_active(&mut self) {
        let (shell, args) = Self::default_shell();
        let cwd = Self::default_cwd();
        let _ = self.terminal_manager_mut().spawn_in_active(&shell, &args, cwd.as_deref());
    }

    /// Vibe Coding tool definitions: (tool_id, linux_bin, win_bin, extra_args, tab_title)
    fn vibe_tool_info(tool: &str) -> Option<(&'static str, &'static str, Vec<String>, &'static str)> {
        Some(match tool {
            "claude"   => ("claude",   "claude.cmd",   vec![], "Claude Code"),
            "opencode" => ("opencode", "opencode.cmd", vec![], "OpenCode"),
            "aider"    => ("aider",    "aider",        vec![], "Aider"),
            "codex"    => ("codex",    "codex.cmd",    vec![], "Codex CLI"),
            "gemini"   => ("gemini",   "gemini.cmd",   vec![], "Gemini CLI"),
            "copilot"  => ("gh",       "gh",           vec!["copilot".into()], "Copilot CLI"),
            _ => return None,
        })
    }

    /// Launch a Vibe Coding CLI tool in a new split pane.
    /// `use_wsl`: true to force WSL launch, false for native.
    fn launch_vibe_tool_env(&mut self, tool: &str, use_wsl: bool) {
        let Some((linux_bin, win_bin, extra_args, tab_title)) = Self::vibe_tool_info(tool) else {
            return;
        };
        let env = if use_wsl { "wsl" } else { "native" };

        // Split right
        self.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
        let cwd = Self::default_cwd();

        let tool_cmd = if extra_args.is_empty() {
            linux_bin.to_string()
        } else {
            format!("{} {}", linux_bin, extra_args.join(" "))
        };

        let (shell, shell_args, spawn_cwd) = if use_wsl && cfg!(target_os = "windows") {
            // Windows host → launch inside WSL via wsl.exe
            let mut wsl_args = vec![];
            if let Some(ref cwd_str) = cwd {
                let wsl_path = Self::windows_path_to_wsl(cwd_str);
                wsl_args.extend(["--cd".to_string(), wsl_path]);
            }
            // Use login shell inside WSL so PATH is complete
            wsl_args.extend(["--".to_string(), "bash".to_string(), "-ilc".to_string(), tool_cmd]);
            ("wsl.exe".to_string(), wsl_args, None)
        } else if use_wsl {
            // Already inside WSL/Linux → just use login shell
            let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            (sh, vec!["-ilc".to_string(), tool_cmd], cwd.as_deref().map(|s| s.to_string()))
        } else if cfg!(target_os = "windows") {
            // Windows native tool
            let bin = win_bin;
            let win_cmd = if extra_args.is_empty() {
                bin.to_string()
            } else {
                format!("{} {}", bin, extra_args.join(" "))
            };
            let (ps, _) = Self::default_shell();
            (ps, vec!["-NoLogo".to_string(), "-Command".to_string(), win_cmd], cwd.as_deref().map(|s| s.to_string()))
        } else {
            // Linux/Mac native tool
            let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            (sh, vec!["-ilc".to_string(), tool_cmd], cwd.as_deref().map(|s| s.to_string()))
        };

        let spawn_cwd_ref = spawn_cwd.as_deref();
        let _ = self.terminal_manager_mut().spawn_in_active(&shell, &shell_args, spawn_cwd_ref);

        // Rename the tab
        let suffix = if env == "wsl" { " (WSL)" } else { "" };
        let title = format!("{}{}", tab_title, suffix);
        let active_id = self.terminal_manager().active_pane_id().cloned();
        if let Some(ref pid) = active_id {
            if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                if let Some(tab) = pane.tabs.last_mut() {
                    tab.title = title;
                }
            }
        }
    }

    /// Open a WSL bash shell in a new tab in the current pane.
    fn launch_wsl_shell(&mut self) {
        self.terminal_manager_mut().add_tab_to_active_pane("WSL".into());
        let cwd = Self::default_cwd();
        let mut wsl_args = vec![];
        if let Some(ref cwd_str) = cwd {
            let wsl_path = Self::windows_path_to_wsl(cwd_str);
            wsl_args.extend(["--cd".to_string(), wsl_path]);
        }
        let _ = self.terminal_manager_mut().spawn_in_active("wsl.exe", &wsl_args, None);
        // Rename the tab
        let active_id = self.terminal_manager().active_pane_id().cloned();
        if let Some(ref pid) = active_id {
            if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                if let Some(tab) = pane.tabs.last_mut() {
                    tab.title = "WSL".to_string();
                }
            }
        }
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
    fn refresh_model(&mut self) {
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
    fn search_navigate(&mut self, forward: bool) {
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
    fn toggle_zoom(&mut self) {
        if self.zoomed_pane.is_some() {
            self.zoomed_pane = None;
        } else if let Some(pid) = self.terminal_manager().active_pane_id().cloned() {
            self.zoomed_pane = Some(pid);
        }
    }

    /// Copy selected text to clipboard via alacritty's selection.
    fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            let text = term.with_term(|t| t.selection_to_string());
            if let Some(text) = text {
                if !text.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                }
            }
            // Clear selection after copy
            term.with_term_mut(|t| { t.selection = None; });
        }
    }

    /// Paste from clipboard into terminal
    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let text = cx.read_from_clipboard()
            .and_then(|item| item.text().map(|s| s.to_string()));
        if let Some(text) = text {
            self.send_paste_text(&text);
        }
    }

    /// Smart paste: if clipboard has an image, save it and insert the file path
    /// formatted for the current AI tool. If clipboard has text, paste normally.
    fn smart_paste(&mut self, cx: &mut Context<Self>) {
        let item = match cx.read_from_clipboard() {
            Some(item) => item,
            None => return,
        };

        // Check for image first
        for entry in item.entries() {
            if let gpui::ClipboardEntry::Image(image) = entry {
                if !image.bytes.is_empty() {
                    if let Some(path) = self.save_clipboard_image(image) {
                        // Detect which AI tool is running and format accordingly
                        let formatted = self.format_image_path_for_tool(&path);
                        self.send_paste_text(&formatted);
                    }
                    return;
                }
            }
        }

        // Fallback to text paste
        if let Some(text) = item.text() {
            self.send_paste_text(&text);
        }
    }

    /// Format the image path for the current terminal context.
    /// On Windows: always provide both Windows and WSL paths, since the terminal
    /// might be running a WSL program (claude, opencode) via wsl.exe.
    fn format_image_path_for_tool(&self, path: &str) -> String {
        if cfg!(target_os = "windows") && path.len() >= 2 && path.as_bytes()[1] == b':' {
            // Windows path detected — convert to WSL format since most Vibe Coding
            // tools run inside WSL. WSL can also read Windows paths via /mnt/,
            // so the WSL path works everywhere.
            Self::windows_path_to_wsl(path)
        } else {
            path.to_string()
        }
    }

    /// Save a clipboard image to ~/.amux/screenshots/ and return the path string.
    fn save_clipboard_image(&self, image: &gpui::Image) -> Option<String> {
        let dir = Self::amux_dir().join("screenshots");
        std::fs::create_dir_all(&dir).ok()?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ext = match image.format {
            gpui::ImageFormat::Png => "png",
            gpui::ImageFormat::Jpeg => "jpg",
            gpui::ImageFormat::Gif => "gif",
            gpui::ImageFormat::Webp => "webp",
            gpui::ImageFormat::Bmp => "bmp",
            _ => "png",
        };
        let filename = format!("screenshot_{}.{}", timestamp, ext);
        let path = dir.join(&filename);
        std::fs::write(&path, &image.bytes).ok()?;
        Some(path.to_string_lossy().to_string())
    }

    /// Send text to active terminal with bracketed paste support.
    fn send_paste_text(&mut self, text: &str) {
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            let bracketed = term.with_term(|t| {
                t.mode().contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE)
            });
            if bracketed {
                term.send_input(b"\x1b[200~");
            }
            term.send_input(text.as_bytes());
            if bracketed {
                term.send_input(b"\x1b[201~");
            }
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

        // WSL Terminal option (Windows only, cached)
        if self.wsl_detected {
            items.push(ContextMenuItem::action("WSL Terminal", None, true)
                .with_icon("WSL"));
        }

        // Workspace startup commands
        {
            let ws_name = self.model.active_workspace_name
                .clone().unwrap_or_else(|| self.active_workspace_id.clone());
            let has_startup = Self::startup_file_path(&ws_name).exists();
            if let Some(last) = items.last_mut() {
                last.separator_after = true;
            }
            if has_startup {
                items.push(ContextMenuItem::action("Run Startup", None, true));
            }
            items.push(ContextMenuItem::action("Edit Startup", None, true));
        }

        // Vibe Coding tools — from cached detection
        if !self.detected_vibe_tools.is_empty() {
            if let Some(last) = items.last_mut() {
                last.separator_after = true;
            }
            for &(_tool_id, label, env) in &self.detected_vibe_tools {
                let icon = if env == "wsl" { "WSL" } else { ">_" };
                items.push(ContextMenuItem::action(label, None, true).with_icon(icon));
            }
        }
        items
    }

    /// Execute a context menu action by label
    fn execute_context_menu_action(&mut self, label: &str, cx: &mut Context<Self>) {
        match label {
            "Copy" => {
                self.copy_selection(cx);
            }
            "Paste" => {
                self.paste_clipboard(cx);
            }
            "Split Right" => {
                self.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
                self.spawn_terminal_in_active();
            }
            "Split Down" => {
                self.terminal_manager_mut().split_active_pane(SplitDirection::Vertical);
                self.spawn_terminal_in_active();
            }
            "New Tab" => {
                self.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                self.spawn_terminal_in_active();
            }
            "Close Pane" => {
                self.zoomed_pane = None; // unzoom on close
                self.terminal_manager_mut().close_active_pane();
            }
            "Zoom Pane" | "Restore Pane" => {
                self.toggle_zoom();
            }
            "WSL Terminal" => {
                self.launch_wsl_shell();
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

    /// Handle key input for the terminal
    pub fn handle_terminal_input(&mut self, key: &str, ctrl: bool, shift: bool, alt: bool) {
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
        
        // Check app cursor key mode from active terminal
        let app_cursor = self.terminal_manager().active_terminal_ref()
            .map(|t| t.with_term(|term| term.mode().contains(alacritty_terminal::term::TermMode::APP_CURSOR)))
            .unwrap_or(false);
        let input = keys::to_pty_with_mode(normalized_key, ctrl, shift, alt, app_cursor);

        // Only send to PTY - PTY will echo back, no local echo needed
        if let Some(terminal) = self.terminal_manager_mut().active_terminal() {
            terminal.send_input(&input);
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

        // Measure font metrics on first render
        let metrics = self.cell_metrics.get_or_insert_with(|| {
            crate::gpui_terminal::measure_cell_metrics(window)
        }).clone();
        let cell_w = metrics.width.max(1.0);  // guard against zero
        let cell_h = metrics.height.max(1.0);

        // Resize terminals — skip during drag to avoid content loss
        if self.resize_drag.is_none() {
            let sidebar_w = if self.sidebar_state.collapsed { SIDEBAR_WIDTH_COLLAPSED } else { SIDEBAR_WIDTH_EXPANDED };
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


        
        // Register IME input handler for Chinese/Japanese/Korean input
        let view_entity = cx.entity().clone();
        let focus_for_ime = self.focus_handle.clone();

        // Main layout - limux/mori style dark theme
        div()
            .track_focus(&self.focus_handle)
            // Register IME handler with a zero-size canvas (invisible, no stray cursor)
            .child(gpui::canvas(
                move |bounds, _window, _cx| bounds,
                move |bounds, _, window, cx| {
                    window.handle_input(
                        &focus_for_ime,
                        gpui::ElementInputHandler::new(bounds, view_entity),
                        cx,
                    );
                },
            ).w(px(0.0)).h(px(0.0)).absolute())
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
                let sidebar_w = if this.sidebar_state.collapsed {
                    SIDEBAR_WIDTH_COLLAPSED
                } else {
                    SIDEBAR_WIDTH_EXPANDED
                };
                if event.position.x.as_f32() < sidebar_w {
                    return;
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
            .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _window, cx| {
                let lines = match event.delta {
                    gpui::ScrollDelta::Lines(pt) => -pt.y,
                    gpui::ScrollDelta::Pixels(pt) => -pt.y.as_f32() / this.cell_dims().1,
                };
                if lines == 0.0 { return; }

                let (mouse_mode, sgr) = this.active_term_mouse_mode();
                let (col, row) = this.pixel_to_term_cell(event.position);

                if mouse_mode {
                    let count = lines.abs().ceil().max(1.0) as usize;
                    let button: u8 = if lines > 0.0 { 64 } else { 65 };
                    for _ in 0..count {
                        this.send_mouse_event(button, col, row, true);
                    }
                } else if let Some(term) = this.terminal_manager_mut().active_terminal() {
                    // No mouse mode: scroll scrollback buffer
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
                            div()
                                .id("sidebar-expanded")
                                .w(px(SIDEBAR_WIDTH_EXPANDED))
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
                                        .children(workspaces.iter().enumerate().map(|(ws_idx, item)| {
                                            let is_active = item.is_active;
                                            let has_ws_activity = !is_active && self.workspace_terminals
                                                .get(&item.id)
                                                .map(|tm| tm.has_any_activity())
                                                .unwrap_or(false);
                                            let bg_color = if is_active { rgb(0x252530) } else { rgb(0x181818) };
                                            let text_color = if is_active { rgb(0xcdd6f4) } else { rgb(0x7f849c) };
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
                                                .when(is_active, |d| d.border_l_2().border_color(rgb(0x89b4fa)))
                                                // Drag to reorder
                                                .on_drag(
                                                    DragWorkspace { workspace_id: drag_id, name: drag_name, index: ws_idx },
                                                    |drag, _, _, cx| {
                                                        cx.stop_propagation();
                                                        cx.new(|_| drag.clone())
                                                    },
                                                )
                                                .drag_over::<DragWorkspace>(|style, _, _, _| {
                                                    style.bg(rgb(0x313244)).border_t_2().border_color(rgb(0x89b4fa))
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
                                                        .text_color(rgb(0xcdd6f4))
                                                        .px_1()
                                                        .bg(rgb(0x313244))
                                                        .rounded(px(2.0))
                                                        .border_1()
                                                        .border_color(rgb(0x89b4fa))
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
                                                                        d.text_color(rgb(0x585b70))
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
                                        .text_color(rgb(0x585b70))
                                        .cursor_pointer()
                                        .hover(|d| d.bg(rgb(0x252530)).text_color(rgb(0xcdd6f4)))
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
                                let sidebar_w = if self.sidebar_state.collapsed { SIDEBAR_WIDTH_COLLAPSED } else { SIDEBAR_WIDTH_EXPANDED };
                                let vp = window.viewport_size();
                                let content_w = vp.width.as_f32() - sidebar_w;
                                let status_bar_h = 28.0_f32;
                                let content_h = vp.height.as_f32() - status_bar_h;
                                let cursor_blink_on = true;
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
                                    render_layout(&single, self.terminal_manager(), Some(&zpid), content_w, content_h, cursor_blink_on, &metrics, true, &renaming_tab, origin_x, origin_y, unsafe { &mut *pb }, cx)
                                } else if let Some(layout) = layout_cloned {
                                    render_layout(&layout, self.terminal_manager(), active_pane_id.as_ref(), content_w, content_h, cursor_blink_on, &metrics, false, &renaming_tab, origin_x, origin_y, unsafe { &mut *pb }, cx)
                                } else {
                                    div().flex_1().bg(rgb(0x1d1f21)).child("No terminal").into_any_element()
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
                        .bg(rgb(0x1e1e2e))
                        .border_1()
                        .border_color(rgb(0x45475a))
                        .shadow_lg()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div().text_xs().text_color(rgb(0x585b70)).child("Find:")
                        )
                        .child(
                            div()
                                .flex_1()
                                .px_2()
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(rgb(0x11111b))
                                .border_1()
                                .border_color(rgb(0x313244))
                                .text_sm()
                                .text_color(rgb(0xcdd6f4))
                                .min_h(px(20.0))
                                .child(if query.is_empty() {
                                    div().text_color(rgb(0x585b70)).child("Type to search...").into_any_element()
                                } else {
                                    div().child(format!("{}▎", query)).into_any_element()
                                })
                        )
                        .child(
                            div().text_xs().text_color(rgb(0x585b70)).child("Enter/Shift+Enter  Esc close")
                        )
                )
            })
    }
}

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
        // If searching, append to search query and auto-navigate
        if let Some((ref mut query, _)) = self.search_state {
            query.push_str(text);
            let q = query.clone();
            drop(query);
            self.search_navigate(true);
            cx.notify();
            return;
        }

        // Send to terminal PTY
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            term.send_input(text.as_bytes());
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self, _range: Option<std::ops::Range<usize>>, _new_text: &str,
        _selected: Option<std::ops::Range<usize>>, _window: &mut Window, _cx: &mut Context<Self>,
    ) {
        // IME composition in progress — we don't show inline preview for terminal
    }

    fn bounds_for_range(
        &mut self, _range: std::ops::Range<usize>, _element_bounds: Bounds<Pixels>,
        _window: &mut Window, _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(
        &mut self, _point: gpui::Point<Pixels>, _window: &mut Window, _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

#[cfg(feature = "gpui")]
impl GpuiShellView {
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
                    self.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
                    self.spawn_terminal_in_active();
                    cx.notify();
                    return;
                }
                "ctrl+shift+d" => {
                    self.terminal_manager_mut().split_active_pane(SplitDirection::Vertical);
                    self.spawn_terminal_in_active();
                    cx.notify();
                    return;
                }
                "ctrl+shift+t" => {
                    self.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                    self.spawn_terminal_in_active();
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

/// Render the right-click context menu
#[cfg(feature = "gpui")]
fn render_context_menu(
    pos: gpui::Point<gpui::Pixels>,
    items: Vec<ContextMenuItem>,
    viewport_w: gpui::Pixels,
    viewport_h: gpui::Pixels,
    cx: &mut Context<GpuiShellView>,
) -> impl IntoElement {
    let menu_w = 240.0_f32;
    // Each item ≈ 30px (6px padding + ~18px text), separator ≈ 10px
    let separator_count = items.iter().filter(|i| i.separator_after).count();
    let menu_h = (items.len() as f32) * 30.0 + (separator_count as f32) * 10.0 + 12.0;
    let max_menu_h = viewport_h.as_f32() * 0.8; // never exceed 80% of viewport

    // Adjust position to keep menu within viewport
    let mut x = pos.x.as_f32();
    let mut y = pos.y.as_f32();
    if x + menu_w > viewport_w.as_f32() {
        x = (viewport_w.as_f32() - menu_w).max(0.0);
    }
    if y + menu_h.min(max_menu_h) > viewport_h.as_f32() {
        y = (viewport_h.as_f32() - menu_h.min(max_menu_h)).max(0.0);
    }

    let mut menu = div()
        .id("context-menu-container")
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(menu_w))
        .max_h(px(max_menu_h))
        .overflow_y_scroll()
        .rounded(px(8.0))
        .bg(rgb(0x282a2e))
        .border_1()
        .border_color(rgb(0x373b41))
        .shadow_lg()
        .py_1()
        .flex()
        .flex_col();

    for item in items {
        let label = item.label;
        let enabled = item.enabled;

        let text_color = if enabled { rgb(0xc5c8c6) } else { rgb(0x4a4d4e) };

        // Left side: optional icon badge + label
        let mut left = div().flex().flex_row().items_center().gap(px(6.0));
        if let Some(icon) = item.icon {
            let (badge_bg, badge_fg) = match icon {
                "WSL" => (rgb(0x2d4f2d), rgb(0x8abeb7)), // green tint for Linux/WSL
                _     => (rgb(0x313244), rgb(0x81a2be)),  // blue for native tools
            };
            left = left.child(
                div()
                    .px(px(4.0))
                    .py(px(1.0))
                    .rounded(px(3.0))
                    .bg(badge_bg)
                    .text_color(badge_fg)
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(icon),
            );
        }
        left = left.child(div().text_sm().text_color(text_color).child(label));

        let row = div()
            .id(gpui::ElementId::Name(label.into()))
            .px_3()
            .py(px(6.0))
            .mx_1()
            .rounded(px(4.0))
            .flex()
            .justify_between()
            .items_center()
            .when(enabled, |d| d.hover(|d| d.bg(rgb(0x373b41))))
            .when(enabled, |d| {
                d.on_click(cx.listener(move |this, _event, _window, cx| {
                    this.execute_context_menu_action(label, cx);
                }))
            })
            .child(left)
            .children(item.shortcut.map(|kb| {
                div()
                    .text_xs()
                    .text_color(rgb(0x696d70))
                    .child(kb)
            }));

        menu = menu.child(row);

        if item.separator_after {
            menu = menu.child(
                div()
                    .mx_2()
                    .my_1()
                    .h(px(1.0))
                    .bg(rgb(0x373b41)),
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
    metrics: &crate::gpui_terminal::CellMetrics,
    is_zoomed: bool,
    renaming_tab: &Option<(String, usize, String)>,
    origin_x: f32,
    origin_y: f32,
    pane_bounds: &mut std::collections::HashMap<String, (f32, f32, f32, f32)>,
    cx: &mut Context<GpuiShellView>,
) -> gpui::AnyElement {
    use amux_platform::terminal::manager::{PaneId, TabLayout};

    match layout {
        TabLayout::Single(pane_id) => {
            // Record this pane's screen bounds for mouse hit-testing.
            // Tab strip (28px) is at the top; terminal content starts below it.
            let tab_strip_h = 28.0_f32;
            pane_bounds.insert(pane_id.0.clone(), (origin_x, origin_y + tab_strip_h, avail_w, (avail_h - tab_strip_h).max(0.0)));
            let is_active = active_pane_id == Some(pane_id);
            let has_multiple_panes = manager.total_panes() > 1;

            // Build per-pane tab strip + terminal content
            // get_pane may return None if layout references a pane that doesn't
            // exist in the panes map (e.g., corrupted saved layout). In that case,
            // we skip the pane and render a placeholder.
            let (tab_strip, content) = if let Some(pane) = manager.get_pane(pane_id) {
                let tabs = pane.tab_titles();
                let pid_for_tabs = pane_id.clone();
                let has_multiple_panes = manager.total_panes() > 1;

                // Left side: tab buttons
                let tab_count = tabs.len();
                let tabs_row = div()
                    .flex()
                    .flex_row()
                    .gap_px()
                    .flex_1()
                    .overflow_hidden()
                    .children(tabs.into_iter().map(|(idx, title, is_tab_active, has_activity)| {
                        let pid_click = pid_for_tabs.clone();
                        let pid_close_tab = pid_for_tabs.clone();
                        let pid_drag = pid_for_tabs.clone();
                        let can_close_tab = tab_count > 1;
                        let drag_title = title.clone();
                        div()
                            .id(gpui::ElementId::Name(
                                format!("{}-tab-{}", pid_for_tabs.0, idx).into(),
                            ))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(4.0))
                            .min_w(px(60.0))
                            .max_w(px(180.0))
                            .flex_shrink()
                            .overflow_hidden()
                            .px_3()
                            .py(px(4.0))
                            .text_xs()
                            .cursor_grab()
                            .text_color(if is_tab_active { rgb(0xcdd6f4) } else { rgb(0x7f849c) })
                            .bg(if is_tab_active { rgb(0x1e1e2e) } else { rgb(0x11111b) })
                            .border_b_2()
                            .border_color(if is_tab_active { rgb(0x89b4fa) } else { rgb(0x11111b) })
                            .when(is_tab_active, |d| d.font_weight(gpui::FontWeight::MEDIUM))
                            .hover(|d| d.bg(rgb(0x252530)))
                            .on_drag(
                                DragTab {
                                    source_pane: pid_drag,
                                    tab_index: idx,
                                    title: drag_title,
                                },
                                |drag, _, _, cx| {
                                    cx.stop_propagation();
                                    cx.new(|_| drag.clone())
                                },
                            )
                            .on_click({
                                let pid_rename = pid_click.clone();
                                let rename_title = title.clone();
                                cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                                    if event.click_count() >= 2 {
                                        // Double-click: start inline rename
                                        this.renaming_tab = Some((pid_rename.0.clone(), idx, String::new()));
                                    } else {
                                        // Single click: switch tab
                                        this.terminal_manager_mut().set_active_pane(&pid_click);
                                        this.terminal_manager_mut().set_active_tab_in_pane(idx);
                                    }
                                    cx.notify();
                                })
                            })
                            .child({
                                let is_tab_renaming = renaming_tab.as_ref()
                                    .map(|(p, i, _): &(String, usize, String)| p == &pid_for_tabs.0 && *i == idx)
                                    .unwrap_or(false);
                                if is_tab_renaming {
                                    let rename_text = renaming_tab.as_ref()
                                        .map(|(_, _, t): &(String, usize, String)| t.clone())
                                        .unwrap_or_default();
                                    div()
                                        .flex_1()
                                        .overflow_hidden()
                                        .text_sm()
                                        .text_color(rgb(0xcdd6f4))
                                        .bg(rgb(0x313244))
                                        .rounded(px(2.0))
                                        .border_1()
                                        .border_color(rgb(0x89b4fa))
                                        .px_1()
                                        .child(if rename_text.is_empty() { "▎".to_string() } else { format!("{}▎", rename_text) })
                                        .into_any_element()
                                } else {
                                    let mut tab_content = div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(4.0))
                                        .overflow_hidden()
                                        .flex_1();
                                    // Activity indicator: green dot for unread output
                                    if has_activity && !is_tab_active {
                                        tab_content = tab_content.child(
                                            div().w(px(6.0)).h(px(6.0)).rounded(px(3.0))
                                                .bg(rgb(0xa6e3a1)).flex_shrink_0()
                                        );
                                    }
                                    tab_content = tab_content.child(
                                        div().whitespace_nowrap().child(title)
                                    );
                                    tab_content.into_any_element()
                                }
                            })
                            .when(can_close_tab, |d| {
                                d.child(
                                    div()
                                        .id(gpui::ElementId::Name(
                                            format!("{}-tab-{}-close", pid_close_tab.0, idx).into(),
                                        ))
                                        .px(px(2.0))
                                        .rounded(px(3.0))
                                        .text_color(rgb(0x585b70))
                                        .hover(|d| d.bg(rgb(0x45475a)).text_color(rgb(0xf38ba8)))
                                        .child("×")
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.terminal_manager_mut().set_active_pane(&pid_close_tab);
                                            if let Some(pane) = this.terminal_manager_mut().get_pane_mut(&pid_close_tab) {
                                                pane.close_tab(idx);
                                            }
                                            cx.notify();
                                        }))
                                )
                            })
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
                    .gap(px(2.0))
                    .px_2()
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
                                this.spawn_terminal_in_active();
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
                                this.spawn_terminal_in_active();
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
                                this.spawn_terminal_in_active();
                                cx.notify();
                            })),
                    )
                    // Zoom ⤢ / Restore ⤡
                    .when(has_multiple_panes || is_zoomed, |d| {
                        let pid_zoom = pane_id.clone();
                        let zoom_icon = if is_zoomed { "⤡" } else { "⤢" };
                        d.child(
                            div()
                                .id(gpui::ElementId::Name(format!("{}-btn-zoom", pane_id.0).into()))
                                .px(px(5.0))
                                .py(px(2.0))
                                .rounded(px(3.0))
                                .text_xs()
                                .text_color(rgb(0x6c7086))
                                .hover(|d| d.bg(rgb(0x313244)).text_color(rgb(0xcdd6f4)))
                                .child(zoom_icon)
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.terminal_manager_mut().set_active_pane(&pid_zoom);
                                    this.toggle_zoom();
                                    cx.notify();
                                })),
                        )
                    })
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

                // Combine into tab strip (relative container for zoom indicator)
                let tab_strip = div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .bg(rgb(0x11111b))
                    .border_b_1()
                    .border_color(rgb(0x252530))
                    .child(tabs_row)
                    .child(actions_row)
                    // Zoom indicator: absolutely centered over the entire tab strip
                    .when(is_zoomed, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left_0()
                                .right_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                // Don't block clicks on tabs/buttons underneath
                                .child(
                                    div()
                                        .px_2()
                                        .py(px(2.0))
                                        .rounded(px(8.0))
                                        .bg(rgb(0x1e1e2e))
                                        .border_1()
                                        .border_color(rgb(0x45475a))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(5.0))
                                        .child(
                                            div()
                                                .w(px(6.0))
                                                .h(px(6.0))
                                                .rounded(px(3.0))
                                                .bg(rgb(0xa6e3a1)) // green for "zoomed" state
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(0xa6adc8))
                                                .child("ZOOMED")
                                        )
                                )
                        )
                    })
                    .into_any_element();

                let content = if let Some(term) = pane.active_terminal_ref() {
                    crate::gpui_terminal::render_alacritty_terminal(term, cursor_blink_on, &metrics, is_active).into_any_element()
                } else {
                    div().flex_1().flex().items_center().justify_center()
                        .bg(rgb(0x1d1f21))
                        .child(
                            div().flex().flex_col().items_center().gap_2()
                                .child(div().text_sm().text_color(rgb(0x585b70)).child("Starting terminal..."))
                        )
                        .into_any_element()
                };
                (tab_strip, content)
            } else {
                (
                    div().into_any_element(),
                    div().flex_1().bg(rgb(0x1e1e2e)).child("Empty pane").into_any_element(),
                )
            };

            let pid = pane_id.clone();
            let pid_drop = pane_id.clone();
            div()
                .id(gpui::ElementId::Name(pane_id.0.clone().into()))
                .flex_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                .bg(rgb(0x1d1f21))
                // Active pane indicator: only show when multiple panes exist
                // No extra border — active pane is indicated by tab strip's blue underline
                // Tab strip at top (limux style)
                .child(tab_strip)
                // Terminal content
                .child(content)
                // Drag-and-drop: visual feedback when dragging a tab over this pane
                .drag_over::<DragTab>(|style, _, _, _| {
                    style.border_t_2().border_color(rgb(0x585b70))
                })
                // Drag-and-drop: accept a dropped tab
                .on_drop(cx.listener(move |this, drag: &DragTab, _window, cx| {
                    this.terminal_manager_mut().move_tab_to_pane(
                        &drag.source_pane,
                        drag.tab_index,
                        &pid_drop,
                    );
                    cx.notify();
                }))
                .on_mouse_down(gpui::MouseButton::Right, {
                    let pid_right = pid.clone();
                    cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                        this.terminal_manager_mut().set_active_pane(&pid_right);
                        cx.notify();
                    })
                })
                // Switch active pane on mouse_down (not click) so it happens
                // BEFORE the root div's mouse_down handler reads active_terminal().
                // This ensures text selection targets the correct pane.
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
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
                .child(render_layout(left, manager, active_pane_id, left_w, avail_h, cursor_blink_on, metrics, is_zoomed, renaming_tab, origin_x, origin_y, pane_bounds, cx));

            let handle = div()
                .id(gpui::ElementId::Name(format!("resize-h-{}", split_id).into()))
                .group("resize-h")
                .w(px(handle_px))
                .flex_shrink_0()
                .cursor_col_resize()
                .child(
                    div()
                        .w(px(1.0))
                        .h_full()
                        .mx_auto()
                        .bg(rgb(0x252530))
                        .group_hover("resize-h", |d| d.w(px(2.0)).bg(rgb(0x585b70)))
                )
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
                .child(render_layout(right, manager, active_pane_id, right_w, avail_h, cursor_blink_on, metrics, is_zoomed, renaming_tab, origin_x + left_w + handle_px, origin_y, pane_bounds, cx));

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
                .child(render_layout(top, manager, active_pane_id, avail_w, top_h, cursor_blink_on, metrics, is_zoomed, renaming_tab, origin_x, origin_y, pane_bounds, cx));

            let handle = div()
                .id(gpui::ElementId::Name(format!("resize-v-{}", split_id).into()))
                .group("resize-v")
                .h(px(handle_px))
                .flex_shrink_0()
                .cursor_ns_resize()
                .child(
                    div()
                        .h(px(1.0))
                        .w_full()
                        .my_auto()
                        .bg(rgb(0x252530))
                        .group_hover("resize-v", |d| d.h(px(2.0)).bg(rgb(0x585b70)))
                )
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
                .child(render_layout(bottom, manager, active_pane_id, avail_w, bottom_h, cursor_blink_on, metrics, is_zoomed, renaming_tab, origin_x, origin_y + top_h + handle_px, pane_bounds, cx));

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

    let mut app = app.clone();
    let model = app.render_with(&GpuiRenderer);

    application().run(move |cx: &mut App| {
        let model = model.clone();
        let app = app.clone();
        
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
        let window_result = cx.open_window(window_opts, |_, cx| {
            cx.new(|cx| {
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

                            // Only re-render when needed: new output, cursor blink, or drag
                            if any_dirty || cursor_blink_toggle || has_drag || this.selecting {
                                cx.notify();
                            }
                            // Deferred startup: spawn PTY processes on first frame
                            if !this.terminals_spawned {
                                this.terminals_spawned = true;
                                let (shell, args) = GpuiShellView::default_shell();
                                let cwd = GpuiShellView::default_cwd();
                                let ws_ids: Vec<String> = this.workspace_terminals.keys().cloned().collect();
                                for ws_id in ws_ids {
                                    if let Some(tm) = this.workspace_terminals.get_mut(&ws_id) {
                                        let pane_ids: Vec<_> = tm.active_layout()
                                            .map(|l| l.pane_ids()).unwrap_or_default();
                                        for pid in pane_ids {
                                            let _ = tm.spawn_in_pane(&pid, &shell, &args, cwd.as_deref());
                                        }
                                    }
                                }
                                cx.notify(); // trigger re-render to show terminal content
                            }
                            // Deferred tool detection: run on third frame
                            if !this.tools_detected && this.cursor_blink_frame >= 3 {
                                this.tools_detected = true;
                                this.detected_vibe_tools = GpuiShellView::detect_all_vibe_tools();
                                this.wsl_detected = GpuiShellView::wsl_available();
                            }
                            // Poll terminal activity for all workspaces (~15fps)
                            if this.cursor_blink_frame % 4 == 0 {
                                for tm in this.workspace_terminals.values_mut() {
                                    tm.poll_activity();
                                }
                                // Clear activity for the active tab since user is looking at it
                                this.terminal_manager_mut().clear_active_activity();
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

                GpuiShellView::new(app, model, cx)
            })
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
pub fn run(_: &amux_ui::DesktopApp) {}
