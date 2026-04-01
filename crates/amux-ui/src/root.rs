use std::path::PathBuf;

use amux_core::{PaneId, SplitAxis, TabId};

use crate::{
    commands::UiAction,
    controller::AppController,
    render::{AppRenderer, TextRenderer},
    AppSnapshot, UiState,
};

#[derive(Clone, Debug)]
pub struct DesktopApp {
    name: String,
    state: UiState,
    controller: AppController,
}

impl DesktopApp {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            state: UiState::default(),
            controller: AppController::new(&name),
        }
    }

    /// Create with real filesystem and terminal backends
    pub fn with_real_backends(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            state: UiState::default(),
            controller: AppController::with_real_fs(&name),
        }
    }

    pub fn banner(&self) -> String {
        format!("{} bootstrap ready", self.name)
    }

    pub fn bootstrap_demo(&mut self) {
        self.controller.bootstrap_demo(&mut self.state);
    }

    pub fn dispatch(&mut self, action: UiAction) -> Vec<amux_core::Event> {
        self.controller.dispatch(&mut self.state, action)
    }

    pub fn run_command(&mut self, input: &str) -> Result<String, String> {
        self.controller.run_command(&mut self.state, input)
    }

    pub fn set_command_palette_query(&mut self, query: impl Into<String>) {
        let _ = self.dispatch(UiAction::SetCommandPaletteQuery(query.into()));
    }

    pub fn clear_command_palette_query(&mut self) {
        let _ = self.dispatch(UiAction::ClearCommandPaletteQuery);
    }

    pub fn append_command_palette_query(&mut self, segment: impl Into<String>) {
        let _ = self.dispatch(UiAction::AppendCommandPaletteQuery(segment.into()));
    }

    pub fn backspace_command_palette_query(&mut self) {
        let _ = self.dispatch(UiAction::BackspaceCommandPaletteQuery);
    }

    pub fn select_next_palette_item(&mut self) {
        let count =
            crate::commands::filtered_palette_commands(&self.state.command_palette_query).len();
        if count == 0 {
            let _ = self.dispatch(UiAction::SetCommandPaletteSelectedIndex(0));
            return;
        }
        let next = (self.state.command_palette_selected_index + 1) % count;
        let _ = self.dispatch(UiAction::SetCommandPaletteSelectedIndex(next));
    }

    pub fn select_previous_palette_item(&mut self) {
        let count =
            crate::commands::filtered_palette_commands(&self.state.command_palette_query).len();
        if count == 0 {
            let _ = self.dispatch(UiAction::SetCommandPaletteSelectedIndex(0));
            return;
        }
        let current = self.state.command_palette_selected_index.min(count - 1);
        let previous = if current == 0 { count - 1 } else { current - 1 };
        let _ = self.dispatch(UiAction::SetCommandPaletteSelectedIndex(previous));
    }

    /// Get the command string of the currently selected palette item.
    pub fn selected_palette_command_str(&self) -> Option<String> {
        let commands =
            crate::commands::filtered_palette_commands(&self.state.command_palette_query);
        if commands.is_empty() {
            return None;
        }
        let index = self.state.command_palette_selected_index
            .min(commands.len().saturating_sub(1));
        Some(commands[index].command.clone())
    }

    pub fn execute_selected_palette_command(&mut self) -> Result<String, String> {
        let commands =
            crate::commands::filtered_palette_commands(&self.state.command_palette_query);
        if commands.is_empty() {
            return Err("no command matches the current palette query".into());
        }
        let index = self
            .state
            .command_palette_selected_index
            .min(commands.len().saturating_sub(1));
        let command = commands[index].command.clone();
        self.run_command(&command)
    }

    pub fn activate_workspace(&mut self, workspace_id: &str) -> Result<(), String> {
        self.controller
            .activate_workspace(&mut self.state, workspace_id)
    }

    pub fn rename_workspace(&mut self, workspace_id: &str, new_name: &str) -> Result<String, String> {
        self.controller
            .rename_workspace(&mut self.state, workspace_id, new_name)
    }

    pub fn split_active_pane(&mut self, axis: SplitAxis) -> Result<(), String> {
        self.controller.split_active_pane(&mut self.state, axis)
    }

    pub fn focus_pane(&mut self, pane_id: PaneId) -> Result<(), String> {
        self.controller.focus_pane(&mut self.state, pane_id)
    }

    pub fn activate_tab(&mut self, pane_id: PaneId, tab_id: TabId) -> Result<(), String> {
        self.controller
            .activate_tab(&mut self.state, pane_id, tab_id)
    }

    pub fn close_tab(&mut self, pane_id: PaneId, tab_id: TabId) -> Result<(), String> {
        self.controller.close_tab(&mut self.state, pane_id, tab_id)
    }

    pub fn snapshot(&mut self) -> AppSnapshot {
        self.controller.snapshot(&mut self.state)
    }

    pub fn render_with<R: AppRenderer>(&mut self, renderer: &R) -> R::Output {
        let name = self.name.clone();
        let snapshot = self.snapshot();
        renderer.render(&name, &snapshot)
    }

    pub fn render_text_ui(&mut self) -> String {
        self.render_with(&TextRenderer)
    }

    pub fn session_path(&self) -> PathBuf {
        self.controller.session_path()
    }
}

#[cfg(test)]
mod tests {
    use super::DesktopApp;

    /// Create a test app with a unique session path to avoid cross-test interference
    fn test_app(suffix: &str) -> DesktopApp {
        let name = format!("amux-test-{}", suffix);
        // Clean up any leftover session
        let session_dir = std::env::temp_dir().join(format!("{}-session", name));
        let _ = std::fs::remove_dir_all(&session_dir);
        DesktopApp::new(name)
    }

    #[test]
    fn command_router_can_launch_agent_and_open_file() {
        let mut app = test_app("launch");
        app.bootstrap_demo();

        let launched = app
            .run_command("agent claude")
            .expect("agent command should work");
        let opened = app
            .run_command("file open notes.md")
            .expect("file command should work");

        assert!(launched.contains("claude"));
        assert!(opened.contains("notes.md"));
        let snapshot = app.snapshot();
        assert!(snapshot
            .open_files
            .iter()
            .any(|file| file.relative_path == "notes.md"));
    }

    #[test]
    fn command_router_reuses_existing_agent_and_editor_tabs() {
        let mut app = test_app("reuse");
        app.bootstrap_demo();

        let before = app.snapshot();
        let before_open_files = before.open_files.len();
        let before_agent_tabs = count_surface_kind(&before.active_workspace, "agent");

        app.run_command("agent codex")
            .expect("agent command should work");
        app.run_command("file open README.md")
            .expect("file command should work");

        let after = app.snapshot();
        assert_eq!(after.open_files.len(), before_open_files);
        assert_eq!(
            count_surface_kind(&after.active_workspace, "agent"),
            before_agent_tabs
        );
    }

    #[test]
    fn selected_palette_command_executes_filtered_command() {
        let mut app = test_app("palette-exec");
        app.bootstrap_demo();
        app.set_command_palette_query("claude");

        let result = app
            .execute_selected_palette_command()
            .expect("selected palette command should execute");

        assert!(result.contains("claude"));
    }

    #[test]
    fn palette_query_can_be_built_incrementally() {
        let mut app = test_app("palette-query");
        app.append_command_palette_query("agent");
        app.append_command_palette_query("claude");

        let snapshot = app.snapshot();
        assert_eq!(snapshot.command_palette_query, "agent claude");

        app.backspace_command_palette_query();
        let snapshot = app.snapshot();
        assert_eq!(snapshot.command_palette_query, "agent claud");
    }

    #[test]
    fn palette_selection_wraps_around_filtered_commands() {
        let mut app = test_app("palette-wrap");
        app.set_command_palette_query("agent");

        app.select_previous_palette_item();
        let snapshot = app.snapshot();
        assert_eq!(snapshot.command_palette_selected_index, 1);

        app.select_next_palette_item();
        let snapshot = app.snapshot();
        assert_eq!(snapshot.command_palette_selected_index, 0);
    }

    fn count_surface_kind(workspace: &Option<crate::WorkspaceSnapshot>, kind: &str) -> usize {
        fn count_layout(layout: &crate::LayoutSnapshot, kind: &str) -> usize {
            match layout {
                crate::LayoutSnapshot::Pane(pane) => pane
                    .tabs
                    .iter()
                    .filter(|tab| tab.surface_kind == kind)
                    .count(),
                crate::LayoutSnapshot::Split(split) => {
                    count_layout(&split.first, kind) + count_layout(&split.second, kind)
                }
            }
        }

        workspace
            .as_ref()
            .map(|workspace| count_layout(&workspace.layout, kind))
            .unwrap_or(0)
    }
}
