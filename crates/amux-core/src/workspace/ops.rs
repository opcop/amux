use crate::{
    activate_tab, active_pane_exists, append_tab, close_pane as core_close_pane, close_tab,
    focus_next_tab as core_focus_next_tab, focus_pane_in_direction as core_focus_pane_in_direction,
    focus_previous_tab as core_focus_previous_tab, split_pane, ClosePaneOutcome, CloseTabOutcome,
    Direction, PaneId, PaneNode, SplitAxis, SurfaceState, TabId, TabState, WorkspaceState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceOpError {
    PaneNotFound(PaneId),
    TabNotFound { pane_id: PaneId, tab_id: TabId },
    CannotRemoveLastTab,
    CannotRemoveLastPane,
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

    pub fn activate_tab(&mut self, pane_id: PaneId, tab_id: TabId) -> Result<(), WorkspaceOpError> {
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

    pub fn close_active_pane(&mut self) -> Result<ClosePaneOutcome, WorkspaceOpError> {
        let pane_id = self.active_pane_id.clone();
        self.close_pane(pane_id)
    }

    pub fn close_pane(&mut self, pane_id: PaneId) -> Result<ClosePaneOutcome, WorkspaceOpError> {
        match core_close_pane(&mut self.layout, &pane_id) {
            ClosePaneOutcome::PaneClosed => Ok(ClosePaneOutcome::PaneClosed),
            ClosePaneOutcome::PaneRemoved => {
                self.ensure_active_pane_fallback();
                Ok(ClosePaneOutcome::PaneRemoved)
            }
            ClosePaneOutcome::CannotRemoveLastPane => Err(WorkspaceOpError::CannotRemoveLastPane),
        }
    }

    pub fn focus_pane_in_direction(
        &mut self,
        direction: Direction,
    ) -> Result<(), WorkspaceOpError> {
        if let Some(target) =
            core_focus_pane_in_direction(&self.layout, &self.active_pane_id, direction)
        {
            self.active_pane_id = target;
            Ok(())
        } else {
            Err(WorkspaceOpError::PaneNotFound(self.active_pane_id.clone()))
        }
    }

    pub fn focus_next_tab(&mut self) -> Result<(), WorkspaceOpError> {
        let pane = self
            .layout
            .find_pane(&self.active_pane_id)
            .ok_or_else(|| WorkspaceOpError::PaneNotFound(self.active_pane_id.clone()))?;

        if let Some(target) =
            core_focus_next_tab(&self.layout, &self.active_pane_id, &pane.active_tab_id)
        {
            if activate_tab(&mut self.layout, &self.active_pane_id, &target) {
                Ok(())
            } else {
                Err(WorkspaceOpError::TabNotFound {
                    pane_id: self.active_pane_id.clone(),
                    tab_id: target,
                })
            }
        } else {
            Err(WorkspaceOpError::TabNotFound {
                pane_id: self.active_pane_id.clone(),
                tab_id: pane.active_tab_id.clone(),
            })
        }
    }

    pub fn focus_previous_tab(&mut self) -> Result<(), WorkspaceOpError> {
        let pane = self
            .layout
            .find_pane(&self.active_pane_id)
            .ok_or_else(|| WorkspaceOpError::PaneNotFound(self.active_pane_id.clone()))?;

        if let Some(target) =
            core_focus_previous_tab(&self.layout, &self.active_pane_id, &pane.active_tab_id)
        {
            if activate_tab(&mut self.layout, &self.active_pane_id, &target) {
                Ok(())
            } else {
                Err(WorkspaceOpError::TabNotFound {
                    pane_id: self.active_pane_id.clone(),
                    tab_id: target,
                })
            }
        } else {
            Err(WorkspaceOpError::TabNotFound {
                pane_id: self.active_pane_id.clone(),
                tab_id: pane.active_tab_id.clone(),
            })
        }
    }

    pub fn pin_active_tab(&mut self) -> Result<(), WorkspaceOpError> {
        let pane = self
            .layout
            .find_pane_mut(&self.active_pane_id)
            .ok_or_else(|| WorkspaceOpError::PaneNotFound(self.active_pane_id.clone()))?;

        if let Some(tab) = pane.tabs.iter_mut().find(|t| t.id == pane.active_tab_id) {
            tab.pinned = true;
            Ok(())
        } else {
            Err(WorkspaceOpError::TabNotFound {
                pane_id: self.active_pane_id.clone(),
                tab_id: pane.active_tab_id.clone(),
            })
        }
    }

    pub fn unpin_active_tab(&mut self) -> Result<(), WorkspaceOpError> {
        let pane = self
            .layout
            .find_pane_mut(&self.active_pane_id)
            .ok_or_else(|| WorkspaceOpError::PaneNotFound(self.active_pane_id.clone()))?;

        if let Some(tab) = pane.tabs.iter_mut().find(|t| t.id == pane.active_tab_id) {
            tab.pinned = false;
            Ok(())
        } else {
            Err(WorkspaceOpError::TabNotFound {
                pane_id: self.active_pane_id.clone(),
                tab_id: pane.active_tab_id.clone(),
            })
        }
    }

    pub fn rename_active_tab(&mut self, new_title: String) -> Result<(), WorkspaceOpError> {
        let pane = self
            .layout
            .find_pane_mut(&self.active_pane_id)
            .ok_or_else(|| WorkspaceOpError::PaneNotFound(self.active_pane_id.clone()))?;

        if let Some(tab) = pane.tabs.iter_mut().find(|t| t.id == pane.active_tab_id) {
            tab.title = new_title;
            Ok(())
        } else {
            Err(WorkspaceOpError::TabNotFound {
                pane_id: self.active_pane_id.clone(),
                tab_id: pane.active_tab_id.clone(),
            })
        }
    }

    pub fn add_recent_file(&mut self, path: String) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        if self.recent_files.len() > 20 {
            self.recent_files.truncate(20);
        }
    }

    pub fn close_other_tabs(&mut self) -> Result<(), WorkspaceOpError> {
        let pane = self
            .layout
            .find_pane_mut(&self.active_pane_id)
            .ok_or_else(|| WorkspaceOpError::PaneNotFound(self.active_pane_id.clone()))?;

        let active_id = pane.active_tab_id.clone();
        pane.tabs.retain(|t| t.id == active_id || t.pinned);

        if !pane.tabs.iter().any(|t| t.id == active_id) {
            pane.active_tab_id = pane
                .tabs
                .first()
                .map(|t| t.id.clone())
                .ok_or_else(|| WorkspaceOpError::CannotRemoveLastTab)?;
        }

        Ok(())
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
        SurfaceState::Browser(_) => "Browser",
    }
}
