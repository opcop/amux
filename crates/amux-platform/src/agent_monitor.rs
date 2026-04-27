//! Agent session monitoring via Claude Code JSONL transcript files.
//!
//! Watches `~/.claude/projects/<encoded-cwd>/*.jsonl` for real-time
//! events: tool uses, sub-agent spawn/completion, token consumption,
//! context window usage, and TodoWrite progress.
//!
//! Complements the existing regex+OSC-based agent status detection
//! with rich structured data that shows *what* an agent is doing,
//! not just *whether* it's running.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// A single todo item from a TodoWrite tool use.
#[derive(Debug, Clone)]
pub struct AgentTodoItem {
    pub content: String,
    pub status: String, // "pending", "in_progress", "completed"
}

/// Rich session state for a Claude Code agent, inferred from JSONL.
#[derive(Debug, Clone, Default)]
pub struct AgentSessionState {
    /// Last tool used (Bash, Read, Edit, Agent, TodoWrite, etc.)
    pub current_tool: Option<String>,
    /// Active sub-agent count
    pub subagent_count: usize,
    /// Types of active sub-agents (e.g. "code-reviewer", "test-engineer")
    pub subagent_types: Vec<String>,
    /// Total tool uses in this session
    pub tool_use_count: usize,
    /// Current model (e.g. "claude-opus-4-6")
    pub model: Option<String>,
    /// Cumulative token usage
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    /// Current context window size (last message's input+cache tokens)
    pub context_tokens: u64,
    /// Current todo list from TodoWrite
    pub todos: Vec<AgentTodoItem>,
    /// Git branch of the project
    pub git_branch: Option<String>,
}

impl AgentSessionState {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
            + self.cache_read_tokens + self.cache_creation_tokens
    }

    pub fn todo_progress(&self) -> (usize, usize) {
        let completed = self.todos.iter().filter(|t| t.status == "completed").count();
        (completed, self.todos.len())
    }

    pub fn context_limit(&self) -> u64 {
        match self.model.as_deref() {
            Some(m) if m.contains("[1m]") || m.contains("-1m") => 1_000_000,
            Some(m) if m.contains("opus-4-6") => 1_000_000,
            Some(m) if m.contains("haiku") => 200_000,
            Some(m) if m.contains("sonnet") => 200_000,
            Some(m) if m.contains("opus") => 200_000,
            _ => 200_000,
        }
    }

    pub fn context_usage_pct(&self) -> u8 {
        let limit = self.context_limit();
        if limit == 0 { 0 } else { ((self.context_tokens as f64 / limit as f64) * 100.0).min(99.0) as u8 }
    }

    pub fn short_model(&self) -> Option<&str> {
        let full = self.model.as_deref()?;
        if full.contains("opus") {
            Some("opus")
        } else if full.contains("sonnet") {
            Some("sonnet")
        } else if full.contains("haiku") {
            Some("haiku")
        } else {
            Some(full)
        }
    }

    /// Human-readable tool label for the current_tool.
    pub fn tool_label(&self) -> Option<String> {
        self.current_tool.as_ref().map(|t| {
            match t.as_str() {
                "TodoWrite" => format!("plan ({}/{})", self.todo_progress().0, self.todo_progress().1),
                "Agent" if !self.subagent_types.is_empty() => {
                    format!("sub:{}", self.subagent_types.join(","))
                }
                _ => t.to_lowercase(),
            }
        })
    }
}

const CHECK_INTERVAL_MS: u64 = 500;
const RESCAN_INTERVAL_SECS: u64 = 5;
const MAX_REQUEST_ID_CACHE: usize = 10_000;

struct PerPaneState {
    jsonl_path: Option<PathBuf>,
    file_position: u64,
    last_mtime: Option<SystemTime>,
    last_check: Instant,
    last_rescan: Instant,
    session: AgentSessionState,
    active_task_ids: HashMap<String, String>,
    counted_request_ids: HashSet<String>,
}

impl PerPaneState {
    fn new() -> Self {
        Self {
            jsonl_path: None,
            file_position: 0,
            last_mtime: None,
            last_check: Instant::now() - Duration::from_secs(10),
            last_rescan: Instant::now() - Duration::from_secs(60),
            session: AgentSessionState::default(),
            active_task_ids: HashMap::new(),
            counted_request_ids: HashSet::new(),
        }
    }
}

/// Monitors Claude Code JSONL transcript files across all panes.
pub struct AgentSessionMonitor {
    panes: HashMap<String, PerPaneState>,
}

impl AgentSessionMonitor {
    pub fn new() -> Self {
        Self { panes: HashMap::new() }
    }

    /// Get the current session state for a pane.
    pub fn state(&self, pane_id: &str) -> Option<&AgentSessionState> {
        self.panes.get(pane_id).map(|p| &p.session)
    }

    /// Get the current session state mutably.
    pub fn state_mut(&mut self, pane_id: &str) -> Option<&mut AgentSessionState> {
        self.panes.get_mut(pane_id).map(|p| &mut p.session)
    }

    /// Update monitoring for a pane. Called from `poll_activity` for
    /// each pane that has Claude Code detected. Throttled internally.
    pub fn update(&mut self, pane_id: &str, cwd: &Path) {
        let monitor = self.panes.entry(pane_id.to_string()).or_insert_with(PerPaneState::new);

        if monitor.last_check.elapsed() < Duration::from_millis(CHECK_INTERVAL_MS) {
            return;
        }
        monitor.last_check = Instant::now();

        // Rescan for new JSONL files periodically, or if path missing
        let path_missing = monitor.jsonl_path.as_ref().map_or(true, |p| !p.exists());
        let stale_scan = monitor.last_rescan.elapsed() > Duration::from_secs(RESCAN_INTERVAL_SECS);
        if path_missing || stale_scan {
            monitor.last_rescan = Instant::now();
            let expected = find_jsonl_path(cwd);
            if monitor.jsonl_path != expected {
                monitor.jsonl_path = expected;
                monitor.file_position = 0;
                monitor.session = AgentSessionState::default();
                monitor.active_task_ids.clear();
                monitor.counted_request_ids.clear();
            }
        }

        let path = match &monitor.jsonl_path {
            Some(p) => p.clone(),
            None => return,
        };

        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => return,
        };
        let mtime = meta.modified().ok();
        if mtime == monitor.last_mtime {
            return;
        }
        monitor.last_mtime = mtime;

        // Truncation detection
        if meta.len() < monitor.file_position {
            monitor.file_position = 0;
            monitor.session = AgentSessionState::default();
            monitor.active_task_ids.clear();
            monitor.counted_request_ids.clear();
        }

        // Read new lines
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(monitor.file_position)).is_err() {
            return;
        }

        let mut new_position = monitor.file_position;
        let mut buf = String::new();
        loop {
            buf.clear();
            let bytes = match reader.read_line(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            if !buf.ends_with('\n') {
                break;
            }
            new_position += bytes as u64;
            process_event(monitor, buf.trim());
        }
        monitor.file_position = new_position;
    }

    /// Remove monitoring state for a pane.
    pub fn remove(&mut self, pane_id: &str) {
        self.panes.remove(pane_id);
    }
}

fn process_event(monitor: &mut PerPaneState, line: &str) {
    let json: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return,
    };

    match json.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "assistant" => {
            let msg = json.get("message");

            let stop_reason = msg.and_then(|m| m.get("stop_reason")).and_then(|v| v.as_str());
            match stop_reason {
                Some("tool_use") | None => {
                    monitor.session.current_tool = None; // will be set below if tool_use blocks present
                }
                Some(_) => {
                    monitor.session.current_tool = None;
                }
            }

            if let Some(model) = msg.and_then(|m| m.get("model")).and_then(|v| v.as_str()) {
                monitor.session.model = Some(model.to_string());
            }

            // Token dedup via requestId
            let request_id = json.get("requestId").and_then(|v| v.as_str()).map(|s| s.to_string());
            let should_count = match &request_id {
                Some(id) => {
                    if monitor.counted_request_ids.len() >= MAX_REQUEST_ID_CACHE {
                        monitor.counted_request_ids.clear();
                    }
                    monitor.counted_request_ids.insert(id.clone())
                }
                None => false,
            };

            if should_count {
                if let Some(usage) = msg.and_then(|m| m.get("usage")) {
                    let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    let cache_read = usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    let cache_create = usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    monitor.session.input_tokens += input;
                    monitor.session.output_tokens += output;
                    monitor.session.cache_read_tokens += cache_read;
                    monitor.session.cache_creation_tokens += cache_create;
                    monitor.session.context_tokens = input + cache_read + cache_create;
                }
            }

            if let Some(branch) = json.get("gitBranch").and_then(|v| v.as_str()) {
                if !branch.is_empty() && branch != "HEAD" {
                    monitor.session.git_branch = Some(branch.to_string());
                }
            }

            if let Some(content) = msg.and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                        continue;
                    }
                    if let Some(name) = block.get("name").and_then(|v| v.as_str()) {
                        monitor.session.current_tool = Some(name.to_string());
                        monitor.session.tool_use_count += 1;

                        if name == "Agent" || name == "Task" {
                            if let Some(tid) = block.get("id").and_then(|v| v.as_str()) {
                                let stype = block
                                    .get("input")
                                    .and_then(|i| i.get("subagent_type"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("general-purpose")
                                    .to_string();
                                monitor.active_task_ids.insert(tid.to_string(), stype);
                                monitor.session.subagent_count = monitor.active_task_ids.len();
                                monitor.session.subagent_types =
                                    monitor.active_task_ids.values().cloned().collect();
                            }
                        }

                        if name == "TodoWrite" {
                            if let Some(todos) = block
                                .get("input")
                                .and_then(|v| v.get("todos"))
                                .and_then(|v| v.as_array())
                            {
                                monitor.session.todos = todos
                                    .iter()
                                    .filter_map(|t| Some(AgentTodoItem {
                                        content: t.get("content")?.as_str()?.to_string(),
                                        status: t.get("status")?.as_str()?.to_string(),
                                    }))
                                    .collect();
                            }
                        }
                    }
                }
            }
        }
        "user" => {
            let content = json.get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array());

            let mut has_tool_result = false;
            if let Some(content) = content {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                        has_tool_result = true;
                        if let Some(tool_use_id) = block.get("tool_use_id").and_then(|v| v.as_str()) {
                            if monitor.active_task_ids.remove(tool_use_id).is_some() {
                                monitor.session.subagent_count = monitor.active_task_ids.len();
                                monitor.session.subagent_types =
                                    monitor.active_task_ids.values().cloned().collect();
                            }
                        }
                    }
                }
            }

            if !has_tool_result {
                monitor.session.current_tool = None;
            }
        }
        _ => {}
    }
}

/// Encode a path to Claude Code's project-name format.
/// Replaces any non-alphanumeric-non-dot char with `-`.
fn encode_cwd_to_project_name(cwd: &Path) -> String {
    let s = cwd.to_string_lossy();
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' {
            result.push(ch);
        } else {
            result.push('-');
        }
    }
    result
}

/// Find the most recently modified JSONL file in Claude's project dir.
fn find_jsonl_path(cwd: &Path) -> Option<PathBuf> {
    let home = crate::dirs::real_user_home()?;
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.exists() {
        return None;
    }
    let encoded = encode_cwd_to_project_name(cwd);
    let project_dir = projects_dir.join(&encoded);
    if !project_dir.exists() {
        return None;
    }
    let mut latest: Option<(PathBuf, SystemTime)> = None;
    for entry in std::fs::read_dir(&project_dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "jsonl") {
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    match &latest {
                        Some((_, old)) if *old >= mtime => {}
                        _ => latest = Some((path, mtime)),
                    }
                }
            }
        }
    }
    latest.map(|(p, _)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_cwd_slashes_become_dashes() {
        let path = PathBuf::from("/Users/foo/bar");
        assert_eq!(encode_cwd_to_project_name(&path), "-Users-foo-bar");
    }

    #[test]
    fn encode_cwd_windows_path() {
        let path = PathBuf::from(r"C:\Users\foo\bar");
        assert_eq!(encode_cwd_to_project_name(&path), "C--Users-foo-bar");
    }

    #[test]
    fn encode_cwd_unicode_to_dashes() {
        let path = PathBuf::from("/home/あいう/test");
        let enc = encode_cwd_to_project_name(&path);
        assert!(enc.contains("test"));
        assert!(!enc.contains("あ"));
    }

    #[test]
    fn session_context_limit_detection() {
        let mut state = AgentSessionState::default();
        state.model = Some("claude-opus-4-6".into());
        assert_eq!(state.context_limit(), 1_000_000);
        state.model = Some("claude-sonnet-4-6".into());
        assert_eq!(state.context_limit(), 200_000);
    }

    #[test]
    fn session_todo_progress() {
        let mut state = AgentSessionState::default();
        state.todos = vec![
            AgentTodoItem { content: "A".into(), status: "completed".into() },
            AgentTodoItem { content: "B".into(), status: "in_progress".into() },
            AgentTodoItem { content: "C".into(), status: "pending".into() },
        ];
        assert_eq!(state.todo_progress(), (1, 3));
    }

    #[test]
    fn session_total_tokens() {
        let mut state = AgentSessionState::default();
        state.input_tokens = 100;
        state.output_tokens = 50;
        state.cache_read_tokens = 1000;
        state.cache_creation_tokens = 200;
        assert_eq!(state.total_tokens(), 1350);
    }
}
