use crate::SessionState;

pub const SESSION_VERSION: u32 = 2;

/// Normalize session after loading from storage.
/// Handles version migrations and ensures data consistency.
pub fn normalize_session(mut session: SessionState) -> SessionState {
    // Migrate from v1 to v2: recent_workspaces might be empty
    if session.recent_workspaces.is_empty() && !session.workspaces.is_empty() {
        // Populate recent workspaces from existing workspaces
        session.recent_workspaces = session
            .workspaces
            .iter()
            .enumerate()
            .rev()
            .take(5)
            .map(|(i, ws)| crate::session::model::RecentWorkspace::new(
                ws.id.clone(),
                ws.name.clone(),
                ws.target.clone(),
                i,
            ))
            .collect();
    }
    
    session.version = SESSION_VERSION;
    session
}

