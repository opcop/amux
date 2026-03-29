//! Terminal emulation module
//! 
//! This module provides terminal backend (PTY) management and ANSI emulation.

pub mod backend;
pub mod emulator;
pub mod session;
pub mod view;
pub mod manager;
pub mod alacritty_view;

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
