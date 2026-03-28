use std::path::PathBuf;

pub fn session_file_path(base: PathBuf) -> PathBuf {
    base.join("session.json")
}

