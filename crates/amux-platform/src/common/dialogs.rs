//! Native folder picker, backed by `rfd` (Rusty File Dialog).
//!
//! `rfd` provides a single API that resolves to:
//!   - AppKit `NSOpenPanel` on macOS
//!   - Win32 `IFileDialog` on Windows
//!   - `xdg-desktop-portal` (FileChooser) on Linux
//!
//! This replaces the previous per-platform implementations that spawned
//! `powershell.exe`, `osascript`, or `zenity`/`kdialog`. Those subprocess
//! approaches were responsible for ~150-300ms of latency on every "Open
//! Workspace" click and only worked when the underlying tool was installed.
//!
//! All three platform adapters share `RfdWorkspaceDialogService`. The Linux
//! variant additionally exposes `is_available()` so capability gating can
//! check whether `xdg-desktop-portal` is reachable at runtime — on a
//! headless system or a desktop environment without portal support the
//! dialog will fail and we want to be honest about that to the UI.

use std::path::PathBuf;

use crate::services::WorkspaceDialogService;

#[derive(Clone, Debug, Default)]
pub struct RfdWorkspaceDialogService;

impl RfdWorkspaceDialogService {
    pub fn new() -> Self {
        Self
    }

    fn pick(&self) -> Result<Option<PathBuf>, String> {
        // rfd's blocking API is appropriate here: the workspace folder picker
        // is invoked from a synchronous user gesture (Ctrl+Shift+N, sidebar
        // button, command palette) and the UI is OK with blocking the input
        // thread for the duration of the dialog. The platform service trait
        // is sync today; we can revisit if we ever need a non-blocking flow.
        let result = rfd::FileDialog::new()
            .set_title("Select AMUX workspace folder")
            .pick_folder();
        Ok(result)
    }
}

impl WorkspaceDialogService for RfdWorkspaceDialogService {
    fn pick_folder(&self) -> Result<Option<PathBuf>, String> {
        self.pick()
    }
}

// Backwards-compatible aliases. The three platform adapters historically
// constructed differently-named services (`WindowsWorkspaceDialogService`,
// `MacosWorkspaceDialogService`, `LinuxWorkspaceDialogService`). Keeping
// the names as type aliases means platform.rs files don't need to change
// in lock-step with this refactor, and the names still document intent
// when read at the call site.
pub type WindowsWorkspaceDialogService = RfdWorkspaceDialogService;
pub type MacosWorkspaceDialogService = RfdWorkspaceDialogService;

#[derive(Clone, Debug, Default)]
pub struct LinuxWorkspaceDialogService {
    inner: RfdWorkspaceDialogService,
}

impl LinuxWorkspaceDialogService {
    /// Construct a Linux folder picker. Always returns `Some` now that
    /// `rfd`'s `xdg-portal` backend handles availability internally — if
    /// the portal isn't reachable at runtime, the eventual `pick_folder`
    /// call will return `Ok(None)` rather than panicking.
    pub fn new() -> Option<Self> {
        Some(Self {
            inner: RfdWorkspaceDialogService::new(),
        })
    }

    /// Whether a folder picker is available on this Linux host.
    ///
    /// We can't cheaply probe `xdg-desktop-portal` without doing IPC, so
    /// this conservatively returns `true`. The capability flag in
    /// `PlatformCapabilities::folder_picker` should still be honored —
    /// if the portal call fails, `pick_folder` returns `Ok(None)` and
    /// the UI should fall back to the command-palette `workspace open`
    /// path the way it does for Windows/macOS.
    pub fn is_available() -> bool {
        true
    }
}

impl WorkspaceDialogService for LinuxWorkspaceDialogService {
    fn pick_folder(&self) -> Result<Option<PathBuf>, String> {
        self.inner.pick_folder()
    }
}
