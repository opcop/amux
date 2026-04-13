use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use amux_agent::{
    AgentLaunchRequest, AgentLauncher, AgentRegistry, AgentStatus, StaticAgentRegistry,
};
use amux_core::{
    AgentLaunchMode, AgentSurfaceState, EditorSurfaceState, PaneId, SplitAxis, SurfaceId,
    SurfaceState, TabId, TerminalLaunchProfile, TerminalSessionId,
};
use amux_platform::{
    DefaultPathMapper, FsBackend, FsEntry, FsService, HostPlatform, InMemoryFsBackend,
    InMemoryTerminalBackend, PathMapper, PathService, RealFsBackend, RealTerminalBackend,
    TerminalBackend, TerminalService, PlatformCapabilities,
};
use amux_session::{FileSessionStore, SessionStore};
use amux_workspace::{FileFilter, WorkspaceService};

use crate::{
    commands::{parse_command, AppCommand, UiAction},
    ActiveSurfaceItem, AgentListItem, AppSnapshot, FileListItem, OpenFileItem, UiState,
};

/// Auto-save configuration
#[derive(Clone, Debug)]
pub struct AutoSaveConfig {
    /// Interval in seconds between auto-saves
    pub interval_secs: u64,
    /// Whether auto-save is enabled
    pub enabled: bool,
}

impl Default for AutoSaveConfig {
    fn default() -> Self {
        Self {
            interval_secs: 60, // Default: save every 60 seconds
            enabled: true,
        }
    }
}

impl AutoSaveConfig {
    /// Create a new config with custom interval
    pub fn new(interval_secs: u64) -> Self {
        Self {
            interval_secs,
            enabled: true,
        }
    }

    /// Disable auto-save
    pub fn disabled() -> Self {
        Self {
            interval_secs: 60,
            enabled: false,
        }
    }
}

/// Tracks auto-save state
#[derive(Clone, Debug)]
pub struct AutoSaveState {
    /// When the last auto-save occurred (Unix timestamp in millis)
    pub last_auto_save: Option<u64>,
    /// Number of auto-saves performed this session
    pub auto_save_count: u32,
}

impl Default for AutoSaveState {
    fn default() -> Self {
        Self {
            last_auto_save: None,
            auto_save_count: 0,
        }
    }
}

/// Wrapper for FsBackend that can be either in-memory or real
#[derive(Clone)]
pub enum FsBackendWrapper {
    InMemory(InMemoryFsBackend),
    Real(RealFsBackend),
    Service(Arc<dyn FsService>),
}

impl std::fmt::Debug for FsBackendWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InMemory(_) => write!(f, "FsBackendWrapper::InMemory"),
            Self::Real(_) => write!(f, "FsBackendWrapper::Real"),
            Self::Service(_) => write!(f, "FsBackendWrapper::Service"),
        }
    }
}

impl FsBackend for FsBackendWrapper {
    fn read_to_string(&self, file: &amux_platform::MappedFile) -> Result<String, String> {
        match self {
            Self::InMemory(backend) => backend.read_to_string(file),
            Self::Real(backend) => backend.read_to_string(file),
            Self::Service(backend) => backend.read_to_string(file),
        }
    }

    fn write_string(&self, file: &amux_platform::MappedFile, content: &str) -> Result<(), String> {
        match self {
            Self::InMemory(backend) => backend.write_string(file, content),
            Self::Real(backend) => backend.write_string(file, content),
            Self::Service(backend) => backend.write_string(file, content),
        }
    }

    fn read_dir(
        &self,
        target: &amux_core::WorkspaceTarget,
        relative_path: &str,
    ) -> Result<Vec<FsEntry>, String> {
        match self {
            Self::InMemory(backend) => backend.read_dir(target, relative_path),
            Self::Real(backend) => backend.read_dir(target, relative_path),
            Self::Service(backend) => backend.read_dir(target, relative_path),
        }
    }
}

/// Wrapper for terminal backends (in-memory mock or real PTY)
#[derive(Clone)]
pub enum TerminalBackendWrapper {
    /// In-memory mock backend for demo/testing
    InMemory(InMemoryTerminalBackend),
    /// Real PTY backend
    Real(RealTerminalBackend),
    /// Injected platform service backend
    Service(Arc<dyn TerminalService>),
}

impl std::fmt::Debug for TerminalBackendWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InMemory(_) => write!(f, "TerminalBackendWrapper::InMemory"),
            Self::Real(_) => write!(f, "TerminalBackendWrapper::Real"),
            Self::Service(_) => write!(f, "TerminalBackendWrapper::Service"),
        }
    }
}

impl TerminalBackend for TerminalBackendWrapper {
    fn create_session(&self, spec: TerminalLaunchProfile) -> Result<TerminalSessionId, String> {
        match self {
            Self::InMemory(backend) => backend.create_session(spec),
            Self::Real(backend) => backend.create_session(spec),
            Self::Service(backend) => backend.create_session(spec),
        }
    }

    fn write_input(&self, id: &TerminalSessionId, data: &[u8]) -> Result<(), String> {
        match self {
            Self::InMemory(backend) => backend.write_input(id, data),
            Self::Real(backend) => backend.write_input(id, data),
            Self::Service(backend) => backend.write_input(id, data),
        }
    }

    fn resize(&self, id: &TerminalSessionId, cols: u16, rows: u16) -> Result<(), String> {
        match self {
            Self::InMemory(backend) => backend.resize(id, cols, rows),
            Self::Real(backend) => backend.resize(id, cols, rows),
            Self::Service(backend) => backend.resize(id, cols, rows),
        }
    }

    fn kill(&self, id: &TerminalSessionId) -> Result<(), String> {
        match self {
            Self::InMemory(backend) => backend.kill(id),
            Self::Real(backend) => backend.kill(id),
            Self::Service(backend) => backend.kill(id),
        }
    }

    fn metadata(
        &self,
        id: &TerminalSessionId,
    ) -> Result<amux_platform::TerminalSessionMetadata, String> {
        match self {
            Self::InMemory(backend) => backend.metadata(id),
            Self::Real(backend) => backend.metadata(id),
            Self::Service(backend) => backend.metadata(id),
        }
    }
}

impl TerminalBackendWrapper {
    /// Create a new in-memory terminal backend
    pub fn new_in_memory() -> Self {
        Self::InMemory(InMemoryTerminalBackend::default())
    }

    /// Create a new real terminal backend
    pub fn new_real() -> Self {
        Self::Real(RealTerminalBackend::new())
    }

    pub fn is_real(&self) -> bool {
        match self {
            Self::Real(_) => true,
            Self::Service(backend) => backend.is_real_terminal(),
            Self::InMemory(_) => false,
        }
    }

    /// Get recent output lines for a session
    pub fn get_recent_output(&self, session_id: &TerminalSessionId, count: usize) -> Vec<String> {
        match self {
            Self::InMemory(backend) => {
                // For in-memory backend, get from records
                let records = backend.records();
                if let Some(record) = records.iter().find(|r| r.metadata.id.0 == session_id.0) {
                    record
                        .writes
                        .iter()
                        .take(count)
                        .map(|w| String::from_utf8_lossy(w).to_string())
                        .collect()
                } else {
                    Vec::new()
                }
            }
            Self::Real(backend) => backend.recent_output_lines(session_id, count),
            Self::Service(backend) => backend.recent_output_lines(session_id, count),
        }
    }
}

#[derive(Clone)]
pub enum PathMapperWrapper {
    Default(DefaultPathMapper),
    Service(Arc<dyn PathService>),
}

impl std::fmt::Debug for PathMapperWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default(_) => write!(f, "PathMapperWrapper::Default"),
            Self::Service(_) => write!(f, "PathMapperWrapper::Service"),
        }
    }
}

impl PathMapper for PathMapperWrapper {
    fn to_display_path(&self, target: &amux_core::WorkspaceTarget) -> String {
        match self {
            Self::Default(mapper) => mapper.to_display_path(target),
            Self::Service(mapper) => mapper.to_display_path(target),
        }
    }

    fn to_runtime_cwd(&self, target: &amux_core::WorkspaceTarget) -> Result<String, String> {
        match self {
            Self::Default(mapper) => mapper.to_runtime_cwd(target),
            Self::Service(mapper) => mapper.to_runtime_cwd(target),
        }
    }

    fn map_file_for_editor(
        &self,
        workspace: &amux_core::WorkspaceTarget,
        relative_path: &str,
    ) -> Result<amux_platform::MappedFile, String> {
        match self {
            Self::Default(mapper) => mapper.map_file_for_editor(workspace, relative_path),
            Self::Service(mapper) => mapper.map_file_for_editor(workspace, relative_path),
        }
    }
}

#[derive(Clone)]
pub struct AppController {
    registry: StaticAgentRegistry,
    platform: Option<Arc<dyn HostPlatform>>,
    terminal_backend: TerminalBackendWrapper,
    fs_backend: FsBackendWrapper,
    path_mapper: PathMapperWrapper,
    session_store: FileSessionStore,
    auto_save: AutoSaveConfig,
    auto_save_state: AutoSaveState,
}

impl std::fmt::Debug for AppController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppController")
            .field("registry", &self.registry)
            .field("platform", &self.platform.as_ref().map(|p| p.id()))
            .field("terminal_backend", &self.terminal_backend)
            .field("fs_backend", &self.fs_backend)
            .field("path_mapper", &self.path_mapper)
            .field("session_store", &self.session_store)
            .field("auto_save", &self.auto_save)
            .field("auto_save_state", &self.auto_save_state)
            .finish()
    }
}

impl AppController {
    pub fn platform_capabilities(&self) -> PlatformCapabilities {
        self.platform
            .as_ref()
            .map(|platform| platform.capabilities())
            .unwrap_or_default()
    }

    pub fn new(app_name: &str) -> Self {
        let mut registry = StaticAgentRegistry::with_defaults();
        registry
            .set_detection("codex", AgentStatus::Installed)
            .set_detection("claude", AgentStatus::Installed);
        Self {
            registry,
            platform: None,
            terminal_backend: TerminalBackendWrapper::new_in_memory(),
            fs_backend: FsBackendWrapper::InMemory(InMemoryFsBackend::default()),
            path_mapper: PathMapperWrapper::Default(DefaultPathMapper),
            session_store: FileSessionStore::new(default_session_dir(app_name)),
            auto_save: AutoSaveConfig::default(),
            auto_save_state: AutoSaveState::default(),
        }
    }

    /// Create controller with real filesystem backend
    pub fn with_real_fs(app_name: &str) -> Self {
        let mut registry = StaticAgentRegistry::with_defaults();
        registry
            .set_detection("codex", AgentStatus::Installed)
            .set_detection("claude", AgentStatus::Installed);
        Self {
            registry,
            platform: None,
            terminal_backend: TerminalBackendWrapper::new_real(),
            fs_backend: FsBackendWrapper::Real(RealFsBackend::new()),
            path_mapper: PathMapperWrapper::Default(DefaultPathMapper),
            session_store: FileSessionStore::new(default_session_dir(app_name)),
            auto_save: AutoSaveConfig::default(),
            auto_save_state: AutoSaveState::default(),
        }
    }

    pub fn with_platform(app_name: &str, platform: Arc<dyn HostPlatform>) -> Self {
        let mut registry = StaticAgentRegistry::with_defaults();
        registry
            .set_detection("codex", AgentStatus::Installed)
            .set_detection("claude", AgentStatus::Installed);
        Self {
            registry,
            platform: Some(Arc::clone(&platform)),
            terminal_backend: TerminalBackendWrapper::Service(platform.terminal()),
            fs_backend: FsBackendWrapper::Service(platform.filesystem()),
            path_mapper: PathMapperWrapper::Service(platform.paths()),
            session_store: FileSessionStore::new(default_session_dir(app_name)),
            auto_save: AutoSaveConfig::default(),
            auto_save_state: AutoSaveState::default(),
        }
    }

    /// Switch to real filesystem backend
    pub fn enable_real_fs(&mut self) {
        self.fs_backend = FsBackendWrapper::Real(RealFsBackend::new());
    }

    /// Enable real terminal backend (real PTY)
    pub fn enable_real_terminal(&mut self) {
        self.terminal_backend = TerminalBackendWrapper::new_real();
    }

    /// Check if using real filesystem
    pub fn is_using_real_fs(&self) -> bool {
        match &self.fs_backend {
            FsBackendWrapper::Real(_) => true,
            FsBackendWrapper::Service(backend) => backend.is_real_fs(),
            FsBackendWrapper::InMemory(_) => false,
        }
    }

    /// Check if using real terminal backend
    pub fn is_using_real_terminal(&self) -> bool {
        self.terminal_backend.is_real()
    }

    pub fn session_path(&self) -> PathBuf {
        self.session_store.path()
    }

    pub fn pick_workspace_folder(&self) -> Result<Option<PathBuf>, String> {
        let platform = self
            .platform
            .as_ref()
            .ok_or_else(|| "workspace folder picker is not configured".to_string())?;
        if !platform.capabilities().folder_picker {
            return Ok(None);
        }
        platform.workspace_dialogs().pick_folder()
    }

    // === Auto-save methods ===

    /// Get current auto-save configuration
    pub fn auto_save_config(&self) -> &AutoSaveConfig {
        &self.auto_save
    }

    /// Update auto-save configuration
    pub fn set_auto_save_config(&mut self, config: AutoSaveConfig) {
        self.auto_save = config;
    }

    /// Enable or disable auto-save
    pub fn set_auto_save_enabled(&mut self, enabled: bool) {
        self.auto_save.enabled = enabled;
    }

    /// Set auto-save interval in seconds
    pub fn set_auto_save_interval(&mut self, interval_secs: u64) {
        self.auto_save.interval_secs = interval_secs;
    }

    /// Check if auto-save is needed and perform it if necessary
    /// Returns true if an auto-save was performed
    pub fn check_auto_save(&mut self, state: &mut UiState, current_time_millis: u64) -> bool {
        if !self.auto_save.enabled {
            return false;
        }

        // Only save if there are unsaved changes
        if !state.dirty {
            return false;
        }

        let interval_millis = self.auto_save.interval_secs * 1000;
        let time_since_last_save = self
            .auto_save_state
            .last_auto_save
            .map(|last| current_time_millis.saturating_sub(last))
            .unwrap_or(interval_millis); // If never saved, save now

        if time_since_last_save >= interval_millis {
            match self.persist_session(state) {
                Ok(()) => {
                    self.auto_save_state.last_auto_save = Some(current_time_millis);
                    self.auto_save_state.auto_save_count += 1;
                    true
                }
                Err(e) => {
                    state.push_activity(format!("auto-save failed: {}", e));
                    false
                }
            }
        } else {
            false
        }
    }

    /// Get the time until the next auto-save (in seconds)
    pub fn time_until_auto_save(&self, current_time_millis: u64) -> Option<u64> {
        if !self.auto_save.enabled {
            return None;
        }

        let interval_millis = self.auto_save.interval_secs * 1000;
        let time_since_last = self
            .auto_save_state
            .last_auto_save
            .map(|last| current_time_millis.saturating_sub(last))
            .unwrap_or(0);

        let remaining = interval_millis.saturating_sub(time_since_last);
        Some(remaining / 1000) // Convert to seconds
    }

    /// Get auto-save statistics
    pub fn auto_save_stats(&self) -> (bool, u32, Option<u64>) {
        (
            self.auto_save.enabled,
            self.auto_save_state.auto_save_count,
            self.auto_save_state.last_auto_save,
        )
    }

    /// Force an immediate auto-save
    pub fn force_auto_save(
        &mut self,
        state: &mut UiState,
        current_time_millis: u64,
    ) -> Result<(), String> {
        self.persist_session(state)?;
        self.auto_save_state.last_auto_save = Some(current_time_millis);
        self.auto_save_state.auto_save_count += 1;
        Ok(())
    }

    // === End auto-save methods ===

    /// Restore an existing session if one is on disk.
    ///
    /// Returns `true` if at least one workspace was restored. This is the
    /// product startup primitive — it is opinion-free: no demo files, no
    /// auto-launched agent, no auto-opened README.
    pub fn restore_session_if_present(&self, state: &mut UiState) -> bool {
        let _ = self.restore_session(state);
        if state.session.workspaces.is_empty() {
            return false;
        }
        // In real-fs mode this is a no-op; in in-memory mode it re-seeds
        // mock files for the restored workspace target so previews work.
        let _ = self.seed_demo_workspace_files(state);
        state.push_activity("session: restored existing workspace state");
        true
    }

    /// Open a workspace at `path` and persist the resulting session.
    ///
    /// Used by the product startup flow when the user passes an explicit
    /// workspace path on the command line, or by code paths that already
    /// know which folder to open.
    pub fn open_local_workspace(&self, state: &mut UiState, path: PathBuf) {
        state.dispatch(UiAction::OpenLocalWorkspace(path));
        let _ = self.seed_demo_workspace_files(state);
        let _ = self.persist_session(state);
    }

    /// Test/dev helper that seeds an opinionated demo workspace.
    ///
    /// Replaces the historical `bootstrap_demo` flow. Kept around because
    /// the unit tests in `root.rs` rely on a populated workspace, an
    /// in-memory README/notes pair, a vertical split, and a pre-launched
    /// codex agent. Production startup MUST NOT call this — it is gated
    /// to `#[cfg(any(test, feature = "demo-bootstrap"))]` so accidental
    /// production use becomes a compile error.
    #[cfg(test)]
    pub fn seed_demo_state(&self, state: &mut UiState) {
        use amux_core::{PreviewKind, PreviewSurfaceState};

        if self.restore_session_if_present(state) {
            return;
        }

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        state.dispatch(UiAction::OpenLocalWorkspace(cwd));

        let _ = self.seed_demo_workspace_files(state);

        let Some(workspace) = state.session.active_workspace() else {
            return;
        };
        let pane_id = workspace.active_pane_id.clone();

        state.dispatch(UiAction::OpenSurface {
            pane_id: pane_id.clone(),
            surface: SurfaceState::Preview(PreviewSurfaceState {
                surface_id: SurfaceId::new("surface-demo-preview"),
                source_relative_path: "README.md".into(),
                kind: PreviewKind::Markdown,
            }),
        });

        state.dispatch(UiAction::SplitPane {
            pane_id,
            axis: SplitAxis::Vertical,
        });

        let _ = self.launch_agent(state, "codex");
        let _ = self.open_file_in_active_workspace(state, "README.md");
        let _ = self.persist_session(state);
        state.push_activity("bootstrap: demo workspace initialized");
    }

    pub fn dispatch(&self, state: &mut UiState, action: UiAction) -> Vec<amux_core::Event> {
        let events = state.dispatch(action);
        if !events.is_empty() || state.last_error.is_none() {
            match self.persist_session(state) {
                Ok(()) => state.push_activity("session: persisted"),
                Err(err) => state.push_activity(format!("session error: {err}")),
            }
        }
        events
    }

    pub fn run_command(&mut self, state: &mut UiState, input: &str) -> Result<String, String> {
        let active_pane_id = state
            .session
            .active_workspace()
            .map(|workspace| workspace.active_pane_id.clone());
        match parse_command(input, active_pane_id)? {
            AppCommand::Ui(action) => {
                self.dispatch(state, action);
                state.push_activity(format!("command ok: {input}"));
                Ok("ok".into())
            }
            AppCommand::LaunchAgent { provider_id } => {
                self.launch_agent(state, &provider_id)?;
                state.push_activity(format!("command ok: agent {provider_id}"));
                Ok(format!("launched agent: {provider_id}"))
            }
            AppCommand::OpenFile { relative_path } => {
                self.open_file_in_active_workspace(state, &relative_path)?;
                state.push_activity(format!("command ok: file open {relative_path}"));
                Ok(format!("opened file: {relative_path}"))
            }
            AppCommand::ShowHelp => {
                state.push_activity("command ok: help");
                Ok(crate::commands::command_help_for(&self.platform_capabilities()).join("\n"))
            }
            AppCommand::SaveSession => match self.persist_session(state) {
                Ok(()) => {
                    state.push_activity("command ok: save");
                    Ok("session saved".into())
                }
                Err(err) => {
                    state.push_activity(format!("command error: save failed - {err}"));
                    Err(format!("save failed: {err}"))
                }
            },
            AppCommand::ResizeSplit(delta) => match self.resize_active_split(state, delta) {
                Ok(_) => {
                    state.push_activity("command ok: resize split");
                    Ok(format!("split resized by {}", delta))
                }
                Err(err) => {
                    state.push_activity(format!("command error: {err}"));
                    Err(err)
                }
            },
            AppCommand::ResetSplitRatios => match self.reset_split_ratios(state) {
                Ok(_) => {
                    state.push_activity("command ok: reset ratios");
                    Ok("split ratios reset".into())
                }
                Err(err) => {
                    state.push_activity(format!("command error: {err}"));
                    Err(err)
                }
            },
            AppCommand::ListWslDistros => {
                if !self.platform_capabilities().wsl_workspace {
                    state.push_activity("command error: wsl not available on this platform");
                    return Err("WSL is not available on this platform".to_string());
                }
                #[cfg(target_os = "windows")]
                {
                    use amux_platform::detect_wsl_distributions;
                    let result = detect_wsl_distributions();
                    let output = if result.wsl_available {
                        if result.distros.is_empty() {
                            "No WSL distributions found".to_string()
                        } else {
                            let mut lines = vec!["WSL Distributions:".to_string()];
                            for distro in &result.distros {
                                let state = match distro.state {
                                    amux_platform::DistroState::Running => "Running",
                                    amux_platform::DistroState::Stopped => "Stopped",
                                    amux_platform::DistroState::Installing => "Installing",
                                };
                                lines.push(format!(
                                    "  {} ({}) v{}",
                                    distro.name, state, distro.version
                                ));
                            }
                            if let Some(default) = &result.default_distro {
                                lines.push(format!("\nDefault: {}", default));
                            }
                            lines.join("\n")
                        }
                    } else {
                        "WSL is not available on this system".to_string()
                    };
                    state.push_activity("command ok: wsl list");
                    Ok(output)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    state.push_activity("command error: wsl not available on this platform");
                    Err("WSL is only available on Windows".to_string())
                }
            }
            AppCommand::EnableAutoSave => {
                self.auto_save.enabled = true;
                state.push_activity("command ok: autosave enabled");
                Ok("Auto-save enabled".into())
            }
            AppCommand::DisableAutoSave => {
                self.auto_save.enabled = false;
                state.push_activity("command ok: autosave disabled");
                Ok("Auto-save disabled".into())
            }
            AppCommand::SetAutoSaveInterval(secs) => {
                if secs < 10 {
                    state.push_activity("command error: interval must be at least 10 seconds");
                    Err("interval must be at least 10 seconds".to_string())
                } else {
                    self.auto_save.interval_secs = secs;
                    state.push_activity(format!("command ok: autosave interval set to {}s", secs));
                    Ok(format!("Auto-save interval set to {} seconds", secs))
                }
            }
            AppCommand::ShowAutoSaveStatus => {
                let (enabled, count, last) = self.auto_save_stats();
                let status =
                    if enabled {
                        format!(
                        "Auto-save: enabled (interval: {}s)\nSaves this session: {}\nLast save: {}",
                        self.auto_save.interval_secs,
                        count,
                        last.map(|ts| format!("{} ago", (std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64 - ts) / 1000))
                        .unwrap_or_else(|| "never".to_string())
                    )
                    } else {
                        format!(
                            "Auto-save: disabled\nSaves this session: {}\nLast save: {}",
                            count,
                            last.map(|ts| format!(
                                "{} ago",
                                (std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis() as u64
                                    - ts)
                                    / 1000
                            ))
                            .unwrap_or_else(|| "never".to_string())
                        )
                    };
                state.push_activity("command ok: autosave status");
                Ok(status)
            }
            AppCommand::BrowseWslRoot => {
                if !self.platform_capabilities().wsl_workspace {
                    state.push_activity("command error: wsl not available on this platform");
                    return Err("WSL is not available on this platform".to_string());
                }
                #[cfg(target_os = "windows")]
                {
                    use amux_platform::detect_wsl_distributions;
                    let result = detect_wsl_distributions();

                    if !result.wsl_available {
                        state.push_activity("command error: wsl not available");
                        return Err("WSL is not available on this system".to_string());
                    }

                    let distro = result
                        .default_distro
                        .or_else(|| result.distros.first().map(|d| d.name.clone()))
                        .ok_or_else(|| "No WSL distributions found".to_string())?;

                    self.browse_wsl_path(state, &distro, "/")
                }
                #[cfg(not(target_os = "windows"))]
                {
                    state.push_activity("command error: wsl not available on this platform");
                    Err("WSL is only available on Windows".to_string())
                }
            }
            AppCommand::BrowseWslPath(_path) => {
                if !self.platform_capabilities().wsl_workspace {
                    state.push_activity("command error: wsl not available on this platform");
                    return Err("WSL is not available on this platform".to_string());
                }
                #[cfg(target_os = "windows")]
                {
                    // Try to get distro from current workspace
                    let distro = state.session.active_workspace()
                        .and_then(|ws| match &ws.target {
                            amux_core::WorkspaceTarget::WslPath { distro, .. } => Some(distro.clone()),
                            _ => None,
                        })
                        .ok_or_else(|| "No active WSL workspace. Use 'workspace open-wsl <distro> <path>' first".to_string())?;

                    self.browse_wsl_path(state, &distro, &_path)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    state.push_activity("command error: wsl not available on this platform");
                    Err("WSL is only available on Windows".to_string())
                }
            }
            // Quick switcher commands
            AppCommand::SwitchWorkspace(n) => self.switch_to_workspace(state, n),
            AppCommand::SwitchNextWorkspace => self.switch_to_next_workspace(state),
            AppCommand::SwitchPreviousWorkspace => self.switch_to_prev_workspace(state),
            AppCommand::FocusNextPane => self.focus_next_pane(state),
            AppCommand::FocusPreviousPane => self.focus_prev_pane(state),
            AppCommand::FocusNextTab => self.focus_next_tab(state),
            AppCommand::FocusPreviousTab => self.focus_prev_tab(state),
            AppCommand::OpenSettings => self.open_settings(state),
            AppCommand::IncreaseFontSize => self.adjust_font_size(state, 2),
            AppCommand::DecreaseFontSize => self.adjust_font_size(state, -2),
            AppCommand::ResetFontSize => self.reset_font_size(state),
            AppCommand::CreateFile { path } => self.create_file(state, &path),
            AppCommand::CreateDirectory { path } => self.create_directory(state, &path),
            AppCommand::DeleteFile { path } => self.delete_file(state, &path),
            AppCommand::RenameFile { old_path, new_path } => {
                self.rename_file(state, &old_path, &new_path)
            }
            AppCommand::CloseWorkspace { id } => self.close_workspace(state, id.as_deref()),
            AppCommand::RenameWorkspace { id, new_name } => {
                self.rename_workspace(state, &id, &new_name)
            }
            AppCommand::ReorderWorkspace {
                from_index,
                to_index,
            } => self.reorder_workspace(state, from_index, to_index),
            AppCommand::OpenBrowser { url } => self.open_browser(state, url.as_deref()),
        }
    }

    fn open_browser(&self, state: &mut UiState, url: Option<&str>) -> Result<String, String> {
        if !self.platform_capabilities().browser_tabs {
            return Err("browser tabs are not available on this platform".to_string());
        }

        let workspace = state
            .session
            .active_workspace()
            .ok_or("no active workspace")?;

        let url = url.unwrap_or("https://www.google.com").to_string();

        let surface = amux_core::BrowserSurfaceState {
            surface_id: amux_core::SurfaceId::new(format!(
                "surface-browser-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )),
            url: url.clone(),
            title: url.clone(),
            can_go_back: false,
            can_go_forward: false,
            is_loading: false,
        };

        state.dispatch(UiAction::OpenSurface {
            pane_id: workspace.active_pane_id.clone(),
            surface: amux_core::SurfaceState::Browser(surface),
        });

        state.dirty = true;
        Ok(format!("opened browser: {}", url))
    }

    fn close_workspace(&self, state: &mut UiState, id: Option<&str>) -> Result<String, String> {
        let workspace_id = match id {
            Some(id) => amux_core::WorkspaceId::new(id),
            None => state
                .session
                .active_workspace_id
                .clone()
                .ok_or("no active workspace")?,
        };

        if state.session.workspaces.len() <= 1 {
            return Err("cannot close the last workspace".to_string());
        }

        let removed = state
            .session
            .remove_workspace(&workspace_id)
            .ok_or("workspace not found")?;

        state.dirty = true;
        state.push_activity(format!("closed workspace: {}", removed.name));
        Ok(format!("closed workspace: {}", removed.name))
    }

    pub fn rename_workspace(
        &self,
        state: &mut UiState,
        id: &str,
        new_name: &str,
    ) -> Result<String, String> {
        let workspace_id = amux_core::WorkspaceId::new(id);
        state
            .session
            .rename_workspace(&workspace_id, new_name.to_string())?;
        state.dirty = true;
        state.push_activity(format!("renamed workspace to: {}", new_name));
        Ok(format!("renamed workspace to: {}", new_name))
    }

    fn reorder_workspace(
        &self,
        state: &mut UiState,
        from_index: usize,
        to_index: usize,
    ) -> Result<String, String> {
        state.session.move_workspace(from_index, to_index)?;
        state.dirty = true;
        state.push_activity("workspace reordered".to_string());
        Ok("workspace reordered".to_string())
    }

    fn create_file(&self, state: &mut UiState, path: &str) -> Result<String, String> {
        let workspace = state
            .session
            .active_workspace()
            .ok_or("no active workspace")?;

        let fs = self.fs_backend.clone();
        let mapped = self
            .path_mapper
            .map_file_for_editor(&workspace.target, path)?;

        fs.write_string(&mapped, "")?;

        state.dirty = true;
        Ok(format!("created file: {}", path))
    }

    fn create_directory(&self, state: &mut UiState, path: &str) -> Result<String, String> {
        let workspace = state
            .session
            .active_workspace()
            .ok_or("no active workspace")?;

        let dir_path = self
            .path_mapper
            .map_file_for_editor(&workspace.target, path)?;

        std::fs::create_dir_all(&dir_path.native_path)
            .map_err(|e| format!("failed to create directory: {}", e))?;

        state.dirty = true;
        state.push_activity(format!("created directory: {}", path));
        Ok(format!("created directory: {}", path))
    }

    fn delete_file(&self, state: &mut UiState, path: &str) -> Result<String, String> {
        let workspace = state
            .session
            .active_workspace()
            .ok_or("no active workspace")?;

        let file_path = self
            .path_mapper
            .map_file_for_editor(&workspace.target, path)?;

        if file_path.native_path.is_dir() {
            std::fs::remove_dir_all(&file_path.native_path)
                .map_err(|e| format!("failed to delete directory: {}", e))?;
        } else {
            std::fs::remove_file(&file_path.native_path)
                .map_err(|e| format!("failed to delete file: {}", e))?;
        }

        state.dirty = true;
        Ok(format!("deleted: {}", path))
    }

    fn rename_file(
        &self,
        state: &mut UiState,
        old_path: &str,
        new_path: &str,
    ) -> Result<String, String> {
        let workspace = state
            .session
            .active_workspace()
            .ok_or("no active workspace")?;

        let old_file = self
            .path_mapper
            .map_file_for_editor(&workspace.target, old_path)?;
        let new_file = self
            .path_mapper
            .map_file_for_editor(&workspace.target, new_path)?;

        if old_file.native_path.is_dir() {
            std::fs::rename(&old_file.native_path, &new_file.native_path)
                .map_err(|e| format!("failed to rename directory: {}", e))?;
        } else {
            std::fs::rename(&old_file.native_path, &new_file.native_path)
                .map_err(|e| format!("failed to rename file: {}", e))?;
        }

        state.dirty = true;
        Ok(format!("renamed {} to {}", old_path, new_path))
    }

    pub fn handle_tab_action(
        &mut self,
        state: &mut UiState,
        action: UiAction,
    ) -> Result<String, String> {
        match action {
            UiAction::PinTab => self.pin_active_tab(state),
            UiAction::UnpinTab => self.unpin_active_tab(state),
            UiAction::RenameTab(new_title) => self.rename_active_tab(state, new_title),
            UiAction::CloseOtherTabs => self.close_other_tabs(state),
            _ => Err("invalid tab action".into()),
        }
    }

    fn pin_active_tab(&self, state: &mut UiState) -> Result<String, String> {
        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or("no active workspace")?;
        workspace.pin_active_tab().map_err(|e| format!("{:?}", e))?;
        state.dirty = true;
        Ok("tab pinned".into())
    }

    fn unpin_active_tab(&self, state: &mut UiState) -> Result<String, String> {
        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or("no active workspace")?;
        workspace
            .unpin_active_tab()
            .map_err(|e| format!("{:?}", e))?;
        state.dirty = true;
        Ok("tab unpinned".into())
    }

    fn rename_active_tab(&self, state: &mut UiState, new_title: String) -> Result<String, String> {
        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or("no active workspace")?;
        workspace
            .rename_active_tab(new_title.clone())
            .map_err(|e| format!("{:?}", e))?;
        state.dirty = true;
        Ok(format!("tab renamed to: {}", new_title))
    }

    fn close_other_tabs(&self, state: &mut UiState) -> Result<String, String> {
        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or("no active workspace")?;
        workspace
            .close_other_tabs()
            .map_err(|e| format!("{:?}", e))?;
        state.dirty = true;
        Ok("other tabs closed".into())
    }

    fn adjust_font_size(&self, state: &mut UiState, delta: i16) -> Result<String, String> {
        let current = state.session.ui_preferences.font_size as i16;
        let new_size = (current + delta).max(8).min(72) as u16;
        state.session.ui_preferences.font_size = new_size;
        state.push_activity(format!("font size: {}px", new_size));
        Ok(format!("font size: {}px", new_size))
    }

    fn reset_font_size(&self, state: &mut UiState) -> Result<String, String> {
        let default_size = 14u16;
        state.session.ui_preferences.font_size = default_size;
        state.push_activity(format!("font size reset to {}px", default_size));
        Ok(format!("font size reset to {}px", default_size))
    }

    #[cfg(target_os = "windows")]
    fn browse_wsl_path(
        &self,
        state: &mut UiState,
        distro: &str,
        path: &str,
    ) -> Result<String, String> {
        use amux_platform::{wsl_parent_path, wsl_read_dir, FsEntry};

        match wsl_read_dir(distro, path) {
            Ok(entries) => {
                let parent = wsl_parent_path(path);
                let mut lines = vec![format!("WSL:{} - {}:", distro, path), "=".repeat(50)];

                // Show parent link if exists
                if let Some(ref parent_path) = parent {
                    lines.push(format!("[.. {}]", parent_path));
                }

                // Sort: dirs first, then files
                let mut dirs: Vec<&FsEntry> = entries.iter().filter(|e| e.is_dir).collect();
                let mut files: Vec<&FsEntry> = entries.iter().filter(|e| !e.is_dir).collect();
                dirs.sort_by_key(|e| &e.name);
                files.sort_by_key(|e| &e.name);

                let dirs_count = dirs.len();
                let files_count = files.len();

                for entry in dirs {
                    lines.push(format!("[DIR]  {}", entry.name));
                }
                for entry in files {
                    lines.push(format!("[FILE] {}", entry.name));
                }

                lines.push(format!(
                    "\n{} items ({} dirs, {} files)",
                    entries.len(),
                    dirs_count,
                    files_count
                ));

                state.push_activity(format!("command ok: browsed wsl {}:{}", distro, path));
                Ok(lines.join("\n"))
            }
            Err(e) => {
                state.push_activity(format!("command error: {}", e));
                Err(format!("Failed to browse {}:{}", path, e))
            }
        }
    }

    // === Quick Switcher Methods ===

    fn switch_to_workspace(&self, state: &mut UiState, index: usize) -> Result<String, String> {
        // Get workspace info first
        let (workspace_id, workspace_name) = {
            let workspaces = &state.session.workspaces;
            if workspaces.is_empty() {
                return Err("no workspaces available".to_string());
            }

            // Convert 1-based index to 0-based
            let idx = index.saturating_sub(1);
            if idx >= workspaces.len() {
                return Err(format!(
                    "workspace {} not found (only {} workspaces)",
                    index,
                    workspaces.len()
                ));
            }

            (workspaces[idx].id.0.clone(), workspaces[idx].name.clone())
        }; // workspaces borrow ends here

        self.activate_workspace(state, &workspace_id)?;
        state.push_activity(format!("switched to workspace {}", index));
        Ok(format!(
            "Switched to workspace {}: {}",
            index, workspace_name
        ))
    }

    fn switch_to_next_workspace(&self, state: &mut UiState) -> Result<String, String> {
        // Get workspace info first
        let (workspace_id, workspace_name, next_idx) = {
            let workspaces = &state.session.workspaces;
            if workspaces.len() <= 1 {
                return Err("only one workspace available".to_string());
            }

            let current_id = state
                .session
                .active_workspace_id
                .as_ref()
                .ok_or("no active workspace")?;

            let current_idx = workspaces
                .iter()
                .position(|w| &w.id == current_id)
                .ok_or("active workspace not found")?;

            let next_idx = (current_idx + 1) % workspaces.len();
            (
                workspaces[next_idx].id.0.clone(),
                workspaces[next_idx].name.clone(),
                next_idx,
            )
        };

        self.activate_workspace(state, &workspace_id)?;
        state.push_activity("switched to next workspace");
        Ok(format!(
            "Switched to workspace {}: {}",
            next_idx + 1,
            workspace_name
        ))
    }

    fn switch_to_prev_workspace(&self, state: &mut UiState) -> Result<String, String> {
        // Get workspace info first
        let (workspace_id, workspace_name, prev_idx) = {
            let workspaces = &state.session.workspaces;
            if workspaces.len() <= 1 {
                return Err("only one workspace available".to_string());
            }

            let current_id = state
                .session
                .active_workspace_id
                .as_ref()
                .ok_or("no active workspace")?;

            let current_idx = workspaces
                .iter()
                .position(|w| &w.id == current_id)
                .ok_or("active workspace not found")?;

            let prev_idx = if current_idx == 0 {
                workspaces.len() - 1
            } else {
                current_idx - 1
            };
            (
                workspaces[prev_idx].id.0.clone(),
                workspaces[prev_idx].name.clone(),
                prev_idx,
            )
        };

        self.activate_workspace(state, &workspace_id)?;
        state.push_activity("switched to previous workspace");
        Ok(format!(
            "Switched to workspace {}: {}",
            prev_idx + 1,
            workspace_name
        ))
    }

    fn focus_next_pane(&self, state: &mut UiState) -> Result<String, String> {
        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or("no active workspace")?;

        use amux_core::layout::get_all_panes;
        let panes = get_all_panes(&workspace.layout);

        if panes.len() <= 1 {
            return Err("only one pane in layout".to_string());
        }

        let current_pane_id = &workspace.active_pane_id.0;
        let current_idx = panes
            .iter()
            .position(|p| p.pane_id.0 == *current_pane_id)
            .unwrap_or(0);

        let next_idx = (current_idx + 1) % panes.len();
        let next_pane_id = panes[next_idx].pane_id.clone();

        workspace.active_pane_id = next_pane_id.clone();
        state.push_activity(format!("focused next pane"));
        Ok(format!("Focused pane {}", next_idx + 1))
    }

    fn focus_prev_pane(&self, state: &mut UiState) -> Result<String, String> {
        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or("no active workspace")?;

        use amux_core::layout::get_all_panes;
        let panes = get_all_panes(&workspace.layout);

        if panes.len() <= 1 {
            return Err("only one pane in layout".to_string());
        }

        let current_pane_id = &workspace.active_pane_id.0;
        let current_idx = panes
            .iter()
            .position(|p| p.pane_id.0 == *current_pane_id)
            .unwrap_or(0);

        let prev_idx = if current_idx == 0 {
            panes.len() - 1
        } else {
            current_idx - 1
        };
        let prev_pane_id = panes[prev_idx].pane_id.clone();

        workspace.active_pane_id = prev_pane_id.clone();
        state.push_activity("focused previous pane");
        Ok(format!("Focused pane {}", prev_idx + 1))
    }

    fn focus_next_tab(&self, state: &mut UiState) -> Result<String, String> {
        use amux_core::layout::find_pane_mut;

        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or("no active workspace")?;

        let pane = find_pane_mut(&mut workspace.layout, &workspace.active_pane_id)
            .ok_or("active pane not found")?;

        if pane.tabs.len() <= 1 {
            return Err("only one tab in pane".to_string());
        }

        let current_idx = pane
            .tabs
            .iter()
            .position(|t| t.id == pane.active_tab_id)
            .unwrap_or(0);
        let next_idx = (current_idx + 1) % pane.tabs.len();

        let tab_id = pane.tabs[next_idx].id.clone();
        pane.active_tab_id = tab_id;

        state.push_activity("focused next tab");
        Ok(format!("Focused tab {}", next_idx + 1))
    }

    fn focus_prev_tab(&self, state: &mut UiState) -> Result<String, String> {
        use amux_core::layout::find_pane_mut;

        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or("no active workspace")?;

        let pane = find_pane_mut(&mut workspace.layout, &workspace.active_pane_id)
            .ok_or("active pane not found")?;

        if pane.tabs.len() <= 1 {
            return Err("only one tab in pane".to_string());
        }

        let current_idx = pane
            .tabs
            .iter()
            .position(|t| t.id == pane.active_tab_id)
            .unwrap_or(0);
        let prev_idx = if current_idx == 0 {
            pane.tabs.len() - 1
        } else {
            current_idx - 1
        };

        let tab_id = pane.tabs[prev_idx].id.clone();
        pane.active_tab_id = tab_id;

        state.push_activity("focused previous tab");
        Ok(format!("Focused tab {}", prev_idx + 1))
    }

    fn resize_active_split(&self, state: &mut UiState, delta: f32) -> Result<(), String> {
        use amux_core::layout::find_split_for_pane;

        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or("no active workspace")?;

        let pane_id = &workspace.active_pane_id;
        let Some((split_id, _axis)) = find_split_for_pane(&workspace.layout, pane_id) else {
            return Err("active pane is not in a split".to_string());
        };

        workspace.layout.resize_split(&split_id, delta);
        Ok(())
    }

    fn reset_split_ratios(&self, state: &mut UiState) -> Result<(), String> {
        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or("no active workspace")?;
        workspace.layout.reset_split_ratios();
        Ok(())
    }

    fn open_settings(&self, state: &mut UiState) -> Result<String, String> {
        use amux_core::surface::{SettingsCategory, SettingsSurfaceState};
        use amux_core::{layout::append_tab, SurfaceId};

        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or("no active workspace")?;

        let pane_id = workspace.active_pane_id.clone();

        let settings = SettingsSurfaceState {
            surface_id: SurfaceId::new("settings"),
            title: "Settings".to_string(),
            selected_category: SettingsCategory::General,
            categories: SettingsCategory::all(),
        };

        let tab_id = amux_core::TabId::new("settings-tab");

        let tab = amux_core::TabState::new(
            tab_id.clone(),
            "Settings",
            false,
            amux_core::SurfaceState::Settings(settings),
        );

        if append_tab(&mut workspace.layout, &pane_id, tab) {
            state.push_activity("opened settings");
            Ok("Settings opened".to_string())
        } else {
            Err("failed to open settings".to_string())
        }
    }

    pub fn snapshot(&self, state: &mut UiState) -> AppSnapshot {
        let capabilities = self.platform_capabilities();
        if let Some(platform) = &self.platform {
            if capabilities.system_metrics {
                state.set_system_metrics(platform.metrics().current_metrics().ok());
            } else {
                state.set_system_metrics(None);
            }
        } else {
            state.refresh_system_metrics_legacy();
        }

        let mut snapshot = state.snapshot();
        snapshot.platform_capabilities = capabilities;
        let active_target = state
            .session
            .active_workspace()
            .map(|workspace| workspace.target.clone());
        let detections = self.registry.detect_all();
        snapshot.agents = self
            .registry
            .list()
            .into_iter()
            .map(|provider| {
                let status = detections
                    .iter()
                    .find(|detection| detection.provider_id == provider.id)
                    .map(|detection| match &detection.status {
                        AgentStatus::Installed => "installed".to_string(),
                        AgentStatus::NotFound => "not_found".to_string(),
                        AgentStatus::NeedsAuth => "needs_auth".to_string(),
                        AgentStatus::Broken(reason) => format!("broken:{reason}"),
                    })
                    .unwrap_or_else(|| "unknown".into());
                let supported = active_target
                    .as_ref()
                    .map(|target| provider.supports_workspace(target))
                    .unwrap_or(false);
                AgentListItem {
                    id: provider.id,
                    name: provider.display_name,
                    status,
                    supported,
                }
            })
            .collect();
        if let Some(workspace) = state.session.active_workspace() {
            let fs = self.fs_backend.clone();
            let service = WorkspaceService::new(self.path_mapper.clone(), fs);
            if let Ok(files) = service.list_files(
                &workspace.target,
                "",
                &FileFilter {
                    query: String::new(),
                    show_hidden: false,
                },
            ) {
                snapshot.files = files
                    .into_iter()
                    .map(|file| FileListItem {
                        name: file.name,
                        relative_path: file.relative_path,
                        is_dir: file.is_dir,
                    })
                    .collect();
            }
            snapshot.open_files = collect_open_files(&workspace.layout);
            snapshot.active_surface =
                self.enrich_active_surface(snapshot.active_surface.take(), &workspace.target);
        }
        snapshot
    }

    pub fn restore_session(&self, state: &mut UiState) -> Result<(), String> {
        state.session = self.session_store.load()?;
        normalize_session_tabs(&mut state.session);
        heal_duplicate_workspace_ids(&mut state.session);
        // Migrate the new group layer onto sessions that predate it.
        // Must run AFTER `heal_duplicate_workspace_ids` so the id
        // rewrites have already settled before we look at group
        // membership.
        state.session.migrate_groups();
        state.dirty = false;
        state.save_status = crate::SaveStatus::Saved("just now".to_string());
        state.push_activity("session: loaded from store");
        Ok(())
    }

    pub fn persist_session(&self, state: &mut UiState) -> Result<(), String> {
        // Mark as saving
        state.save_status = crate::SaveStatus::Saving;
        self.session_store.save(&state.session)?;
        // Mark as saved
        state.mark_saved();
        state.push_activity("session: saved");
        Ok(())
    }

    pub fn activate_workspace(
        &self,
        state: &mut UiState,
        workspace_id: &str,
    ) -> Result<(), String> {
        let workspace_exists = state
            .session
            .workspaces
            .iter()
            .any(|workspace| workspace.id.0 == workspace_id);
        if !workspace_exists {
            return Err(format!("unknown workspace: {workspace_id}"));
        }

        state.session.active_workspace_id = Some(amux_core::WorkspaceId::new(workspace_id));
        state.last_error = None;
        state.push_activity(format!("workspace: activated {workspace_id}"));
        self.persist_session(state)?;
        Ok(())
    }

    pub fn split_active_pane(&self, state: &mut UiState, axis: SplitAxis) -> Result<(), String> {
        let pane_id = state
            .session
            .active_workspace()
            .ok_or_else(|| "no active workspace".to_string())?
            .active_pane_id
            .clone();
        let _ = self.dispatch(state, UiAction::SplitPane { pane_id, axis });
        Ok(())
    }

    pub fn focus_pane(&self, state: &mut UiState, pane_id: PaneId) -> Result<(), String> {
        let _ = self.dispatch(state, UiAction::FocusPane(pane_id));
        if let Some(error) = &state.last_error {
            return Err(error.clone());
        }
        Ok(())
    }

    pub fn activate_tab(
        &self,
        state: &mut UiState,
        pane_id: PaneId,
        tab_id: TabId,
    ) -> Result<(), String> {
        let workspace = state
            .session
            .active_workspace_mut()
            .ok_or_else(|| "no active workspace".to_string())?;
        workspace
            .activate_tab(pane_id, tab_id)
            .map_err(|err| format!("{err:?}"))?;
        state.last_error = None;
        state.push_activity("tab: activated");
        self.persist_session(state)?;
        Ok(())
    }

    pub fn close_tab(
        &self,
        state: &mut UiState,
        pane_id: PaneId,
        tab_id: TabId,
    ) -> Result<(), String> {
        let _ = self.dispatch(state, UiAction::CloseTab { pane_id, tab_id });
        if let Some(error) = &state.last_error {
            return Err(error.clone());
        }
        Ok(())
    }

    pub fn launch_agent(&self, state: &mut UiState, provider_id: &str) -> Result<(), String> {
        let workspace = state
            .session
            .active_workspace()
            .ok_or_else(|| "no active workspace".to_string())?
            .clone();
        if let Some((pane_id, tab_id)) = find_agent_tab(&workspace.layout, provider_id) {
            let active = state
                .session
                .active_workspace_mut()
                .ok_or_else(|| "no active workspace".to_string())?;
            active
                .activate_tab(pane_id, tab_id)
                .map_err(|err| format!("{err:?}"))?;
            self.persist_session(state)?;
            return Ok(());
        }
        let provider = self
            .registry
            .provider(provider_id)
            .ok_or_else(|| format!("unknown provider: {provider_id}"))?;
        let cwd = self.path_mapper.to_runtime_cwd(&workspace.target).ok();
        let launcher = AgentLauncher::new(self.terminal_backend.clone());
        let plan = launcher.plan(
            &provider,
            AgentLaunchRequest {
                provider_id: provider.id.clone(),
                mode: AgentLaunchMode::AttachedTerminal,
                target: workspace.target.clone(),
                cwd: cwd.clone(),
            },
        )?;
        let result = launcher.launch(plan)?;

        state.dispatch(UiAction::OpenSurface {
            pane_id: workspace.active_pane_id,
            surface: SurfaceState::Agent(AgentSurfaceState {
                surface_id: SurfaceId::new(format!("surface-agent-{}", provider.id)),
                session_id: Some(result.session_id),
                agent_instance_id: None,
                provider_id: provider.id,
                launch_mode: AgentLaunchMode::AttachedTerminal,
                cwd,
            }),
        });
        self.persist_session(state)?;
        Ok(())
    }

    pub fn open_file_in_active_workspace(
        &self,
        state: &mut UiState,
        relative_path: &str,
    ) -> Result<(), String> {
        let workspace = state
            .session
            .active_workspace()
            .ok_or_else(|| "no active workspace".to_string())?
            .clone();
        if let Some((pane_id, tab_id)) = find_editor_tab(&workspace.layout, relative_path) {
            let active = state
                .session
                .active_workspace_mut()
                .ok_or_else(|| "no active workspace".to_string())?;
            active
                .activate_tab(pane_id, tab_id)
                .map_err(|err| format!("{err:?}"))?;
            self.persist_session(state)?;
            return Ok(());
        }
        let fs = self.fs_backend.clone();
        let service = WorkspaceService::new(self.path_mapper.clone(), fs);
        let opened = service.open_file(&workspace.target, relative_path)?;

        state.dispatch(UiAction::OpenSurface {
            pane_id: workspace.active_pane_id,
            surface: SurfaceState::Editor(EditorSurfaceState {
                surface_id: SurfaceId::new(format!("surface-editor-{}", relative_path)),
                relative_path: opened.relative_path,
                language: language_for_path(relative_path),
                dirty: false,
                readonly: false,
            }),
        });
        self.persist_session(state)?;
        Ok(())
    }

    fn seed_demo_workspace_files(&self, state: &UiState) -> Result<(), String> {
        let Some(workspace) = state.session.active_workspace() else {
            return Ok(());
        };

        // For real fs mode, we skip seeding - files will be read from disk
        if self.is_using_real_fs() {
            return Ok(());
        }

        // For in-memory mode, seed demo files
        if let FsBackendWrapper::InMemory(memory_fs) = &self.fs_backend {
            memory_fs.add_dir(
                &workspace.target,
                "",
                vec![
                    FsEntry {
                        name: "src".into(),
                        relative_path: "src".into(),
                        is_dir: true,
                    },
                    FsEntry {
                        name: "README.md".into(),
                        relative_path: "README.md".into(),
                        is_dir: false,
                    },
                    FsEntry {
                        name: "notes.md".into(),
                        relative_path: "notes.md".into(),
                        is_dir: false,
                    },
                ],
            )?;
            let readme = self
                .path_mapper
                .map_file_for_editor(&workspace.target, "README.md")?;
            let notes = self
                .path_mapper
                .map_file_for_editor(&workspace.target, "notes.md")?;
            memory_fs.add_file(&readme, "# AMUX\n\nWindows-first AI workspace")?;
            memory_fs.add_file(&notes, "todo:\n- wire file tree into GPUI")?;
        }
        Ok(())
    }

    fn enrich_active_surface(
        &self,
        active_surface: Option<ActiveSurfaceItem>,
        target: &amux_core::WorkspaceTarget,
    ) -> Option<ActiveSurfaceItem> {
        let mut active_surface = active_surface?;

        match active_surface.surface_kind {
            "editor" => {
                if let Some(path) = extract_summary_value(&active_surface.summary_lines, "Path:") {
                    if let Ok(mapped) = self.path_mapper.map_file_for_editor(target, &path) {
                        if let Ok(content) = &self.fs_backend.read_to_string(&mapped) {
                            active_surface.content_lines = content_preview_lines(&content);
                        }
                    }
                }
            }
            "preview" => {
                if let Some(path) = extract_summary_value(&active_surface.summary_lines, "Source:")
                {
                    if let Ok(mapped) = self.path_mapper.map_file_for_editor(target, &path) {
                        if let Ok(content) = &self.fs_backend.read_to_string(&mapped) {
                            active_surface.content_lines = content_preview_lines(&content);
                        }
                    }
                }
            }
            "agent" | "terminal" => {
                if let Some(session_id) =
                    extract_summary_value(&active_surface.summary_lines, "Session:")
                {
                    if let Some(lines) = terminal_preview_lines(&self.terminal_backend, &session_id)
                    {
                        active_surface.content_lines = lines;
                    }
                } else if let Some(provider) =
                    extract_summary_value(&active_surface.summary_lines, "Provider:")
                {
                    active_surface.content_lines =
                        vec![format!("Status: attached to {provider} session")];
                }
            }
            _ => {}
        }

        Some(active_surface)
    }
}

fn default_session_dir(app_name: &str) -> PathBuf {
    // For the canonical app_name="AMUX" we want the same `~/.amux`
    // directory the rest of the desktop layer uses, so the controller
    // and the GPUI shell agree on a single config root and both honor
    // the `AMUX_HOME` override. For any other app_name (test fixtures
    // that pick a unique slug to keep their session files isolated),
    // fall back to `~/.{slug}` so each test gets its own dir without
    // touching `~/.amux/`.
    let slug = app_name.to_ascii_lowercase().replace(' ', "-");
    if slug == "amux" {
        return amux_platform::amux_home_dir();
    }
    if let Some(home) = amux_platform::real_user_home() {
        home.join(format!(".{slug}"))
    } else {
        std::env::temp_dir().join(format!("{slug}-session"))
    }
}

fn language_for_path(relative_path: &str) -> Option<String> {
    if relative_path.ends_with(".md") {
        Some("markdown".into())
    } else if relative_path.ends_with(".rs") {
        Some("rust".into())
    } else {
        None
    }
}

fn extract_summary_value(lines: &[String], prefix: &str) -> Option<String> {
    lines.iter().find_map(|line| {
        line.strip_prefix(prefix)
            .map(|value| value.trim().to_string())
    })
}

fn content_preview_lines(content: &str) -> Vec<String> {
    content
        .lines()
        .take(50)
        .map(|line| line.to_string())
        .collect()
}

fn terminal_preview_lines(
    backend: &TerminalBackendWrapper,
    session_id: &str,
) -> Option<Vec<String>> {
    let session_id = TerminalSessionId::new(session_id.to_string());
    let output_lines = backend.get_recent_output(&session_id, 14);

    if output_lines.is_empty() {
        return Some(vec![
            format!("Session: {}", session_id.0),
            "Recent IO: (none)".into(),
        ]);
    }

    let mut lines = vec![format!("Session: {}", session_id.0)];
    lines.push("Recent IO:".into());
    lines.extend(output_lines);
    Some(lines)
}

fn collect_open_files(layout: &amux_core::LayoutNode) -> Vec<OpenFileItem> {
    match layout {
        amux_core::LayoutNode::Pane(pane) => pane
            .tabs
            .iter()
            .filter_map(|tab| match &tab.surface {
                SurfaceState::Editor(editor) => Some(OpenFileItem {
                    relative_path: editor.relative_path.clone(),
                    display_path: editor.relative_path.clone(),
                    content_preview: format!(
                        "{}{}",
                        editor.language.clone().unwrap_or_else(|| "text".into()),
                        if editor.dirty { " (dirty)" } else { "" }
                    ),
                }),
                _ => None,
            })
            .collect(),
        amux_core::LayoutNode::Split(split) => {
            let mut files = collect_open_files(&split.first);
            files.extend(collect_open_files(&split.second));
            files
        }
    }
}

fn find_editor_tab(layout: &amux_core::LayoutNode, relative_path: &str) -> Option<(PaneId, TabId)> {
    match layout {
        amux_core::LayoutNode::Pane(pane) => pane.tabs.iter().find_map(|tab| match &tab.surface {
            SurfaceState::Editor(editor) if editor.relative_path == relative_path => {
                Some((pane.pane_id.clone(), tab.id.clone()))
            }
            _ => None,
        }),
        amux_core::LayoutNode::Split(split) => find_editor_tab(&split.first, relative_path)
            .or_else(|| find_editor_tab(&split.second, relative_path)),
    }
}

fn find_agent_tab(layout: &amux_core::LayoutNode, provider_id: &str) -> Option<(PaneId, TabId)> {
    match layout {
        amux_core::LayoutNode::Pane(pane) => pane.tabs.iter().find_map(|tab| match &tab.surface {
            SurfaceState::Agent(agent) if agent.provider_id == provider_id => {
                Some((pane.pane_id.clone(), tab.id.clone()))
            }
            _ => None,
        }),
        amux_core::LayoutNode::Split(split) => find_agent_tab(&split.first, provider_id)
            .or_else(|| find_agent_tab(&split.second, provider_id)),
    }
}

/// Heal any duplicate workspace ids left behind by the old
/// count-based `next_workspace_id`. Walks the workspace list in
/// order; the first occurrence of each id keeps it, subsequent
/// duplicates get renamed to a fresh `workspace-N` (where N is one
/// past the highest existing numeric suffix). Also fixes
/// `active_workspace_id` if it pointed at an index whose surviving
/// owner was one of the renamed duplicates.
///
/// Runs on every session load so users who already hit the bug
/// get their state automatically cleaned up on next launch,
/// without having to delete `session.json` by hand.
fn heal_duplicate_workspace_ids(session: &mut amux_core::SessionState) {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    // Collect the max numeric suffix across all current ids so we
    // can assign fresh ones that won't collide with anything.
    let mut max_n: usize = session
        .workspaces
        .iter()
        .filter_map(|ws| ws.id.0.strip_prefix("workspace-")?.parse::<usize>().ok())
        .max()
        .unwrap_or(0);

    for i in 0..session.workspaces.len() {
        let id = session.workspaces[i].id.0.clone();
        if seen.insert(id.clone()) {
            continue;
        }
        // Duplicate. Mint a fresh id and rewrite the workspace.
        max_n += 1;
        let fresh = format!("workspace-{}", max_n);
        // If the duplicate was the active one, repoint active at
        // the fresh id. (If the ACTIVE was the original — i.e. the
        // first occurrence — we leave active alone.)
        let was_active = session.active_workspace_id.as_ref().map(|w| w.0.as_str())
            == Some(&id);
        let is_second_or_later = i > 0 && session.workspaces[..i].iter().any(|w| w.id.0 == id);
        if was_active && is_second_or_later {
            // Ambiguous — the old id matched both the original and
            // this duplicate. Keep active pointing at the original
            // (which still has the old id), so this duplicate just
            // gets a fresh id and loses its "active" status.
        }
        session.workspaces[i].id = amux_core::WorkspaceId::new(&fresh);
        seen.insert(fresh);
    }
}

fn normalize_session_tabs(session: &mut amux_core::SessionState) {
    for workspace in &mut session.workspaces {
        let mut seen_editors = BTreeSet::new();
        let mut seen_agents = BTreeSet::new();
        normalize_layout_tabs(&mut workspace.layout, &mut seen_editors, &mut seen_agents);
    }
}

fn normalize_layout_tabs(
    layout: &mut amux_core::LayoutNode,
    seen_editors: &mut BTreeSet<String>,
    seen_agents: &mut BTreeSet<String>,
) {
    match layout {
        amux_core::LayoutNode::Pane(pane) => {
            let original_active = pane.active_tab_id.clone();
            let mut filtered = Vec::with_capacity(pane.tabs.len());
            for tab in pane.tabs.drain(..) {
                let keep = match &tab.surface {
                    SurfaceState::Editor(editor) => {
                        seen_editors.insert(editor.relative_path.clone())
                    }
                    SurfaceState::Agent(agent) => seen_agents.insert(agent.provider_id.clone()),
                    _ => true,
                };
                if keep {
                    filtered.push(tab);
                }
            }
            if filtered.is_empty() {
                pane.tabs = Vec::new();
                return;
            }
            pane.active_tab_id = if filtered.iter().any(|tab| tab.id == original_active) {
                original_active
            } else {
                filtered
                    .last()
                    .map(|tab| tab.id.clone())
                    .expect("filtered tabs should not be empty")
            };
            pane.tabs = filtered;
        }
        amux_core::LayoutNode::Split(split) => {
            normalize_layout_tabs(&mut split.first, seen_editors, seen_agents);
            normalize_layout_tabs(&mut split.second, seen_editors, seen_agents);
        }
    }
}
