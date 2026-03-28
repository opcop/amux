#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTreeNode {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
}

impl FileTreeNode {
    pub fn new(name: impl Into<String>, relative_path: impl Into<String>, is_dir: bool) -> Self {
        Self {
            name: name.into(),
            relative_path: relative_path.into(),
            is_dir,
        }
    }
}
