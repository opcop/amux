mod agent;
mod editor;
mod file_tree;
mod preview;
mod terminal;

use crate::{AgentInstanceId, SurfaceId, TerminalSessionId};

pub use agent::*;
pub use editor::*;
pub use file_tree::*;
pub use preview::*;
pub use terminal::*;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TabState {
    pub id: crate::TabId,
    pub title: String,
    pub pinned: bool,
    pub surface: SurfaceState,
}

impl TabState {
    pub fn new(
        id: crate::TabId,
        title: impl Into<String>,
        pinned: bool,
        surface: SurfaceState,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            pinned,
            surface,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SurfaceState {
    Terminal(TerminalSurfaceState),
    Agent(AgentSurfaceState),
    FileTree(FileTreeSurfaceState),
    Editor(EditorSurfaceState),
    Preview(PreviewSurfaceState),
    Welcome(WelcomeSurfaceState),
    Settings(SettingsSurfaceState),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WelcomeSurfaceState {
    pub surface_id: SurfaceId,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SettingsSurfaceState {
    pub surface_id: SurfaceId,
    pub title: String,
    /// Currently selected category
    pub selected_category: SettingsCategory,
    /// All available categories
    pub categories: Vec<SettingsCategory>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SettingsCategory {
    General,
    Appearance,
    Editor,
    Terminal,
    Keyboard,
    AutoSave,
    Workspace,
}

impl SettingsCategory {
    pub fn all() -> Vec<Self> {
        vec![
            SettingsCategory::General,
            SettingsCategory::Appearance,
            SettingsCategory::Editor,
            SettingsCategory::Terminal,
            SettingsCategory::Keyboard,
            SettingsCategory::AutoSave,
            SettingsCategory::Workspace,
        ]
    }
    
    pub fn label(&self) -> &'static str {
        match self {
            SettingsCategory::General => "General",
            SettingsCategory::Appearance => "Appearance",
            SettingsCategory::Editor => "Editor",
            SettingsCategory::Terminal => "Terminal",
            SettingsCategory::Keyboard => "Keyboard",
            SettingsCategory::AutoSave => "Auto-save",
            SettingsCategory::Workspace => "Workspace",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentLaunchMode {
    AttachedTerminal,
    ManagedProcess,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentAttachment {
    pub agent_instance_id: Option<AgentInstanceId>,
    pub session_id: Option<TerminalSessionId>,
}
