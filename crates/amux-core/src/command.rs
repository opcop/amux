use crate::{PaneId, SplitAxis, SurfaceState, TabId, WorkspaceId, WorkspaceTarget};

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    OpenWorkspace(WorkspaceTarget),
    CloseWorkspace(WorkspaceId),
    FocusPane(PaneId),
    OpenSurface { pane_id: PaneId, surface: SurfaceState },
    CloseTab { pane_id: PaneId, tab_id: TabId },
    SplitPane { pane_id: PaneId, axis: SplitAxis },
    ResizeSplit { split_id: String, delta: f32 },
    ResetSplitRatios,
    SaveSession,
}
