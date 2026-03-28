use crate::AgentListItem;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentLauncherPanel {
    agents: Vec<AgentListItem>,
}

impl AgentLauncherPanel {
    pub fn new(agents: Vec<AgentListItem>) -> Self {
        Self { agents }
    }

    pub fn render_text(&self) -> String {
        if self.agents.is_empty() {
            return "Agents\n  (none)".into();
        }

        let mut lines = vec!["Agents".to_string()];
        for agent in &self.agents {
            let support = if agent.supported { "ready" } else { "unsupported" };
            lines.push(format!("  - {} [{} | {}]", agent.name, agent.status, support));
        }
        lines.join("\n")
    }
}
