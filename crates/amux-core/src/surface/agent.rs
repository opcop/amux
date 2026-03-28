use crate::{AgentInstanceId, AgentLaunchMode, SurfaceId, TerminalSessionId};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentSurfaceState {
    pub surface_id: SurfaceId,
    pub session_id: Option<TerminalSessionId>,
    pub agent_instance_id: Option<AgentInstanceId>,
    pub provider_id: String,
    pub launch_mode: AgentLaunchMode,
    pub cwd: Option<String>,
}
