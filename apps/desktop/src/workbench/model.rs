use serde::{Deserialize, Serialize};

pub(crate) const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MissionStatus {
    Planning,
    Active,
    Validating,
    Complete,
    Archived,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GoalStatus {
    Todo,
    InProgress,
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TaskStatus {
    Todo,
    Assigned,
    InProgress,
    Review,
    Done,
    Blocked,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Mission {
    pub(crate) schema_version: u32,
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) status: MissionStatus,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Goal {
    pub(crate) schema_version: u32,
    pub(crate) id: String,
    pub(crate) mission_id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) acceptance: String,
    pub(crate) priority: u32,
    pub(crate) status: GoalStatus,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Proof {
    pub(crate) notes: String,
    pub(crate) tests: Option<String>,
    pub(crate) files_changed: Vec<String>,
    pub(crate) commands_run: Vec<String>,
    pub(crate) errors: Vec<String>,
    pub(crate) pr: Option<String>,
    pub(crate) ci: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Task {
    pub(crate) schema_version: u32,
    pub(crate) id: String,
    pub(crate) mission_id: String,
    pub(crate) goal_id: Option<String>,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) status: TaskStatus,
    pub(crate) assignee_pane_id: Option<String>,
    pub(crate) assignee_agent_kind: Option<String>,
    pub(crate) assigned_at: Option<String>,
    pub(crate) last_activity_at: Option<String>,
    pub(crate) priority: u32,
    pub(crate) depends_on: Vec<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) specialty: Option<String>,
    pub(crate) proof: Option<Proof>,
    pub(crate) blocked_reason: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EventKind {
    MissionCreated,
    MissionCompleted,
    GoalCreated,
    GoalCompleted,
    TaskCreated,
    TaskAssigned,
    TaskStarted,
    TaskCompleted,
    TaskFailed,
    TaskBlocked,
    AgentMessageSent,
    ProofRecorded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkbenchEvent {
    pub(crate) schema_version: u32,
    pub(crate) id: String,
    pub(crate) kind: EventKind,
    pub(crate) message: String,
    pub(crate) task_id: Option<String>,
    pub(crate) pane_id: Option<String>,
    pub(crate) created_at: String,
}

impl Proof {
    pub(crate) fn from_notes(notes: impl Into<String>) -> Self {
        Self {
            notes: notes.into(),
            ..Self::default()
        }
    }

    pub(crate) fn from_text_or_json(value: impl Into<String>) -> Self {
        let raw = value.into();
        serde_json::from_str::<Self>(&raw).unwrap_or_else(|_| Self::from_notes(raw))
    }
}

impl TaskStatus {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Assigned => "assigned",
            Self::InProgress => "in progress",
            Self::Review => "review",
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_accepts_json_or_free_text() {
        let structured = Proof::from_text_or_json(
            r#"{"notes":"done","tests":"cargo test","files_changed":["store.rs"]}"#,
        );
        assert_eq!(structured.notes, "done");
        assert_eq!(structured.tests.as_deref(), Some("cargo test"));
        assert_eq!(structured.files_changed, vec!["store.rs"]);

        let free_text = Proof::from_text_or_json("manual proof");
        assert_eq!(free_text.notes, "manual proof");
        assert!(free_text.files_changed.is_empty());
    }
}
