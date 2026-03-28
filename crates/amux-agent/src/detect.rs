#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentStatus {
    Installed,
    NotFound,
    Broken(String),
    NeedsAuth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentDetection {
    pub provider_id: String,
    pub status: AgentStatus,
}

impl AgentDetection {
    pub fn is_available(&self) -> bool {
        matches!(self.status, AgentStatus::Installed)
    }
}
