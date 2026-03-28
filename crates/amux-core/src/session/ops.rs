use std::path::Path;

use crate::{
    Command, Event, PaneId, PaneNode, SessionState, SplitAxis, SurfaceId,
    SurfaceState, TabId, TabState, WelcomeSurfaceState, WorkspaceId, WorkspaceOpError,
    WorkspaceState, WorkspaceTarget, default_surface_title,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionOpError {
    NoActiveWorkspace,
    WorkspaceNotFound(WorkspaceId),
    WorkspaceOp(WorkspaceOpError),
}

impl From<WorkspaceOpError> for SessionOpError {
    fn from(value: WorkspaceOpError) -> Self {
        Self::WorkspaceOp(value)
    }
}

impl SessionState {
    pub fn active_workspace(&self) -> Option<&WorkspaceState> {
        let active_id = self.active_workspace_id.as_ref()?;
        self.workspaces.iter().find(|workspace| &workspace.id == active_id)
    }

    pub fn active_workspace_mut(&mut self) -> Option<&mut WorkspaceState> {
        let active_id = self.active_workspace_id.as_ref()?;
        self.workspaces
            .iter_mut()
            .find(|workspace| &workspace.id == active_id)
    }

    pub fn apply(&mut self, command: Command) -> Result<Vec<Event>, SessionOpError> {
        match command {
            Command::OpenWorkspace(target) => {
                let workspace = build_workspace(
                    WorkspaceId::new(next_workspace_id(self.workspaces.len())),
                    derive_workspace_name(&target),
                    target,
                );
                let workspace_id = workspace.id.clone();
                self.active_workspace_id = Some(workspace_id.clone());
                self.workspaces.push(workspace);
                Ok(vec![Event::WorkspaceOpened(workspace_id)])
            }
            Command::CloseWorkspace(workspace_id) => {
                let Some(index) = self.workspaces.iter().position(|ws| ws.id == workspace_id) else {
                    return Err(SessionOpError::WorkspaceNotFound(workspace_id));
                };
                let removed = self.workspaces.remove(index);
                if self.active_workspace_id.as_ref() == Some(&removed.id) {
                    self.active_workspace_id = self.workspaces.first().map(|ws| ws.id.clone());
                }
                Ok(vec![Event::WorkspaceClosed(removed.id)])
            }
            Command::FocusPane(pane_id) => {
                let workspace = self
                    .active_workspace_mut()
                    .ok_or(SessionOpError::NoActiveWorkspace)?;
                workspace.focus_pane(pane_id.clone())?;
                Ok(vec![Event::PaneFocused(pane_id)])
            }
            Command::OpenSurface { pane_id, surface } => {
                let workspace = self
                    .active_workspace_mut()
                    .ok_or(SessionOpError::NoActiveWorkspace)?;
                let tab = build_surface_tab(
                    TabId::new(next_tab_id(workspace)),
                    surface,
                );
                workspace.add_tab_to_pane(pane_id, tab)?;
                Ok(Vec::new())
            }
            Command::CloseTab { pane_id, tab_id } => {
                let workspace = self
                    .active_workspace_mut()
                    .ok_or(SessionOpError::NoActiveWorkspace)?;
                workspace.close_tab(pane_id.clone(), tab_id.clone())?;
                Ok(vec![Event::TabClosed { pane_id, tab_id }])
            }
            Command::SplitPane { pane_id, axis } => {
                let workspace = self
                    .active_workspace_mut()
                    .ok_or(SessionOpError::NoActiveWorkspace)?;
                let split_id = next_split_id(workspace);
                let new_pane_id = PaneId::new(next_pane_id(workspace));
                let tab = build_surface_tab(
                    TabId::new(next_tab_id(workspace)),
                    SurfaceState::Welcome(WelcomeSurfaceState {
                        surface_id: SurfaceId::new(next_surface_id(workspace)),
                        title: format!("New {}", split_axis_label(axis)),
                    }),
                );
                let pane = PaneNode::with_tab(new_pane_id.clone(), tab);
                workspace.split_pane(pane_id, axis, split_id, pane)?;
                Ok(vec![Event::PaneFocused(new_pane_id)])
            }
            Command::ResizeSplit { split_id, delta } => {
                let workspace = self
                    .active_workspace_mut()
                    .ok_or(SessionOpError::NoActiveWorkspace)?;
                workspace.layout.resize_split(&split_id, delta);
                Ok(vec![Event::SplitResized(split_id)])
            }
            Command::ResetSplitRatios => {
                let workspace = self
                    .active_workspace_mut()
                    .ok_or(SessionOpError::NoActiveWorkspace)?;
                workspace.layout.reset_split_ratios();
                Ok(vec![Event::SplitRatiosReset])
            }
            Command::SaveSession => Ok(vec![Event::SessionSaved]),
        }
    }
}

fn build_workspace(id: WorkspaceId, name: String, target: WorkspaceTarget) -> WorkspaceState {
    let pane_id = PaneId::new(format!("pane-{}-1", id.0));
    let tab = TabState::new(
        TabId::new(format!("tab-{}-1", id.0)),
        "Welcome",
        false,
        SurfaceState::Welcome(WelcomeSurfaceState {
            surface_id: SurfaceId::new(format!("surface-{}-1", id.0)),
            title: "Welcome".into(),
        }),
    );

    WorkspaceState {
        id,
        name,
        target,
        layout: crate::LayoutNode::single_pane(PaneNode::with_tab(pane_id.clone(), tab)),
        active_pane_id: pane_id,
        env_profile_id: None,
        default_agent_provider_id: None,
        recent_files: Vec::new(),
    }
}

fn build_surface_tab(id: TabId, surface: SurfaceState) -> TabState {
    let title = default_surface_title(&surface);
    TabState::new(id, title, false, surface)
}

fn derive_workspace_name(target: &WorkspaceTarget) -> String {
    match target {
        WorkspaceTarget::WindowsPath { path } => path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| path.display().to_string()),
        WorkspaceTarget::WslPath { path, .. } => Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| path.clone()),
    }
}

fn split_axis_label(axis: SplitAxis) -> &'static str {
    match axis {
        SplitAxis::Horizontal => "Horizontal Split",
        SplitAxis::Vertical => "Vertical Split",
    }
}

fn next_workspace_id(count: usize) -> String {
    format!("workspace-{}", count + 1)
}

fn next_split_id(workspace: &WorkspaceState) -> String {
    format!("split-{}", count_layout_splits(&workspace.layout) + 1)
}

fn next_pane_id(workspace: &WorkspaceState) -> String {
    format!("pane-{}", count_layout_panes(&workspace.layout) + 1)
}

fn next_tab_id(workspace: &WorkspaceState) -> String {
    format!("tab-{}", count_tabs(&workspace.layout) + 1)
}

fn next_surface_id(workspace: &WorkspaceState) -> String {
    format!("surface-{}", count_tabs(&workspace.layout) + 1)
}

fn count_layout_splits(layout: &crate::LayoutNode) -> usize {
    match layout {
        crate::LayoutNode::Pane(_) => 0,
        crate::LayoutNode::Split(split) => {
            1 + count_layout_splits(&split.first) + count_layout_splits(&split.second)
        }
    }
}

fn count_layout_panes(layout: &crate::LayoutNode) -> usize {
    match layout {
        crate::LayoutNode::Pane(_) => 1,
        crate::LayoutNode::Split(split) => {
            count_layout_panes(&split.first) + count_layout_panes(&split.second)
        }
    }
}

fn count_tabs(layout: &crate::LayoutNode) -> usize {
    match layout {
        crate::LayoutNode::Pane(pane) => pane.tabs.len(),
        crate::LayoutNode::Split(split) => count_tabs(&split.first) + count_tabs(&split.second),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{Command, PreviewKind, PreviewSurfaceState, SplitAxis, SurfaceId, SurfaceState};

    use super::{SessionOpError, SessionState, WorkspaceTarget};

    #[test]
    fn open_workspace_sets_active_workspace() {
        let mut session = SessionState::default();

        let events = session
            .apply(Command::OpenWorkspace(WorkspaceTarget::WindowsPath {
                path: PathBuf::from("D:/repo/amux"),
            }))
            .expect("workspace should open");

        assert_eq!(session.workspaces.len(), 1);
        assert_eq!(events.len(), 1);
        assert_eq!(session.active_workspace().unwrap().name, "amux");
    }

    #[test]
    fn open_surface_adds_tab_to_target_pane() {
        let mut session = SessionState::default();
        session
            .apply(Command::OpenWorkspace(WorkspaceTarget::WindowsPath {
                path: PathBuf::from("D:/repo/amux"),
            }))
            .expect("workspace should open");

        let pane_id = session.active_workspace().unwrap().active_pane_id.clone();
        session
            .apply(Command::OpenSurface {
                pane_id: pane_id.clone(),
                surface: SurfaceState::Preview(PreviewSurfaceState {
                    surface_id: SurfaceId::new("surface-preview"),
                    source_relative_path: "README.md".into(),
                    kind: PreviewKind::Markdown,
                }),
            })
            .expect("surface should open");

        let workspace = session.active_workspace().unwrap();
        let crate::LayoutNode::Pane(pane) = &workspace.layout else {
            panic!("expected root pane");
        };
        assert_eq!(pane.pane_id, pane_id);
        assert_eq!(pane.tabs.len(), 2);
    }

    #[test]
    fn split_pane_creates_new_active_pane() {
        let mut session = SessionState::default();
        session
            .apply(Command::OpenWorkspace(WorkspaceTarget::WindowsPath {
                path: PathBuf::from("D:/repo/amux"),
            }))
            .expect("workspace should open");

        let pane_id = session.active_workspace().unwrap().active_pane_id.clone();
        let events = session
            .apply(Command::SplitPane {
                pane_id,
                axis: SplitAxis::Vertical,
            })
            .expect("split should succeed");

        let workspace = session.active_workspace().unwrap();
        assert_ne!(workspace.active_pane_id.0, "pane-workspace-1-1");
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn pane_commands_fail_without_active_workspace() {
        let mut session = SessionState::default();

        let err = session
            .apply(Command::SplitPane {
                pane_id: crate::PaneId::new("pane-1"),
                axis: SplitAxis::Horizontal,
            })
            .expect_err("should fail without workspace");

        assert_eq!(err, SessionOpError::NoActiveWorkspace);
    }
}
