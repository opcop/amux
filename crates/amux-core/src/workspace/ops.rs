use crate::{
    CloseTabOutcome, PaneId, PaneNode, SplitAxis, SurfaceState, TabId, TabState, WorkspaceState,
    activate_tab, active_pane_exists, append_tab, close_tab, split_pane,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceOpError {
    PaneNotFound(PaneId),
    TabNotFound { pane_id: PaneId, tab_id: TabId },
    CannotRemoveLastTab,
}

impl WorkspaceState {
    pub fn focus_pane(&mut self, pane_id: PaneId) -> Result<(), WorkspaceOpError> {
        if !active_pane_exists(&self.layout, &pane_id) {
            return Err(WorkspaceOpError::PaneNotFound(pane_id));
        }
        self.active_pane_id = pane_id;
        Ok(())
    }

    pub fn add_tab_to_active_pane(&mut self, tab: TabState) -> Result<(), WorkspaceOpError> {
        self.add_tab_to_pane(self.active_pane_id.clone(), tab)
    }

    pub fn add_tab_to_pane(
        &mut self,
        pane_id: PaneId,
        tab: TabState,
    ) -> Result<(), WorkspaceOpError> {
        if append_tab(&mut self.layout, &pane_id, tab) {
            self.active_pane_id = pane_id;
            Ok(())
        } else {
            Err(WorkspaceOpError::PaneNotFound(pane_id))
        }
    }

    pub fn activate_tab(
        &mut self,
        pane_id: PaneId,
        tab_id: TabId,
    ) -> Result<(), WorkspaceOpError> {
        if activate_tab(&mut self.layout, &pane_id, &tab_id) {
            self.active_pane_id = pane_id;
            Ok(())
        } else {
            Err(WorkspaceOpError::TabNotFound { pane_id, tab_id })
        }
    }

    pub fn split_active_pane(
        &mut self,
        axis: SplitAxis,
        split_id: impl Into<String>,
        new_pane: PaneNode,
    ) -> Result<(), WorkspaceOpError> {
        self.split_pane(self.active_pane_id.clone(), axis, split_id, new_pane)
    }

    pub fn split_pane(
        &mut self,
        pane_id: PaneId,
        axis: SplitAxis,
        split_id: impl Into<String>,
        new_pane: PaneNode,
    ) -> Result<(), WorkspaceOpError> {
        let new_pane_id = new_pane.pane_id.clone();
        if split_pane(&mut self.layout, &pane_id, axis, split_id, new_pane) {
            self.active_pane_id = new_pane_id;
            Ok(())
        } else {
            Err(WorkspaceOpError::PaneNotFound(pane_id))
        }
    }

    pub fn close_tab(
        &mut self,
        pane_id: PaneId,
        tab_id: TabId,
    ) -> Result<CloseTabOutcome, WorkspaceOpError> {
        match close_tab(&mut self.layout, &pane_id, &tab_id) {
            CloseTabOutcome::TabClosed => {
                self.ensure_active_pane_fallback();
                Ok(CloseTabOutcome::TabClosed)
            }
            CloseTabOutcome::PaneRemoved => {
                self.ensure_active_pane_fallback();
                Ok(CloseTabOutcome::PaneRemoved)
            }
            CloseTabOutcome::NotFound => Err(WorkspaceOpError::TabNotFound { pane_id, tab_id }),
            CloseTabOutcome::CannotRemoveLastTab => Err(WorkspaceOpError::CannotRemoveLastTab),
        }
    }

    fn ensure_active_pane_fallback(&mut self) {
        if active_pane_exists(&self.layout, &self.active_pane_id) {
            return;
        }
        if let Some(first) = first_pane_id(&self.layout) {
            self.active_pane_id = first;
        }
    }
}

fn first_pane_id(layout: &crate::LayoutNode) -> Option<PaneId> {
    match layout {
        crate::LayoutNode::Pane(pane) => Some(pane.pane_id.clone()),
        crate::LayoutNode::Split(split) => {
            first_pane_id(&split.first).or_else(|| first_pane_id(&split.second))
        }
    }
}

pub fn default_surface_title(surface: &SurfaceState) -> &'static str {
    match surface {
        SurfaceState::Terminal(_) => "Terminal",
        SurfaceState::Agent(_) => "Agent",
        SurfaceState::FileTree(_) => "Files",
        SurfaceState::Editor(_) => "Editor",
        SurfaceState::Preview(_) => "Preview",
        SurfaceState::Welcome(_) => "Welcome",
        SurfaceState::Settings(_) => "Settings",
    }
}
