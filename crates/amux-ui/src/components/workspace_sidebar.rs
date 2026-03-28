use crate::WorkspaceListItem;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSidebar {
    items: Vec<WorkspaceListItem>,
}

impl WorkspaceSidebar {
    pub fn new(items: Vec<WorkspaceListItem>) -> Self {
        Self { items }
    }

    pub fn render_text(&self) -> String {
        if self.items.is_empty() {
            return "Workspaces\n  (empty)".into();
        }

        let mut lines = vec!["Workspaces".to_string()];
        for item in &self.items {
            let marker = if item.is_active { "*" } else { "-" };
            lines.push(format!("  {} {}", marker, item.name));
        }
        lines.join("\n")
    }
}
