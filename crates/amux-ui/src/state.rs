use amux_core::{
    Event, LayoutNode, SaveStatus as CoreSaveStatus, SessionOpError, SessionState, SplitAxis,
    SurfaceState, TabState, WorkspaceState, WorkspaceTarget,
};

/// Stringify a workspace target for the snapshot's `target_path`
/// field. `LocalPath` / `WindowsPath` stringify via `PathBuf::display`
/// (lossy on non-UTF-8, but fine for spawn cwd). `WslPath` carries a
/// Linux-style path as a plain string and is returned as-is; the
/// desktop consumer is expected to gate WSL spawns separately.
fn workspace_target_path_string(target: &WorkspaceTarget) -> Option<String> {
    match target {
        WorkspaceTarget::LocalPath { path } | WorkspaceTarget::WindowsPath { path } => {
            Some(path.display().to_string())
        }
        WorkspaceTarget::WslPath { path, .. } => Some(path.clone()),
    }
}
use amux_platform::{format_bytes, format_cpu_usage, get_load_status, PlatformCapabilities, SystemMetrics};

use crate::commands::UiAction;

/// Alias for the core SaveStatus type
pub type SaveStatus = CoreSaveStatus;

#[derive(Clone, Debug)]
pub struct UiState {
    pub session: SessionState,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub command_palette_selected_index: usize,
    pub last_error: Option<String>,
    pub activity_log: Vec<String>,
    pub save_status: SaveStatus,
    pub dirty: bool, // True if there are unsaved changes
    /// System metrics collector (refreshed on snapshot)
    pub system_metrics: Option<SystemMetrics>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceListItem {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    /// Target path as a plain string (stringified from the
    /// `WorkspaceTarget`). Used by the desktop shell to spawn the
    /// workspace's terminals in the right directory instead of
    /// inheriting amux's own launch cwd (which is `/` when launched
    /// from a macOS .app bundle and produces a `PWD=/` shell that
    /// prompt themes flag with a lock icon).
    pub target_path: Option<String>,
    /// Which [`WorkspaceGroupListItem`] this workspace belongs to.
    /// Pre-group sessions (or the single-default-group case) route
    /// everything through the default group id and the sidebar
    /// collapses it to a flat render.
    pub group_id: String,
}

/// Mirror of `amux_core::WorkspaceGroup` in the snapshot. The
/// sidebar iterates these to decide which group headers to draw
/// and in which order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceGroupListItem {
    pub id: String,
    /// Display name. Empty string is meaningful: the sidebar
    /// treats the default / migration group's empty name as "do
    /// not render a group header", which is how upgrading users
    /// keep the pre-group flat layout.
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentListItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub supported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileListItem {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenFileItem {
    pub relative_path: String,
    pub display_path: String,
    pub content_preview: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveSurfaceItem {
    pub pane_id: String,
    pub tab_id: String,
    pub tab_title: String,
    pub surface_kind: &'static str,
    pub summary_lines: Vec<String>,
    pub content_lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabSnapshot {
    pub id: String,
    pub title: String,
    pub is_active: bool,
    pub surface_kind: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneSnapshot {
    pub id: String,
    pub is_active: bool,
    pub tabs: Vec<TabSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutSnapshot {
    Split(SplitSnapshot),
    Pane(PaneSnapshot),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitSnapshot {
    pub axis: SplitAxis,
    pub first: Box<LayoutSnapshot>,
    pub second: Box<LayoutSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub id: String,
    pub name: String,
    pub layout: LayoutSnapshot,
}

#[derive(Clone, Debug)]
pub struct AppSnapshot {
    pub workspaces: Vec<WorkspaceListItem>,
    pub workspace_groups: Vec<WorkspaceGroupListItem>,
    pub recent_workspaces: Vec<RecentWorkspaceItem>,
    pub agents: Vec<AgentListItem>,
    pub files: Vec<FileListItem>,
    pub open_files: Vec<OpenFileItem>,
    pub active_surface: Option<ActiveSurfaceItem>,
    pub active_workspace: Option<WorkspaceSnapshot>,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub command_palette_selected_index: usize,
    pub last_error: Option<String>,
    pub activity_log: Vec<String>,
    pub save_status: String, // "saved 2m ago", "unsaved", "saving"
    pub dirty: bool,
    pub platform_capabilities: PlatformCapabilities,
    // Status bar fields
    pub status_wsl_distro: Option<String>, // Current WSL distro if using WSL workspace
    pub status_split_count: usize,         // Number of splits in current layout
    pub status_terminal_shell: Option<String>, // Current terminal shell type
    // System metrics
    pub status_cpu_usage: Option<String>,    // e.g., "45%"
    pub status_cpu_cores: Option<String>,    // e.g., "8 cores"
    pub status_memory_usage: Option<String>, // e.g., "4.2/16.0 GB"
    pub status_memory_percent: Option<f32>,  // e.g., 26.2
    pub status_load_color: Option<String>,   // "green", "yellow", "red"
}

/// Simplified recent workspace item for UI display
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentWorkspaceItem {
    pub id: String,
    pub name: String,
    pub path: String, // Shortened path for display
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            session: SessionState::default(),
            command_palette_open: false,
            command_palette_query: String::new(),
            command_palette_selected_index: 0,
            last_error: None,
            activity_log: Vec::new(),
            save_status: SaveStatus::default(),
            dirty: false,
            system_metrics: None,
        }
    }
}

impl UiState {
    pub fn dispatch(&mut self, action: UiAction) -> Vec<Event> {
        match action {
            UiAction::ToggleCommandPalette => {
                self.command_palette_open = !self.command_palette_open;
                if self.command_palette_open {
                    self.command_palette_selected_index = 0;
                }
                self.push_activity(format!(
                    "ui: command palette {}",
                    if self.command_palette_open {
                        "opened"
                    } else {
                        "closed"
                    }
                ));
                Vec::new()
            }
            UiAction::SetCommandPaletteQuery(query) => {
                self.command_palette_query = query;
                self.command_palette_selected_index = 0;
                self.push_activity(format!(
                    "ui: command palette query '{}'",
                    self.command_palette_query
                ));
                Vec::new()
            }
            UiAction::AppendCommandPaletteQuery(segment) => {
                if self.command_palette_query.is_empty() {
                    self.command_palette_query = segment;
                } else {
                    self.command_palette_query =
                        format!("{} {}", self.command_palette_query.trim_end(), segment);
                }
                self.command_palette_selected_index = 0;
                self.push_activity(format!(
                    "ui: command palette query '{}'",
                    self.command_palette_query
                ));
                Vec::new()
            }
            UiAction::BackspaceCommandPaletteQuery => {
                self.command_palette_query.pop();
                while self.command_palette_query.ends_with(' ') {
                    self.command_palette_query.pop();
                }
                self.command_palette_selected_index = 0;
                self.push_activity(format!(
                    "ui: command palette query '{}'",
                    self.command_palette_query
                ));
                Vec::new()
            }
            UiAction::ClearCommandPaletteQuery => {
                self.command_palette_query.clear();
                self.command_palette_selected_index = 0;
                self.push_activity("ui: command palette query cleared");
                Vec::new()
            }
            UiAction::SetCommandPaletteSelectedIndex(index) => {
                self.command_palette_selected_index = index;
                self.push_activity(format!(
                    "ui: command palette selection {}",
                    self.command_palette_selected_index
                ));
                Vec::new()
            }
            UiAction::SelectNextCommandPaletteItem => {
                self.command_palette_selected_index =
                    self.command_palette_selected_index.saturating_add(1);
                self.push_activity("ui: command palette selection advanced");
                Vec::new()
            }
            UiAction::SelectPreviousCommandPaletteItem => {
                self.command_palette_selected_index =
                    self.command_palette_selected_index.saturating_sub(1);
                self.push_activity("ui: command palette selection rewound");
                Vec::new()
            }
            UiAction::OpenWorkspacePicker => Vec::new(),
            other => match other.to_core_command() {
                Some(command) => match self.session.apply(command) {
                    Ok(events) => {
                        self.last_error = None;
                        self.dirty = true; // Mark as dirty after any state change
                        for event in &events {
                            self.push_activity(format_event(event));
                        }
                        events
                    }
                    Err(err) => {
                        self.last_error = Some(format_session_error(err));
                        if let Some(error) = &self.last_error {
                            self.push_activity(format!("error: {error}"));
                        }
                        Vec::new()
                    }
                },
                None => Vec::new(),
            },
        }
    }

    /// Mark the session as saved and update status
    pub fn mark_saved(&mut self) {
        self.dirty = false;
        self.session.mark_saved();
        self.save_status = SaveStatus::Saved("just now".to_string());
    }

    /// Get the current save status description
    pub fn get_save_status(&self) -> SaveStatus {
        if self.dirty {
            return SaveStatus::Unsaved;
        }
        self.session.save_status()
    }

    pub fn snapshot(&mut self) -> AppSnapshot {
        // Format save status
        let save_status = match self.get_save_status() {
            SaveStatus::Saved(desc) => format!("saved {}", desc),
            SaveStatus::Unsaved => "unsaved".to_string(),
            SaveStatus::Saving => "saving...".to_string(),
        };

        // Format recent workspaces
        let recent_workspaces = self
            .session
            .recent_workspaces
            .iter()
                .take(5)
                .map(|r| {
                    let path = match &r.target {
                        amux_core::WorkspaceTarget::LocalPath { path } => {
                            path.to_string_lossy().to_string()
                        }
                        amux_core::WorkspaceTarget::WindowsPath { path } => {
                            path.to_string_lossy().to_string()
                        }
                    amux_core::WorkspaceTarget::WslPath { path, distro } => {
                        format!("{}:{}", distro, path)
                    }
                };
                RecentWorkspaceItem {
                    id: r.id.0.clone(),
                    name: r.name.clone(),
                    path,
                }
            })
            .collect();

        AppSnapshot {
            workspaces: self
                .session
                .workspaces
                .iter()
                .map(|workspace| WorkspaceListItem {
                    id: workspace.id.0.clone(),
                    name: workspace.name.clone(),
                    is_active: self.session.active_workspace_id.as_ref() == Some(&workspace.id),
                    target_path: workspace_target_path_string(&workspace.target),
                    group_id: workspace.group_id.0.clone(),
                })
                .collect(),
            workspace_groups: self
                .session
                .groups
                .iter()
                .map(|group| WorkspaceGroupListItem {
                    id: group.id.0.clone(),
                    name: group.name.clone(),
                })
                .collect(),
            recent_workspaces,
            agents: Vec::new(),
            files: Vec::new(),
            open_files: Vec::new(),
            active_surface: self
                .session
                .active_workspace()
                .and_then(active_surface_snapshot),
            active_workspace: self.session.active_workspace().map(workspace_snapshot),
            command_palette_open: self.command_palette_open,
            command_palette_query: self.command_palette_query.clone(),
            command_palette_selected_index: self.command_palette_selected_index,
            last_error: self.last_error.clone(),
            activity_log: self.activity_log.clone(),
            save_status,
            dirty: self.dirty,
            platform_capabilities: PlatformCapabilities::default(),
            // Status bar fields
            status_wsl_distro: self
                .session
                .active_workspace()
                .and_then(|ws| match &ws.target {
                    amux_core::WorkspaceTarget::WslPath { distro, .. } => Some(distro.clone()),
                    _ => None,
                }),
            status_split_count: self
                .session
                .active_workspace()
                .map(|ws| count_splits(&ws.layout))
                .unwrap_or(0),
            status_terminal_shell: self.get_active_terminal_shell(),
            // System metrics
            status_cpu_usage: self
                .system_metrics
                .as_ref()
                .map(|m| format_cpu_usage(m.cpu_usage)),
            status_cpu_cores: self
                .system_metrics
                .as_ref()
                .map(|m| format!("{} cores", m.cpu_count)),
            status_memory_usage: self.system_metrics.as_ref().map(|m| {
                format!(
                    "{}/{}",
                    format_bytes(m.memory_used),
                    format_bytes(m.memory_total)
                )
            }),
            status_memory_percent: self.system_metrics.as_ref().map(|m| m.memory_usage_percent),
            status_load_color: self
                .system_metrics
                .as_ref()
                .map(|m| get_load_status(m).to_string()),
        }
    }

    pub fn set_system_metrics(&mut self, metrics: Option<SystemMetrics>) {
        self.system_metrics = metrics;
    }

    /// Legacy metrics path used before HostPlatform injection is available.
    pub fn refresh_system_metrics_legacy(&mut self) {
        use amux_platform::SystemMetricsCollector;

        self.system_metrics = Some(SystemMetricsCollector::new().get_metrics());
    }

    fn get_active_terminal_shell(&self) -> Option<String> {
        self.session
            .active_workspace()
            .and_then(|ws| find_terminal_surface(&ws.layout, &ws.active_pane_id.0))
    }

    pub fn push_activity(&mut self, message: impl Into<String>) {
        self.activity_log.push(message.into());
        const MAX_LOG_ENTRIES: usize = 20;
        if self.activity_log.len() > MAX_LOG_ENTRIES {
            let drain = self.activity_log.len() - MAX_LOG_ENTRIES;
            self.activity_log.drain(0..drain);
        }
    }
}

fn workspace_snapshot(workspace: &WorkspaceState) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        id: workspace.id.0.clone(),
        name: workspace.name.clone(),
        layout: collect_layout(&workspace.layout, &workspace.active_pane_id.0),
    }
}

fn collect_layout(layout: &LayoutNode, active_pane_id: &str) -> LayoutSnapshot {
    match layout {
        LayoutNode::Pane(pane) => LayoutSnapshot::Pane(PaneSnapshot {
            id: pane.pane_id.0.clone(),
            is_active: pane.pane_id.0 == active_pane_id,
            tabs: pane
                .tabs
                .iter()
                .map(|tab| tab_snapshot(tab, &pane.active_tab_id.0))
                .collect(),
        }),
        LayoutNode::Split(split) => LayoutSnapshot::Split(SplitSnapshot {
            axis: split.axis,
            first: Box::new(collect_layout(&split.first, active_pane_id)),
            second: Box::new(collect_layout(&split.second, active_pane_id)),
        }),
    }
}

fn tab_snapshot(tab: &TabState, active_tab_id: &str) -> TabSnapshot {
    TabSnapshot {
        id: tab.id.0.clone(),
        title: tab.title.clone(),
        is_active: tab.id.0 == active_tab_id,
        surface_kind: surface_kind(&tab.surface),
    }
}

fn surface_kind(surface: &SurfaceState) -> &'static str {
    match surface {
        SurfaceState::Terminal(_) => "terminal",
        SurfaceState::Agent(_) => "agent",
        SurfaceState::FileTree(_) => "file_tree",
        SurfaceState::Editor(_) => "editor",
        SurfaceState::Preview(_) => "preview",
        SurfaceState::Welcome(_) => "welcome",
        SurfaceState::Settings(_) => "settings",
        SurfaceState::Browser(_) => "browser",
    }
}

fn active_surface_snapshot(workspace: &WorkspaceState) -> Option<ActiveSurfaceItem> {
    find_active_surface(&workspace.layout, &workspace.active_pane_id.0)
}

fn find_active_surface(layout: &LayoutNode, active_pane_id: &str) -> Option<ActiveSurfaceItem> {
    match layout {
        LayoutNode::Pane(pane) if pane.pane_id.0 == active_pane_id => {
            let active_tab = pane
                .tabs
                .iter()
                .find(|tab| tab.id.0 == pane.active_tab_id.0)?;
            Some(ActiveSurfaceItem {
                pane_id: pane.pane_id.0.clone(),
                tab_id: active_tab.id.0.clone(),
                tab_title: active_tab.title.clone(),
                surface_kind: surface_kind(&active_tab.surface),
                summary_lines: surface_summary_lines(&active_tab.surface),
                content_lines: Vec::new(),
            })
        }
        LayoutNode::Pane(_) => None,
        LayoutNode::Split(split) => find_active_surface(&split.first, active_pane_id)
            .or_else(|| find_active_surface(&split.second, active_pane_id)),
    }
}

fn surface_summary_lines(surface: &SurfaceState) -> Vec<String> {
    match surface {
        SurfaceState::Terminal(terminal) => vec![
            format!("Shell: {:?}", terminal.launch_profile.shell),
            format!(
                "CWD: {}",
                terminal.cwd.clone().unwrap_or_else(|| "unset".into())
            ),
            format!(
                "Session: {}",
                terminal
                    .session_id
                    .as_ref()
                    .map(|id| id.0.clone())
                    .unwrap_or_else(|| "detached".into())
            ),
        ],
        SurfaceState::Agent(agent) => vec![
            format!("Provider: {}", agent.provider_id),
            format!("Mode: {:?}", agent.launch_mode),
            format!(
                "CWD: {}",
                agent.cwd.clone().unwrap_or_else(|| "unset".into())
            ),
        ],
        SurfaceState::FileTree(file_tree) => vec![
            format!(
                "Filter: {}",
                if file_tree.filter.is_empty() {
                    "(none)"
                } else {
                    &file_tree.filter
                }
            ),
            format!(
                "Selected: {}",
                file_tree
                    .selected
                    .clone()
                    .unwrap_or_else(|| "(none)".into())
            ),
            format!("Show hidden: {}", file_tree.show_hidden),
        ],
        SurfaceState::Editor(editor) => vec![
            format!("Path: {}", editor.relative_path),
            format!(
                "Language: {}",
                editor.language.clone().unwrap_or_else(|| "text".into())
            ),
            format!("Readonly: {}", editor.readonly),
        ],
        SurfaceState::Preview(preview) => vec![
            format!("Source: {}", preview.source_relative_path),
            format!("Kind: {:?}", preview.kind),
        ],
        SurfaceState::Welcome(welcome) => vec![format!("Title: {}", welcome.title)],
        SurfaceState::Settings(settings) => vec![
            format!("Category: {}", settings.selected_category.label()),
            format!("{} categories", settings.categories.len()),
        ],
        SurfaceState::Browser(browser) => vec![
            format!("URL: {}", browser.url),
            format!("Title: {}", browser.title),
        ],
    }
}

/// Count the number of splits in a layout
fn count_splits(layout: &LayoutNode) -> usize {
    match layout {
        LayoutNode::Split(split) => 1 + count_splits(&split.first) + count_splits(&split.second),
        LayoutNode::Pane(_) => 0,
    }
}

/// Find the terminal shell type in the active pane
fn find_terminal_surface(layout: &LayoutNode, active_pane_id: &str) -> Option<String> {
    match layout {
        LayoutNode::Pane(pane) if pane.pane_id.0 == active_pane_id => pane
            .tabs
            .iter()
            .find(|tab| matches!(tab.surface, SurfaceState::Terminal(_)))
            .and_then(|tab| match &tab.surface {
                SurfaceState::Terminal(terminal) => {
                    Some(format!("{:?}", terminal.launch_profile.shell))
                }
                _ => None,
            }),
        LayoutNode::Pane(_) => None,
        LayoutNode::Split(split) => find_terminal_surface(&split.first, active_pane_id)
            .or_else(|| find_terminal_surface(&split.second, active_pane_id)),
    }
}

fn format_session_error(err: SessionOpError) -> String {
    match err {
        SessionOpError::NoActiveWorkspace => "No active workspace".into(),
        SessionOpError::WorkspaceNotFound(id) => format!("Workspace not found: {id}"),
        SessionOpError::WorkspaceOp(inner) => format!("Workspace operation failed: {inner:?}"),
    }
}

fn format_event(event: &Event) -> String {
    match event {
        Event::WorkspaceOpened(id) => format!("event: workspace opened {id}"),
        Event::WorkspaceClosed(id) => format!("event: workspace closed {id}"),
        Event::PaneFocused(id) => format!("event: pane focused {id}"),
        Event::TabClosed { pane_id, tab_id } => {
            format!("event: tab closed {tab_id} in {pane_id}")
        }
        Event::TerminalAttached(id) => format!("event: terminal attached {id}"),
        Event::SplitResized(split_id) => format!("event: split resized {split_id}"),
        Event::SplitRatiosReset => "event: split ratios reset".into(),
        Event::SessionSaved => "event: session saved".into(),
    }
}
