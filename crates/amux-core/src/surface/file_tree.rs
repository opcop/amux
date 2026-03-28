use std::collections::BTreeSet;

use crate::{SurfaceId, WorkspaceTarget};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileTreeSurfaceState {
    pub surface_id: SurfaceId,
    pub root: WorkspaceTarget,
    pub filter: String,
    pub selected: Option<String>,
    pub expanded: BTreeSet<String>,
    pub show_hidden: bool,
}
