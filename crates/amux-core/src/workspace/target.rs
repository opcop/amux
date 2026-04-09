use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub enum WorkspaceTarget {
    LocalPath { path: PathBuf },
    WindowsPath { path: PathBuf },
    WslPath { distro: String, path: String },
}
