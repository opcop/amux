use std::fmt;

use crate::{
    commands::{
        filtered_palette_commands_for, palette_filter_help, palette_query_suggestions, PaletteCommand,
    },
    ActiveSurfaceItem, AppSnapshot, LayoutSnapshot,
};

use super::AppRenderer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuiSection {
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuiWorkspaceItem {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    pub target_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuiAgentItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub supported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuiFileItem {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuiOpenFileItem {
    pub relative_path: String,
    pub display_path: String,
    pub content_preview: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuiTabItem {
    pub pane_id: String,
    pub tab_id: String,
    pub title: String,
    pub surface_kind: &'static str,
    pub is_active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuiPaneItem {
    pub pane_id: String,
    pub is_active: bool,
    pub tab_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuiPaletteCommandItem {
    pub command: String,
    pub label: String,
    pub description: String,
    pub category: String,
    pub keybinding: Option<String>,
    pub is_selected: bool,
}

impl From<(&PaletteCommand, bool)> for GpuiPaletteCommandItem {
    fn from((cmd, selected): (&PaletteCommand, bool)) -> Self {
        Self {
            command: cmd.command.clone(),
            label: cmd.label.clone(),
            description: cmd.description.clone(),
            category: cmd.category.label().to_string(),
            keybinding: cmd.keybinding.clone(),
            is_selected: selected,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuiActiveSurfaceItem {
    pub pane_id: String,
    pub tab_id: String,
    pub tab_title: String,
    pub surface_kind: &'static str,
    pub summary_lines: Vec<String>,
    pub content_lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuiWindowModel {
    pub title: String,
    pub workspace_count: usize,
    pub layout_node_count: usize,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub command_palette_selected_index: usize,
    pub command_palette_command_count: usize, // Count of filtered commands
    pub active_workspace_name: Option<String>,
    pub last_activity: Option<String>,
    pub palette_filters: Vec<String>,
    pub palette_query_suggestions: Vec<String>,
    pub palette_commands: Vec<GpuiPaletteCommandItem>,
    pub selected_palette_command: Option<String>,
    pub workspace_items: Vec<GpuiWorkspaceItem>,
    pub agent_items: Vec<GpuiAgentItem>,
    pub file_items: Vec<GpuiFileItem>,
    pub open_file_items: Vec<GpuiOpenFileItem>,
    pub tab_items: Vec<GpuiTabItem>,
    pub pane_items: Vec<GpuiPaneItem>,
    pub active_surface: Option<GpuiActiveSurfaceItem>,
    pub sections: Vec<GpuiSection>,
    // Status bar fields
    pub status_save: String, // "saved 2m ago", "unsaved", "saving"
    pub status_wsl_distro: Option<String>,
    pub status_split_count: usize,
    pub status_terminal_shell: Option<String>,
    // System metrics
    pub status_cpu_usage: Option<String>,
    pub status_memory_usage: Option<String>,
    pub status_load_color: Option<String>,
    pub local_workspace_supported: bool,
    pub wsl_supported: bool,
    pub browser_supported: bool,
    pub folder_picker_supported: bool,
}

impl fmt::Display for GpuiWindowModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "GpuiWindowModel(title={}, workspaces={}, layout_nodes={})",
            self.title, self.workspace_count, self.layout_node_count
        )?;
        for section in &self.sections {
            writeln!(f, "[{}]", section.title)?;
            for line in &section.lines {
                writeln!(f, "  {}", line)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuiRenderer;

impl AppRenderer for GpuiRenderer {
    type Output = GpuiWindowModel;

    fn render(&self, app_name: &str, snapshot: &AppSnapshot) -> Self::Output {
        let layout_node_count = snapshot
            .active_workspace
            .as_ref()
            .map(|workspace| count_layout_nodes(&workspace.layout))
            .unwrap_or(0);
        let palette_commands = filtered_palette_commands_for(
            &snapshot.command_palette_query,
            &snapshot.platform_capabilities,
        );
        let selected_index = clamped_selected_index(
            snapshot.command_palette_selected_index,
            palette_commands.len(),
        );
        let palette_items: Vec<GpuiPaletteCommandItem> = palette_commands
            .iter()
            .enumerate()
            .map(|(i, cmd)| GpuiPaletteCommandItem::from((cmd, i == selected_index)))
            .collect();

        GpuiWindowModel {
            title: app_name.to_string(),
            workspace_count: snapshot.workspaces.len(),
            layout_node_count,
            command_palette_open: snapshot.command_palette_open,
            command_palette_query: snapshot.command_palette_query.clone(),
            command_palette_selected_index: selected_index,
            command_palette_command_count: palette_items.len(),
            active_workspace_name: snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.is_active)
                .map(|workspace| workspace.name.clone()),
            last_activity: snapshot.activity_log.last().cloned(),
            palette_filters: palette_filter_help()
                .iter()
                .map(|filter| (*filter).to_string())
                .collect(),
            palette_query_suggestions: palette_query_suggestions()
                .iter()
                .map(|item| (*item).to_string())
                .collect(),
            selected_palette_command: palette_commands
                .get(selected_index)
                .map(|c| c.command.clone()),
            palette_commands: palette_items,
            workspace_items: snapshot
                .workspaces
                .iter()
                .map(|workspace| GpuiWorkspaceItem {
                    id: workspace.id.clone(),
                    name: workspace.name.clone(),
                    is_active: workspace.is_active,
                    target_path: workspace.target_path.clone(),
                })
                .collect(),
            agent_items: snapshot
                .agents
                .iter()
                .map(|agent| GpuiAgentItem {
                    id: agent.id.clone(),
                    name: agent.name.clone(),
                    status: agent.status.clone(),
                    supported: agent.supported,
                })
                .collect(),
            file_items: snapshot
                .files
                .iter()
                .map(|file| GpuiFileItem {
                    name: file.name.clone(),
                    relative_path: file.relative_path.clone(),
                    is_dir: file.is_dir,
                })
                .collect(),
            open_file_items: snapshot
                .open_files
                .iter()
                .map(|file| GpuiOpenFileItem {
                    relative_path: file.relative_path.clone(),
                    display_path: file.display_path.clone(),
                    content_preview: file.content_preview.clone(),
                })
                .collect(),
            tab_items: snapshot
                .active_workspace
                .as_ref()
                .map(|workspace| collect_tabs(&workspace.layout))
                .unwrap_or_default(),
            pane_items: snapshot
                .active_workspace
                .as_ref()
                .map(|workspace| collect_panes(&workspace.layout))
                .unwrap_or_default(),
            active_surface: snapshot.active_surface.as_ref().map(map_active_surface),
            sections: vec![
                GpuiSection {
                    title: "Sidebar".into(),
                    lines: snapshot
                        .workspaces
                        .iter()
                        .map(|workspace| {
                            let marker = if workspace.is_active { "*" } else { "-" };
                            format!("{marker} {}", workspace.name)
                        })
                        .collect(),
                },
                GpuiSection {
                    title: "Agents".into(),
                    lines: snapshot
                        .agents
                        .iter()
                        .map(|agent| format!("{} [{}]", agent.name, agent.status))
                        .collect(),
                },
                GpuiSection {
                    title: "Files".into(),
                    lines: snapshot
                        .files
                        .iter()
                        .map(|file| file.relative_path.clone())
                        .collect(),
                },
                GpuiSection {
                    title: "Layout".into(),
                    lines: snapshot
                        .active_workspace
                        .as_ref()
                        .map(|workspace| flatten_layout(&workspace.layout))
                        .unwrap_or_else(|| vec!["No workspace".into()]),
                },
                GpuiSection {
                    title: "Activity".into(),
                    lines: if snapshot.activity_log.is_empty() {
                        vec!["Ready".into()]
                    } else {
                        snapshot.activity_log.clone()
                    },
                },
            ],
            // Status bar fields
            status_save: snapshot.save_status.clone(),
            status_wsl_distro: snapshot.status_wsl_distro.clone(),
            status_split_count: snapshot.status_split_count,
            status_terminal_shell: snapshot.status_terminal_shell.clone(),
            // System metrics
            status_cpu_usage: snapshot.status_cpu_usage.clone(),
            status_memory_usage: snapshot.status_memory_usage.clone(),
            status_load_color: snapshot.status_load_color.clone(),
            local_workspace_supported: snapshot.platform_capabilities.local_workspace,
            wsl_supported: snapshot.platform_capabilities.wsl_workspace,
            browser_supported: snapshot.platform_capabilities.browser_tabs,
            folder_picker_supported: snapshot.platform_capabilities.folder_picker,
        }
    }
}

fn map_active_surface(item: &ActiveSurfaceItem) -> GpuiActiveSurfaceItem {
    GpuiActiveSurfaceItem {
        pane_id: item.pane_id.clone(),
        tab_id: item.tab_id.clone(),
        tab_title: item.tab_title.clone(),
        surface_kind: item.surface_kind,
        summary_lines: item.summary_lines.clone(),
        content_lines: item.content_lines.clone(),
    }
}

fn clamped_selected_index(selected_index: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        selected_index.min(len - 1)
    }
}

fn count_layout_nodes(layout: &LayoutSnapshot) -> usize {
    match layout {
        LayoutSnapshot::Pane(_) => 1,
        LayoutSnapshot::Split(split) => {
            1 + count_layout_nodes(&split.first) + count_layout_nodes(&split.second)
        }
    }
}

fn flatten_layout(layout: &LayoutSnapshot) -> Vec<String> {
    fn visit(layout: &LayoutSnapshot, depth: usize, lines: &mut Vec<String>) {
        match layout {
            LayoutSnapshot::Pane(pane) => {
                let active = if pane.is_active { "*" } else { "-" };
                lines.push(format!("{}{} Pane {}", "  ".repeat(depth), active, pane.id));
            }
            LayoutSnapshot::Split(split) => {
                lines.push(format!("{}Split({:?})", "  ".repeat(depth), split.axis));
                visit(&split.first, depth + 1, lines);
                visit(&split.second, depth + 1, lines);
            }
        }
    }

    let mut lines = Vec::new();
    visit(layout, 0, &mut lines);
    lines
}

fn collect_tabs(layout: &LayoutSnapshot) -> Vec<GpuiTabItem> {
    fn visit(layout: &LayoutSnapshot, tabs: &mut Vec<GpuiTabItem>) {
        match layout {
            LayoutSnapshot::Pane(pane) => {
                for tab in &pane.tabs {
                    tabs.push(GpuiTabItem {
                        pane_id: pane.id.clone(),
                        tab_id: tab.id.clone(),
                        title: tab.title.clone(),
                        surface_kind: tab.surface_kind,
                        is_active: tab.is_active,
                    });
                }
            }
            LayoutSnapshot::Split(split) => {
                visit(&split.first, tabs);
                visit(&split.second, tabs);
            }
        }
    }

    let mut tabs = Vec::new();
    visit(layout, &mut tabs);
    tabs
}

fn collect_panes(layout: &LayoutSnapshot) -> Vec<GpuiPaneItem> {
    fn visit(layout: &LayoutSnapshot, panes: &mut Vec<GpuiPaneItem>) {
        match layout {
            LayoutSnapshot::Pane(pane) => panes.push(GpuiPaneItem {
                pane_id: pane.id.clone(),
                is_active: pane.is_active,
                tab_count: pane.tabs.len(),
            }),
            LayoutSnapshot::Split(split) => {
                visit(&split.first, panes);
                visit(&split.second, panes);
            }
        }
    }

    let mut panes = Vec::new();
    visit(layout, &mut panes);
    panes
}

#[cfg(test)]
mod tests {
    use crate::{
        AgentListItem, AppSnapshot, FileListItem, LayoutSnapshot, OpenFileItem, PaneSnapshot,
        SplitSnapshot, TabSnapshot, WorkspaceListItem, WorkspaceSnapshot,
    };

    use super::{AppRenderer, GpuiRenderer};

    #[test]
    fn gpui_renderer_builds_window_model() {
        let snapshot = AppSnapshot {
            workspaces: vec![WorkspaceListItem {
                id: "workspace-1".into(),
                name: "demo".into(),
                is_active: true,
                target_path: None,
            }],
            recent_workspaces: vec![],
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
            active_surface: Some(crate::ActiveSurfaceItem {
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
            activity_log: vec!["session: loaded from store".into()],
        };

        let model = GpuiRenderer.render("AMUX", &snapshot);
        assert_eq!(model.workspace_count, 1);
        assert_eq!(model.layout_node_count, 3);
        assert!(!model.command_palette_open);
        assert!(model.command_palette_query.is_empty());
        assert_eq!(model.command_palette_selected_index, 0);
        assert_eq!(
            model.command_palette_command_count,
            model.palette_commands.len()
        );
        assert_eq!(model.active_workspace_name.as_deref(), Some("demo"));
        assert_eq!(
            model.last_activity.as_deref(),
            Some("session: loaded from store")
        );
        assert!(!model.palette_filters.is_empty());
        assert!(!model.palette_query_suggestions.is_empty());
        assert!(!model.palette_commands.is_empty());
        assert_eq!(model.selected_palette_command.as_deref(), Some("help"));
        assert_eq!(model.workspace_items.len(), 1);
        assert_eq!(model.agent_items.len(), 1);
        assert_eq!(model.file_items.len(), 1);
        assert_eq!(model.open_file_items.len(), 1);
        assert_eq!(model.tab_items.len(), 2);
        assert_eq!(model.pane_items.len(), 2);
        assert!(model.active_surface.is_some());
        assert!(model
            .sections
            .iter()
            .any(|section| section.title == "Layout"));
    }

    #[test]
    fn gpui_renderer_filters_palette_commands_by_query() {
        let snapshot = AppSnapshot {
            workspaces: vec![],
            agents: vec![],
            files: vec![],
            open_files: vec![],
            active_surface: None,
            active_workspace: None,
            command_palette_open: true,
            command_palette_query: "agent".into(),
            command_palette_selected_index: 3,
            last_error: None,
            activity_log: vec![],
        };

        let model = GpuiRenderer.render("AMUX", &snapshot);

        assert_eq!(model.command_palette_query, "agent");
        assert_eq!(model.command_palette_selected_index, 0);
        assert_eq!(
            model.selected_palette_command.as_deref(),
            Some("agent codex")
        );
        assert!(model
            .palette_commands
            .iter()
            .all(|item| item.command.contains("agent")
                || item.label.to_ascii_lowercase().contains("agent")
                || item.description.to_ascii_lowercase().contains("agent")
                || item.category.to_ascii_lowercase().contains("agent")));
        assert!(!model.palette_commands.is_empty());
    }
}
