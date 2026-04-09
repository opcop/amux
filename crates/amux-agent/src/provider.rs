use amux_core::WorkspaceTarget;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionTarget {
    Local,
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
        match target {
            WorkspaceTarget::LocalPath { .. } => {
                self.supported_targets.contains(&ExecutionTarget::Local)
            }
            WorkspaceTarget::WindowsPath { .. } => {
                self.supported_targets.contains(&ExecutionTarget::WindowsLocal)
                    || self.supported_targets.contains(&ExecutionTarget::Local)
            }
            WorkspaceTarget::WslPath { .. } => self.supported_targets.contains(&ExecutionTarget::Wsl),
        }
    }
}
