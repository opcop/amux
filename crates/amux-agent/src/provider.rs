use amux_core::WorkspaceTarget;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionTarget {
    WindowsLocal,
    Wsl,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectionRule {
    pub program: String,
    pub version_args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentProvider {
    pub id: String,
    pub display_name: String,
    pub command: String,
    pub args_template: Vec<String>,
    pub detection: DetectionRule,
    pub supported_targets: Vec<ExecutionTarget>,
}

impl AgentProvider {
    pub fn supports_workspace(&self, target: &WorkspaceTarget) -> bool {
        let execution_target = match target {
            WorkspaceTarget::WindowsPath { .. } => ExecutionTarget::WindowsLocal,
            WorkspaceTarget::WslPath { .. } => ExecutionTarget::Wsl,
        };
        self.supported_targets.contains(&execution_target)
    }
}
