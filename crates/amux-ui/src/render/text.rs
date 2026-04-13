use crate::{
    commands::filtered_palette_commands,
    components::{
        AgentLauncherPanel, CommandPalette, FileExplorerPanel, OpenFilesPanel, PaneGrid, TitleBar,
        WorkspaceSidebar,
    },
    panels::ActivityPanel,
    AppSnapshot,
};

use super::AppRenderer;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextRenderer;

impl AppRenderer for TextRenderer {
    type Output = String;

    fn render(&self, app_name: &str, snapshot: &AppSnapshot) -> Self::Output {
        let title_bar = TitleBar::new(
            app_name.to_string(),
            snapshot.active_workspace.as_ref().map(|ws| ws.name.clone()),
        )
        .render_text();
        let sidebar = WorkspaceSidebar::new(snapshot.workspaces.clone()).render_text();
        let agents = AgentLauncherPanel::new(snapshot.agents.clone()).render_text();
        let files = FileExplorerPanel::new(snapshot.files.clone()).render_text();
        let open_files = OpenFilesPanel::new(snapshot.open_files.clone()).render_text();
        let pane_grid = PaneGrid::new(snapshot.active_workspace.clone()).render_text();
        let activity = ActivityPanel {
            last_error: snapshot.last_error.clone(),
            entries: snapshot.activity_log.clone(),
        }
        .render_text();
        let command_palette = CommandPalette {
            open: snapshot.command_palette_open,
            query: snapshot.command_palette_query.clone(),
            selected_index: snapshot.command_palette_selected_index,
            commands: filtered_palette_commands(&snapshot.command_palette_query),
        }
        .render_text();

        [
            title_bar,
            String::new(),
            sidebar,
            String::new(),
            agents,
            String::new(),
            files,
            String::new(),
            open_files,
            String::new(),
            pane_grid,
            String::new(),
            activity,
            command_palette,
        ]
        .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use amux_platform::PlatformCapabilities;

    use crate::{
        ActiveSurfaceItem, AgentListItem, AppSnapshot, FileListItem, LayoutSnapshot, OpenFileItem,
        PaneSnapshot, SplitSnapshot, TabSnapshot, WorkspaceListItem, WorkspaceSnapshot,
    };

    use super::{AppRenderer, TextRenderer};

    #[test]
    fn text_renderer_includes_split_hierarchy() {
        let snapshot = AppSnapshot {
            workspaces: vec![WorkspaceListItem {
                id: "workspace-1".into(),
                name: "demo".into(),
                is_active: true,
                target_path: None,
                group_id: amux_core::DEFAULT_WORKSPACE_GROUP_ID.into(),
            }],
            workspace_groups: vec![],
            agents: vec![AgentListItem {
                id: "codex".into(),
                name: "Codex".into(),
                status: "installed".into(),
                supported: true,
            }],
            files: vec![FileListItem {
                name: "README.md".into(),
                relative_path: "README.md".into(),
                is_dir: false,
            }],
            open_files: vec![OpenFileItem {
                relative_path: "README.md".into(),
                display_path: "README.md".into(),
                content_preview: "markdown".into(),
            }],
            active_surface: Some(ActiveSurfaceItem {
                pane_id: "pane-2".into(),
                tab_id: "tab-2".into(),
                tab_title: "Preview".into(),
                surface_kind: "preview",
                summary_lines: vec!["Source: README.md".into()],
                content_lines: vec!["# AMUX".into()],
            }),
            active_workspace: Some(WorkspaceSnapshot {
                id: "workspace-1".into(),
                name: "demo".into(),
                layout: LayoutSnapshot::Split(SplitSnapshot {
                    axis: amux_core::SplitAxis::Vertical,
                    first: Box::new(LayoutSnapshot::Pane(PaneSnapshot {
                        id: "pane-1".into(),
                        is_active: false,
                        tabs: vec![TabSnapshot {
                            id: "tab-1".into(),
                            title: "Welcome".into(),
                            is_active: true,
                            surface_kind: "welcome",
                        }],
                    })),
                    second: Box::new(LayoutSnapshot::Pane(PaneSnapshot {
                        id: "pane-2".into(),
                        is_active: true,
                        tabs: vec![TabSnapshot {
                            id: "tab-2".into(),
                            title: "Preview".into(),
                            is_active: true,
                            surface_kind: "preview",
                        }],
                    })),
                }),
            }),
            command_palette_open: false,
            command_palette_query: String::new(),
            command_palette_selected_index: 0,
            last_error: None,
            activity_log: vec!["event: workspace opened workspace-1".into()],
            recent_workspaces: vec![],
            save_status: "saved just now".into(),
            dirty: false,
            platform_capabilities: PlatformCapabilities::default(),
            status_wsl_distro: None,
            status_split_count: 0,
            status_terminal_shell: None,
            status_cpu_usage: None,
            status_cpu_cores: None,
            status_memory_usage: None,
            status_memory_percent: None,
            status_load_color: None,
        };

        let output = TextRenderer.render("AMUX", &snapshot);
        assert!(output.contains("Agents"));
        assert!(output.contains("Files"));
        assert!(output.contains("Open Files"));
        assert!(output.contains("Split(vertical)"));
        assert!(output.contains("Pane pane-1"));
        assert!(output.contains("Pane pane-2"));
    }
}
