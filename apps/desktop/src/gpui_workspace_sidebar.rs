//! Shared sidebar state and lightweight data models for the desktop shell.
//!
//! The live GPUI sidebar rendering now lives in `gpui_entry.rs`. This module
//! intentionally only keeps the state and item types that are shared across the
//! shell and input handlers.

/// Sidebar display mode.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug, PartialEq)]
pub enum SidebarMode {
    Workspaces,
    Agents,
    Workbench,
}

/// Item representing an agent in the sidebar.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct AgentSidebarItem {
    pub pane_id: String,
    pub tab_title: String,
    pub agent_kind: Option<String>,
    pub agent_status: Option<String>,
    pub status_icon: String,
    pub status_color: u32,
    /// Rich session data from JSONL monitoring (Claude Code only)
    pub session_tool: Option<String>,
    pub session_tokens: Option<u64>,
    pub session_subagents: usize,
    pub session_todo_done: usize,
    pub session_todo_total: usize,
}

/// State for the workspace sidebar.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct WorkspaceSidebarState {
    pub collapsed: bool,
    /// User-resizable sidebar width (pixels). Clamped to [120, 480].
    pub width: f32,
    /// Current sidebar display mode.
    pub mode: SidebarMode,
}

#[cfg(feature = "gpui")]
impl Default for WorkspaceSidebarState {
    fn default() -> Self {
        Self {
            collapsed: false,
            width: 220.0,
            mode: SidebarMode::Workspaces,
        }
    }
}
