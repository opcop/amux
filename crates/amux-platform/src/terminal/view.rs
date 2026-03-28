//! Terminal View - connects PTY backend to emulator
//!
//! This module provides a complete terminal view that:
//! - Manages a PTY session
//! - Parses ANSI output into a cell grid
//! - Handles keyboard input

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use amux_core::{ShellKind, TerminalLaunchProfile, TerminalSessionId, WorkspaceTarget};

use crate::terminal::backend::{RealTerminalBackend, TerminalBackend};
use crate::terminal::emulator::TerminalEmulator;

/// Callback type for terminal output
pub type OutputCallback = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// A terminal view that combines PTY backend with emulator
pub struct TerminalView {
    /// Terminal emulator for rendering
    emulator: TerminalEmulator,
    /// PTY backend
    backend: RealTerminalBackend,
    /// PTY session ID
    session_id: Option<TerminalSessionId>,
    /// Is this terminal active
    active: bool,
    /// Reader thread handle
    reader_handle: Option<thread::JoinHandle<()>>,
    /// Should reader continue
    should_run: Arc<Mutex<bool>>,
    /// Output callback (optional, for external consumers)
    output_callback: Option<OutputCallback>,
    /// Pending output buffer — reader thread writes, main thread drains to emulator
    pending_output: Arc<Mutex<Vec<u8>>>,
}

impl TerminalView {
    /// Create a new terminal view with default size
    pub fn new() -> Self {
        Self {
            emulator: TerminalEmulator::new(),
            backend: RealTerminalBackend::new(),
            session_id: None,
            active: false,
            reader_handle: None,
            should_run: Arc::new(Mutex::new(false)),
            output_callback: None,
            pending_output: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create a new terminal view with specific size
    pub fn with_size(cols: usize, rows: usize) -> Self {
        Self {
            emulator: TerminalEmulator::with_size(cols, rows),
            backend: RealTerminalBackend::new(),
            session_id: None,
            active: false,
            reader_handle: None,
            should_run: Arc::new(Mutex::new(false)),
            output_callback: None,
            pending_output: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Spawn a new PTY session with the given profile
    pub fn spawn(&mut self, profile: TerminalLaunchProfile) -> Result<(), String> {
        // Stop any existing reader
        self.stop_reader();

        // Kill existing session
        if let Some(ref sid) = self.session_id {
            let _ = self.backend.kill(sid);
        }

        // Create session
        let session_id = self.backend.create_session(profile)?;
        self.session_id = Some(session_id.clone());

        // Start reader thread
        self.start_reader();

        self.active = true;
        Ok(())
    }

    /// Spawn with Windows/WSL default profile
    pub fn spawn_default(&mut self, target: WorkspaceTarget) -> Result<(), String> {
        let profile = TerminalLaunchProfile {
            target,
            shell: ShellKind::PowerShell,
            cwd: None,
            env: std::collections::BTreeMap::new(),
            title: Some("Terminal".to_string()),
        };
        self.spawn(profile)
    }

    /// Start the reader thread that polls PTY output
    fn start_reader(&mut self) {
        let should_run = Arc::clone(&self.should_run);
        let backend = self.backend.clone();
        let session_id = self.session_id.clone().unwrap();
        let callback = self.output_callback.clone();
        let pending = Arc::clone(&self.pending_output);

        // Mark as running
        {
            let mut sr = self.should_run.lock().unwrap();
            *sr = true;
        }

        self.reader_handle = Some(thread::spawn(move || {
            let mut buf = [0u8; 8192];

            while *should_run.lock().unwrap() {
                // Try to read from PTY
                match backend.read_output(&session_id, &mut buf) {
                    Ok(0) => {
                        // No data, sleep briefly
                        thread::sleep(Duration::from_millis(16));
                    }
                    Ok(n) => {
                        let data = &buf[..n];

                        // Append to pending buffer for main thread to drain
                        if let Ok(mut pending) = pending.lock() {
                            pending.extend_from_slice(data);
                        }

                        // Also notify external callback if set
                        if let Some(ref cb) = callback {
                            cb(data);
                        }
                    }
                    Err(_) => {
                        // Session ended or error
                        break;
                    }
                }
            }
        }));
    }

    /// Stop the reader thread
    pub fn stop_reader(&mut self) {
        // Signal stop
        {
            let mut sr = self.should_run.lock().unwrap();
            *sr = false;
        }

        // Wait for thread to finish
        if let Some(handle) = self.reader_handle.take() {
            let _ = handle.join();
        }
    }

    /// Drain pending PTY output into the emulator.
    /// Call this from the main/render thread before reading the grid.
    /// Returns true if any new data was processed.
    pub fn poll(&mut self) -> bool {
        let data = {
            let mut pending = self.pending_output.lock().unwrap();
            if pending.is_empty() {
                return false;
            }
            std::mem::take(&mut *pending)
        };
        self.emulator.feed(&data);
        true
    }

    /// Check if there is pending output waiting
    pub fn has_pending_output(&self) -> bool {
        self.pending_output.lock().map(|p| !p.is_empty()).unwrap_or(false)
    }

    /// Feed data to the emulator directly (for local echo / testing)
    pub fn feed(&mut self, data: &[u8]) {
        self.emulator.feed(data);
    }

    /// Feed keyboard input to the terminal PTY
    pub fn send_input(&self, data: &[u8]) -> Result<(), String> {
        if let Some(ref session_id) = self.session_id {
            self.backend.write_input(session_id, data)
        } else {
            Err("No active session".to_string())
        }
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: usize, rows: usize) -> Result<(), String> {
        self.emulator.resize(cols, rows);

        if let Some(ref session_id) = self.session_id {
            self.backend.resize(session_id, cols as u16, rows as u16)?;
        }

        Ok(())
    }

    /// Get the emulator
    pub fn emulator(&self) -> &TerminalEmulator {
        &self.emulator
    }

    /// Get mutable emulator
    pub fn emulator_mut(&mut self) -> &mut TerminalEmulator {
        &mut self.emulator
    }

    /// Get cursor position
    pub fn cursor(&self) -> &crate::terminal::emulator::Cursor {
        self.emulator.cursor()
    }

    /// Check if terminal is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Kill the terminal session
    pub fn kill(&mut self) -> Result<(), String> {
        self.stop_reader();

        if let Some(ref session_id) = self.session_id {
            self.backend.kill(session_id)?;
        }

        self.session_id = None;
        self.active = false;
        Ok(())
    }

    /// Set output callback (called from reader thread)
    pub fn set_output_callback<F>(&mut self, callback: F)
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        self.output_callback = Some(Arc::new(callback));
    }

    /// Clear the terminal
    pub fn clear(&mut self) {
        self.emulator.feed(b"\x1b[2J\x1b[H");
    }

    /// Send text to the terminal (for copy/paste)
    pub fn send_text(&self, text: &str) -> Result<(), String> {
        self.send_input(text.as_bytes())
    }
}

impl Default for TerminalView {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TerminalView {
    fn drop(&mut self) {
        self.stop_reader();
        let _ = self.kill();
    }
}

/// Keyboard input handler
pub mod keys {
    /// Convert keyboard event to PTY input bytes
    pub fn to_pty(key: &str, ctrl: bool, shift: bool, alt: bool) -> Vec<u8> {
        // Special keys
        match key {
            "Enter" => return vec![0x0D],
            "Tab" => return vec![0x09],
            "Escape" => return vec![0x1B],
            "Backspace" => return vec![0x7F],
            "ArrowUp" => return escape_seq("A", ctrl, shift, alt),
            "ArrowDown" => return escape_seq("B", ctrl, shift, alt),
            "ArrowRight" => return escape_seq("C", ctrl, shift, alt),
            "ArrowLeft" => return escape_seq("D", ctrl, shift, alt),
            "Home" => return escape_seq("H", ctrl, shift, alt),
            "End" => return escape_seq("F", ctrl, shift, alt),
            "PageUp" => return escape_seq("5~", ctrl, shift, alt),
            "PageDown" => return escape_seq("6~", ctrl, shift, alt),
            "Insert" => return escape_seq("2~", ctrl, shift, alt),
            "Delete" => return escape_seq("3~", ctrl, shift, alt),
            "F1" => return vec![0x1B, 0x4F, 0x50],
            "F2" => return vec![0x1B, 0x4F, 0x51],
            "F3" => return vec![0x1B, 0x4F, 0x52],
            "F4" => return vec![0x1B, 0x4F, 0x53],
            "F5" => return vec![0x1B, 0x5B, 0x31, 0x35, 0x7E],
            "F6" => return vec![0x1B, 0x5B, 0x31, 0x37, 0x7E],
            "F7" => return vec![0x1B, 0x5B, 0x31, 0x38, 0x7E],
            "F8" => return vec![0x1B, 0x5B, 0x31, 0x39, 0x7E],
            "F9" => return vec![0x1B, 0x5B, 0x32, 0x30, 0x7E],
            "F10" => return vec![0x1B, 0x5B, 0x32, 0x31, 0x7E],
            "F11" => return vec![0x1B, 0x5B, 0x32, 0x33, 0x7E],
            "F12" => return vec![0x1B, 0x5B, 0x32, 0x34, 0x7E],
            _ => {}
        }

        // Control characters
        if ctrl && key.len() == 1 {
            if let Some(c) = key.chars().next() {
                if c.is_ascii_alphabetic() {
                    let ctrl_char = (c.to_ascii_uppercase() as u8) - b'A' + 1;
                    return vec![ctrl_char];
                }
                match c {
                    '[' => return vec![0x1B],
                    '\\' => return vec![0x1C],
                    ']' => return vec![0x1D],
                    '^' => return vec![0x1E],
                    '_' => return vec![0x1F],
                    _ => {}
                }
            }
        }

        // Alt modifier
        let mut result = Vec::new();
        if alt {
            result.push(0x1B);
        }

        // Handle shift for special characters
        if shift && key == "Space" {
            result.push(b' ');
            return result;
        }

        // Regular characters
        if key == "Space" {
            result.push(b' ');
            return result;
        }

        for c in key.chars() {
            if c == ' ' {
                result.push(b' ');
            } else if c.is_ascii() {
                let byte = if shift && c.is_ascii_lowercase() {
                    c.to_ascii_uppercase() as u8
                } else {
                    c as u8
                };
                result.push(byte);
            } else {
                // UTF-8
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                result.extend_from_slice(encoded.as_bytes());
            }
        }

        result
    }

    fn escape_seq(suffix: &str, ctrl: bool, _shift: bool, alt: bool) -> Vec<u8> {
        let mut result = Vec::new();

        if alt {
            result.push(0x1B);
        }

        result.push(0x1B);
        result.push(b'[');

        if ctrl {
            result.push(b'1');
            result.push(b';');
            result.push(b'5');
        }

        result.extend_from_slice(suffix.as_bytes());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enter() {
        assert_eq!(keys::to_pty("Enter", false, false, false), vec![0x0D]);
    }

    #[test]
    fn test_ctrl_c() {
        assert_eq!(keys::to_pty("c", true, false, false), vec![0x03]);
    }

    #[test]
    fn test_arrow_up() {
        assert_eq!(keys::to_pty("ArrowUp", false, false, false), vec![0x1B, 0x5B, 0x41]);
    }

    #[test]
    fn test_poll_drains_pending_output() {
        let mut view = TerminalView::new();
        // Simulate reader thread writing to pending buffer
        {
            let mut pending = view.pending_output.lock().unwrap();
            pending.extend_from_slice(b"Hello");
        }
        assert!(view.has_pending_output());
        assert!(view.poll());
        assert!(!view.has_pending_output());
        // Emulator should now have 'H' at position 0
        assert_eq!(view.emulator().grid()[0][0].ch, 'H');
    }
}
