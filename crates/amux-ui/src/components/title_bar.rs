#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleBar {
    app_name: String,
    workspace_name: Option<String>,
}

impl TitleBar {
    pub fn new(app_name: impl Into<String>, workspace_name: Option<String>) -> Self {
        Self {
            app_name: app_name.into(),
            workspace_name,
        }
    }

    pub fn render_text(&self) -> String {
        match &self.workspace_name {
            Some(workspace_name) => format!("{} | {}", self.app_name, workspace_name),
            None => self.app_name.clone(),
        }
    }
}

