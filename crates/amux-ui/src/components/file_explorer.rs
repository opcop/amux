use crate::FileListItem;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileExplorerPanel {
    files: Vec<FileListItem>,
}

impl FileExplorerPanel {
    pub fn new(files: Vec<FileListItem>) -> Self {
        Self { files }
    }

    pub fn render_text(&self) -> String {
        if self.files.is_empty() {
            return "Files\n  (empty)".into();
        }

        let mut lines = vec!["Files".to_string()];
        for file in &self.files {
            let kind = if file.is_dir { "dir" } else { "file" };
            lines.push(format!("  - {} [{}]", file.relative_path, kind));
        }
        lines.join("\n")
    }
}
