use amux_core::SplitAxis;

use crate::{LayoutSnapshot, PaneSnapshot, WorkspaceSnapshot};

use super::TabStrip;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneGrid {
    workspace: Option<WorkspaceSnapshot>,
}

impl PaneGrid {
    pub fn new(workspace: Option<WorkspaceSnapshot>) -> Self {
        Self { workspace }
    }

    pub fn render_text(&self) -> String {
        let Some(workspace) = &self.workspace else {
            return "Pane Grid\n  No workspace open".into();
        };

        let mut lines = vec![format!("Pane Grid [{}]", workspace.name)];
        render_layout(&workspace.layout, 1, &mut lines);
        lines.join("\n")
    }
}

fn render_pane(pane: &PaneSnapshot) -> String {
    let focus = if pane.is_active { "*" } else { "-" };
    let tabs = TabStrip::new(pane.tabs.clone()).render_text();
    format!("  {} Pane {} :: {}", focus, pane.id, tabs)
}

fn render_layout(layout: &LayoutSnapshot, depth: usize, lines: &mut Vec<String>) {
    match layout {
        LayoutSnapshot::Pane(pane) => {
            lines.push(format!(
                "{}{}",
                indent(depth),
                render_pane(pane).trim_start()
            ));
        }
        LayoutSnapshot::Split(split) => {
            lines.push(format!("{}{}", indent(depth), split_label(split.axis)));
            render_layout(&split.first, depth + 1, lines);
            render_layout(&split.second, depth + 1, lines);
        }
    }
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn split_label(axis: SplitAxis) -> &'static str {
    match axis {
        SplitAxis::Horizontal => "Split(horizontal)",
        SplitAxis::Vertical => "Split(vertical)",
    }
}
