//! Terminal emulation module
//!
//! This module provides terminal backend (PTY) management and ANSI emulation.

pub mod backend;
pub mod keys;
pub mod manager;
pub mod alacritty_view;
pub mod osc_intercept;

// The live terminal stack is `alacritty_view::AlacrittyTerminal` (wraps
// `alacritty_terminal::Term`) plus `backend` for PTY spawn and `manager`
// for the pane/tab tree. The old in-house ANSI parser and its
// TerminalView / TerminalSessionManager wrappers were removed together
// with the unused `gpui_terminal_component.rs` desktop module.

/// Query the current working directory of a process by PID.
/// On Windows, uses sysinfo (which reads the PEB via NtQueryInformationProcess).
/// On Linux, reads /proc/PID/cwd (but the caller already handles that path).
#[cfg(target_os = "windows")]
pub fn win_process_cwd(pid: u32) -> Option<String> {
    use sysinfo::{System, Pid, ProcessRefreshKind, UpdateKind, RefreshKind, ProcessesToUpdate};

    let refresh = ProcessRefreshKind::new().with_cwd(UpdateKind::Always);
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(refresh)
    );
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        true,
        refresh,
    );
    let proc = sys.process(Pid::from_u32(pid))?;
    let cwd = proc.cwd()?;
    Some(cwd.to_string_lossy().to_string())
}

// Re-export terminal backend types
pub use backend::{
    TerminalBackend, TerminalLaunchSpec,
    TerminalSessionMetadata, TerminalSessionKind, RealTerminalBackend,
    InMemoryTerminalBackend, MockTerminalRecord,
};

// Re-export manager types
pub use manager::{TerminalManager, TabId, PaneId, PaneLayout, SplitDirection, TerminalPane};

// Re-export from amux_core
pub use amux_core::TerminalSessionId;
