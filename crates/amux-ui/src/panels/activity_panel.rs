#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityPanel {
    pub last_error: Option<String>,
    pub entries: Vec<String>,
}

impl ActivityPanel {
    pub fn render_text(&self) -> String {
        let mut lines = vec!["Activity".to_string()];
        match &self.last_error {
            Some(error) => lines.push(format!("  Error: {error}")),
            None => lines.push("  Ready".into()),
        }
        for entry in &self.entries {
            lines.push(format!("  - {entry}"));
        }
        lines.join("\n")
    }
}
