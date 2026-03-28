use std::path::PathBuf;

use amux_core::WorkspaceTarget;

use crate::windows::paths::wsl_unc_path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedFile {
    pub display_path: String,
    pub native_path: PathBuf,
}

pub trait PathMapper {
    fn to_display_path(&self, target: &WorkspaceTarget) -> String;
    fn to_runtime_cwd(&self, target: &WorkspaceTarget) -> Result<String, String>;
    fn map_file_for_editor(
        &self,
        workspace: &WorkspaceTarget,
        relative_path: &str,
    ) -> Result<MappedFile, String>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DefaultPathMapper;

impl PathMapper for DefaultPathMapper {
    fn to_display_path(&self, target: &WorkspaceTarget) -> String {
        match target {
            WorkspaceTarget::WindowsPath { path } => path.display().to_string(),
            WorkspaceTarget::WslPath { distro, path } => format!("{distro}:{path}"),
        }
    }

    fn to_runtime_cwd(&self, target: &WorkspaceTarget) -> Result<String, String> {
        match target {
            WorkspaceTarget::WindowsPath { path } => Ok(path.display().to_string()),
            WorkspaceTarget::WslPath { path, .. } => Ok(path.clone()),
        }
    }

    fn map_file_for_editor(
        &self,
        workspace: &WorkspaceTarget,
        relative_path: &str,
    ) -> Result<MappedFile, String> {
        match workspace {
            WorkspaceTarget::WindowsPath { path } => Ok(MappedFile {
                display_path: path.join(relative_path).display().to_string(),
                native_path: path.join(relative_path),
            }),
            WorkspaceTarget::WslPath { distro, path } => {
                let unix_path = join_unix_path(path, relative_path);
                Ok(MappedFile {
                    display_path: format!("{distro}:{unix_path}"),
                    native_path: PathBuf::from(wsl_unc_path(distro, &unix_path)),
                })
            }
        }
    }
}

fn join_unix_path(base: &str, relative_path: &str) -> String {
    let base = base.trim_end_matches('/');
    let relative = relative_path.trim_start_matches('/');
    format!("{base}/{relative}")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use amux_core::WorkspaceTarget;

    use super::{DefaultPathMapper, PathMapper};

    #[test]
    fn maps_wsl_file_to_unc_path() {
        let mapper = DefaultPathMapper;
        let mapped = mapper
            .map_file_for_editor(
                &WorkspaceTarget::WslPath {
                    distro: "Ubuntu".into(),
                    path: "/home/user/amux".into(),
                },
                "README.md",
            )
            .expect("mapping should succeed");

        assert_eq!(mapped.display_path, "Ubuntu:/home/user/amux/README.md");
        assert_eq!(
            mapped.native_path,
            PathBuf::from(r"\\wsl$\Ubuntu\home\user\amux\README.md")
        );
    }
}
