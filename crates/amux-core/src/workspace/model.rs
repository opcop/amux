use crate::{LayoutNode, PaneId, WorkspaceGroupId, WorkspaceId, WorkspaceTarget};

/// The canonical "ungrouped / default" group id. Old sessions that
/// predate the group concept migrate their workspaces into this id
/// on load. The UI treats this id specially — it renders its members
/// at the top of the sidebar without a group header, so existing
/// users don't see a surprise "Default" label after upgrading.
pub const DEFAULT_WORKSPACE_GROUP_ID: &str = "group-default";

/// A named organizational container that holds workspaces. Groups
/// don't own any terminal / pane / layout state of their own — they
/// only exist so the user can cluster workspaces into logical
/// buckets (e.g. "client work", "side projects", "scratch").
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceGroup {
    pub id: WorkspaceGroupId,
    /// Display name. The empty string is meaningful: it signals
    /// "don't render a group header for this group" and is used by
    /// the default / migration group so legacy users see a flat
    /// workspace list as before.
    pub name: String,
}

impl WorkspaceGroup {
    /// The default / ungrouped bucket. Used by migration and by
    /// first-launch bootstrapping. Its name is empty so the sidebar
    /// renders its members without a header.
    pub fn default_group() -> Self {
        Self {
            id: WorkspaceGroupId::new(DEFAULT_WORKSPACE_GROUP_ID),
            name: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceState {
    pub id: WorkspaceId,
    pub name: String,
    pub target: WorkspaceTarget,
    pub layout: LayoutNode,
    pub active_pane_id: PaneId,
    pub env_profile_id: Option<String>,
    pub default_agent_provider_id: Option<String>,
    pub recent_files: Vec<String>,
    /// Which [`WorkspaceGroup`] this workspace belongs to. Old
    /// sessions that predate the group concept get this field
    /// populated with [`DEFAULT_WORKSPACE_GROUP_ID`] on load via
    /// `#[serde(default)]` + a migration pass.
    #[serde(default = "default_workspace_group_id")]
    pub group_id: WorkspaceGroupId,
}

fn default_workspace_group_id() -> WorkspaceGroupId {
    WorkspaceGroupId::new(DEFAULT_WORKSPACE_GROUP_ID)
}
