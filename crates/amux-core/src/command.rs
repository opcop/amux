use crate::{PaneId, SplitAxis, SurfaceState, TabId, WorkspaceId, WorkspaceTarget};

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    OpenWorkspace(WorkspaceTarget),
    /// Create a brand-new workspace with the given target, **skipping
    /// target-equality dedup**. Used by the sidebar "+ New" entry
    /// point where the intent is "give me another organizational
    /// bucket that happens to start at `$HOME`", not "re-open the
    /// folder I already have". The resulting workspace's name is
    /// auto-disambiguated ("arden", "arden 2", "arden 3", …) so
    /// multiple siblings are visually distinct until the user
    /// renames them.
    CreateWorkspace(WorkspaceTarget),
    CloseWorkspace(WorkspaceId),
    FocusPane(PaneId),
    OpenSurface { pane_id: PaneId, surface: SurfaceState },
    CloseTab { pane_id: PaneId, tab_id: TabId },
    SplitPane { pane_id: PaneId, axis: SplitAxis },
    ResizeSplit { split_id: String, delta: f32 },
    ResetSplitRatios,
    SaveSession,
}
