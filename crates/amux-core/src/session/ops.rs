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
                // Dedupe: if a workspace with the same target is already in
                // the session, just activate it instead of pushing a fresh
                // copy. Without this, every `--workspace <path>` launch (or
                // sidebar "Open Workspace" / Ctrl+Shift+N click on the same
                // folder) accumulates a duplicate entry — so a smoke loop
                // that opens `/tmp/foo` six times ends up with a session
                // containing six identical `/tmp/foo` workspaces. The
                // sidebar then renders six rows for the same project, and
                // the session.json grows monotonically across launches.
                if let Some(existing) =
                    self.workspaces.iter().find(|ws| ws.target == target)
                {
                    let workspace_id = existing.id.clone();
                    self.active_workspace_id = Some(workspace_id.clone());
                    return Ok(vec![Event::WorkspaceOpened(workspace_id)]);
                }
                let workspace = build_workspace(
                    WorkspaceId::new(next_workspace_id(&self.workspaces)),
                    derive_workspace_name(&target),
                    target,
                );
                let workspace_id = workspace.id.clone();
                self.active_workspace_id = Some(workspace_id.clone());
                self.workspaces.push(workspace);
                Ok(vec![Event::WorkspaceOpened(workspace_id)])
            }
            Command::CreateWorkspace(target) => {
                // Unlike `OpenWorkspace`, this path intentionally
                // SKIPS dedup — the sidebar "+ New" button is the
                // caller, and its whole point is "give me another
                // fresh workspace even though one with this target
                // already exists". We still disambiguate the name
                // so the sidebar rows are visually distinct until
                // the user renames them.
                let base = derive_workspace_name(&target);
                let name = disambiguate_workspace_name(&self.workspaces, &base);
                let workspace = build_workspace(
                    WorkspaceId::new(next_workspace_id(&self.workspaces)),
                    name,
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
        // New workspaces go into the default / ungrouped bucket.
        // Phase 3 of the group rollout will add UI / commands to
        // move workspaces between groups; until then, the sidebar
        // renders this group without a header so the layout is
        // indistinguishable from the pre-group one.
        group_id: crate::WorkspaceGroupId::new(crate::DEFAULT_WORKSPACE_GROUP_ID),
    }
}

fn build_surface_tab(id: TabId, surface: SurfaceState) -> TabState {
    let title = default_surface_title(&surface);
    TabState::new(id, title, false, surface)
}

fn derive_workspace_name(target: &WorkspaceTarget) -> String {
    match target {
        WorkspaceTarget::LocalPath { path } => path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| path.display().to_string()),
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

/// Append a numeric suffix to `base` so the result doesn't match
/// any existing workspace name. Used by `CreateWorkspace` where the
/// caller wants a fresh workspace at the same target as an existing
/// one (sidebar "+ New"), and showing three rows all labeled
/// "arden" would be useless. Produces `base`, then `base 2`,
/// `base 3`, … in sequence.
fn disambiguate_workspace_name(workspaces: &[WorkspaceState], base: &str) -> String {
    let taken: std::collections::HashSet<&str> =
        workspaces.iter().map(|ws| ws.name.as_str()).collect();
    if !taken.contains(base) {
        return base.to_string();
    }
    for n in 2.. {
        let candidate = format!("{base} {n}");
        if !taken.contains(candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!("usize counter exhausted");
}

/// Pick the next `workspace-N` id by scanning **existing** ids for
/// the highest N and returning N+1. A naive `count + 1` collides the
/// moment the user closes any workspace that isn't the most recent
/// one: e.g. with `[workspace-1, workspace-2]`, closing `workspace-1`
/// leaves `[workspace-2]` at `count == 1`, and the next open would
/// produce `workspace-2` again — the sidebar then renders two rows
/// that share one id, both highlight as active, and the group_hover
/// effect double-fires across them. Scanning for the max dodges the
/// whole class by never reusing an id while any peer still holds it.
fn next_workspace_id(workspaces: &[WorkspaceState]) -> String {
    let max_n = workspaces
        .iter()
        .filter_map(|ws| ws.id.0.strip_prefix("workspace-")?.parse::<usize>().ok())
        .max()
        .unwrap_or(0);
    format!("workspace-{}", max_n + 1)
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

    use crate::{
        Command, PreviewKind, PreviewSurfaceState, SplitAxis, SurfaceId, SurfaceState,
        WorkspaceId,
    };
    use super::build_workspace;

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
    fn open_local_workspace_derives_name() {
        let mut session = SessionState::default();

        session
            .apply(Command::OpenWorkspace(WorkspaceTarget::LocalPath {
                path: PathBuf::from("/Users/arden/amux"),
            }))
            .expect("workspace should open");

        assert_eq!(session.active_workspace().unwrap().name, "amux");
    }

    #[test]
    fn open_workspace_dedupes_existing_target() {
        // Regression: every `--workspace <path>` (or sidebar "Open
        // Workspace" / Ctrl+Shift+N click on a folder already in the
        // session) used to push a fresh duplicate. The session would
        // grow monotonically across launches and the sidebar would
        // render N copies of the same project. Now, opening the same
        // target twice should reuse the existing workspace and just
        // re-activate it.
        let mut session = SessionState::default();
        let target = WorkspaceTarget::LocalPath {
            path: PathBuf::from("/tmp/foo"),
        };

        session
            .apply(Command::OpenWorkspace(target.clone()))
            .expect("first open should succeed");
        let first_id = session.active_workspace_id.clone();
        assert_eq!(session.workspaces.len(), 1);

        // Open another workspace in between to make sure dedup
        // doesn't depend on "most recently opened".
        session
            .apply(Command::OpenWorkspace(WorkspaceTarget::LocalPath {
                path: PathBuf::from("/tmp/bar"),
            }))
            .expect("second open should succeed");
        assert_eq!(session.workspaces.len(), 2);

        // Re-open `/tmp/foo` — must NOT add a third entry, must
        // re-activate the existing one.
        let events = session
            .apply(Command::OpenWorkspace(target.clone()))
            .expect("re-open should succeed");
        assert_eq!(
            session.workspaces.len(),
            2,
            "duplicate target must not push a new workspace"
        );
        assert_eq!(
            session.active_workspace_id, first_id,
            "re-opening must re-activate the original workspace id"
        );
        assert_eq!(events.len(), 1);
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

    #[test]
    fn create_workspace_skips_dedup_and_disambiguates_name() {
        // `Command::CreateWorkspace` is the sidebar "+ New" entry
        // point. Unlike `OpenWorkspace`, every call must produce a
        // new row — otherwise the button appears to do nothing the
        // second time the user clicks it. Names are auto-suffixed
        // so the rows are visually distinct until the user renames
        // them.
        let mut session = SessionState::default();
        let home = WorkspaceTarget::LocalPath {
            path: PathBuf::from("/tmp/arden"),
        };

        for _ in 0..3 {
            session
                .apply(Command::CreateWorkspace(home.clone()))
                .expect("create");
        }

        assert_eq!(session.workspaces.len(), 3, "each create must push a new row");
        assert_eq!(session.workspaces[0].name, "arden");
        assert_eq!(session.workspaces[1].name, "arden 2");
        assert_eq!(session.workspaces[2].name, "arden 3");
        // All three must have unique ids.
        let ids: std::collections::HashSet<_> =
            session.workspaces.iter().map(|ws| ws.id.0.as_str()).collect();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn default_session_has_default_group() {
        // Freshly-constructed sessions must always have at least
        // the default group present, so the rest of the code can
        // assume `groups` is non-empty without null-checking.
        let session = SessionState::default();
        assert!(!session.groups.is_empty(), "default session must seed the default group");
        assert_eq!(session.groups[0].id.0, crate::DEFAULT_WORKSPACE_GROUP_ID);
        assert_eq!(session.groups[0].name, "", "default group name must be empty");
    }

    #[test]
    fn new_workspace_lands_in_default_group() {
        // Every workspace created via `OpenWorkspace` gets the
        // default group id — the sidebar picks members by group_id,
        // so a missing assignment would make new workspaces invisible
        // under the new group-aware rendering.
        let mut session = SessionState::default();
        session
            .apply(Command::OpenWorkspace(WorkspaceTarget::LocalPath {
                path: PathBuf::from("/tmp/project"),
            }))
            .expect("open");
        assert_eq!(session.workspaces.len(), 1);
        assert_eq!(
            session.workspaces[0].group_id.0,
            crate::DEFAULT_WORKSPACE_GROUP_ID
        );
    }

    #[test]
    fn migrate_groups_is_idempotent() {
        // Running migration twice must be a no-op. The session
        // loader calls it on every `restore_session`, so a non-
        // idempotent migration would keep growing the groups list.
        let mut session = SessionState::default();
        session
            .apply(Command::OpenWorkspace(WorkspaceTarget::LocalPath {
                path: PathBuf::from("/tmp/a"),
            }))
            .expect("open");
        session.migrate_groups();
        let first = session.clone();
        session.migrate_groups();
        assert_eq!(session, first);
    }

    #[test]
    fn migrate_groups_rehomes_orphan_workspaces() {
        // Simulate loading a pre-group session by hand: empty
        // `groups`, and a workspace whose `group_id` points at a
        // group that doesn't exist. Migration must put the default
        // group back and re-home the orphan under it.
        let mut session = SessionState::default();
        session.groups.clear();
        session.workspaces.push(build_workspace(
            WorkspaceId::new("workspace-1"),
            "legacy".into(),
            WorkspaceTarget::LocalPath { path: PathBuf::from("/tmp/legacy") },
        ));
        // Point at a group that doesn't exist to simulate an orphan.
        session.workspaces[0].group_id =
            crate::WorkspaceGroupId::new("group-ghost");

        session.migrate_groups();

        assert_eq!(session.groups.len(), 1);
        assert_eq!(session.groups[0].id.0, crate::DEFAULT_WORKSPACE_GROUP_ID);
        assert_eq!(
            session.workspaces[0].group_id.0,
            crate::DEFAULT_WORKSPACE_GROUP_ID,
            "orphan workspace must be re-homed into the default group"
        );
    }

    #[test]
    fn open_workspace_after_delete_assigns_fresh_id() {
        // Regression for the sidebar "two rows both highlighted" bug:
        // `next_workspace_id` used to derive from `workspaces.len()`,
        // so closing a non-last workspace and then opening a new one
        // would hand the new entry an id that's already taken. Both
        // rows then matched `active_workspace_id == ws.id` and both
        // rendered as selected, and the group_hover effect double-
        // fired across them because they shared the same `ws-group-*`
        // key. The fix scans existing ids for the maximum N and
        // returns N+1, which can never collide with a live peer.
        let mut session = SessionState::default();
        session
            .apply(Command::OpenWorkspace(WorkspaceTarget::LocalPath {
                path: PathBuf::from("/tmp/a"),
            }))
            .expect("open a");
        session
            .apply(Command::OpenWorkspace(WorkspaceTarget::LocalPath {
                path: PathBuf::from("/tmp/b"),
            }))
            .expect("open b");
        assert_eq!(session.workspaces[0].id.0, "workspace-1");
        assert_eq!(session.workspaces[1].id.0, "workspace-2");

        // Close the *first* one (not the most recent) so len drops
        // back to 1 while the surviving id is workspace-2.
        session
            .apply(Command::CloseWorkspace(WorkspaceId::new("workspace-1")))
            .expect("close a");
        assert_eq!(session.workspaces.len(), 1);
        assert_eq!(session.workspaces[0].id.0, "workspace-2");

        // Opening a fresh workspace must get a brand new id, NOT
        // collide with the surviving workspace-2.
        session
            .apply(Command::OpenWorkspace(WorkspaceTarget::LocalPath {
                path: PathBuf::from("/tmp/c"),
            }))
            .expect("open c");
        assert_eq!(session.workspaces.len(), 2);
        assert_eq!(session.workspaces[0].id.0, "workspace-2");
        assert_eq!(
            session.workspaces[1].id.0, "workspace-3",
            "new workspace must get a fresh id, not reuse workspace-2"
        );
        let ids: std::collections::HashSet<_> =
            session.workspaces.iter().map(|ws| ws.id.0.as_str()).collect();
        assert_eq!(ids.len(), 2, "all ids must be unique");
    }
}
