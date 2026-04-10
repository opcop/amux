use std::sync::Arc;

use crate::{
    BrowserService, ClipboardService, DefaultPathMapper, FsService, HostPlatform,
    MetricsService, NoopBrowserService, PathService,
    PlatformCapabilities, PlatformId, RealFsBackend, RealTerminalBackend,
    TerminalService, WorkspaceDialogService,
};
use crate::common::{ArboardClipboardService, CollectorMetricsService, MacosWorkspaceDialogService};

#[derive(Clone)]
pub struct MacosPlatform {
    capabilities: PlatformCapabilities,
    terminal: Arc<dyn TerminalService>,
    filesystem: Arc<dyn FsService>,
    paths: Arc<dyn PathService>,
    clipboard: Arc<dyn ClipboardService>,
    browser: Arc<dyn BrowserService>,
    metrics: Arc<dyn MetricsService>,
    workspace_dialogs: Arc<dyn WorkspaceDialogService>,
}

impl std::fmt::Debug for MacosPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MacosPlatform")
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

impl MacosPlatform {
    pub fn new() -> Self {
        Self {
            capabilities: PlatformCapabilities {
                local_workspace: true,
                wsl_workspace: false,
                // wry 0.53 supports WKWebView on macOS via objc2-app-kit;
                // build_as_child(window_handle) accepts the AppKit
                // window handle that GPUI exposes via raw_window_handle.
                // No additional macOS-specific runtime work is needed
                // beyond the capability flag — the existing
                // gpui_browser.rs path is cross-platform.
                browser_tabs: true,
                image_clipboard: true,
                system_metrics: true,
                folder_picker: true,
            },
            terminal: Arc::new(RealTerminalBackend::new()),
            filesystem: Arc::new(RealFsBackend::new()),
            paths: Arc::new(DefaultPathMapper),
            clipboard: Arc::new(ArboardClipboardService::new()),
            browser: Arc::new(NoopBrowserService),
            metrics: Arc::new(CollectorMetricsService::default()),
            workspace_dialogs: Arc::new(MacosWorkspaceDialogService::new()),
        }
    }
}

impl Default for MacosPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl HostPlatform for MacosPlatform {
    fn id(&self) -> PlatformId {
        PlatformId::Macos
    }

    fn capabilities(&self) -> PlatformCapabilities {
        self.capabilities.clone()
    }

    fn terminal(&self) -> Arc<dyn TerminalService> {
        Arc::clone(&self.terminal)
    }

    fn filesystem(&self) -> Arc<dyn FsService> {
        Arc::clone(&self.filesystem)
    }

    fn paths(&self) -> Arc<dyn PathService> {
        Arc::clone(&self.paths)
    }

    fn clipboard(&self) -> Arc<dyn ClipboardService> {
        Arc::clone(&self.clipboard)
    }

    fn browser(&self) -> Arc<dyn BrowserService> {
        Arc::clone(&self.browser)
    }

    fn metrics(&self) -> Arc<dyn MetricsService> {
        Arc::clone(&self.metrics)
    }

    fn workspace_dialogs(&self) -> Arc<dyn WorkspaceDialogService> {
        Arc::clone(&self.workspace_dialogs)
    }
}


#[cfg(test)]
mod tests {
    use super::MacosPlatform;
    use crate::{HostPlatform, PlatformId};

    #[test]
    fn macos_platform_exposes_expected_defaults() {
        let platform = MacosPlatform::new();
        assert_eq!(platform.id(), PlatformId::Macos);
        let caps = platform.capabilities();
        assert!(caps.local_workspace);
        assert!(!caps.wsl_workspace);
        assert!(caps.image_clipboard, "macOS now has a real clipboard backend");
        assert!(caps.folder_picker, "macOS folder picker uses native AppKit via rfd");
        assert!(caps.browser_tabs, "macOS browser uses wry+WKWebView via build_as_child");
        assert!(caps.system_metrics);
    }

    #[test]
    fn macos_platform_constructs_real_services() {
        // Smoke test: every service handle should be obtainable without panicking.
        // We don't actually call into the OS clipboard / picker here because CI
        // may not have a window server; that's covered by manual smoke runs.
        let platform = MacosPlatform::new();
        let _ = platform.terminal();
        let _ = platform.filesystem();
        let _ = platform.paths();
        let _ = platform.clipboard();
        let _ = platform.browser();
        let _ = platform.metrics();
        let _ = platform.workspace_dialogs();
    }
}
