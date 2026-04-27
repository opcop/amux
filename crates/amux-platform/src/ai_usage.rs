//! AI usage tracking — aggregates token consumption and session
//! stats from the JSONL agent monitor.
//!
//! Reads are pure in-process lookups (no network requests). The
//! agent_monitor already tracks per-session token counts from
//! Claude Code JSONL files; this module provides summary views.

use crate::agent_monitor::AgentSessionState;

/// Aggregated usage across all Claude Code sessions.
#[derive(Clone, Debug, Default)]
pub struct AiUsageSummary {
    /// Total tokens across all sessions
    pub total_tokens: u64,
    /// Total tool uses
    pub total_tool_uses: usize,
    /// Active sub-agents across all sessions
    pub active_subagents: usize,
    /// Sessions tracked
    pub session_count: usize,
    /// Per-session breakdowns
    pub sessions: Vec<SessionUsage>,
}

#[derive(Clone, Debug)]
pub struct SessionUsage {
    pub pane_id: String,
    pub model: String,
    pub tokens: u64,
    pub context_pct: u8,
    pub tool_uses: usize,
    pub subagents: usize,
    pub current_tool: Option<String>,
}

impl AiUsageSummary {
    /// Build a summary from a set of session states.
    pub fn from_sessions(sessions: &[(String, &AgentSessionState)]) -> Self {
        let mut total_tokens = 0u64;
        let mut total_tool_uses = 0usize;
        let mut active_subagents = 0usize;

        let session_breakdowns: Vec<SessionUsage> = sessions
            .iter()
            .filter(|(_, s)| s.total_tokens() > 0) // only active sessions
            .map(|(pane_id, s)| {
                total_tokens += s.total_tokens();
                total_tool_uses += s.tool_use_count;
                active_subagents += s.subagent_count;
                SessionUsage {
                    pane_id: pane_id.clone(),
                    model: s.short_model().unwrap_or("?").to_string(),
                    tokens: s.total_tokens(),
                    context_pct: s.context_usage_pct(),
                    tool_uses: s.tool_use_count,
                    subagents: s.subagent_count,
                    current_tool: s.tool_label(),
                }
            })
            .collect();

        AiUsageSummary {
            total_tokens,
            total_tool_uses,
            active_subagents,
            session_count: session_breakdowns.len(),
            sessions: session_breakdowns,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.session_count == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_monitor::{AgentSessionState, AgentTodoItem};

    #[test]
    fn empty_sessions_produces_empty_summary() {
        let summary = AiUsageSummary::from_sessions(&[]);
        assert!(summary.is_empty());
    }

    #[test]
    fn sessions_with_no_tokens_are_filtered() {
        let state = AgentSessionState::default();
        let summary = AiUsageSummary::from_sessions(&[("pane-1".into(), &state)]);
        assert!(summary.is_empty());
    }

    #[test]
    fn active_sessions_are_summarized() {
        let mut state = AgentSessionState::default();
        state.input_tokens = 50_000;
        state.output_tokens = 10_000;
        state.model = Some("claude-opus-4-6".into());
        state.tool_use_count = 42;
        state.context_tokens = 600_000;

        let summary = AiUsageSummary::from_sessions(&[("pane-1".into(), &state)]);
        assert_eq!(summary.session_count, 1);
        assert_eq!(summary.total_tokens, 60_000);
        assert_eq!(summary.total_tool_uses, 42);
        assert_eq!(summary.sessions[0].context_pct, 60); // 600K / 1M
    }
}
