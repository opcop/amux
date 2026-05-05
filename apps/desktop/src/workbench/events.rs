use std::io::Write;

use super::model::{EventKind, SCHEMA_VERSION, WorkbenchEvent};

pub(crate) fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub(crate) fn now_string() -> String {
    now_millis().to_string()
}

pub(crate) fn new_event(
    kind: EventKind,
    message: impl Into<String>,
    task_id: Option<String>,
    pane_id: Option<String>,
) -> WorkbenchEvent {
    let created_at = now_string();
    WorkbenchEvent {
        schema_version: SCHEMA_VERSION,
        id: format!("evt-{}", now_millis()),
        kind,
        message: message.into(),
        task_id,
        pane_id,
        created_at,
    }
}

pub(crate) fn append_event(path: &std::path::Path, event: &WorkbenchEvent) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(event).map_err(std::io::Error::other)?;
    writeln!(file, "{line}")?;
    Ok(())
}
