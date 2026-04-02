//! Terminal emulation module
//!
//! This module provides terminal backend (PTY) management and ANSI emulation.

pub mod backend;
pub mod emulator;
pub mod session;
pub mod view;
pub mod manager;
pub mod alacritty_view;

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

// Re-export emulator types
pub use emulator::{TerminalEmulator, Cell, Color, Cursor, DEFAULT_COLS, SCROLLBACK_LINES};

// Re-export terminal backend types
pub use backend::{
    TerminalBackend, TerminalLaunchSpec, 
    TerminalSessionMetadata, TerminalSessionKind, RealTerminalBackend, 
    InMemoryTerminalBackend, MockTerminalRecord,
};

// Re-export session types
pub use session::{TerminalSession, TerminalSessionManager, keyboard_to_pty};

// Re-export view types
pub use view::{TerminalView, keys};

// Re-export manager types
pub use manager::{TerminalManager, TabId, PaneId, PaneLayout, SplitDirection, TerminalPane};

// Re-export from amux_core
pub use amux_core::TerminalSessionId;
