use std::path::PathBuf;

use amux_core::{WorkspaceState, WorkspaceTarget};
use amux_platform::{FsBackend, PathMapper};

use crate::{FileFilter, FileTreeNode};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRef {
    pub name: String,
    pub target: WorkspaceTarget,
}

pub trait WorkspaceStore {
    fn list_recent(&self) -> Result<Vec<WorkspaceRef>, String>;
    fn open(&self, target: WorkspaceTarget) -> Result<WorkspaceState, String>;
    fn save_metadata(&self, workspace: &WorkspaceState) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenFile {
    pub display_path: String,
    pub relative_path: String,
    pub content: String,
}

pub struct WorkspaceService<P, F> {
    path_mapper: P,
    fs_backend: F,
}

impl<P, F> WorkspaceService<P, F> {
    pub fn new(path_mapper: P, fs_backend: F) -> Self {
        Self {
            path_mapper,
            fs_backend,
        }
    }
}

impl<P: PathMapper, F: FsBackend> WorkspaceService<P, F> {
    pub fn list_files(
        &self,
        target: &WorkspaceTarget,
        relative_path: &str,
        filter: &FileFilter,
    ) -> Result<Vec<FileTreeNode>, String> {
        let mut entries = self.fs_backend.read_dir(target, relative_path)?;
        entries.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
        });

        Ok(entries
            .into_iter()
            .filter(|entry| filter.matches(&entry.relative_path, &entry.name))
            .map(|entry| FileTreeNode::new(entry.name, entry.relative_path, entry.is_dir))
            .collect())
    }

    pub fn open_file(
        &self,
        target: &WorkspaceTarget,
        relative_path: &str,
    ) -> Result<OpenFile, String> {
        let mapped = self
            .path_mapper
            .map_file_for_editor(target, relative_path)?;
        let content = self.fs_backend.read_to_string(&mapped)?;
        Ok(OpenFile {
            display_path: mapped.display_path,
            relative_path: relative_path.to_string(),
            content,
        })
    }

    pub fn save_file(
        &self,
        target: &WorkspaceTarget,
        relative_path: &str,
        content: &str,
    ) -> Result<(), String> {
        let mapped = self
            .path_mapper
            .map_file_for_editor(target, relative_path)?;
        self.fs_backend.write_string(&mapped, content)
    }
}

pub fn derive_workspace_name(target: &WorkspaceTarget) -> String {
    match target {
        WorkspaceTarget::WindowsPath { path } => path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| path.display().to_string()),
        WorkspaceTarget::WslPath { path, .. } => PathBuf::from(path)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| path.clone()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use amux_core::WorkspaceTarget;
    use amux_platform::{DefaultPathMapper, FsEntry, InMemoryFsBackend, MappedFile};

    use crate::{derive_workspace_name, FileFilter, WorkspaceService};

    #[test]
    fn service_lists_and_opens_files() {
        let target = WorkspaceTarget::WindowsPath {
            path: PathBuf::from("D:/repo/amux"),
        };
        let fs = InMemoryFsBackend::default();
        fs.add_dir(
            &target,
            "",
            vec![
                FsEntry {
                    name: "src".into(),
                    relative_path: "src".into(),
                    is_dir: true,
                },
                FsEntry {
                    name: "README.md".into(),
                    relative_path: "README.md".into(),
                    is_dir: false,
                },
                FsEntry {
                    name: ".env".into(),
                    relative_path: ".env".into(),
                    is_dir: false,
                },
            ],
        )
        .expect("root dir should be added");
        fs.add_file(
            &MappedFile {
                display_path: "D:/repo/amux/README.md".into(),
                native_path: PathBuf::from("D:/repo/amux/README.md"),
            },
            "# AMUX",
        )
        .expect("file should be added");

        let service = WorkspaceService::new(DefaultPathMapper, fs);
        let files = service
            .list_files(
                &target,
                "",
                &FileFilter {
                    query: String::new(),
                    show_hidden: false,
                },
            )
            .expect("files should be listed");
        let opened = service
            .open_file(&target, "README.md")
            .expect("file should open");

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "src");
        assert_eq!(opened.content, "# AMUX");
    }

    #[test]
    fn derives_workspace_name_from_target() {
        assert_eq!(
            derive_workspace_name(&WorkspaceTarget::WindowsPath {
                path: PathBuf::from("D:/repo/amux")
            }),
            "amux"
        );
        assert_eq!(
            derive_workspace_name(&WorkspaceTarget::WslPath {
                distro: "Ubuntu".into(),
                path: "/home/user/demo".into()
            }),
            "demo"
        );
    }
}
