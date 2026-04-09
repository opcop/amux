//! Terminal emulation module
//!
//! This module provides terminal backend (PTY) management and ANSI emulation.

pub mod backend;
pub mod keys;
pub mod manager;
pub mod alacritty_view;

// The following modules are dead code retained on disk for historical reference.
// They are excluded from the compile graph so they neither generate warnings nor
// rot alongside the live terminal stack. See developer-handoff.md §6.1.
//
// - `emulator`: original in-house ANSI parser / cell grid (TerminalEmulator).
//   Superseded by `alacritty_view::AlacrittyTerminal`, which wraps
//   `alacritty_terminal::Term` and is the only emulator the desktop now drives.
// - `view`: thin TerminalView wrapper around the in-house emulator. Already
//   marked "no longer used by desktop" in docs/HANDOFF-CANVAS-RENDERING.md.
// - `session`: parallel TerminalSessionManager + keyboard_to_pty implementation
//   whose only consumer was the (also unused) `gpui_terminal_component.rs`.
//
// pub mod emulator;
// pub mod view;
// pub mod session;

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
