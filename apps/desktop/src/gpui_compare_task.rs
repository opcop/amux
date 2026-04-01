//! Compare Task — MVP for multi-agent comparison
//!
//! User enters a requirement, selects 2 agents, and Amux:
//! 1. Creates a side-by-side layout with one agent per pane
//! 2. Sends the same requirement to both agents
//! 3. Shows progress in a Task Bar
//! 4. User evaluates results and picks a winner

use amux_platform::terminal::manager::{AgentStatus, PaneId};

/// State of a compare task
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct CompareTask {
    /// User's requirement text
    pub prompt: String,
    /// Agent entries: (tool_id, display_name, pane_id, status)
    pub agents: Vec<CompareAgent>,
    /// Current phase
    pub phase: ComparePhase,
    /// Whether the task bar is expanded
    pub expanded: bool,
    /// Frame when the task was created (for elapsed time)
    pub created_frame: u32,
}

#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct CompareAgent {
    pub tool_id: String,
    pub display_name: String,
    pub pane_id: Option<PaneId>,
    pub status: CompareAgentStatus,
}

#[cfg(feature = "gpui")]
#[derive(Clone, Debug, PartialEq)]
pub enum CompareAgentStatus {
    /// Waiting to start (agent not yet launched)
    Pending,
    /// Agent is running (thinking/working)
    Running,
    /// Agent is waiting for user input
    Waiting,
    /// Agent finished
    Done,
    /// Agent errored
    Error,
}

#[cfg(feature = "gpui")]
impl CompareAgentStatus {
    pub fn from_agent_status(s: &AgentStatus) -> Self {
        match s {
            AgentStatus::Thinking => Self::Running,
            AgentStatus::Waiting => Self::Waiting,
            AgentStatus::Done => Self::Done,
            AgentStatus::Error => Self::Error,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Pending => "⏳",
            Self::Running => "⟳",
            Self::Waiting => "●",
            Self::Done => "✓",
            Self::Error => "✗",
        }
    }

    pub fn color(&self) -> u32 {
        match self {
            Self::Pending => 0x585b70,
            Self::Running => 0x89b4fa,
            Self::Waiting => 0xf9e2af,
            Self::Done => 0xa6e3a1,
            Self::Error => 0xf38ba8,
        }
    }
}

#[cfg(feature = "gpui")]
#[derive(Clone, Debug, PartialEq)]
pub enum ComparePhase {
    /// Picking agents (dialog open)
    Setup,
    /// Agents are running
    Running,
    /// All agents finished — user is reviewing
    Review,
    /// User dismissed / applied
    Dismissed,
}

/// State for the compare setup dialog
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct CompareSetupState {
    /// Requirement text input
    pub prompt: String,
    /// Available agents: (tool_id, display_name, selected)
    pub agents: Vec<(String, String, bool)>,
}

#[cfg(feature = "gpui")]
impl CompareSetupState {
    pub fn new(available_tools: &[(&str, &str, &str)], wsl_detected: bool) -> Self {
        let mut agents: Vec<(String, String, bool)> = Vec::new();
        // Only include actual AI agents (not WSL shell)
        for &(tool_id, label, _) in available_tools {
            // Pre-select first two agents
            let selected = agents.len() < 2;
            agents.push((tool_id.into(), label.into(), selected));
        }
        Self {
            prompt: String::new(),
            agents,
        }
    }

    pub fn selected_count(&self) -> usize {
        self.agents.iter().filter(|(_, _, s)| *s).count()
    }

    pub fn toggle_agent(&mut self, index: usize) {
        if let Some((_, _, selected)) = self.agents.get_mut(index) {
            *selected = !*selected;
        }
    }

    pub fn can_start(&self) -> bool {
        !self.prompt.trim().is_empty() && self.selected_count() >= 2
    }
}
