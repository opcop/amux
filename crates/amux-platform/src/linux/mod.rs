use std::sync::{Arc, Mutex};

use crate::{
    BrowserService, ClipboardService, DefaultPathMapper, FsService, HostPlatform,
    MetricsService, NoopBrowserService, NoopWorkspaceDialogService,
    PathService, PlatformCapabilities, PlatformId, RealFsBackend, RealTerminalBackend,
    SystemMetrics, SystemMetricsCollector, TerminalService, WorkspaceDialogService,
};
use crate::common::{ArboardClipboardService, LinuxWorkspaceDialogService};

#[derive(Clone)]
pub struct LinuxPlatform {
    capabilities: PlatformCapabilities,
    terminal: Arc<dyn TerminalService>,
    filesystem: Arc<dyn FsService>,
    paths: Arc<dyn PathService>,
    clipboard: Arc<dyn ClipboardService>,
    browser: Arc<dyn BrowserService>,
    metrics: Arc<dyn MetricsService>,
    workspace_dialogs: Arc<dyn WorkspaceDialogService>,
}

impl std::fmt::Debug for LinuxPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinuxPlatform")
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

impl LinuxPlatform {
    pub fn new() -> Self {
        let workspace_dialogs: Arc<dyn WorkspaceDialogService> = LinuxWorkspaceDialogService::new()
            .map(|service| Arc::new(service) as Arc<dyn WorkspaceDialogService>)
            .unwrap_or_else(|| Arc::new(NoopWorkspaceDialogService));
        Self {
            capabilities: PlatformCapabilities {
                local_workspace: true,
                wsl_workspace: false,
                browser_tabs: false,
                image_clipboard: true,
                system_metrics: true,
                folder_picker: LinuxWorkspaceDialogService::is_available(),
            },
            terminal: Arc::new(RealTerminalBackend::new()),
            filesystem: Arc::new(RealFsBackend::new()),
            paths: Arc::new(DefaultPathMapper),
            clipboard: Arc::new(ArboardClipboardService::new()),
            browser: Arc::new(NoopBrowserService),
            metrics: Arc::new(CollectorMetricsService::default()),
            workspace_dialogs,
        }
    }
}

impl Default for LinuxPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl HostPlatform for LinuxPlatform {
    fn id(&self) -> PlatformId {
        PlatformId::Linux
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
