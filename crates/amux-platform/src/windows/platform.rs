use std::sync::{Arc, Mutex};

use crate::{
    BrowserService, ClipboardService, DefaultPathMapper, FsService, HostPlatform, MetricsService,
    NoopBrowserService, PathService, PlatformCapabilities, PlatformId,
    RealFsBackend, RealTerminalBackend, SystemMetrics, SystemMetricsCollector, TerminalService,
    WorkspaceDialogService,
};
use crate::common::{ArboardClipboardService, WindowsWorkspaceDialogService};

/// Windows host adapter for the existing stable platform services.
///
/// Phase 2 intentionally wraps the current implementations without changing
/// their behavior. WSL support remains encapsulated inside the terminal/path/fs
/// implementations that already exist today.
#[derive(Clone)]
pub struct WindowsPlatform {
    capabilities: PlatformCapabilities,
    terminal: Arc<dyn TerminalService>,
    filesystem: Arc<dyn FsService>,
    paths: Arc<dyn PathService>,
    clipboard: Arc<dyn ClipboardService>,
    browser: Arc<dyn BrowserService>,
    metrics: Arc<dyn MetricsService>,
    workspace_dialogs: Arc<dyn WorkspaceDialogService>,
}

impl std::fmt::Debug for WindowsPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowsPlatform")
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

impl WindowsPlatform {
    pub fn new() -> Self {
        Self {
            capabilities: PlatformCapabilities {
                local_workspace: true,
                wsl_workspace: true,
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
            workspace_dialogs: Arc::new(WindowsWorkspaceDialogService::new()),
        }
    }

    pub fn with_terminal(mut self, terminal: Arc<dyn TerminalService>) -> Self {
        self.terminal = terminal;
        self
    }

    pub fn with_filesystem(mut self, filesystem: Arc<dyn FsService>) -> Self {
        self.filesystem = filesystem;
        self
    }

    pub fn with_paths(mut self, paths: Arc<dyn PathService>) -> Self {
        self.paths = paths;
        self
    }

    pub fn with_clipboard(mut self, clipboard: Arc<dyn ClipboardService>) -> Self {
        self.clipboard = clipboard;
        self
    }

    pub fn with_browser(mut self, browser: Arc<dyn BrowserService>) -> Self {
        self.browser = browser;
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<dyn MetricsService>) -> Self {
        self.metrics = metrics;
        self
    }

    pub fn with_workspace_dialogs(
        mut self,
        workspace_dialogs: Arc<dyn WorkspaceDialogService>,
    ) -> Self {
        self.workspace_dialogs = workspace_dialogs;
        self
    }

    pub fn with_capabilities(mut self, capabilities: PlatformCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }
}

impl Default for WindowsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl HostPlatform for WindowsPlatform {
    fn id(&self) -> PlatformId {
        PlatformId::Windows
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

#[derive(Default)]
struct CollectorMetricsService {
    collector: Mutex<SystemMetricsCollector>,
}

impl std::fmt::Debug for CollectorMetricsService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectorMetricsService").finish()
    }
}

impl MetricsService for CollectorMetricsService {
    fn current_metrics(&self) -> Result<SystemMetrics, String> {
        let mut collector = self
            .collector
            .lock()
            .map_err(|_| "system metrics mutex poisoned".to_string())?;
        Ok(collector.get_metrics())
    }
}

#[cfg(test)]
mod tests {
    use super::WindowsPlatform;
    use crate::{HostPlatform, PlatformId};

    #[test]
    fn windows_platform_exposes_expected_defaults() {
        let platform = WindowsPlatform::new();

        assert_eq!(platform.id(), PlatformId::Windows);
        let caps = platform.capabilities();
        assert!(caps.local_workspace);
        assert!(caps.wsl_workspace);
        assert!(caps.browser_tabs);
        assert!(caps.system_metrics);
    }
}
