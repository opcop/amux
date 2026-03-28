use std::collections::BTreeMap;

use crate::{SurfaceId, TerminalSessionId, WorkspaceTarget};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ShellKind {
    PowerShell,
    Cmd,
    WslDefault,
    WslDistro(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TerminalLaunchProfile {
    pub target: WorkspaceTarget,
    pub shell: ShellKind,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
    pub title: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TerminalSurfaceState {
    pub surface_id: SurfaceId,
    pub session_id: Option<TerminalSessionId>,
    pub launch_profile: TerminalLaunchProfile,
    pub cwd: Option<String>,
    pub title_override: Option<String>,
}
