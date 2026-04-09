#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslDistribution {
    pub name: String,
}

use amux_core::{ShellKind, WorkspaceTarget};

use crate::TerminalLaunchSpec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslLaunchCommand {
    pub program: String,
    pub args: Vec<String>,
    pub distro: Option<String>,
}

impl WslLaunchCommand {
    /// Build command to launch a login shell in the distro
    pub fn login_shell(distro: &str, path: &str) -> Self {
        Self {
            program: "wsl.exe".into(),
            args: vec![
                "-d".into(),
                distro.into(),
                "--cd".into(),
                path.into(),
                "--".into(),
                "-l".into(), // Login shell
            ],
            distro: Some(distro.into()),
        }
    }

    /// Build command to run a specific command in the distro
    pub fn exec(distro: &str, path: &str, command: &[&str]) -> Self {
        let mut args = vec![
            "-d".into(),
            distro.into(),
            "--cd".into(),
            path.into(),
            "--".into(),
        ];
        args.extend(command.iter().map(|s| s.to_string()));

        Self {
            program: "wsl.exe".into(),
            args,
            distro: Some(distro.into()),
        }
    }

    /// Build command to start a systemd session (for distros with systemd)
    pub fn systemd_shell(distro: &str, path: &str) -> Self {
        Self {
            program: "wsl.exe".into(),
            args: vec![
                "-d".into(),
                distro.into(),
                "--cd".into(),
                path.into(),
                "--".into(),
                "bash".into(),
                "-c".into(),
                "exec systemd-run --user /bin/bash -l".into(),
            ],
            distro: Some(distro.into()),
        }
    }
}

pub fn build_wsl_command(spec: &TerminalLaunchSpec) -> Result<WslLaunchCommand, String> {
    let (distro, target_path) = match (&spec.target, &spec.shell) {
        (WorkspaceTarget::WslPath { distro, path }, ShellKind::WslDefault) => {
            (Some(distro.as_str()), spec.cwd.as_deref().unwrap_or(path.as_str()))
        }
        (WorkspaceTarget::WslPath { distro: _, path }, ShellKind::WslDistro(selected)) => {
            (Some(selected.as_str()), spec.cwd.as_deref().unwrap_or(path.as_str()))
        }
        (WorkspaceTarget::LocalPath { .. }, ShellKind::WslDefault)
        | (WorkspaceTarget::WindowsPath { .. }, ShellKind::WslDefault) => {
            (None, spec.cwd.as_deref().unwrap_or("/"))
        }
        (WorkspaceTarget::LocalPath { .. }, ShellKind::WslDistro(selected))
        | (WorkspaceTarget::WindowsPath { .. }, ShellKind::WslDistro(selected)) => {
            (Some(selected.as_str()), spec.cwd.as_deref().unwrap_or("/"))
        }
        _ => return Err("spec is not a WSL launch target".into()),
    };

    let shell_program = match &spec.shell {
        ShellKind::WslDefault | ShellKind::WslDistro(_) => "bash",
        _ => return Err("spec is not a WSL shell".into()),
    };

    let mut args = Vec::new();
    if let Some(distro) = distro {
        args.push("-d".into());
        args.push(distro.into());
    }
    args.push("--cd".into());
    args.push(target_path.into());
    args.push("--".into());
    args.push(shell_program.into());

    Ok(WslLaunchCommand {
        program: "wsl.exe".into(),
        args,
        distro: distro.map(String::from),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use amux_core::{ShellKind, WorkspaceTarget};

    use super::build_wsl_command;
    use crate::TerminalLaunchSpec;

    #[test]
    fn builds_wsl_distro_command_for_wsl_workspace() {
        let command = build_wsl_command(&TerminalLaunchSpec {
            target: WorkspaceTarget::WslPath {
                distro: "Ubuntu".into(),
                path: "/home/user/amux".into(),
            },
            shell: ShellKind::WslDistro("Ubuntu".into()),
            cwd: Some("/home/user/amux".into()),
            env: BTreeMap::new(),
            title: None,
        })
        .expect("wsl command should be built");

        assert_eq!(command.program, "wsl.exe");
        assert_eq!(
            command.args,
            vec!["-d", "Ubuntu", "--cd", "/home/user/amux", "--", "bash"]
        );
    }
}
