//! Workspace Sidebar UI component
//!
//! Provides a workspace management panel with:
//! - Workspace list with active indicator
//! - Recent workspaces
//! - New/open workspace actions
//! - Collapse/expand toggle

#[cfg(feature = "gpui")]
use gpui::{
    rgb, px, div, prelude::*, AnyElement, FontWeight, IntoElement,
    ParentElement, Styled,
};
#[cfg(feature = "gpui")]
use amux_ui::GpuiWorkspaceItem;
#[cfg(feature = "gpui")]
use crate::gpui_components::action_button;

/// Sidebar display mode
#[cfg(feature = "gpui")]
#[derive(Clone, Debug, PartialEq)]
pub enum SidebarMode {
    Workspaces,
    Agents,
}

/// Item representing an agent in the sidebar
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct AgentSidebarItem {
    pub pane_id: String,
    pub tab_title: String,
    pub agent_kind: Option<String>,
    pub agent_status: Option<String>,
    pub status_icon: String,
    pub status_color: u32,
    pub workspace_name: String,
}

/// State for the workspace sidebar
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct WorkspaceSidebarState {
    pub collapsed: bool,
    pub show_recent: bool,
    /// User-resizable sidebar width (pixels). Clamped to [120, 480].
    pub width: f32,
    /// Current sidebar display mode.
    pub mode: SidebarMode,
}

#[cfg(feature = "gpui")]
impl Default for WorkspaceSidebarState {
    fn default() -> Self {
        Self {
            collapsed: false,
            show_recent: true,
            width: 220.0,
            mode: SidebarMode::Workspaces,
        }
    }
}

/// Render the full workspace sidebar
#[cfg(feature = "gpui")]
pub fn render_workspace_sidebar(
    workspaces: &[GpuiWorkspaceItem],
    state: &WorkspaceSidebarState,
) -> AnyElement {
    if state.collapsed {
        render_collapsed_sidebar_content(workspaces).into_any_element()
    } else {
        render_expanded_sidebar_content(workspaces).into_any_element()
    }
}

/// Render the collapsed sidebar content (icon-only mode)
#[cfg(feature = "gpui")]
fn render_collapsed_sidebar_content(workspaces: &[GpuiWorkspaceItem]) -> impl IntoElement {
    let total_count = workspaces.len();

    div()
        .w_12()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .p_2()
        .border_r_1()
        .border_color(rgb(0xd6d3d1))
        .bg(rgb(0xefe8db))
        .child(
            div()
                .w_8()
                .h_8()
                .rounded_md()
                .bg(rgb(0x7c3aed))
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(rgb(0xffffff))
                .font_weight(FontWeight::BOLD)
                .child("A"),
        )
        .child(
            div()
                .w_8()
                .h_px()
                .bg(rgb(0xd6d3d1)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .text_center()
                        .child(total_count.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .text_center()
                        .child("WS"),
                ),
        )
}

/// Render the expanded workspace sidebar content
#[cfg(feature = "gpui")]
fn render_expanded_sidebar_content(workspaces: &[GpuiWorkspaceItem]) -> impl IntoElement {
    div()
        .w_56()
        .flex()
        .flex_col()
        .gap_3()
        .p_3()
        .border_r_1()
        .border_color(rgb(0xd6d3d1))
        .bg(rgb(0xefe8db))
        // Header
        .child(render_sidebar_header())
        // Quick Actions
        .child(render_quick_actions())
        // Active Workspace
        .child(render_active_workspace_section(workspaces))
        // Recent Workspaces
        .child(render_recent_workspaces_section(workspaces))
}

/// Render the sidebar header with title and collapse button
#[cfg(feature = "gpui")]
fn render_sidebar_header() -> impl IntoElement {
    div()
        .flex()
        .justify_between()
        .items_center()
        .pb_2()
        .border_b_1()
        .border_color(rgb(0x2a2a2a))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    // Logo icon: mini split-pane with accent dividers
                    div()
                        .w_6()
                        .h_6()
                        .rounded(px(5.0))
                        .bg(rgb(0x1a1c2e))
                        .border_1()
                        .border_color(rgb(0x2a2d3d))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(rgb(0x81a2be))
                        .font_weight(FontWeight::BOLD)
                        .child("A"),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0xc5c8c6))
                        .child("AMUX"),
                ),
        )
        .child(
            div()
                .px_1p5()
                .py_0p5()
                .rounded_sm()
                .hover(|h| h.bg(rgb(0x313244)))
                .text_xs()
                .text_color(rgb(0x585b70))
                .child("[−]"),
        )
}

/// Render quick action buttons
#[cfg(feature = "gpui")]
fn render_quick_actions() -> impl IntoElement {
    div()
        .flex()
        .gap_2()
        .child(
            div()
                .flex_1()
                .px_2()
                .py_1p5()
                .rounded_md()
                .bg(rgb(0x7c3aed))
                .hover(|h| h.bg(rgb(0x6d28d9)))
                .flex()
                .items_center()
                .justify_center()
                .gap_1()
                .text_xs()
                .text_color(rgb(0xffffff))
                .font_weight(FontWeight::MEDIUM)
                .child("📁")
                .child("New"),
        )
        .child(
            div()
                .flex_1()
                .px_2()
                .py_1p5()
                .rounded_md()
                .border_1()
                .border_color(rgb(0xd6d3d1))
                .bg(rgb(0xffffff))
                .hover(|h| h.bg(rgb(0xf9fafb)))
                .flex()
                .items_center()
                .justify_center()
                .gap_1()
                .text_xs()
                .text_color(rgb(0x374151))
                .child("📂")
                .child("Open"),
        )
}

/// Render the active workspace section
#[cfg(feature = "gpui")]
fn render_active_workspace_section(workspaces: &[GpuiWorkspaceItem]) -> impl IntoElement {
    let active = workspaces.iter().find(|w| w.is_active);

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(0x9ca3af))
                .child("─── CURRENT ───"),
        )
        .child(
            div()
                .p_2()
                .rounded_md()
                .bg(rgb(0xffffff))
                .border_1()
                .border_color(rgb(0xd6d3d1))
                .flex()
                .flex_col()
                .gap_1()
                .when(active.is_some(), |this| {
                    this.border_color(rgb(0x7c3aed))
                        .bg(rgb(0xfaf5ff))
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_base()
                                .text_color(rgb(0x7c3aed))
                                .child("📁"),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0x1f2937))
                                .child(
                                    active
                                        .map(|w| w.name.clone())
                                        .unwrap_or_else(|| "No workspace".to_string()),
                                ),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_full()
                                .bg(rgb(0x22c55e))
                                .text_xs()
                                .text_color(rgb(0xffffff))
                                .child("Active"),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .child(
                            active
                                .map(|w| w.id.clone())
                                .unwrap_or_else(|| "Select a workspace".to_string()),
                        ),
                ),
        )
}

/// Render the recent workspaces section
#[cfg(feature = "gpui")]
fn render_recent_workspaces_section(workspaces: &[GpuiWorkspaceItem]) -> impl IntoElement {
    let recent: Vec<_> = workspaces
        .iter()
        .filter(|w| !w.is_active)
        .take(5)
        .collect();

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x9ca3af))
                        .child("─── RECENT ───"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x9ca3af))
                        .child(format!("{}/5", recent.len())),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1(),
        );

    // Build the list
    let mut list = div()
        .flex()
        .flex_col()
        .gap_1();

    if recent.is_empty() {
        list = list.child(
            div()
                .p_2()
                .rounded_md()
                .text_xs()
                .text_color(rgb(0x9ca3af))
                .text_center()
                .child("No recent workspaces"),
        );
    } else {
        for workspace in recent {
            list = list.child(render_workspace_item(workspace));
        }
    }

    list
}

/// Render a single workspace item
#[cfg(feature = "gpui")]
fn render_workspace_item(workspace: &GpuiWorkspaceItem) -> impl IntoElement {
    let name = workspace.name.clone();
    let _id = workspace.id.clone();

    div()
        .p_2()
        .rounded_md()
        .hover(|h| h.bg(rgb(0xe5e7eb)))
        .cursor_pointer()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x6b7280))
                .child("📁"),
        )
        .child(
            div()
                .flex_1()
                .text_sm()
                .text_color(rgb(0x374151))
                .child(name),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x9ca3af))
                .child("→"),
        )
}

/// Render workspace actions panel
#[cfg(feature = "gpui")]
pub fn render_workspace_actions() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(0x9ca3af))
                .child("─── ACTIONS ───"),
        )
        .child(
            action_button("new-workspace", "New Workspace")
                .flex()
                .items_center()
                .gap_2()
                .text_sm()
                .child("➕"),
        )
        .child(
            action_button("open-workspace", "Open Folder")
                .flex()
                .items_center()
                .gap_2()
                .text_sm()
                .child("📂"),
        )
}

/// Render workspace targets (Windows/WSL)
#[cfg(feature = "gpui")]
pub fn render_workspace_targets() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(0x9ca3af))
                .child("─── TARGET ───"),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(0xd6d3d1))
                        .bg(rgb(0xffffff))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap_1()
                        .text_xs()
                        .text_color(rgb(0x374151))
                        .child("🪟")
                        .child("Windows"),
                )
                .child(
                    div()
                        .flex_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(0xe5e7eb))
                        .hover(|h| h.border_color(rgb(0xd6d3d1)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap_1()
                        .text_xs()
                        .text_color(rgb(0x9ca3af))
                        .child("🐧")
                        .child("WSL"),
                ),
        )
}

/// Agent status group for ordering in the sidebar.
#[cfg(feature = "gpui")]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AgentGroup {
    Attention, // waiting / error
    Running,   // thinking
    Completed, // done
    Idle,      // no agent detected
}

#[cfg(feature = "gpui")]
impl AgentGroup {
    fn from_status(status: Option<&str>) -> Self {
        match status {
            Some("waiting") | Some("error") => AgentGroup::Attention,
            Some("thinking...") => AgentGroup::Running,
            Some("done") => AgentGroup::Completed,
            _ => AgentGroup::Idle,
        }
    }

    fn label(self) -> &'static str {
        match self {
            AgentGroup::Attention => "ATTENTION",
            AgentGroup::Running   => "RUNNING",
            AgentGroup::Completed => "COMPLETED",
            AgentGroup::Idle      => "IDLE",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            AgentGroup::Attention => "!",
            AgentGroup::Running   => "*",
            AgentGroup::Completed => "+",
            AgentGroup::Idle      => "-",
        }
    }

    fn color(self) -> u32 {
        match self {
            AgentGroup::Attention => 0xf9e2af, // yellow
            AgentGroup::Running   => 0x81a2be, // blue
            AgentGroup::Completed => 0xb5bd68, // green
            AgentGroup::Idle      => 0x969896, // gray
        }
    }
}

/// Render the agents sidebar content (grouped by status).
/// Returns a list of `AnyElement` children to be placed inside the sidebar column.
#[cfg(feature = "gpui")]
pub fn render_agents_sidebar_content(agents: &[AgentSidebarItem]) -> Vec<AnyElement> {
    // Group agents by status
    let mut grouped: std::collections::BTreeMap<AgentGroup, Vec<&AgentSidebarItem>> =
        std::collections::BTreeMap::new();
    for agent in agents {
        let group = AgentGroup::from_status(agent.agent_status.as_deref());
        grouped.entry(group).or_default().push(agent);
    }

    let mut elements: Vec<AnyElement> = Vec::new();

    if agents.is_empty() {
        elements.push(
            div()
                .px_3()
                .py_2()
                .text_xs()
                .text_color(rgb(0x969896))
                .child("No panes in workspace")
                .into_any_element(),
        );
        return elements;
    }

    for (group, items) in &grouped {
        // Group header
        elements.push(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px_3()
                .pt(px(8.0))
                .pb(px(4.0))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(group.color()))
                        .font_weight(FontWeight::BOLD)
                        .child(group.icon()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(group.color()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(group.label()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x585b70))
                        .child(format!("({})", items.len())),
                )
                .into_any_element(),
        );

        // Agent rows
        for agent in items {
            elements.push(render_agent_row(agent));
        }
    }

    elements
}

/// Render a single agent row in the sidebar.
/// The click handler must be wired externally via `on_click` on the parent
/// because this function does not have access to `cx.listener()`.
/// Instead, we return a plain div with an `id` based on pane_id.
#[cfg(feature = "gpui")]
fn render_agent_row(agent: &AgentSidebarItem) -> AnyElement {
    let icon_color = agent.status_color;
    let icon = agent.status_icon.clone();
    let title = agent.tab_title.clone();
    let pane_short = if agent.pane_id.len() > 8 {
        agent.pane_id[agent.pane_id.len() - 6..].to_string()
    } else {
        agent.pane_id.clone()
    };
    let kind_label = agent.agent_kind.clone().unwrap_or_default();

    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px_3()
        .py(px(5.0))
        .mx_1()
        .rounded(px(4.0))
        .cursor_pointer()
        .hover(|d| d.bg(rgb(0x252530)))
        // Status icon
        .child(
            div()
                .text_xs()
                .text_color(rgb(icon_color))
                .child(icon),
        )
        // Title
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_sm()
                .text_color(rgb(0xc5c8c6))
                .child(title),
        )
        // Agent kind badge
        .when(!kind_label.is_empty(), |d| {
            d.child(
                div()
                    .text_xs()
                    .text_color(rgb(0x585b70))
                    .child(kind_label),
            )
        })
        // Pane ID
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x45475a))
                .child(pane_short),
        )
        .into_any_element()
}
