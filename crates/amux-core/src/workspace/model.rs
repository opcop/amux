use crate::{LayoutNode, PaneId, WorkspaceId, WorkspaceTarget};

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
}
