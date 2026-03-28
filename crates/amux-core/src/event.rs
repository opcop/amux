use crate::{PaneId, TabId, TerminalSessionId, WorkspaceId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    WorkspaceOpened(WorkspaceId),
    WorkspaceClosed(WorkspaceId),
    PaneFocused(PaneId),
    TabClosed { pane_id: PaneId, tab_id: TabId },
    TerminalAttached(TerminalSessionId),
    SplitResized(String),
    SplitRatiosReset,
    SessionSaved,
}

