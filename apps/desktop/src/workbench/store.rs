use std::path::{Path, PathBuf};

use super::events::{append_event, new_event, now_string};
use super::model::{
    EventKind, Goal, GoalStatus, Mission, MissionStatus, Proof, SCHEMA_VERSION, Task, TaskStatus,
    WorkbenchEvent,
};

#[derive(Clone, Debug)]
pub(crate) struct WorkbenchStore {
    root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoreIssue {
    pub(crate) path: PathBuf,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CreateMission {
    pub(crate) title: String,
    pub(crate) description: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CreateGoal {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) acceptance: String,
    pub(crate) priority: u32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CreateTask {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) goal_id: Option<String>,
    pub(crate) priority: u32,
    pub(crate) depends_on: Vec<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) specialty: Option<String>,
}

impl WorkbenchStore {
    pub(crate) fn for_workspace(workspace_id: &str) -> Self {
        let safe = safe_name(workspace_id);
        Self {
            root: amux_platform::amux_home_dir()
                .join("workspaces")
                .join(safe)
                .join("workbench"),
        }
    }

    #[cfg(test)]
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn dispatch_path(&self, task_id: &str) -> PathBuf {
        self.root.join("dispatch").join(format!("{task_id}.md"))
    }

    pub(crate) fn load_mission(&self) -> Option<Mission> {
        read_json(&self.root.join("mission.json")).ok()
    }

    pub(crate) fn storage_issues(&self) -> Vec<StoreIssue> {
        let mut issues = Vec::new();
        collect_json_issue::<Mission>(&self.root.join("mission.json"), &mut issues);
        collect_dir_json_issues::<Goal>(&self.goals_dir(), &mut issues);
        collect_dir_json_issues::<Task>(&self.tasks_dir(), &mut issues);
        issues
    }

    pub(crate) fn create_mission(&self, input: CreateMission) -> std::io::Result<Mission> {
        let now = now_string();
        let mission = Mission {
            schema_version: SCHEMA_VERSION,
            id: "mission".to_string(),
            title: input.title,
            description: input.description,
            status: MissionStatus::Active,
            created_at: now.clone(),
            updated_at: now,
        };
        write_json(&self.root.join("mission.json"), &mission)?;
        self.record(
            EventKind::MissionCreated,
            format!("Mission created: {}", mission.title),
            None,
            None,
        );
        Ok(mission)
    }

    pub(crate) fn complete_mission(&self) -> std::io::Result<Mission> {
        let mut mission = self.load_mission().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "mission not found")
        })?;
        mission.status = MissionStatus::Complete;
        mission.updated_at = now_string();
        write_json(&self.root.join("mission.json"), &mission)?;
        self.record(
            EventKind::MissionCompleted,
            format!("Mission completed: {}", mission.title),
            None,
            None,
        );
        Ok(mission)
    }

    pub(crate) fn list_goals(&self) -> Vec<Goal> {
        read_dir_json(&self.goals_dir())
    }

    pub(crate) fn load_goal(&self, goal_id: &str) -> Option<Goal> {
        find_json_by_id(&self.goals_dir(), goal_id).and_then(|path| read_json(&path).ok())
    }

    pub(crate) fn create_goal(&self, input: CreateGoal) -> std::io::Result<Goal> {
        let mission = self.ensure_mission()?;
        let id = self.next_goal_id();
        let now = now_string();
        let goal = Goal {
            schema_version: SCHEMA_VERSION,
            id,
            mission_id: mission.id,
            title: input.title,
            description: input.description,
            acceptance: input.acceptance,
            priority: input.priority.max(1),
            status: GoalStatus::Todo,
            created_at: now.clone(),
            updated_at: now,
        };
        write_json(&self.goal_path(&goal.id, &goal.title), &goal)?;
        self.record(
            EventKind::GoalCreated,
            format!("Goal created: {}", goal.title),
            None,
            None,
        );
        Ok(goal)
    }

    pub(crate) fn complete_goal(&self, goal_id: &str) -> std::io::Result<Goal> {
        let mut goal = self.require_goal(goal_id)?;
        goal.status = GoalStatus::Done;
        goal.updated_at = now_string();
        self.save_goal(&goal)?;
        self.record(
            EventKind::GoalCompleted,
            format!("Goal completed: {}", goal.title),
            None,
            None,
        );
        Ok(goal)
    }

    pub(crate) fn list_tasks(&self) -> Vec<Task> {
        read_dir_json(&self.tasks_dir())
    }

    /// Find tasks currently assigned to or in progress for a pane.
    pub(crate) fn find_active_tasks_for_pane(&self, pane_id: &str) -> Vec<Task> {
        self.list_tasks()
            .into_iter()
            .filter(|t| {
                matches!(t.status, TaskStatus::Assigned | TaskStatus::InProgress)
                    && t.assignee_pane_id.as_deref() == Some(pane_id)
            })
            .collect()
    }

    pub(crate) fn load_task(&self, task_id: &str) -> Option<Task> {
        find_json_by_id(&self.tasks_dir(), task_id).and_then(|path| read_json(&path).ok())
    }

    pub(crate) fn blocked_dependencies_for(task: &Task, tasks: &[Task]) -> Vec<String> {
        let done: std::collections::HashSet<&str> = tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Done)
            .map(|task| task.id.as_str())
            .collect();
        task.depends_on
            .iter()
            .filter(|dep| !done.contains(dep.as_str()))
            .cloned()
            .collect()
    }

    pub(crate) fn create_task(&self, input: CreateTask) -> std::io::Result<Task> {
        let mission = self.ensure_mission()?;
        if let Some(goal_id) = input.goal_id.as_deref() {
            self.require_goal(goal_id)?;
        }
        let id = self.next_task_id();
        let now = now_string();
        let task = Task {
            schema_version: SCHEMA_VERSION,
            id,
            mission_id: mission.id,
            goal_id: input.goal_id,
            title: input.title,
            description: input.description,
            status: TaskStatus::Todo,
            assignee_pane_id: None,
            assignee_agent_kind: None,
            assigned_at: None,
            last_activity_at: None,
            priority: input.priority.max(1),
            depends_on: input.depends_on,
            tags: input.tags,
            specialty: input.specialty,
            proof: None,
            blocked_reason: None,
            created_at: now.clone(),
            updated_at: now,
        };
        write_json(&self.task_path(&task.id, &task.title), &task)?;
        self.record(
            EventKind::TaskCreated,
            format!("Task created: {}", task.title),
            Some(task.id.clone()),
            None,
        );
        Ok(task)
    }

    pub(crate) fn assign_task(
        &self,
        task_id: &str,
        pane_id: String,
        agent_kind: Option<String>,
    ) -> std::io::Result<Task> {
        let mut task = self.require_task(task_id)?;
        let now = now_string();
        task.status = TaskStatus::InProgress;
        task.assignee_pane_id = Some(pane_id.clone());
        task.assignee_agent_kind = agent_kind;
        task.assigned_at = Some(now.clone());
        task.last_activity_at = Some(now.clone());
        task.blocked_reason = None;
        task.updated_at = now;
        self.save_task(&task)?;
        self.record(
            EventKind::TaskAssigned,
            format!("Task {} assigned to {}", task.id, pane_id),
            Some(task.id.clone()),
            Some(pane_id),
        );
        Ok(task)
    }

    pub(crate) fn complete_task(&self, task_id: &str, proof: Proof) -> std::io::Result<Task> {
        let mut task = self.require_task(task_id)?;
        task.status = TaskStatus::Done;
        task.proof = Some(proof);
        let now = now_string();
        task.last_activity_at = Some(now.clone());
        task.updated_at = now;
        self.save_task(&task)?;
        self.record(
            EventKind::TaskCompleted,
            format!("Task completed: {}", task.title),
            Some(task.id.clone()),
            task.assignee_pane_id.clone(),
        );
        self.record(
            EventKind::ProofRecorded,
            format!("Proof recorded for task {}", task.id),
            Some(task.id.clone()),
            task.assignee_pane_id.clone(),
        );
        Ok(task)
    }

    pub(crate) fn block_task(&self, task_id: &str, reason: String) -> std::io::Result<Task> {
        let mut task = self.require_task(task_id)?;
        task.status = TaskStatus::Blocked;
        task.blocked_reason = Some(reason.clone());
        let now = now_string();
        task.last_activity_at = Some(now.clone());
        task.updated_at = now;
        self.save_task(&task)?;
        self.record(
            EventKind::TaskBlocked,
            format!("Task {} blocked: {}", task.id, reason),
            Some(task.id.clone()),
            task.assignee_pane_id.clone(),
        );
        Ok(task)
    }

    pub(crate) fn recover_missing_assignees(
        &self,
        existing_pane_ids: &std::collections::HashSet<String>,
    ) -> std::io::Result<Vec<Task>> {
        let mut recovered = Vec::new();
        for mut task in self.list_tasks() {
            if !matches!(task.status, TaskStatus::Assigned | TaskStatus::InProgress) {
                continue;
            }
            let Some(pane_id) = task.assignee_pane_id.clone() else {
                continue;
            };
            if existing_pane_ids.contains(&pane_id) {
                continue;
            }

            task.status = TaskStatus::Blocked;
            task.blocked_reason = Some(format!("assigned pane missing: {pane_id}"));
            let now = now_string();
            task.last_activity_at = Some(now.clone());
            task.updated_at = now;
            self.save_task(&task)?;
            self.record(
                EventKind::TaskBlocked,
                format!(
                    "Task {} blocked because assigned pane disappeared: {}",
                    task.id, pane_id
                ),
                Some(task.id.clone()),
                Some(pane_id),
            );
            recovered.push(task);
        }
        Ok(recovered)
    }

    pub(crate) fn write_dispatch(&self, task_id: &str, prompt: &str) -> std::io::Result<PathBuf> {
        let path = self.dispatch_path(task_id);
        write_bytes(&path, prompt.as_bytes())?;
        Ok(path)
    }

    pub(crate) fn list_task_events(&self, task_id: &str, limit: usize) -> Vec<WorkbenchEvent> {
        let path = self.root.join("events.jsonl");
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(_) => return Vec::new(),
        };
        let mut events: Vec<WorkbenchEvent> = raw
            .lines()
            .filter_map(|line| serde_json::from_str::<WorkbenchEvent>(line).ok())
            .filter(|event| event.task_id.as_deref() == Some(task_id))
            .collect();
        events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        events.truncate(limit);
        events
    }

    pub(crate) fn record_agent_message_sent(&self, task_id: &str, pane_id: &str) {
        self.record(
            EventKind::AgentMessageSent,
            format!("Dispatch prompt sent for task {task_id} to {pane_id}"),
            Some(task_id.to_string()),
            Some(pane_id.to_string()),
        );
    }

    pub(crate) fn record_task_send_failed(&self, task_id: &str, pane_id: &str, reason: &str) {
        self.record(
            EventKind::TaskFailed,
            format!("Task {task_id} send to {pane_id} failed: {reason}"),
            Some(task_id.to_string()),
            Some(pane_id.to_string()),
        );
    }

    fn ensure_mission(&self) -> std::io::Result<Mission> {
        if let Some(mission) = self.load_mission() {
            return Ok(mission);
        }
        self.create_mission(CreateMission {
            title: "Untitled Mission".to_string(),
            description: String::new(),
        })
    }

    fn require_task(&self, task_id: &str) -> std::io::Result<Task> {
        self.load_task(task_id).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("task not found: {task_id}"),
            )
        })
    }

    fn require_goal(&self, goal_id: &str) -> std::io::Result<Goal> {
        self.load_goal(goal_id).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("goal not found: {goal_id}"),
            )
        })
    }

    fn save_goal(&self, goal: &Goal) -> std::io::Result<()> {
        let existing = find_json_by_id(&self.goals_dir(), &goal.id);
        let path = self.goal_path(&goal.id, &goal.title);
        write_json(&path, goal)?;
        if let Some(existing) = existing {
            if existing != path {
                let _ = std::fs::remove_file(existing);
            }
        }
        Ok(())
    }

    fn save_task(&self, task: &Task) -> std::io::Result<()> {
        let existing = find_json_by_id(&self.tasks_dir(), &task.id);
        let path = self.task_path(&task.id, &task.title);
        write_json(&path, task)?;
        if let Some(existing) = existing {
            if existing != path {
                let _ = std::fs::remove_file(existing);
            }
        }
        Ok(())
    }

    fn record(
        &self,
        kind: EventKind,
        message: impl Into<String>,
        task_id: Option<String>,
        pane_id: Option<String>,
    ) {
        let event = new_event(kind, message, task_id, pane_id);
        let _ = append_event(&self.root.join("events.jsonl"), &event);
    }

    fn goals_dir(&self) -> PathBuf {
        self.root.join("goals")
    }

    fn tasks_dir(&self) -> PathBuf {
        self.root.join("tasks")
    }

    fn goal_path(&self, id: &str, title: &str) -> PathBuf {
        self.goals_dir()
            .join(format!("{id}-{}.json", slugify(title)))
    }

    fn task_path(&self, id: &str, title: &str) -> PathBuf {
        self.tasks_dir()
            .join(format!("{id}-{}.json", slugify(title)))
    }

    fn next_goal_id(&self) -> String {
        next_numeric_id(&self.goals_dir(), 2)
    }

    fn next_task_id(&self) -> String {
        next_numeric_id(&self.tasks_dir(), 3)
    }
}

fn safe_name(value: &str) -> String {
    let safe = value.replace(['/', '\\', ':', ' '], "_");
    if safe.is_empty() {
        "default".to_string()
    } else {
        safe
    }
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 50 {
            break;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed.to_string()
    }
}

fn next_numeric_id(dir: &Path, width: usize) -> String {
    let max = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| name.split(['-', '.']).next()?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("{:0width$}", max + 1, width = width)
}

fn find_json_by_id(dir: &Path, id: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| {
                    name == format!("{id}.json") || name.starts_with(&format!("{id}-"))
                })
        })
}

fn read_dir_json<T>(dir: &Path) -> Vec<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| read_json(&path).ok())
        .collect()
}

fn collect_json_issue<T>(path: &Path, issues: &mut Vec<StoreIssue>)
where
    T: for<'de> serde::Deserialize<'de>,
{
    if !path.exists() {
        return;
    }
    if let Err(err) = read_json::<T>(path) {
        issues.push(StoreIssue {
            path: path.to_path_buf(),
            message: err.to_string(),
        });
    }
}

fn collect_dir_json_issues<T>(dir: &Path, issues: &mut Vec<StoreIssue>)
where
    T: for<'de> serde::Deserialize<'de>,
{
    let paths = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"));
    for path in paths {
        collect_json_issue::<T>(&path, issues);
    }
}

fn read_json<T>(path: &Path) -> std::io::Result<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let raw = std::fs::read_to_string(path)?;
    serde_json::from_str(&raw).map_err(std::io::Error::other)
}

fn write_json<T>(path: &Path, value: &T) -> std::io::Result<()>
where
    T: serde::Serialize,
{
    let json = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    write_bytes(path, &json)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
    })?;
    std::fs::create_dir_all(parent)?;
    let file_name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no file name")
    })?;
    let tmp = parent.join(format!(".{file_name}.tmp"));
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_completes_task() {
        let temp = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::new(temp.path().join("workbench"));

        let mission = store
            .create_mission(CreateMission {
                title: "Build workbench".to_string(),
                description: "MVP".to_string(),
            })
            .unwrap();
        assert_eq!(mission.title, "Build workbench");
        let completed_mission = store.complete_mission().unwrap();
        assert_eq!(completed_mission.status, MissionStatus::Complete);

        let goal = store
            .create_goal(CreateGoal {
                title: "Task store".to_string(),
                acceptance: "Tasks persist".to_string(),
                priority: 1,
                ..CreateGoal::default()
            })
            .unwrap();
        assert_eq!(goal.id, "01");

        let task = store
            .create_task(CreateTask {
                title: "Persist task".to_string(),
                goal_id: Some(goal.id),
                tags: vec!["rust".to_string()],
                ..CreateTask::default()
            })
            .unwrap();
        assert_eq!(task.id, "001");
        let assigned = store
            .assign_task(&task.id, "pane-1".to_string(), Some("codex".to_string()))
            .unwrap();
        assert_eq!(assigned.assignee_pane_id.as_deref(), Some("pane-1"));
        assert!(assigned.assigned_at.is_some());
        assert!(assigned.last_activity_at.is_some());

        let completed_goal = store.complete_goal("01").unwrap();
        assert_eq!(completed_goal.status, GoalStatus::Done);

        let completed = store
            .complete_task(&task.id, Proof::from_notes("cargo test passed"))
            .unwrap();
        assert_eq!(completed.status, TaskStatus::Done);
        assert_eq!(
            completed.proof.as_ref().map(|p| p.notes.as_str()),
            Some("cargo test passed")
        );
        store.record_agent_message_sent(&completed.id, "pane-1");
        assert!(store.root.join("events.jsonl").exists());
        let events = std::fs::read_to_string(store.root.join("events.jsonl")).unwrap();
        assert!(events.contains("agent_message_sent"));
        assert!(!store.list_task_events(&completed.id, 10).is_empty());
    }

    #[test]
    fn reports_blocked_task_dependencies() {
        let temp = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::new(temp.path().join("workbench"));
        store
            .create_mission(CreateMission {
                title: "Mission".to_string(),
                description: String::new(),
            })
            .unwrap();
        let first = store
            .create_task(CreateTask {
                title: "First".to_string(),
                ..CreateTask::default()
            })
            .unwrap();
        let second = store
            .create_task(CreateTask {
                title: "Second".to_string(),
                depends_on: vec![first.id.clone()],
                ..CreateTask::default()
            })
            .unwrap();

        assert_eq!(
            WorkbenchStore::blocked_dependencies_for(&second, &store.list_tasks()),
            vec![first.id.clone()]
        );
        store
            .complete_task(&first.id, Proof::from_notes("done"))
            .unwrap();
        assert!(WorkbenchStore::blocked_dependencies_for(&second, &store.list_tasks()).is_empty());
    }

    #[test]
    fn reports_corrupt_json_without_breaking_lists() {
        let temp = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::new(temp.path().join("workbench"));
        store
            .create_mission(CreateMission {
                title: "Recover bad state".to_string(),
                description: String::new(),
            })
            .unwrap();
        std::fs::create_dir_all(store.tasks_dir()).unwrap();
        std::fs::write(store.tasks_dir().join("999-bad.json"), "{not json").unwrap();

        assert!(store.list_tasks().is_empty());
        let issues = store.storage_issues();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].path.ends_with("999-bad.json"));
    }

    #[test]
    fn recovers_tasks_assigned_to_missing_panes() {
        let temp = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::new(temp.path().join("workbench"));
        let task = store
            .create_task(CreateTask {
                title: "Recover assignment".to_string(),
                ..CreateTask::default()
            })
            .unwrap();
        store
            .assign_task(&task.id, "pane-gone".to_string(), Some("Codex".to_string()))
            .unwrap();

        let recovered = store
            .recover_missing_assignees(&std::collections::HashSet::new())
            .unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].status, TaskStatus::Blocked);
        assert_eq!(
            recovered[0].blocked_reason.as_deref(),
            Some("assigned pane missing: pane-gone")
        );
    }

    #[test]
    fn slugifies_empty_titles() {
        assert_eq!(slugify("!!!"), "untitled");
        assert_eq!(slugify("Task Store MVP"), "task-store-mvp");
    }
}
