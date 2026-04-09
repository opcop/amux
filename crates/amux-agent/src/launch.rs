use std::collections::BTreeMap;

use amux_core::{
    AgentLaunchMode, ShellKind, TerminalLaunchProfile, TerminalSessionId, WorkspaceTarget,
};
use amux_platform::TerminalBackend;

use crate::{AgentProvider, ExecutionTarget};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentLaunchRequest {
    pub provider_id: String,
    pub mode: AgentLaunchMode,
    pub target: WorkspaceTarget,
    pub cwd: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentLaunchPlan {
    pub provider_id: String,
    pub command: String,
    pub shell: ShellKind,
    pub cwd: Option<String>,
    pub bootstrap_input: Vec<u8>,
    pub terminal: TerminalLaunchProfile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentLaunchResult {
    pub session_id: TerminalSessionId,
    pub bootstrap_written: bool,
}

pub struct AgentLauncher<B> {
    terminal_backend: B,
}

impl<B> AgentLauncher<B> {
    pub fn new(terminal_backend: B) -> Self {
        Self { terminal_backend }
    }
}

impl<B: TerminalBackend> AgentLauncher<B> {
    pub fn plan(
        &self,
        provider: &AgentProvider,
        request: AgentLaunchRequest,
    ) -> Result<AgentLaunchPlan, String> {
        if !provider.supports_workspace(&request.target) {
            return Err(format!(
                "provider {} does not support target {:?}",
                provider.id, request.target
            ));
        }

        let shell = default_shell_for_target(&request.target);
        let bootstrap_command = bootstrap_command(provider);
        let bootstrap_input = format!("{bootstrap_command}\n").into_bytes();

        Ok(AgentLaunchPlan {
            provider_id: provider.id.clone(),
            command: bootstrap_command,
            shell: shell.clone(),
            cwd: request.cwd.clone(),
            bootstrap_input: bootstrap_input.clone(),
            terminal: TerminalLaunchProfile {
                target: request.target,
                shell,
                cwd: request.cwd,
                env: BTreeMap::new(),
                title: Some(format!("Agent: {}", provider.display_name)),
            },
        })
    }

    pub fn launch(&self, plan: AgentLaunchPlan) -> Result<AgentLaunchResult, String> {
        let session_id = self.terminal_backend.create_session(plan.terminal)?;
        self.terminal_backend
            .write_input(&session_id, &plan.bootstrap_input)?;
        Ok(AgentLaunchResult {
            session_id,
            bootstrap_written: true,
        })
    }
}

fn default_shell_for_target(target: &WorkspaceTarget) -> ShellKind {
    match target {
        WorkspaceTarget::LocalPath { .. } => ShellKind::SystemDefault,
        WorkspaceTarget::WindowsPath { .. } => ShellKind::PowerShell,
        WorkspaceTarget::WslPath { distro, .. } => ShellKind::WslDistro(distro.clone()),
    }
}

fn bootstrap_command(provider: &AgentProvider) -> String {
    let args = provider.args_template.join(" ");
    if args.is_empty() {
        provider.command.clone()
    } else {
        format!("{} {}", provider.command, args)
    }
}

pub fn execution_target_for_workspace(target: &WorkspaceTarget) -> ExecutionTarget {
    match target {
        WorkspaceTarget::LocalPath { .. } => ExecutionTarget::Local,
        WorkspaceTarget::WindowsPath { .. } => ExecutionTarget::WindowsLocal,
        WorkspaceTarget::WslPath { .. } => ExecutionTarget::Wsl,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use amux_core::{AgentLaunchMode, ShellKind, WorkspaceTarget};
    use amux_platform::InMemoryTerminalBackend;

    use crate::{AgentProvider, DetectionRule, ExecutionTarget};

    use super::{AgentLaunchRequest, AgentLauncher};

    #[test]
    fn launcher_plans_and_bootstraps_windows_agent_session() {
        let provider = AgentProvider {
            id: "codex".into(),
            display_name: "Codex".into(),
            command: "codex".into(),
            args_template: vec!["--approval-mode".into(), "never".into()],
            detection: DetectionRule {
                program: "codex".into(),
                version_args: vec!["--version".into()],
            },
            supported_targets: vec![
                ExecutionTarget::Local,
                ExecutionTarget::WindowsLocal,
                ExecutionTarget::Wsl,
            ],
        };
        let backend = InMemoryTerminalBackend::default();
        let launcher = AgentLauncher::new(backend.clone());

        let plan = launcher
            .plan(
                &provider,
                AgentLaunchRequest {
                    provider_id: "codex".into(),
                    mode: AgentLaunchMode::AttachedTerminal,
                    target: WorkspaceTarget::WindowsPath {
                        path: PathBuf::from("D:/repo/amux"),
                    },
                    cwd: Some("D:/repo/amux".into()),
                },
            )
            .expect("plan should succeed");

        let result = launcher.launch(plan).expect("launch should succeed");
        let record = backend
            .records()
            .into_iter()
            .find(|record| record.metadata.id == result.session_id)
            .expect("record should exist");

        assert_eq!(record.metadata.title.as_deref(), Some("Agent: Codex"));
        assert_eq!(record.writes[0], b"codex --approval-mode never\n");
    }

    #[test]
    fn launcher_uses_system_shell_for_local_workspace() {
        let provider = AgentProvider {
            id: "codex".into(),
            display_name: "Codex".into(),
            command: "codex".into(),
            args_template: vec!["--approval-mode".into(), "never".into()],
            detection: DetectionRule {
                program: "codex".into(),
                version_args: vec!["--version".into()],
            },
            supported_targets: vec![ExecutionTarget::Local, ExecutionTarget::Wsl],
        };
        let backend = InMemoryTerminalBackend::default();
        let launcher = AgentLauncher::new(backend);

        let plan = launcher
            .plan(
                &provider,
                AgentLaunchRequest {
                    provider_id: "codex".into(),
                    mode: AgentLaunchMode::AttachedTerminal,
                    target: WorkspaceTarget::LocalPath {
                        path: PathBuf::from("/Users/arden/amux"),
                    },
                    cwd: Some("/Users/arden/amux".into()),
                },
            )
            .expect("plan should succeed");

        assert_eq!(plan.shell, ShellKind::SystemDefault);
    }
}
