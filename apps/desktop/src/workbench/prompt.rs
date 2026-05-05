use std::path::Path;

use super::model::{Goal, Mission, Task};

const MAX_DISPATCH_PROMPT_CHARS: usize = 8000;

pub(crate) struct DispatchContext<'a> {
    pub(crate) mission: Option<&'a Mission>,
    pub(crate) goal: Option<&'a Goal>,
    pub(crate) task: &'a Task,
    pub(crate) project_guidelines: Option<String>,
    pub(crate) recent_done: Vec<&'a Task>,
}

pub(crate) fn build_dispatch_prompt(ctx: DispatchContext<'_>) -> String {
    let mut out = String::new();
    out.push_str("You are working inside Amux Agent Workbench.\n\n");

    if let Some(mission) = ctx.mission {
        out.push_str("## Mission\n");
        out.push_str(&mission.title);
        out.push('\n');
        if !mission.description.trim().is_empty() {
            out.push_str(&mission.description);
            out.push('\n');
        }
        out.push('\n');
    }

    if let Some(goal) = ctx.goal {
        out.push_str("## Goal\n");
        out.push_str(&goal.title);
        out.push('\n');
        if !goal.acceptance.trim().is_empty() {
            out.push_str("Acceptance: ");
            out.push_str(&goal.acceptance);
            out.push('\n');
        }
        out.push('\n');
    }

    out.push_str("## Task\n");
    out.push_str(&ctx.task.title);
    out.push('\n');
    if !ctx.task.description.trim().is_empty() {
        out.push_str(&ctx.task.description);
        out.push('\n');
    }
    out.push_str(&format!("Priority: P{}\n", ctx.task.priority));
    if !ctx.task.tags.is_empty() {
        out.push_str(&format!("Tags: {}\n", ctx.task.tags.join(", ")));
    }
    if !ctx.task.depends_on.is_empty() {
        out.push_str(&format!("Depends on: {}\n", ctx.task.depends_on.join(", ")));
    }
    out.push('\n');

    if let Some(guidelines) = ctx.project_guidelines.filter(|s| !s.trim().is_empty()) {
        out.push_str("## Project Guidelines\n");
        out.push_str(guidelines.trim());
        out.push_str("\n\n");
    }

    if !ctx.recent_done.is_empty() {
        out.push_str("## Recent Completed Tasks\n");
        for task in ctx.recent_done {
            out.push_str(&format!("- {}: {}", task.id, task.title));
            if let Some(proof) = &task.proof {
                if !proof.notes.trim().is_empty() {
                    out.push_str(&format!(" — {}", proof.notes.trim()));
                }
            }
            out.push('\n');
        }
        out.push('\n');
    }

    out.push_str(
        "## Available Amux Commands\n\
         - amux pane list\n\
         - amux pane read <pane-id> --lines 40\n\
         - amux pane message <pane-id> \"message\"\n\
         - amux task done <task-id> --proof \"what changed, tests run, files touched\"\n\
         - amux task block <task-id> --reason \"why blocked\"\n\n",
    );
    let completion = format!(
        "## Completion Protocol\n\
         When done, run:\n\
         amux task done {} --proof \"what changed, tests run, files touched\"\n",
        ctx.task.id
    );
    finish_prompt(out, completion)
}

pub(crate) fn load_project_guidelines(workspace_root: Option<&str>) -> Option<String> {
    let root = Path::new(workspace_root?);
    for name in ["AGENTS.md", "CLAUDE.md"] {
        let path = root.join(name);
        if let Ok(content) = std::fs::read_to_string(path) {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                continue;
            }
            return Some(truncate_chars(trimmed, 1200));
        }
    }
    None
}

fn truncate_chars(value: &str, max: usize) -> String {
    let mut out: String = value.chars().take(max).collect();
    if value.chars().count() > max {
        out.push_str("\n... (truncated)");
    }
    out
}

fn finish_prompt(mut body: String, completion: String) -> String {
    let body_len = body.chars().count();
    let completion_len = completion.chars().count();
    if body_len + completion_len <= MAX_DISPATCH_PROMPT_CHARS {
        body.push_str(&completion);
        return body;
    }

    let max_body = MAX_DISPATCH_PROMPT_CHARS.saturating_sub(completion_len + 32);
    let mut out = truncate_chars(&body, max_body);
    out.push_str("\n\n");
    out.push_str(&completion);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::model::{SCHEMA_VERSION, TaskStatus};

    #[test]
    fn prompt_includes_completion_protocol() {
        let task = Task {
            schema_version: SCHEMA_VERSION,
            id: "001".to_string(),
            mission_id: "mission".to_string(),
            goal_id: None,
            title: "Add store".to_string(),
            description: String::new(),
            status: TaskStatus::Todo,
            assignee_pane_id: None,
            assignee_agent_kind: None,
            assigned_at: None,
            last_activity_at: None,
            priority: 1,
            depends_on: vec![],
            tags: vec!["rust".to_string()],
            specialty: None,
            proof: None,
            blocked_reason: None,
            created_at: "1".to_string(),
            updated_at: "1".to_string(),
        };

        let prompt = build_dispatch_prompt(DispatchContext {
            mission: None,
            goal: None,
            task: &task,
            project_guidelines: None,
            recent_done: vec![],
        });

        assert!(prompt.contains("amux task done 001"));
        assert!(prompt.contains("Add store"));
    }

    #[test]
    fn prompt_keeps_completion_protocol_when_truncated() {
        let task = Task {
            schema_version: SCHEMA_VERSION,
            id: "999".to_string(),
            mission_id: "mission".to_string(),
            goal_id: None,
            title: "Long task".to_string(),
            description: "x".repeat(20_000),
            status: TaskStatus::Todo,
            assignee_pane_id: None,
            assignee_agent_kind: None,
            assigned_at: None,
            last_activity_at: None,
            priority: 1,
            depends_on: vec![],
            tags: vec![],
            specialty: None,
            proof: None,
            blocked_reason: None,
            created_at: "1".to_string(),
            updated_at: "1".to_string(),
        };
        let prompt = build_dispatch_prompt(DispatchContext {
            mission: None,
            goal: None,
            task: &task,
            project_guidelines: None,
            recent_done: vec![],
        });

        assert!(prompt.chars().count() <= MAX_DISPATCH_PROMPT_CHARS);
        assert!(prompt.contains("amux task done 999"));
    }
}
