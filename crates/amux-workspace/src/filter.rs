#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFilter {
    pub query: String,
    pub show_hidden: bool,
}

impl FileFilter {
    pub fn matches(&self, relative_path: &str, name: &str) -> bool {
        if !self.show_hidden && name.starts_with('.') {
            return false;
        }
        if self.query.is_empty() {
            return true;
        }

        let query = self.query.to_ascii_lowercase();
        name.to_ascii_lowercase().contains(&query)
            || relative_path.to_ascii_lowercase().contains(&query)
    }
}

#[cfg(test)]
mod tests {
    use super::FileFilter;

    #[test]
    fn filter_hides_hidden_files_by_default() {
        let filter = FileFilter {
            query: String::new(),
            show_hidden: false,
        };

        assert!(!filter.matches(".env", ".env"));
        assert!(filter.matches("src/main.rs", "main.rs"));
    }
}
