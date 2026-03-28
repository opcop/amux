use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use amux_core::WorkspaceTarget;

use crate::MappedFile;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FsEntry {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
}

pub trait FsBackend: Send + Sync {
    fn read_to_string(&self, file: &MappedFile) -> Result<String, String>;
    fn write_string(&self, file: &MappedFile, content: &str) -> Result<(), String>;
    fn read_dir(&self, target: &WorkspaceTarget, relative_path: &str) -> Result<Vec<FsEntry>, String>;
}

/// Real filesystem backend that reads/writes actual files on disk
#[derive(Clone, Debug, Default)]
pub struct RealFsBackend;

impl RealFsBackend {
    pub fn new() -> Self {
        Self
    }

    /// Get the absolute path for a directory within a workspace target
    fn target_dir(target: &WorkspaceTarget, relative_path: &str) -> Result<PathBuf, String> {
        let base = match target {
            WorkspaceTarget::WindowsPath { path } => path.clone(),
            WorkspaceTarget::WslPath { distro, path } => {
                // WSL paths are converted to UNC by PathMapper, use them directly
                let unc_path = crate::windows::paths::wsl_unc_path(distro, path);
                PathBuf::from(unc_path)
            }
        };

        if relative_path.is_empty() {
            Ok(base)
        } else {
            Ok(base.join(relative_path))
        }
    }
}

impl FsBackend for RealFsBackend {
    fn read_to_string(&self, file: &MappedFile) -> Result<String, String> {
        fs::read_to_string(&file.native_path)
            .map_err(|e| format!("failed to read {}: {}", file.native_path.display(), e))
    }

    fn write_string(&self, file: &MappedFile, content: &str) -> Result<(), String> {
        // Ensure parent directory exists
        if let Some(parent) = file.native_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directory: {}", e))?;
        }
        fs::write(&file.native_path, content)
            .map_err(|e| format!("failed to write {}: {}", file.native_path.display(), e))
    }

    fn read_dir(&self, target: &WorkspaceTarget, relative_path: &str) -> Result<Vec<FsEntry>, String> {
        let dir_path = Self::target_dir(target, relative_path)?;

        let entries = fs::read_dir(&dir_path)
            .map_err(|e| format!("failed to read directory {}: {}", dir_path.display(), e))?;

        let mut result = Vec::new();
        let base_name = relative_path.trim_start_matches('/');

        for entry in entries {
            let entry = entry.map_err(|e| format!("failed to read dir entry: {}", e))?;
            let metadata = entry.metadata()
                .map_err(|e| format!("failed to read metadata: {}", e))?;

            let name = entry.file_name()
                .to_string_lossy()
                .into_owned();

            // Build relative path for this entry
            let entry_relative = if base_name.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", base_name, name)
            };

            result.push(FsEntry {
                name,
                relative_path: entry_relative,
                is_dir: metadata.is_dir(),
            });
        }

        Ok(result)
    }
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryFsBackend {
    files: Arc<Mutex<BTreeMap<String, String>>>,
    directories: Arc<Mutex<BTreeMap<String, Vec<FsEntry>>>>,
}

impl InMemoryFsBackend {
    pub fn add_dir(
        &self,
        target: &WorkspaceTarget,
        relative_path: &str,
        entries: Vec<FsEntry>,
    ) -> Result<(), String> {
        let key = dir_key(target, relative_path);
        let mut directories = self
            .directories
            .lock()
            .map_err(|_| "fs backend mutex poisoned".to_string())?;
        directories.insert(key, entries);
        Ok(())
    }

    pub fn add_file(&self, file: &MappedFile, content: impl Into<String>) -> Result<(), String> {
        let mut files = self
            .files
            .lock()
            .map_err(|_| "fs backend mutex poisoned".to_string())?;
        files.insert(file.native_path.display().to_string(), content.into());
        Ok(())
    }
}

impl FsBackend for InMemoryFsBackend {
    fn read_to_string(&self, file: &MappedFile) -> Result<String, String> {
        let files = self
            .files
            .lock()
            .map_err(|_| "fs backend mutex poisoned".to_string())?;
        files
            .get(&file.native_path.display().to_string())
            .cloned()
            .ok_or_else(|| format!("file not found: {}", file.native_path.display()))
    }

    fn write_string(&self, file: &MappedFile, content: &str) -> Result<(), String> {
        let mut files = self
            .files
            .lock()
            .map_err(|_| "fs backend mutex poisoned".to_string())?;
        files.insert(file.native_path.display().to_string(), content.to_string());
        Ok(())
    }

    fn read_dir(&self, target: &WorkspaceTarget, relative_path: &str) -> Result<Vec<FsEntry>, String> {
        let directories = self
            .directories
            .lock()
            .map_err(|_| "fs backend mutex poisoned".to_string())?;
        directories
            .get(&dir_key(target, relative_path))
            .cloned()
            .ok_or_else(|| format!("directory not found: {}", dir_key(target, relative_path)))
    }
}

fn dir_key(target: &WorkspaceTarget, relative_path: &str) -> String {
    let root = match target {
        WorkspaceTarget::WindowsPath { path } => path.display().to_string(),
        WorkspaceTarget::WslPath { distro, path } => format!("{distro}:{path}"),
    };
    if relative_path.is_empty() {
        root
    } else {
        format!("{root}::{relative_path}")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use amux_core::WorkspaceTarget;

    use crate::{FsBackend, MappedFile};

    use super::{FsEntry, InMemoryFsBackend, RealFsBackend};

    #[test]
    fn in_memory_fs_reads_dirs_and_files() {
        let fs = InMemoryFsBackend::default();
        let target = WorkspaceTarget::WindowsPath {
            path: PathBuf::from("D:/repo/amux"),
        };

        fs.add_dir(
            &target,
            "",
            vec![FsEntry {
                name: "README.md".into(),
                relative_path: "README.md".into(),
                is_dir: false,
            }],
        )
        .expect("dir should be added");

        let file = MappedFile {
            display_path: "D:/repo/amux/README.md".into(),
            native_path: PathBuf::from("D:/repo/amux/README.md"),
        };
        fs.add_file(&file, "# Hello").expect("file should be added");

        let entries = fs.read_dir(&target, "").expect("dir should be readable");
        let content = fs.read_to_string(&file).expect("file should be readable");

        assert_eq!(entries.len(), 1);
        assert_eq!(content, "# Hello");
    }

    #[test]
    fn real_fs_reads_and_writes_temp_files() {
        let tmp = std::env::temp_dir().join("amux-test-real-fs");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("should create temp dir");

        let fs = RealFsBackend::new();
        let target = WorkspaceTarget::WindowsPath { path: tmp.clone() };

        // Write a file
        let file = MappedFile {
            display_path: "test.txt".into(),
            native_path: tmp.join("test.txt"),
        };
        fs.write_string(&file, "hello world").expect("should write");

        // Read it back
        let content = fs.read_to_string(&file).expect("should read");
        assert_eq!(content, "hello world");

        // List directory
        let entries = fs.read_dir(&target, "").expect("should list dir");
        assert!(entries.iter().any(|e| e.name == "test.txt" && !e.is_dir));

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn real_fs_reads_subdirectories() {
        let tmp = std::env::temp_dir().join("amux-test-real-fs-sub");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("src")).expect("should create subdir");
        std::fs::write(tmp.join("README.md"), "# Test").expect("should write");
        std::fs::write(tmp.join("src/main.rs"), "fn main() {}").expect("should write");

        let fs = RealFsBackend::new();
        let target = WorkspaceTarget::WindowsPath { path: tmp.clone() };

        // List root
        let entries = fs.read_dir(&target, "").expect("should list root");
        assert!(entries.iter().any(|e| e.name == "src" && e.is_dir));
        assert!(entries.iter().any(|e| e.name == "README.md" && !e.is_dir));

        // List subdirectory
        let sub_entries = fs.read_dir(&target, "src").expect("should list src");
        assert!(sub_entries.iter().any(|e| e.name == "main.rs"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
