use crate::OpenFileItem;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenFilesPanel {
    files: Vec<OpenFileItem>,
}

impl OpenFilesPanel {
    pub fn new(files: Vec<OpenFileItem>) -> Self {
        Self { files }
    }

    pub fn render_text(&self) -> String {
        if self.files.is_empty() {
            return "Open Files\n  (none)".into();
        }

        let mut lines = vec!["Open Files".to_string()];
        for file in &self.files {
            lines.push(format!(
                "  - {} [{}]",
                file.relative_path, file.content_preview
            ));
        }
        lines.join("\n")
    }
}
