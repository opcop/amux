use amux_core::WorkspaceTarget;

use crate::{AgentDetection, AgentProvider, AgentStatus, ExecutionTarget};

pub trait AgentRegistry {
    fn list(&self) -> Vec<AgentProvider>;
    fn detect_all(&self) -> Vec<AgentDetection>;
}

#[derive(Clone, Debug, Default)]
pub struct StaticAgentRegistry {
    providers: Vec<AgentProvider>,
    detections: Vec<AgentDetection>,
}

impl StaticAgentRegistry {
    pub fn with_defaults() -> Self {
        let providers = default_providers();
        let detections = providers
            .iter()
            .map(|provider| AgentDetection {
                provider_id: provider.id.clone(),
                status: AgentStatus::NotFound,
            })
            .collect();
        Self {
            providers,
            detections,
        }
    }

    pub fn set_detection(
        &mut self,
        provider_id: impl Into<String>,
        status: AgentStatus,
    ) -> &mut Self {
        let provider_id = provider_id.into();
        if let Some(detection) = self
            .detections
            .iter_mut()
            .find(|detection| detection.provider_id == provider_id)
        {
            detection.status = status;
        } else {
            self.detections.push(AgentDetection {
                provider_id,
                status,
            });
        }
        self
    }

    pub fn available_for_workspace(&self, target: &WorkspaceTarget) -> Vec<AgentProvider> {
        self.providers
            .iter()
            .filter(|provider| provider.supports_workspace(target))
            .cloned()
            .collect()
    }

    pub fn provider(&self, id: &str) -> Option<AgentProvider> {
        self.providers.iter().find(|provider| provider.id == id).cloned()
    }
}

impl AgentRegistry for StaticAgentRegistry {
    fn list(&self) -> Vec<AgentProvider> {
        self.providers.clone()
    }

    fn detect_all(&self) -> Vec<AgentDetection> {
        self.detections.clone()
    }
}

fn default_providers() -> Vec<AgentProvider> {
    vec![
        AgentProvider {
            id: "codex".into(),
            display_name: "Codex".into(),
            command: "codex".into(),
            args_template: Vec::new(),
            detection: crate::DetectionRule {
                program: "codex".into(),
                version_args: vec!["--version".into()],
            },
            supported_targets: vec![
                ExecutionTarget::Local,
                ExecutionTarget::WindowsLocal,
                ExecutionTarget::Wsl,
            ],
        },
        AgentProvider {
            id: "claude".into(),
            display_name: "Claude Code".into(),
            command: "claude".into(),
            args_template: Vec::new(),
            detection: crate::DetectionRule {
                program: "claude".into(),
                version_args: vec!["--version".into()],
            },
            supported_targets: vec![
                ExecutionTarget::Local,
                ExecutionTarget::WindowsLocal,
                ExecutionTarget::Wsl,
            ],
        },
        AgentProvider {
            id: "opencode".into(),
            display_name: "OpenCode".into(),
            command: "opencode".into(),
            args_template: Vec::new(),
            detection: crate::DetectionRule {
                program: "opencode".into(),
                version_args: vec!["--version".into()],
            },
            supported_targets: vec![
                ExecutionTarget::Local,
                ExecutionTarget::WindowsLocal,
                ExecutionTarget::Wsl,
            ],
        },
        AgentProvider {
            id: "aider".into(),
            display_name: "Aider".into(),
            command: "aider".into(),
            args_template: Vec::new(),
            detection: crate::DetectionRule {
                program: "aider".into(),
                version_args: vec!["--version".into()],
            },
            supported_targets: vec![
                ExecutionTarget::Local,
                ExecutionTarget::WindowsLocal,
                ExecutionTarget::Wsl,
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use amux_core::WorkspaceTarget;

    use crate::{AgentRegistry, AgentStatus, StaticAgentRegistry};

    #[test]
    fn registry_filters_providers_for_workspace() {
        let registry = StaticAgentRegistry::with_defaults();
        let providers = registry.available_for_workspace(&WorkspaceTarget::WindowsPath {
            path: PathBuf::from("D:/repo/amux"),
        });

        assert!(providers.iter().any(|provider| provider.id == "codex"));
        assert!(providers.iter().any(|provider| provider.id == "claude"));
    }

    #[test]
    fn registry_filters_providers_for_local_workspace() {
        let registry = StaticAgentRegistry::with_defaults();
        let providers = registry.available_for_workspace(&WorkspaceTarget::LocalPath {
            path: PathBuf::from("/Users/arden/amux"),
        });

        assert!(providers.iter().any(|provider| provider.id == "codex"));
        assert!(providers.iter().any(|provider| provider.id == "claude"));
    }

    #[test]
    fn registry_updates_detection_state() {
        let mut registry = StaticAgentRegistry::with_defaults();
        registry.set_detection("codex", AgentStatus::Installed);

        let codex = registry
            .detect_all()
            .into_iter()
            .find(|detection| detection.provider_id == "codex")
            .expect("codex detection should exist");

        assert!(codex.is_available());
    }
}
