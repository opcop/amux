use std::time::{SystemTime, UNIX_EPOCH};

use crate::{WorkspaceId, WorkspaceState};

/// Represents a recent workspace entry with metadata
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecentWorkspace {
    pub id: WorkspaceId,
    pub name: String,
    pub target: crate::WorkspaceTarget,
    pub last_accessed: u64,     // Unix timestamp
    pub workspace_index: usize, // Index in workspaces vec
}

impl RecentWorkspace {
    pub fn new(
        id: WorkspaceId,
        name: String,
        target: crate::WorkspaceTarget,
        workspace_index: usize,
    ) -> Self {
        let last_accessed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            id,
            name,
            target,
            last_accessed,
            workspace_index,
        }
    }
}

/// Represents a pane layout configuration
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaneLayoutConfig {
    pub pane_id: String,
    pub pane_ratio: Option<f32>, // Ratio of pane in split (0.0 - 1.0)
}

/// Represents a split layout configuration
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SplitLayoutConfig {
    pub axis: crate::SplitAxis,
    pub ratio: f32, // Split ratio (0.0 - 1.0)
    pub first: Box<LayoutConfig>,
    pub second: Box<LayoutConfig>,
}

/// Represents a workspace layout configuration
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LayoutConfig {
    Pane(PaneLayoutConfig),
    Split(SplitLayoutConfig),
}

/// UI preferences that persist across sessions
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UiPreferences {
    pub sidebar_collapsed: bool,
    pub sidebar_width: u32,
    pub font_size: u16,
    pub theme: String,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            sidebar_collapsed: false,
            sidebar_width: 240,
            font_size: 14,
            theme: "system".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SessionState {
    pub version: u32,
    pub active_workspace_id: Option<WorkspaceId>,
    pub workspaces: Vec<WorkspaceState>,
    pub recent_workspaces: Vec<RecentWorkspace>,
    pub ui_preferences: UiPreferences,
    pub last_saved: Option<u64>, // Unix timestamp
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            version: 1,
            active_workspace_id: None,
            workspaces: Vec::new(),
            recent_workspaces: Vec::new(),
            ui_preferences: UiPreferences::default(),
            last_saved: None,
        }
    }
}

impl SessionState {
    /// Update recent workspaces list when a workspace is accessed
    pub fn touch_workspace(&mut self, workspace_id: &WorkspaceId) {
        if let Some(index) = self.workspaces.iter().position(|ws| &ws.id == workspace_id) {
            let workspace = &self.workspaces[index];

            // Remove from recent if already exists
            self.recent_workspaces.retain(|r| &r.id != workspace_id);

            // Add to front of recent list
            self.recent_workspaces.insert(
                0,
                RecentWorkspace::new(
                    workspace.id.clone(),
                    workspace.name.clone(),
                    workspace.target.clone(),
                    index,
                ),
            );

            // Keep only last 10 recent workspaces
            self.recent_workspaces.truncate(10);
        }
    }

    /// Mark session as saved
    pub fn mark_saved(&mut self) {
        self.last_saved = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
    }

    /// Get save status description
    pub fn save_status(&self) -> SaveStatus {
        match self.last_saved {
            Some(ts) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let diff = now.saturating_sub(ts);
                if diff < 60 {
                    SaveStatus::Saved("just now".to_string())
                } else if diff < 3600 {
                    SaveStatus::Saved(format!("{}m ago", diff / 60))
                } else {
                    SaveStatus::Saved(format!("{}h ago", diff / 3600))
                }
            }
            None => SaveStatus::Unsaved,
        }
    }

    pub fn remove_workspace(&mut self, workspace_id: &WorkspaceId) -> Option<WorkspaceState> {
        if let Some(index) = self.workspaces.iter().position(|ws| &ws.id == workspace_id) {
            let removed = self.workspaces.remove(index);

            // Update active workspace if needed
            if self.active_workspace_id.as_ref() == Some(workspace_id) {
                self.active_workspace_id = self.workspaces.first().map(|ws| ws.id.clone());
            }

            // Remove from recent
            self.recent_workspaces.retain(|r| &r.id != workspace_id);

            Some(removed)
        } else {
            None
        }
    }

    pub fn rename_workspace(
        &mut self,
        workspace_id: &WorkspaceId,
        new_name: String,
    ) -> Result<(), String> {
        let workspace = self
            .workspaces
            .iter_mut()
            .find(|ws| &ws.id == workspace_id)
            .ok_or_else(|| "workspace not found".to_string())?;

        workspace.name = new_name;
        Ok(())
    }

    pub fn move_workspace(&mut self, from_index: usize, to_index: usize) -> Result<(), String> {
        if from_index >= self.workspaces.len() || to_index >= self.workspaces.len() {
            return Err("invalid workspace index".to_string());
        }
        let workspace = self.workspaces.remove(from_index);
        self.workspaces.insert(to_index, workspace);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SaveStatus {
    Saved(String),
    Unsaved,
    Saving,
}

impl Default for SaveStatus {
    fn default() -> Self {
        SaveStatus::Unsaved
    }
}
