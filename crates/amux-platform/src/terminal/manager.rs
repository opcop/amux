//! Terminal Manager — per-pane tabs + nested splits (limux-style)
//!
//! Each pane has its own tab strip. Panes can be split arbitrarily.
//! Uses AlacrittyTerminal for full VT100/xterm escape sequence support.

use std::collections::HashMap;

use serde::{Serialize, Deserialize};
use crate::terminal::alacritty_view::AlacrittyTerminal;

/// Unique ID for a pane
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaneId(pub String);

/// Split direction
#[derive(Clone, Debug, Copy)]
pub enum SplitDirection {
    Horizontal, // Side by side
    Vertical,   // Top and bottom
}

/// AI agent status detected from terminal output
#[derive(Clone, Debug, PartialEq)]
pub enum AgentStatus {
    /// Agent is processing/thinking (spinner, "Thinking...", etc.)
    Thinking,
    /// Agent is waiting for user input (prompt visible)
    Waiting,
    /// Agent has finished its task
    Done,
    /// Agent encountered an error
    Error,
}

impl AgentStatus {
    pub fn label(&self) -> &'static str {
        match self {
            AgentStatus::Thinking => "thinking...",
            AgentStatus::Waiting  => "waiting",
            AgentStatus::Done     => "done",
            AgentStatus::Error    => "error",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            AgentStatus::Thinking => "⟳",
            AgentStatus::Waiting  => "●",
            AgentStatus::Done     => "✓",
            AgentStatus::Error    => "✗",
        }
    }

    /// Catppuccin Mocha color for each status (RGB u32)
    pub fn color_rgb(&self) -> u32 {
        match self {
            AgentStatus::Thinking => 0x89b4fa, // blue
            AgentStatus::Waiting  => 0xf9e2af, // yellow
            AgentStatus::Done     => 0xa6e3a1, // green
            AgentStatus::Error    => 0xf38ba8, // red
        }
    }
}

/// Notification emitted when an agent's status changes
#[derive(Clone, Debug)]
pub struct AgentNotification {
    pub pane_id: PaneId,
    pub tab_index: usize,
    pub tab_title: String,
    pub agent_kind: AgentKind,
    pub new_status: AgentStatus,
}

/// Known AI agent type for status detection
#[derive(Clone, Debug, PartialEq)]
pub enum AgentKind {
    Claude,
    Aider,
    OpenCode,
    Codex,
    Gemini,
    Copilot,
}

/// Information about a pane for the Bridge API
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaneInfo {
    pub pane_id: PaneId,
    pub tab_title: String,
    pub agent_kind: Option<String>,
    pub agent_status: Option<String>,
    pub tab_kind: String, // "terminal", "browser", "preview"
}

/// The kind of content a tab holds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TabKind {
    /// Terminal emulator (default)
    Terminal,
    /// Embedded browser (WebView2). `browser_id` links to desktop-layer state.
    Browser { url: String, #[serde(default)] browser_id: u64 },
    /// File preview (markdown, syntax highlight)
    Preview { path: String },
}

impl TabKind {
    pub fn is_terminal(&self) -> bool { matches!(self, TabKind::Terminal) }
    pub fn is_browser(&self) -> bool { matches!(self, TabKind::Browser { .. }) }
    pub fn is_preview(&self) -> bool { matches!(self, TabKind::Preview { .. }) }

    /// Short label for the tab bar
    pub fn icon(&self) -> &'static str {
        match self {
            TabKind::Terminal => "",
            TabKind::Browser { .. } => "\u{1F310}", // 🌐
            TabKind::Preview { .. } => "\u{1F4C4}", // 📄
        }
    }
}

/// A tab inside a pane — can be terminal, browser, or preview.
pub struct PaneTab {
    pub title: String,
    /// User set a custom title (overrides terminal-reported title)
    pub custom_title: bool,
    /// What kind of content this tab holds
    pub kind: TabKind,
    /// Terminal emulator (only for TabKind::Terminal)
    pub terminal: Option<AlacrittyTerminal>,
    /// Activity indicator: set when new output detected, cleared when user views the tab
    pub has_activity: bool,
    /// True when the terminal child process has exited
    pub exited: bool,
    /// Working directory at spawn time (used for session restore)
    pub cwd: Option<String>,
    /// Shell program and args used to spawn this terminal (for inheriting on split/new tab)
    pub shell_cmd: Option<(String, Vec<String>)>,
    /// Detected AI agent type (None if this is a plain terminal)
    pub agent_kind: Option<AgentKind>,
    /// Current agent status (None if not an agent or not yet detected)
    pub agent_status: Option<AgentStatus>,
    /// Last known cursor line (for activity detection)
    last_cursor_line: i32,
}

/// A pane with its own tab strip (like limux)
pub struct TerminalPane {
    pub id: PaneId,
    pub tabs: Vec<PaneTab>,
    pub active_tab: usize,
}

impl TerminalPane {
    pub fn new(id: PaneId) -> Self {
        Self {
            id,
            tabs: vec![PaneTab {
                title: "Terminal".to_string(),
                custom_title: false,
                kind: TabKind::Terminal,
                terminal: None,
                has_activity: false,
                exited: false,
                cwd: None,
                shell_cmd: None,
                agent_kind: None,
                agent_status: None,
                last_cursor_line: 0,
            }],
            active_tab: 0,
        }
    }

    /// Get the active terminal
    pub fn active_terminal(&mut self) -> Option<&mut AlacrittyTerminal> {
        self.tabs.get_mut(self.active_tab)?.terminal.as_mut()
    }

    /// Check if the active tab's terminal has exited
    pub fn active_tab_exited(&self) -> bool {
        self.tabs.get(self.active_tab).is_some_and(|t| t.exited)
    }

    /// Get the active tab's current working directory.
    /// Prefers live /proc/PID/cwd if available, falls back to saved spawn-time cwd.
    pub fn active_tab_live_cwd(&self) -> Option<String> {
        let tab = self.tabs.get(self.active_tab)?;
        if let Some(ref term) = tab.terminal {
            if let Some(live_cwd) = term.current_cwd() {
                return Some(live_cwd);
            }
        }
        tab.cwd.clone()
    }

    /// Get ONLY the live cwd from the running process (sysinfo/proc).
    /// Returns None if it can't be determined (common on Windows).
    /// Does NOT fall back to saved spawn-time cwd.
    pub fn active_tab_process_cwd(&self) -> Option<String> {
        let tab = self.tabs.get(self.active_tab)?;
        let term = tab.terminal.as_ref()?;
        term.current_cwd()
    }

    /// Get the saved spawn-time cwd (may be stale after `cd`).
    pub fn active_tab_saved_cwd(&self) -> Option<String> {
        let tab = self.tabs.get(self.active_tab)?;
        tab.cwd.clone()
    }

    /// Get the active terminal (immutable)
    pub fn active_terminal_ref(&self) -> Option<&AlacrittyTerminal> {
        self.tabs.get(self.active_tab)?.terminal.as_ref()
    }

    /// Add a new terminal tab to this pane and make it active
    pub fn add_tab(&mut self, title: String) -> usize {
        self.add_tab_with_kind(title, TabKind::Terminal)
    }

    /// Add a new browser tab to this pane and make it active.
    /// `browser_id` is an opaque ID linking to desktop-layer WebView2 state.
    pub fn add_browser_tab(&mut self, url: &str, browser_id: u64) -> usize {
        let title = if url.is_empty() { "Browser".to_string() } else { url.to_string() };
        self.add_tab_with_kind(title, TabKind::Browser { url: url.to_string(), browser_id })
    }

    /// Add a new preview tab to this pane and make it active
    pub fn add_preview_tab(&mut self, path: &str) -> usize {
        let filename = std::path::Path::new(path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        self.add_tab_with_kind(filename, TabKind::Preview { path: path.to_string() })
    }

    /// Add a tab with a specific kind
    fn add_tab_with_kind(&mut self, title: String, kind: TabKind) -> usize {
        // Terminal is spawned separately after tab creation
        self.tabs.push(PaneTab {
            title,
            custom_title: false,
            kind,
            terminal: None,
            has_activity: false,
            exited: false,
            cwd: None,
            shell_cmd: None,
            agent_kind: None,
            agent_status: None,
            last_cursor_line: 0,
        });
        self.active_tab = self.tabs.len() - 1;
        self.active_tab
    }

    /// Close a tab by index. Returns false if it's the last tab.
    pub fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return false;
        }
        // Kill the terminal process before dropping to prevent orphaned
        // child processes. The AlacrittyTerminal::Drop impl also does this,
        // but we do it explicitly here for safety.
        if let Some(ref term) = self.tabs[index].terminal {
            term.kill_child();
        }
        self.tabs.remove(index);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        true
    }

    /// Tab count
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Get the kind of the active tab
    pub fn active_tab_kind(&self) -> Option<&TabKind> {
        self.tabs.get(self.active_tab).map(|t| &t.kind)
    }

    /// Tab titles for rendering
    /// Returns (index, title, is_active, has_activity, exited, agent_status_label_with_color, tab_kind) for each tab.
    pub fn tab_titles(&self) -> Vec<(usize, String, bool, bool, bool, Option<(String, u32)>, &TabKind)> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let title = if t.custom_title {
                    t.title.clone()
                } else {
                    match &t.kind {
                        TabKind::Terminal => {
                            t.terminal.as_ref()
                                .and_then(|term| term.title())
                                .unwrap_or_else(|| t.title.clone())
                        }
                        TabKind::Browser { url, .. } => {
                            if t.title.is_empty() || t.title == "Browser" {
                                // Shorten URL for display
                                url.replace("https://", "").replace("http://", "")
                                    .trim_end_matches('/')
                                    .to_string()
                            } else {
                                t.title.clone()
                            }
                        }
                        TabKind::Preview { path } => {
                            std::path::Path::new(path)
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| t.title.clone())
                        }
                    }
                };
                let status_info = t.agent_status.as_ref().map(|s| {
                    (format!("{} {}", s.icon(), s.label()), s.color_rgb())
                });
                (i, title, i == self.active_tab, t.has_activity, t.exited, status_info, &t.kind)
            })
            .collect()
    }

    /// Set active tab by index
    pub fn set_active_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }
}

/// Pane layout tree — splits of panes
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PaneLayout {
    Single(PaneId),
    Horizontal {
        left: Box<PaneLayout>,
        right: Box<PaneLayout>,
        ratio: f32,
    },
    Vertical {
        top: Box<PaneLayout>,
        bottom: Box<PaneLayout>,
        ratio: f32,
    },
}

impl PaneLayout {
    pub fn pane_ids(&self) -> Vec<PaneId> {
        match self {
            PaneLayout::Single(id) => vec![id.clone()],
            PaneLayout::Horizontal { left, right, .. } => {
                let mut ids = left.pane_ids();
                ids.extend(right.pane_ids());
                ids
            }
            PaneLayout::Vertical { top, bottom, .. } => {
                let mut ids = top.pane_ids();
                ids.extend(bottom.pane_ids());
                ids
            }
        }
    }

    pub fn pane_count(&self) -> usize {
        self.pane_ids().len()
    }
}

/// Reusable layout template — structure + pane labels, no terminal state
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutTemplate {
    pub name: String,
    pub description: String,
    pub layout: PaneLayout,
    /// Pane ID → label for tab title
    #[serde(default)]
    pub pane_labels: HashMap<String, String>,
    /// Built-in templates cannot be deleted
    #[serde(default)]
    pub builtin: bool,
}

impl LayoutTemplate {
    fn new(name: &str, desc: &str, layout: PaneLayout, labels: &[(&str, &str)], builtin: bool) -> Self {
        Self {
            name: name.to_string(),
            description: desc.to_string(),
            layout,
            pane_labels: labels.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            builtin,
        }
    }

    /// Built-in layout templates
    pub fn builtins() -> Vec<Self> {
        vec![
            Self::new(
                "AI + Shell",
                "Left 70% AI agent, right 30% shell",
                PaneLayout::Horizontal {
                    left: Box::new(PaneLayout::Single(PaneId("pane-1".into()))),
                    right: Box::new(PaneLayout::Single(PaneId("pane-2".into()))),
                    ratio: 0.7,
                },
                &[("pane-1", "AI"), ("pane-2", "Shell")],
                true,
            ),
            Self::new(
                "AI + Test + Git",
                "Left AI, right-top test runner, right-bottom git",
                PaneLayout::Horizontal {
                    left: Box::new(PaneLayout::Single(PaneId("pane-1".into()))),
                    right: Box::new(PaneLayout::Vertical {
                        top: Box::new(PaneLayout::Single(PaneId("pane-2".into()))),
                        bottom: Box::new(PaneLayout::Single(PaneId("pane-3".into()))),
                        ratio: 0.5,
                    }),
                    ratio: 0.6,
                },
                &[("pane-1", "AI"), ("pane-2", "Test"), ("pane-3", "Git")],
                true,
            ),
            Self::new(
                "Multi-Agent",
                "Two AI agents top, shell bottom",
                PaneLayout::Vertical {
                    top: Box::new(PaneLayout::Horizontal {
                        left: Box::new(PaneLayout::Single(PaneId("pane-1".into()))),
                        right: Box::new(PaneLayout::Single(PaneId("pane-2".into()))),
                        ratio: 0.5,
                    }),
                    bottom: Box::new(PaneLayout::Single(PaneId("pane-3".into()))),
                    ratio: 0.7,
                },
                &[("pane-1", "Agent 1"), ("pane-2", "Agent 2"), ("pane-3", "Shell")],
                true,
            ),
            Self::new(
                "Full Stack",
                "4-grid: frontend, backend, test, shell",
                PaneLayout::Horizontal {
                    left: Box::new(PaneLayout::Vertical {
                        top: Box::new(PaneLayout::Single(PaneId("pane-1".into()))),
                        bottom: Box::new(PaneLayout::Single(PaneId("pane-2".into()))),
                        ratio: 0.5,
                    }),
                    right: Box::new(PaneLayout::Vertical {
                        top: Box::new(PaneLayout::Single(PaneId("pane-3".into()))),
                        bottom: Box::new(PaneLayout::Single(PaneId("pane-4".into()))),
                        ratio: 0.5,
                    }),
                    ratio: 0.5,
                },
                &[("pane-1", "Frontend"), ("pane-2", "Backend"), ("pane-3", "Test"), ("pane-4", "Shell")],
                true,
            ),
        ]
    }
}

// Keep TabLayout as alias for compatibility
pub type TabLayout = PaneLayout;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub String);

/// Saved tab metadata for session persistence
#[derive(Serialize, Deserialize)]
struct SavedTab {
    title: String,
    custom_title: bool,
    /// Tab content type (defaults to Terminal for backward compat with old layouts)
    #[serde(default = "SavedTab::default_kind")]
    kind: TabKind,
    cwd: Option<String>,
    /// Shell program and args (e.g. ["wsl.exe", "--cd", "/path"]) for restoring WSL tabs
    #[serde(default)]
    shell_cmd: Option<(String, Vec<String>)>,
}

impl SavedTab {
    fn default_kind() -> TabKind { TabKind::Terminal }
}

/// Saved pane metadata for session persistence
#[derive(Serialize, Deserialize)]
struct SavedPane {
    tabs: Vec<SavedTab>,
    active_tab: usize,
}

/// Serializable layout state for persistence
#[derive(Serialize, Deserialize)]
struct LayoutState {
    layout: PaneLayout,
    active_pane: PaneId,
    next_pane_num: usize,
    /// Per-pane tab state (None for backward compat with old layouts.json)
    #[serde(default)]
    pane_states: Option<HashMap<String, SavedPane>>,
}

/// Terminal manager — layout tree of panes, each pane has its own tabs
pub struct TerminalManager {
    layout: PaneLayout,
    panes: HashMap<PaneId, TerminalPane>,
    active_pane: PaneId,
    next_pane_num: usize,
    /// Scrollback buffer size for new terminals
    scrollback_lines: usize,
    /// Workspace name for env var injection into spawned terminals
    workspace_name: Option<String>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self::with_scrollback(10000)
    }

    pub fn with_scrollback(scrollback_lines: usize) -> Self {
        let pane_id = PaneId("pane-1".to_string());
        let pane = TerminalPane::new(pane_id.clone());
        let mut panes = HashMap::new();
        panes.insert(pane_id.clone(), pane);

        Self {
            layout: PaneLayout::Single(pane_id.clone()),
            panes,
            active_pane: pane_id,
            next_pane_num: 2,
            scrollback_lines,
            workspace_name: None,
        }
    }

    /// Create a TerminalManager from a layout template (no terminals spawned).
    pub fn from_template(template: &LayoutTemplate) -> Self {
        let pane_ids = template.layout.pane_ids();
        let mut panes = HashMap::new();
        let mut max_num = 1_usize;
        for id in &pane_ids {
            let mut pane = TerminalPane::new(id.clone());
            if let Some(label) = template.pane_labels.get(&id.0) {
                if let Some(tab) = pane.tabs.first_mut() {
                    tab.title = label.clone();
                    tab.custom_title = true;
                }
            }
            // Track highest pane number for next_pane_num
            if let Some(num_str) = id.0.strip_prefix("pane-") {
                if let Ok(n) = num_str.parse::<usize>() {
                    max_num = max_num.max(n);
                }
            }
            panes.insert(id.clone(), pane);
        }
        let active_pane = pane_ids.first().cloned()
            .unwrap_or_else(|| PaneId("pane-1".to_string()));
        Self {
            layout: template.layout.clone(),
            panes,
            active_pane,
            next_pane_num: max_num + 1,
            scrollback_lines: 10000,
            workspace_name: None,
        }
    }

    /// Set the workspace name for env var injection into spawned terminals.
    pub fn set_workspace_name(&mut self, name: &str) {
        self.workspace_name = Some(name.to_string());
    }

    /// Capture the current layout as a reusable template.
    pub fn to_template(&self, name: &str, description: &str) -> LayoutTemplate {
        let mut pane_labels = HashMap::new();
        for (id, pane) in &self.panes {
            if let Some(tab) = pane.tabs.get(pane.active_tab) {
                pane_labels.insert(id.0.clone(), tab.title.clone());
            }
        }
        LayoutTemplate {
            name: name.to_string(),
            description: description.to_string(),
            layout: self.layout.clone(),
            pane_labels,
            builtin: false,
        }
    }

    pub fn set_scrollback(&mut self, lines: usize) {
        self.scrollback_lines = lines;
    }

    fn next_pane_id(&mut self) -> PaneId {
        let id = PaneId(format!("pane-{}", self.next_pane_num));
        self.next_pane_num += 1;
        id
    }

    // === Active pane/terminal access ===

    pub fn active_terminal(&mut self) -> Option<&mut AlacrittyTerminal> {
        let pane = self.panes.get_mut(&self.active_pane)?;
        pane.active_terminal()
    }

    pub fn active_terminal_ref(&self) -> Option<&AlacrittyTerminal> {
        let pane = self.panes.get(&self.active_pane)?;
        pane.active_terminal_ref()
    }

    /// Get the active pane's active tab's current working directory.
    /// Reads live /proc/PID/cwd if available, falls back to saved spawn-time cwd.
    pub fn active_cwd(&self) -> Option<String> {
        self.panes.get(&self.active_pane)?.active_tab_live_cwd()
    }

    /// Get ONLY the live process cwd (no fallback to saved cwd).
    pub fn active_process_cwd(&self) -> Option<String> {
        self.panes.get(&self.active_pane)?.active_tab_process_cwd()
    }

    /// Get the saved spawn-time cwd (may be stale).
    pub fn active_saved_cwd(&self) -> Option<String> {
        self.panes.get(&self.active_pane)?.active_tab_saved_cwd()
    }

    /// Get the active pane's active tab's shell command (program + args)
    pub fn active_shell_cmd(&self) -> Option<(&str, &[String])> {
        let pane = self.panes.get(&self.active_pane)?;
        let tab = pane.tabs.get(pane.active_tab)?;
        tab.shell_cmd.as_ref().map(|(s, a)| (s.as_str(), a.as_slice()))
    }

    /// Get the active terminal's title (set by the shell via OSC 0/2)
    pub fn active_terminal_title(&self) -> Option<String> {
        let pane = self.panes.get(&self.active_pane)?;
        let tab = pane.tabs.get(pane.active_tab)?;
        tab.terminal.as_ref()?.title()
    }

    /// Iterate over all terminals across all panes and tabs (immutable)
    pub fn all_terminals(&self) -> impl Iterator<Item = &AlacrittyTerminal> {
        self.panes.values()
            .flat_map(|pane| pane.tabs.iter())
            .filter_map(|tab| tab.terminal.as_ref())
    }

    pub fn active_pane_id(&self) -> Option<&PaneId> {
        Some(&self.active_pane)
    }

    pub fn active_pane_mut(&mut self) -> Option<&mut TerminalPane> {
        self.panes.get_mut(&self.active_pane)
    }

    pub fn set_active_pane(&mut self, pane_id: &PaneId) {
        if self.panes.contains_key(pane_id) {
            self.active_pane = pane_id.clone();
        }
    }

    /// Send text to a specific pane's active terminal with bracketed paste support.
    pub fn send_text_to_pane(&mut self, pane_id: &PaneId, text: &str) {
        if let Some(pane) = self.panes.get_mut(pane_id) {
            if let Some(term) = pane.active_terminal() {
                term.send_paste_input(text);
            }
        }
    }

    /// List all panes except the given one, returning (PaneId, active_tab_title).
    pub fn other_pane_summaries(&self, exclude: &PaneId) -> Vec<(PaneId, String)> {
        self.panes.iter()
            .filter(|(id, _)| *id != exclude)
            .map(|(id, pane)| {
                let title = pane.tab_titles().into_iter()
                    .find(|(_, _, active, ..)| *active)
                    .map(|(_, t, ..)| t)
                    .unwrap_or_else(|| id.0.clone());
                (id.clone(), title)
            })
            .collect()
    }

    pub fn set_active_tab_in_pane(&mut self, tab_index: usize) {
        if let Some(pane) = self.panes.get_mut(&self.active_pane) {
            pane.set_active_tab(tab_index);
        }
    }

    pub fn get_pane(&self, pane_id: &PaneId) -> Option<&TerminalPane> {
        self.panes.get(pane_id)
    }

    /// Iterate all panes
    pub fn all_panes(&self) -> impl Iterator<Item = &TerminalPane> {
        self.panes.values()
    }

    pub fn get_pane_mut(&mut self, pane_id: &PaneId) -> Option<&mut TerminalPane> {
        self.panes.get_mut(pane_id)
    }

    // === Resize terminals ===

    pub fn resize_pane_terminals(&mut self, pane_id: &PaneId, width_px: f32, height_px: f32, cell_w: f32, cell_h: f32) {
        let tab_strip_h = 28.0_f32;
        let padding = 8.0_f32;
        let cols = ((width_px - padding) / cell_w).floor().max(1.0) as u16;
        let rows = ((height_px - tab_strip_h - padding) / cell_h).floor().max(1.0) as u16;
        if let Some(pane) = self.panes.get_mut(pane_id) {
            for tab in &mut pane.tabs {
                if let Some(ref mut term) = tab.terminal {
                    let (cur_cols, cur_rows) = term.dimensions();
                    if cur_cols != cols || cur_rows != rows {
                        term.resize(cols, rows);
                    }
                }
            }
        }
    }

    pub fn resize_all_panes(&mut self, avail_w: f32, avail_h: f32, cell_w: f32, cell_h: f32) {
        if let Some(layout) = self.active_layout().cloned() {
            let sizes = Self::compute_pane_sizes(&layout, avail_w, avail_h);
            for (pane_id, w, h) in sizes {
                self.resize_pane_terminals(&pane_id, w, h, cell_w, cell_h);
            }
        }
    }

    fn compute_pane_sizes(layout: &PaneLayout, w: f32, h: f32) -> Vec<(PaneId, f32, f32)> {
        match layout {
            PaneLayout::Single(id) => vec![(id.clone(), w, h)],
            PaneLayout::Horizontal { left, right, ratio } => {
                let handle = 6.0_f32;
                let usable = (w - handle).max(0.0);
                let lw = usable * ratio;
                let rw = usable * (1.0 - ratio);
                let mut sizes = Self::compute_pane_sizes(left, lw, h);
                sizes.extend(Self::compute_pane_sizes(right, rw, h));
                sizes
            }
            PaneLayout::Vertical { top, bottom, ratio } => {
                let handle = 6.0_f32;
                let usable = (h - handle).max(0.0);
                let th = usable * ratio;
                let bh = usable * (1.0 - ratio);
                let mut sizes = Self::compute_pane_sizes(top, w, th);
                sizes.extend(Self::compute_pane_sizes(bottom, w, bh));
                sizes
            }
        }
    }

    // === Spawn ===

    /// Create an AlacrittyTerminal with CWD fallback: tries cwd first, then None on failure.
    /// Returns (terminal, actual_cwd_used) so callers can record what worked.
    fn create_terminal_with_fallback(
        shell: &str, args: &[String], cwd: Option<&str>, scrollback: usize,
        extra_env: &std::collections::HashMap<String, String>,
    ) -> Result<(AlacrittyTerminal, Option<String>), String> {
        match AlacrittyTerminal::with_scrollback(120, 40, 8, 20, shell, args, cwd, scrollback, extra_env) {
            Ok(t) => Ok((t, cwd.map(|s| s.to_string()))),
            Err(e) if cwd.is_some() => {
                eprintln!("[amux] spawn failed with cwd {:?}: {}, retrying with default", cwd, e);
                let t = AlacrittyTerminal::with_scrollback(120, 40, 8, 20, shell, args, None, scrollback, extra_env)?;
                Ok((t, None))
            }
            Err(e) => Err(e),
        }
    }

    /// Spawn a terminal in the active pane's active tab using AlacrittyTerminal
    pub fn spawn_in_active(&mut self, shell: &str, args: &[String], cwd: Option<&str>) -> Result<(), String> {
        self.spawn_in_pane(&self.active_pane.clone(), shell, args, cwd)
    }

    /// Spawn a terminal in a specific pane's active tab.
    /// If cwd is invalid, automatically retries with no cwd (OS default).
    pub fn spawn_in_pane(&mut self, pane_id: &PaneId, shell: &str, args: &[String], cwd: Option<&str>) -> Result<(), String> {
        let pane = self.panes.get_mut(pane_id).ok_or("pane not found")?;
        let tab = pane.tabs.get_mut(pane.active_tab).ok_or("no active tab")?;
        if tab.terminal.is_some() {
            return Ok(()); // already has a terminal
        }
        let mut extra_env = std::collections::HashMap::new();
        extra_env.insert("AMUX_PANE_ID".to_string(), pane_id.0.clone());
        if let Some(ref ws) = self.workspace_name {
            extra_env.insert("AMUX_WORKSPACE".to_string(), ws.clone());
        }
        match Self::create_terminal_with_fallback(shell, args, cwd, self.scrollback_lines, &extra_env) {
            Ok((term, actual_cwd)) => {
                tab.terminal = Some(term);
                tab.cwd = actual_cwd;
                tab.shell_cmd = Some((shell.to_string(), args.to_vec()));
                Ok(())
            }
            Err(e) => {
                // Mark the tab with an error state so the render layer can show
                // an error message instead of a silently blank tab.
                tab.title = format!("Spawn failed: {}", e);
                tab.custom_title = true;
                tab.exited = true;
                Err(e)
            }
        }
    }

    /// Spawn terminals for all tabs in a pane, using each tab's saved cwd if available.
    /// Used during session restore to populate all tabs at once.
    pub fn spawn_all_tabs_in_pane(&mut self, pane_id: &PaneId, shell: &str, args: &[String], default_cwd: Option<&str>) {
        let pane = match self.panes.get_mut(pane_id) {
            Some(p) => p,
            None => return,
        };
        for tab in &mut pane.tabs {
            if tab.terminal.is_some() {
                continue;
            }
            let cwd_owned = tab.cwd.clone();
            let cwd = cwd_owned.as_deref().or(default_cwd);
            // Use tab's saved shell_cmd if available (e.g. WSL), else use provided default
            let (tab_shell, tab_args) = if let Some((ref s, ref a)) = tab.shell_cmd {
                (s.as_str(), a.as_slice())
            } else {
                (shell, args)
            };
            let mut extra_env = std::collections::HashMap::new();
            extra_env.insert("AMUX_PANE_ID".to_string(), pane_id.0.clone());
            if let Some(ref ws) = self.workspace_name {
                extra_env.insert("AMUX_WORKSPACE".to_string(), ws.clone());
            }
            let result = Self::create_terminal_with_fallback(tab_shell, tab_args, cwd, self.scrollback_lines, &extra_env);
            let (term, used_cwd) = match result {
                Ok(pair) => pair,
                Err(e) => {
                    // Mark the tab as failed so the render layer can show
                    // an error state instead of a silently blank tab.
                    tab.title = format!("Spawn failed: {}", e);
                    tab.custom_title = true;
                    tab.exited = true;
                    eprintln!("[amux] spawn tab failed: {}", e);
                    continue;
                }
            };
            tab.terminal = Some(term);
            tab.cwd = used_cwd;
            if tab.shell_cmd.is_none() {
                tab.shell_cmd = Some((tab_shell.to_string(), tab_args.to_vec()));
            }
        }
    }

    /// Restart the terminal in the active pane's active tab (replace dead terminal with new one).
    /// If cwd is invalid, automatically retries with no cwd (OS default).
    pub fn restart_active_terminal(&mut self, shell: &str, args: &[String], cwd: Option<&str>) -> Result<(), String> {
        let active_pane_id = self.active_pane.clone();
        let pane = self.panes.get_mut(&active_pane_id).ok_or("pane not found")?;
        let tab = pane.tabs.get_mut(pane.active_tab).ok_or("no active tab")?;
        // Drop old terminal
        tab.terminal = None;
        tab.exited = false;
        // Spawn new one with fallback
        let mut extra_env = std::collections::HashMap::new();
        extra_env.insert("AMUX_PANE_ID".to_string(), active_pane_id.0.clone());
        if let Some(ref ws) = self.workspace_name {
            extra_env.insert("AMUX_WORKSPACE".to_string(), ws.clone());
        }
        let (term, actual_cwd) = Self::create_terminal_with_fallback(shell, args, cwd, self.scrollback_lines, &extra_env)?;
        tab.terminal = Some(term);
        tab.cwd = actual_cwd;
        tab.shell_cmd = Some((shell.to_string(), args.to_vec()));
        Ok(())
    }

    // === Tab operations (per-pane) ===

    pub fn add_tab_to_active_pane(&mut self, title: String) -> Option<usize> {
        let pane = self.panes.get_mut(&self.active_pane)?;
        Some(pane.add_tab(title))
    }

    pub fn close_active_tab(&mut self) -> bool {
        let pane = self.panes.get_mut(&self.active_pane);
        match pane {
            Some(pane) => pane.close_tab(pane.active_tab),
            None => false,
        }
    }

    // === Split ===

    pub fn split_active_pane(&mut self, direction: SplitDirection) {
        let new_pane_id = self.next_pane_id();
        let new_pane = TerminalPane::new(new_pane_id.clone());
        self.panes.insert(new_pane_id.clone(), new_pane);

        let active = self.active_pane.clone();
        if !Self::split_in_layout(&mut self.layout, &active, &new_pane_id, direction) {
            eprintln!("[amux] split_in_layout failed: active pane {:?} not found in layout", active);
            self.panes.remove(&new_pane_id);
            return;
        }
        self.active_pane = new_pane_id;
    }

    fn split_in_layout(layout: &mut PaneLayout, target: &PaneId, new_pane: &PaneId, direction: SplitDirection) -> bool {
        match layout {
            PaneLayout::Single(id) if id == target => {
                let old = std::mem::replace(layout, PaneLayout::Single(PaneId("temp".to_string())));
                *layout = match direction {
                    SplitDirection::Horizontal => PaneLayout::Horizontal {
                        left: Box::new(old),
                        right: Box::new(PaneLayout::Single(new_pane.clone())),
                        ratio: 0.5,
                    },
                    SplitDirection::Vertical => PaneLayout::Vertical {
                        top: Box::new(old),
                        bottom: Box::new(PaneLayout::Single(new_pane.clone())),
                        ratio: 0.5,
                    },
                };
                true
            }
            PaneLayout::Horizontal { left, right, .. } => {
                Self::split_in_layout(left, target, new_pane, direction)
                    || Self::split_in_layout(right, target, new_pane, direction)
            }
            PaneLayout::Vertical { top, bottom, .. } => {
                Self::split_in_layout(top, target, new_pane, direction)
                    || Self::split_in_layout(bottom, target, new_pane, direction)
            }
            _ => false,
        }
    }

    // === Close pane ===

    pub fn close_active_pane(&mut self) -> bool {
        if self.panes.len() <= 1 {
            return false;
        }
        let closed = self.active_pane.clone();
        // Kill all terminal processes in the pane being closed to prevent
        // orphaned child processes.
        if let Some(pane) = self.panes.get(&closed) {
            for tab in &pane.tabs {
                if let Some(ref term) = tab.terminal {
                    term.kill_child();
                }
            }
        }
        if Self::remove_from_layout(&mut self.layout, &closed) {
            self.panes.remove(&closed);
            self.active_pane = Self::first_pane(&self.layout)
                .or_else(|| self.panes.keys().next().cloned())
                .unwrap_or_else(|| PaneId("pane-1".to_string()));
            true
        } else {
            false
        }
    }

    fn remove_from_layout(layout: &mut PaneLayout, target: &PaneId) -> bool {
        match layout {
            PaneLayout::Horizontal { left, right, .. } => {
                if matches!(**left, PaneLayout::Single(ref id) if id == target) {
                    *layout = *right.clone();
                    return true;
                }
                if matches!(**right, PaneLayout::Single(ref id) if id == target) {
                    *layout = *left.clone();
                    return true;
                }
                Self::remove_from_layout(left, target)
                    || Self::remove_from_layout(right, target)
            }
            PaneLayout::Vertical { top, bottom, .. } => {
                if matches!(**top, PaneLayout::Single(ref id) if id == target) {
                    *layout = *bottom.clone();
                    return true;
                }
                if matches!(**bottom, PaneLayout::Single(ref id) if id == target) {
                    *layout = *top.clone();
                    return true;
                }
                Self::remove_from_layout(top, target)
                    || Self::remove_from_layout(bottom, target)
            }
            _ => false,
        }
    }

    // === Move tab between panes ===

    /// Move a tab from one pane to another.
    /// If the source pane becomes empty, it is removed from the layout.
    /// Returns true if the move was successful.
    pub fn move_tab_to_pane(
        &mut self,
        source_pane: &PaneId,
        tab_index: usize,
        target_pane: &PaneId,
    ) -> bool {
        if source_pane == target_pane {
            return false;
        }
        // Validate both panes exist and tab index is valid
        let src_tab_count = match self.panes.get(source_pane) {
            Some(p) => p.tabs.len(),
            None => return false,
        };
        if tab_index >= src_tab_count {
            return false;
        }
        if !self.panes.contains_key(target_pane) {
            return false;
        }

        // Remove tab from source
        let src = match self.panes.get_mut(source_pane) {
            Some(p) => p,
            None => return false,
        };
        let tab = src.tabs.remove(tab_index);
        if src.active_tab >= src.tabs.len() && !src.tabs.is_empty() {
            src.active_tab = src.tabs.len() - 1;
        }

        // Add tab to target and make it active
        let target = match self.panes.get_mut(target_pane) {
            Some(p) => p,
            None => return false,
        };
        target.tabs.push(tab);
        target.active_tab = target.tabs.len() - 1;

        // If source pane is now empty, remove it from layout
        let source_empty = self.panes.get(source_pane).map_or(true, |p| p.tabs.is_empty());
        if source_empty {
            if Self::remove_from_layout(&mut self.layout, source_pane) {
                self.panes.remove(source_pane);
            } else {
                eprintln!("[amux] remove_from_layout failed in move_tab_to_pane: source pane {:?} not found in layout", source_pane);
            }
            // If the closed pane was active, switch to target
            if &self.active_pane == source_pane {
                self.active_pane = target_pane.clone();
            }
        }

        // Make target the active pane
        self.active_pane = target_pane.clone();
        true
    }

    /// Reorder a tab within the same pane (drag to new position).
    pub fn reorder_tab(&mut self, pane_id: &PaneId, from: usize, to: usize) -> bool {
        let pane = match self.panes.get_mut(pane_id) {
            Some(p) => p,
            None => return false,
        };
        if from >= pane.tabs.len() || to >= pane.tabs.len() || from == to {
            return false;
        }
        let tab = pane.tabs.remove(from);
        pane.tabs.insert(to, tab);
        // Keep active_tab pointing at the same tab after the move
        if pane.active_tab == from {
            pane.active_tab = to;
        } else if from < pane.active_tab && to >= pane.active_tab {
            pane.active_tab -= 1;
        } else if from > pane.active_tab && to <= pane.active_tab {
            pane.active_tab += 1;
        }
        true
    }

    // === Resize ===

    /// Reset all split ratios to 0.5 (equal split).
    pub fn equalize_splits(&mut self) {
        Self::reset_ratios(&mut self.layout);
    }

    fn reset_ratios(layout: &mut PaneLayout) {
        match layout {
            PaneLayout::Single(_) => {}
            PaneLayout::Horizontal { left, right, ratio } => {
                *ratio = 0.5;
                Self::reset_ratios(left);
                Self::reset_ratios(right);
            }
            PaneLayout::Vertical { top, bottom, ratio } => {
                *ratio = 0.5;
                Self::reset_ratios(top);
                Self::reset_ratios(bottom);
            }
        }
    }

    pub fn update_split_ratio(&mut self, first_pane_id: &PaneId, new_ratio: f32) {
        Self::update_ratio_in_layout(&mut self.layout, first_pane_id, new_ratio);
    }

    fn update_ratio_in_layout(layout: &mut PaneLayout, target_second: &PaneId, new_ratio: f32) -> bool {
        match layout {
            PaneLayout::Single(_) => false,
            PaneLayout::Horizontal { left, right, ratio } => {
                if Self::first_pane(right).as_ref() == Some(target_second) {
                    *ratio = new_ratio.clamp(0.1, 0.9);
                    return true;
                }
                Self::update_ratio_in_layout(left, target_second, new_ratio)
                    || Self::update_ratio_in_layout(right, target_second, new_ratio)
            }
            PaneLayout::Vertical { top, bottom, ratio } => {
                if Self::first_pane(bottom).as_ref() == Some(target_second) {
                    *ratio = new_ratio.clamp(0.1, 0.9);
                    return true;
                }
                Self::update_ratio_in_layout(top, target_second, new_ratio)
                    || Self::update_ratio_in_layout(bottom, target_second, new_ratio)
            }
        }
    }

    // === Activity detection ===

    /// Check all terminals for new output. Mark inactive pane tabs that have activity.
    /// Also detects AI agent status from terminal output.
    /// Returns notifications for agent status transitions on non-active tabs.
    /// Called from the polling loop.
    pub fn poll_activity(&mut self) -> Vec<AgentNotification> {
        let mut notifications = Vec::new();
        let active_pane_id = self.active_pane.clone();
        for (pane_id, pane) in &mut self.panes {
            let is_active_pane = *pane_id == active_pane_id;
            for (tab_idx, tab) in pane.tabs.iter_mut().enumerate() {
                let is_active_tab = tab_idx == pane.active_tab && is_active_pane;
                if let Some(ref term) = tab.terminal {
                    // Detect child process exit
                    if term.child_exited() && !tab.exited {
                        tab.exited = true;
                        if !is_active_tab {
                            tab.has_activity = true;
                        }
                    }
                    let cursor_line = term.with_term(|t| {
                        t.renderable_content().cursor.point.line.0
                    });
                    if cursor_line != tab.last_cursor_line {
                        tab.last_cursor_line = cursor_line;
                        if !is_active_tab {
                            tab.has_activity = true;
                        }
                    }

                    // Auto-detect agent kind from terminal title on first output
                    if tab.agent_kind.is_none() {
                        if let Some(title) = term.title() {
                            tab.agent_kind = Self::detect_agent_kind(&title);
                        }
                    }

                    // Detect agent status from recent terminal output
                    if tab.agent_kind.is_some() {
                        let old_status = tab.agent_status.clone();
                        let lines = term.last_lines(5);
                        tab.agent_status = Self::detect_agent_status(
                            tab.agent_kind.as_ref().unwrap(),
                            &lines,
                            tab.exited,
                        );
                        // Notify on status change (thinking→done or thinking→waiting)
                        if !is_active_tab && old_status != tab.agent_status {
                            if matches!(tab.agent_status, Some(AgentStatus::Done | AgentStatus::Waiting | AgentStatus::Error)) {
                                tab.has_activity = true;
                                let title = tab.terminal.as_ref()
                                    .and_then(|t| t.title())
                                    .unwrap_or_else(|| tab.title.clone());
                                notifications.push(AgentNotification {
                                    pane_id: pane_id.clone(),
                                    tab_index: tab_idx,
                                    tab_title: title,
                                    agent_kind: tab.agent_kind.clone().unwrap(),
                                    new_status: tab.agent_status.clone().unwrap(),
                                });
                            }
                        }
                    }
                }
            }
        }
        notifications
    }

    /// Collect summary of all detected agents across all panes/tabs.
    /// Returns (short_name, status_icon, color_rgb) for each agent tab.
    pub fn agent_summaries(&self) -> Vec<(String, &'static str, u32)> {
        let mut out = Vec::new();
        for pane in self.panes.values() {
            for tab in &pane.tabs {
                if let (Some(kind), Some(status)) = (&tab.agent_kind, &tab.agent_status) {
                    let name = if tab.custom_title {
                        tab.title.clone()
                    } else {
                        format!("{:?}", kind)
                    };
                    out.push((name, status.icon(), status.color_rgb()));
                }
            }
        }
        out
    }

    /// Detect AI agent kind from terminal title
    fn detect_agent_kind(title: &str) -> Option<AgentKind> {
        let title_lower = title.to_lowercase();
        if title_lower.contains("claude") { return Some(AgentKind::Claude); }
        if title_lower.contains("aider") { return Some(AgentKind::Aider); }
        if title_lower.contains("opencode") { return Some(AgentKind::OpenCode); }
        if title_lower.contains("codex") { return Some(AgentKind::Codex); }
        if title_lower.contains("gemini") { return Some(AgentKind::Gemini); }
        if title_lower.contains("copilot") { return Some(AgentKind::Copilot); }
        None
    }

    /// Detect agent status from the last few lines of terminal output.
    fn detect_agent_status(kind: &AgentKind, lines: &[String], exited: bool) -> Option<AgentStatus> {
        if exited {
            return Some(AgentStatus::Done);
        }
        if lines.is_empty() {
            return None;
        }

        // Check lines from bottom up for the most recent signal
        for line in lines.iter().rev() {
            let trimmed = line.trim();
            if trimmed.is_empty() { continue; }

            // Common error patterns (check first — errors override other states)
            if trimmed.contains("Error") || trimmed.contains("error:")
                || trimmed.contains("ERROR") || trimmed.contains("failed")
                || trimmed.contains("Permission denied") || trimmed.contains("panic")
            {
                return Some(AgentStatus::Error);
            }

            match kind {
                AgentKind::Claude => {
                    // Claude Code: ">" prompt = waiting, spinner/progress = thinking
                    if trimmed.ends_with('>')
                        || trimmed.ends_with('>') && trimmed.contains('$')
                        || trimmed == ">"
                    {
                        return Some(AgentStatus::Waiting);
                    }
                    if trimmed.contains("Thinking") || trimmed.contains("⠋")
                        || trimmed.contains("⠙") || trimmed.contains("⠹")
                        || trimmed.contains("⠸") || trimmed.contains("⠼")
                        || trimmed.contains("⠴") || trimmed.contains("⠦")
                        || trimmed.contains("⠧") || trimmed.contains("⠇")
                        || trimmed.contains("⠏")
                    {
                        return Some(AgentStatus::Thinking);
                    }
                }
                AgentKind::Aider => {
                    // Aider: "aider>" or ">" = waiting, "Thinking..." = thinking
                    if trimmed.ends_with('>') || trimmed.starts_with("aider>") {
                        return Some(AgentStatus::Waiting);
                    }
                    if trimmed.contains("Thinking") || trimmed.contains("sending") {
                        return Some(AgentStatus::Thinking);
                    }
                }
                AgentKind::OpenCode => {
                    if trimmed.ends_with('>') || trimmed.contains("opencode>") {
                        return Some(AgentStatus::Waiting);
                    }
                    if trimmed.contains("Thinking") || trimmed.contains("Processing") {
                        return Some(AgentStatus::Thinking);
                    }
                }
                AgentKind::Codex => {
                    if trimmed.ends_with('>') {
                        return Some(AgentStatus::Waiting);
                    }
                    if trimmed.contains("Thinking") || trimmed.contains("Running") {
                        return Some(AgentStatus::Thinking);
                    }
                }
                AgentKind::Gemini | AgentKind::Copilot => {
                    if trimmed.ends_with('>') || trimmed.ends_with("$ ") {
                        return Some(AgentStatus::Waiting);
                    }
                    if trimmed.contains("Thinking") || trimmed.contains("Generating") {
                        return Some(AgentStatus::Thinking);
                    }
                }
            }
        }
        // If agent is detected but no status signal found, assume thinking (output in progress)
        Some(AgentStatus::Thinking)
    }

    /// Clear activity flag for the active pane's active tab.
    pub fn clear_active_activity(&mut self) {
        if let Some(pane) = self.panes.get_mut(&self.active_pane) {
            if let Some(tab) = pane.tabs.get_mut(pane.active_tab) {
                tab.has_activity = false;
            }
        }
    }

    /// Check if any pane in this manager has activity (for workspace-level notification).
    pub fn has_any_activity(&self) -> bool {
        self.panes.values().any(|pane| {
            pane.tabs.iter().any(|tab| tab.has_activity)
        })
    }

    // === Layout / query ===

    pub fn active_layout(&self) -> Option<&PaneLayout> {
        Some(&self.layout)
    }

    pub fn pane_iter(&self) -> impl Iterator<Item = (&PaneId, &TerminalPane)> {
        self.panes.iter()
    }

    pub fn total_panes(&self) -> usize {
        self.panes.len()
    }

    pub fn total_tabs(&self) -> usize {
        self.panes.values().map(|p| p.tab_count()).sum()
    }

    fn first_pane(layout: &PaneLayout) -> Option<PaneId> {
        match layout {
            PaneLayout::Single(id) => Some(id.clone()),
            PaneLayout::Horizontal { left, .. } => Self::first_pane(left),
            PaneLayout::Vertical { top, .. } => Self::first_pane(top),
        }
    }

    // === Layout persistence ===

    /// Serialize the current layout to JSON
    pub fn save_layout(&self) -> String {
        let mut pane_states = HashMap::new();
        for (id, pane) in &self.panes {
            let tabs: Vec<SavedTab> = pane.tabs.iter().map(|t| SavedTab {
                title: t.title.clone(),
                custom_title: t.custom_title,
                kind: t.kind.clone(),
                cwd: t.cwd.clone(),
                shell_cmd: t.shell_cmd.clone(),
            }).collect();
            pane_states.insert(id.0.clone(), SavedPane {
                tabs,
                active_tab: pane.active_tab,
            });
        }
        let state = LayoutState {
            layout: self.layout.clone(),
            active_pane: self.active_pane.clone(),
            next_pane_num: self.next_pane_num,
            pane_states: Some(pane_states),
        };
        serde_json::to_string(&state).unwrap_or_else(|e| {
            eprintln!("[amux] save_layout serialization failed: {}", e);
            String::new()
        })
    }

    /// Ensure all pane IDs in the layout have corresponding pane entries.
    /// Creates missing panes so rendering never hits "Empty pane".
    pub fn heal_layout(&mut self) {
        let layout_ids = self.layout.pane_ids();
        for id in &layout_ids {
            if !self.panes.contains_key(id) {
                self.panes.insert(id.clone(), TerminalPane::new(id.clone()));
            }
        }
        // Also ensure active_pane is valid
        if !self.panes.contains_key(&self.active_pane) {
            if let Some(first) = layout_ids.first() {
                self.active_pane = first.clone();
            }
        }
    }

    /// Restore layout from JSON, creating empty panes for each pane ID
    pub fn restore_layout(json: &str) -> Option<Self> {
        let state: LayoutState = serde_json::from_str(json).ok()?;
        let pane_ids = state.layout.pane_ids();
        if pane_ids.is_empty() {
            return None;
        }
        let mut panes = HashMap::new();
        for id in &pane_ids {
            // Restore tab state if saved, otherwise create default single tab
            let pane = if let Some(ref ps) = state.pane_states {
                if let Some(saved) = ps.get(&id.0) {
                    let tabs: Vec<PaneTab> = saved.tabs.iter().map(|st| PaneTab {
                        title: st.title.clone(),
                        custom_title: st.custom_title,
                        kind: st.kind.clone(),
                        terminal: None,
                        has_activity: false,
                        exited: false,
                        cwd: st.cwd.clone(),
                        shell_cmd: st.shell_cmd.clone(),
                        agent_kind: None,
                        agent_status: None,
                        last_cursor_line: 0,
                    }).collect();
                    let active_tab = if saved.active_tab < tabs.len() { saved.active_tab } else { 0 };
                    if tabs.is_empty() {
                        TerminalPane::new(id.clone())
                    } else {
                        TerminalPane { id: id.clone(), tabs, active_tab }
                    }
                } else {
                    TerminalPane::new(id.clone())
                }
            } else {
                TerminalPane::new(id.clone())
            };
            panes.insert(id.clone(), pane);
        }
        // Validate active_pane exists in restored layout, fallback to first pane
        let active_pane = if pane_ids.contains(&state.active_pane) {
            state.active_pane
        } else {
            pane_ids[0].clone()
        };
        Some(Self {
            layout: state.layout,
            panes,
            active_pane,
            next_pane_num: state.next_pane_num,
            scrollback_lines: 10000, // overridden by caller with config value
            workspace_name: None,
        })
    }

    // ── Bridge API (Phase 1.1) ──────────────────────────────────────────

    /// List all panes with metadata for the Bridge API.
    pub fn pane_list(&self) -> Vec<PaneInfo> {
        self.panes.iter().map(|(id, pane)| {
            let tab = pane.tabs.get(pane.active_tab);
            let tab_title = tab.map(|t| {
                if t.custom_title {
                    t.title.clone()
                } else if let Some(ref term) = t.terminal {
                    term.title().unwrap_or_else(|| t.title.clone())
                } else {
                    t.title.clone()
                }
            }).unwrap_or_default();
            let agent_kind = tab.and_then(|t| t.agent_kind.as_ref().map(|k| format!("{:?}", k)));
            let agent_status = tab.and_then(|t| t.agent_status.as_ref().map(|s| s.label().to_string()));
            let tab_kind = tab.map(|t| match &t.kind {
                TabKind::Terminal => "terminal",
                TabKind::Browser { .. } => "browser",
                TabKind::Preview { .. } => "preview",
            }).unwrap_or("terminal").to_string();
            PaneInfo {
                pane_id: id.clone(),
                tab_title,
                agent_kind,
                agent_status,
                tab_kind,
            }
        }).collect()
    }

    /// Read the last N lines (capped at 200) from a pane's active terminal.
    pub fn pane_read(&self, pane_id: &PaneId, lines: usize) -> Option<Vec<String>> {
        let pane = self.panes.get(pane_id)?;
        let tab = pane.tabs.get(pane.active_tab)?;
        let term = tab.terminal.as_ref()?;
        Some(term.last_lines(lines.min(200)))
    }

    /// Send text followed by a newline to a pane's active terminal.
    pub fn pane_send_text(&mut self, target: &PaneId, text: &str) -> Result<(), String> {
        let pane = self.panes.get_mut(target).ok_or_else(|| format!("pane not found: {}", target.0))?;
        let term = pane.active_terminal().ok_or_else(|| "pane has no active terminal".to_string())?;
        term.send_input(text.as_bytes());
        term.send_input(b"\n");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> TerminalManager {
        TerminalManager::with_scrollback(10000)
    }

    fn pane_id(n: usize) -> PaneId {
        PaneId(format!("pane-{}", n))
    }

    #[test]
    fn test_single_pane_layout() {
        let mgr = make_manager();
        let layout = mgr.active_layout().unwrap();
        let ids = layout.pane_ids();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], pane_id(1));
    }

    #[test]
    fn test_split_horizontal() {
        let mut mgr = make_manager();
        mgr.split_active_pane(SplitDirection::Horizontal);
        let ids = mgr.active_layout().unwrap().pane_ids();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn test_split_vertical() {
        let mut mgr = make_manager();
        mgr.split_active_pane(SplitDirection::Vertical);
        let ids = mgr.active_layout().unwrap().pane_ids();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn test_close_pane_merges_layout() {
        let mut mgr = make_manager();
        mgr.split_active_pane(SplitDirection::Horizontal);
        assert_eq!(mgr.active_layout().unwrap().pane_ids().len(), 2);
        assert_eq!(mgr.panes.len(), 2);
        let closed = mgr.close_active_pane();
        assert!(closed);
        assert_eq!(mgr.active_layout().unwrap().pane_ids().len(), 1);
        assert_eq!(mgr.panes.len(), 1);
    }

    #[test]
    fn test_close_last_pane_returns_false() {
        let mut mgr = make_manager();
        let closed = mgr.close_active_pane();
        assert!(!closed);
        assert_eq!(mgr.panes.len(), 1);
    }

    #[test]
    fn test_add_tab_to_active_pane() {
        let mut mgr = make_manager();
        let idx = mgr.add_tab_to_active_pane("Test Tab".into());
        assert!(idx.is_some());
        let pane = mgr.get_pane(&pane_id(1)).unwrap();
        assert_eq!(pane.tab_count(), 2);
    }

    #[test]
    fn test_close_last_tab_returns_false() {
        let mut mgr = make_manager();
        let closed = mgr.close_active_tab();
        assert!(!closed);
    }

    #[test]
    fn test_close_tab_removes_correct_tab() {
        let mut mgr = make_manager();
        mgr.add_tab_to_active_pane("Tab 2".into());
        mgr.add_tab_to_active_pane("Tab 3".into());
        assert_eq!(mgr.panes[&pane_id(1)].tab_count(), 3);
        let closed = mgr.close_active_tab();
        assert!(closed);
        assert_eq!(mgr.panes[&pane_id(1)].tab_count(), 2);
    }

    #[test]
    fn test_move_tab_between_panes() {
        let mut mgr = make_manager();
        mgr.split_active_pane(SplitDirection::Horizontal);
        // Add a tab to pane-1
        mgr.set_active_pane(&pane_id(1));
        mgr.add_tab_to_active_pane("Movable".into());
        // Move tab from pane-1 to pane-2
        let moved = mgr.move_tab_to_pane(&pane_id(1), 1, &pane_id(2));
        assert!(moved);
        assert_eq!(mgr.panes[&pane_id(1)].tab_count(), 1);
        assert_eq!(mgr.panes[&pane_id(2)].tab_count(), 2);
    }

    #[test]
    fn test_move_tab_removes_empty_source_pane() {
        let mut mgr = make_manager();
        mgr.split_active_pane(SplitDirection::Horizontal);
        // pane-1 has 1 tab, pane-2 has 1 tab
        assert_eq!(mgr.panes[&pane_id(1)].tab_count(), 1);
        assert_eq!(mgr.panes[&pane_id(2)].tab_count(), 1);
        // Move the only tab from pane-2 to pane-1
        let moved = mgr.move_tab_to_pane(&pane_id(2), 0, &pane_id(1));
        assert!(moved);
        // pane-2 should be removed from layout
        assert!(!mgr.panes.contains_key(&pane_id(2)));
        assert_eq!(mgr.active_layout().unwrap().pane_ids().len(), 1);
    }

    #[test]
    fn test_total_panes() {
        let mut mgr = make_manager();
        assert_eq!(mgr.total_panes(), 1);
        mgr.split_active_pane(SplitDirection::Horizontal);
        assert_eq!(mgr.total_panes(), 2);
    }

    #[test]
    fn test_pane_list() {
        let mut mgr = make_manager();
        mgr.split_active_pane(SplitDirection::Horizontal);
        let list = mgr.pane_list();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|p| p.pane_id == pane_id(1)));
        assert!(list.iter().any(|p| p.pane_id == pane_id(2)));
    }
}
