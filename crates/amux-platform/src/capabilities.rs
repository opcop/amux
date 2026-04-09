use serde::{Deserialize, Serialize};

/// Host operating system identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformId {
    Windows,
    Macos,
    Linux,
    Unknown,
}

impl PlatformId {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unknown
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Linux => "linux",
            Self::Unknown => "unknown",
        }
    }
}

/// Declares which platform-specific capabilities are available on the current host.
///
/// This is intentionally coarse-grained for the first phase of the cross-platform
/// migration. More detailed flags can be added as macOS/Linux implementations land.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformCapabilities {
    pub local_workspace: bool,
    pub wsl_workspace: bool,
    pub browser_tabs: bool,
    pub image_clipboard: bool,
    pub system_metrics: bool,
    pub folder_picker: bool,
}

impl Default for PlatformCapabilities {
    fn default() -> Self {
        Self {
            local_workspace: true,
            wsl_workspace: cfg!(target_os = "windows"),
            browser_tabs: true,
            image_clipboard: true,
            system_metrics: true,
            folder_picker: false,
        }
    }
}

