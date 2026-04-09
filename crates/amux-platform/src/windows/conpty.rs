use amux_core::{ShellKind, TerminalSessionId, WorkspaceTarget};

use crate::{TerminalLaunchSpec, TerminalSessionKind, TerminalSessionMetadata};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConPtyCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ConPtyBackend;

impl ConPtyBackend {
    pub fn build_command(spec: &TerminalLaunchSpec) -> Result<ConPtyCommand, String> {
        match (&spec.target, &spec.shell) {
            (WorkspaceTarget::LocalPath { .. }, ShellKind::SystemDefault)
            | (WorkspaceTarget::WindowsPath { .. }, ShellKind::SystemDefault)
            | (WorkspaceTarget::LocalPath { .. }, ShellKind::PowerShell)
            | (WorkspaceTarget::WindowsPath { .. }, ShellKind::PowerShell) => Ok(ConPtyCommand {
                program: "powershell.exe".into(),
                args: vec!["-NoLogo".into()],
                cwd: spec.cwd.clone(),
            }),
            (WorkspaceTarget::LocalPath { .. }, ShellKind::Cmd)
            | (WorkspaceTarget::WindowsPath { .. }, ShellKind::Cmd) => Ok(ConPtyCommand {
                program: "cmd.exe".into(),
                args: Vec::new(),
                cwd: spec.cwd.clone(),
            }),
            (WorkspaceTarget::LocalPath { .. }, ShellKind::WslDefault)
            | (WorkspaceTarget::WindowsPath { .. }, ShellKind::WslDefault) => Err(
                "WSL shells should be launched through the WSL launcher, not ConPTY directly".into(),
            ),
            (WorkspaceTarget::LocalPath { .. }, ShellKind::WslDistro(_))
            | (WorkspaceTarget::WindowsPath { .. }, ShellKind::WslDistro(_)) => Err(
                "WSL distro shells should be launched through the WSL launcher".into(),
            ),
            (WorkspaceTarget::LocalPath { .. }, ShellKind::Bash)
            | (WorkspaceTarget::LocalPath { .. }, ShellKind::Zsh)
            | (WorkspaceTarget::LocalPath { .. }, ShellKind::Fish)
            | (WorkspaceTarget::LocalPath { .. }, ShellKind::Custom(_))
            | (WorkspaceTarget::WindowsPath { .. }, ShellKind::Bash)
            | (WorkspaceTarget::WindowsPath { .. }, ShellKind::Zsh)
            | (WorkspaceTarget::WindowsPath { .. }, ShellKind::Fish)
            | (WorkspaceTarget::WindowsPath { .. }, ShellKind::Custom(_)) => Err(
                "Unix shells are not supported by the Windows ConPTY launcher".into(),
            ),
            (WorkspaceTarget::WslPath { .. }, _) => Err(
                "WSL workspace targets should use the WSL launcher instead of ConPTY".into(),
            ),
        }
    }

    pub fn metadata_stub(
        id: TerminalSessionId,
        spec: &TerminalLaunchSpec,
    ) -> TerminalSessionMetadata {
        TerminalSessionMetadata {
            id,
            kind: TerminalSessionKind::WindowsConPty,
            target: spec.target.clone(),
            shell: spec.shell.clone(),
            cwd: spec.cwd.clone(),
            title: spec.title.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use amux_core::{ShellKind, WorkspaceTarget};

    use super::ConPtyBackend;
    use crate::TerminalLaunchSpec;

    #[test]
    fn builds_windows_powershell_command() {
        let command = ConPtyBackend::build_command(&TerminalLaunchSpec {
            target: WorkspaceTarget::WindowsPath {
                path: PathBuf::from("D:/repo/amux"),
            },
            shell: ShellKind::PowerShell,
            cwd: Some("D:/repo/amux".into()),
            env: BTreeMap::new(),
            title: None,
        })
        .expect("powershell command should be supported");

        assert_eq!(command.program, "powershell.exe");
        assert_eq!(command.cwd.as_deref(), Some("D:/repo/amux"));
    }
}
