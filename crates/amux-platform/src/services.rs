use std::path::PathBuf;
use std::sync::Arc;

use crate::{
    DefaultPathMapper, FsBackend, InMemoryFsBackend, InMemoryTerminalBackend, MappedFile,
    PathMapper, PlatformCapabilities, PlatformId, RealFsBackend, RealTerminalBackend,
    SystemMetrics, TerminalBackend,
};
use amux_core::{TerminalLaunchProfile, TerminalSessionId, WorkspaceTarget};

/// Stable host abstraction for wiring platform-specific services into UI/runtime layers.
pub trait HostPlatform: Send + Sync {
    fn id(&self) -> PlatformId;
    fn capabilities(&self) -> PlatformCapabilities;
    fn terminal(&self) -> Arc<dyn TerminalService>;
    fn filesystem(&self) -> Arc<dyn FsService>;
    fn paths(&self) -> Arc<dyn PathService>;
    fn clipboard(&self) -> Arc<dyn ClipboardService>;
    fn browser(&self) -> Arc<dyn BrowserService>;
    fn metrics(&self) -> Arc<dyn MetricsService>;
    fn workspace_dialogs(&self) -> Arc<dyn WorkspaceDialogService>;
}

/// Terminal service abstraction. Blanket-implemented for existing terminal backends.
pub trait TerminalService: TerminalBackend + Send + Sync {
    fn recent_output_lines(&self, _session_id: &TerminalSessionId, _count: usize) -> Vec<String> {
        Vec::new()
    }

    fn is_real_terminal(&self) -> bool {
        false
    }
}

/// Filesystem service abstraction. Blanket-implemented for existing fs backends.
pub trait FsService: FsBackend + Send + Sync {
    fn is_real_fs(&self) -> bool {
        false
    }
}

/// Path mapping abstraction. Blanket-implemented for existing path mappers.
pub trait PathService: PathMapper + Send + Sync {}

impl TerminalService for RealTerminalBackend {
    fn recent_output_lines(&self, session_id: &TerminalSessionId, count: usize) -> Vec<String> {
        self.get_recent_output(session_id, count)
            .into_iter()
            .map(|line| line.text)
            .collect()
    }

    fn is_real_terminal(&self) -> bool {
        true
    }
}

impl TerminalService for InMemoryTerminalBackend {
    fn recent_output_lines(&self, session_id: &TerminalSessionId, count: usize) -> Vec<String> {
        self.records()
            .into_iter()
            .find(|record| record.metadata.id == *session_id)
            .map(|record| {
                record
                    .writes
                    .into_iter()
                    .take(count)
                    .map(|write| String::from_utf8_lossy(&write).to_string())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl FsService for RealFsBackend {
    fn is_real_fs(&self) -> bool {
        true
    }
}

impl FsService for InMemoryFsBackend {}

impl PathService for DefaultPathMapper {}

/// Clipboard abstraction used by the desktop shell.
pub trait ClipboardService: Send + Sync {
    fn read_text(&self) -> Result<Option<String>, String>;
    fn write_text(&self, text: &str) -> Result<(), String>;
    fn read_image(&self) -> Result<Option<ClipboardImage>, String>;
}

/// Raw clipboard image payload.
///
/// The platform layer intentionally exposes images as uncompressed RGBA8
/// pixels rather than an encoded format. Native clipboard APIs (AppKit,
/// Win32, X11/Wayland data control) hand us pixel data, and the desktop
/// shell is the right layer to decide whether it wants PNG, JPEG, BMP, or
/// to skip encoding entirely (e.g. blit straight into a GPUI image).
///
/// `rgba.len()` MUST equal `width * height * 4`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Browser host abstraction. Phase 1 only models capability and URL lifecycle hooks.
pub trait BrowserService: Send + Sync {
    fn is_available(&self) -> bool;
    fn open_external(&self, url: &str) -> Result<(), String>;
}

/// System metrics abstraction.
pub trait MetricsService: Send + Sync {
    fn current_metrics(&self) -> Result<SystemMetrics, String>;
}

/// Native dialogs for workspace selection.
pub trait WorkspaceDialogService: Send + Sync {
    fn pick_folder(&self) -> Result<Option<PathBuf>, String>;
}

/// Shared no-op implementations so Phase 1 can wire the abstractions without changing behavior.
#[derive(Clone, Debug, Default)]
pub struct NoopClipboardService;

impl ClipboardService for NoopClipboardService {
    fn read_text(&self) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn write_text(&self, _text: &str) -> Result<(), String> {
        Ok(())
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, String> {
        Ok(None)
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoopBrowserService;

impl BrowserService for NoopBrowserService {
    fn is_available(&self) -> bool {
        false
    }

    fn open_external(&self, _url: &str) -> Result<(), String> {
        Err("browser service not configured".to_string())
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoopMetricsService;

impl MetricsService for NoopMetricsService {
    fn current_metrics(&self) -> Result<SystemMetrics, String> {
        Err("metrics service not configured".to_string())
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoopWorkspaceDialogService;

impl WorkspaceDialogService for NoopWorkspaceDialogService {
    fn pick_folder(&self) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
}

/// Small helper that keeps Phase 1 test scaffolding concise.
pub fn launch_spec_title(spec: &TerminalLaunchProfile) -> Option<&str> {
    spec.title.as_deref()
}

/// Small helper shared by future platform adapters.
pub fn workspace_runtime_target(target: &WorkspaceTarget) -> &WorkspaceTarget {
    target
}

/// Small helper shared by future platform adapters.
pub fn session_id_str(id: &TerminalSessionId) -> &str {
    &id.0
}

/// Small helper shared by future path service tests.
pub fn mapped_file_display(file: &MappedFile) -> &str {
    &file.display_path
}
