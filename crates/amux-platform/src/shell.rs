#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformShell {
    PowerShell,
    Cmd,
    Wsl(String),
}

