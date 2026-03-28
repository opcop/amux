use std::fs;
use std::path::PathBuf;

use amux_core::SessionState;

use crate::{JsonSessionCodec, SessionCodec, session_file_path};

pub trait SessionStore {
    fn load(&self) -> Result<SessionState, String>;
    fn save(&self, session: &SessionState) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSessionStore<C = JsonSessionCodec> {
    base_dir: PathBuf,
    codec: C,
}

impl FileSessionStore<JsonSessionCodec> {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            codec: JsonSessionCodec,
        }
    }
}

impl<C> FileSessionStore<C> {
    pub fn with_codec(base_dir: impl Into<PathBuf>, codec: C) -> Self {
        Self {
            base_dir: base_dir.into(),
            codec,
        }
    }

    pub fn path(&self) -> PathBuf {
        session_file_path(self.base_dir.clone())
    }
}

impl<C: SessionCodec> SessionStore for FileSessionStore<C> {
    fn load(&self) -> Result<SessionState, String> {
        let path = self.path();
        if !path.exists() {
            return Ok(SessionState::default());
        }
        let raw = fs::read_to_string(&path).map_err(|err| err.to_string())?;
        self.codec.decode(&raw)
    }

    fn save(&self, session: &SessionState) -> Result<(), String> {
        let path = self.path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let raw = self.codec.encode(session)?;
        fs::write(path, raw).map_err(|err| err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use amux_core::SessionState;

    use super::{FileSessionStore, SessionStore};

    #[test]
    fn file_store_saves_and_loads_session() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("amux-session-test-{unique}"));
        let store = FileSessionStore::new(&base);

        let session = SessionState::default();
        store.save(&session).expect("session should save");
        let loaded = store.load().expect("session should load");

        assert_eq!(loaded.version, 2);
    }
}
