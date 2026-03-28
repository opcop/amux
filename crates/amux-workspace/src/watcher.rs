#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchEvent {
    Changed(String),
    Removed(String),
}

