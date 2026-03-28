//! Terminal Session Manager
//! 
//! Manages terminal sessions and connects PTY backend to emulator.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use amux_core::{ShellKind, TerminalLaunchProfile, TerminalSessionId, WorkspaceTarget};

use crate::terminal::backend::{RealTerminalBackend, TerminalBackend};
use crate::terminal::emulator::TerminalEmulator;

/// A single terminal session that combines PTY backend with emulator
pub struct TerminalSession {
    /// Terminal emulator for rendering
    emulator: TerminalEmulator,
    /// Terminal session ID from backend
    session_id: TerminalSessionId,
    /// Is this session still active
    active: bool,
}

impl TerminalSession {
    /// Create a new terminal session
    pub fn new(session_id: TerminalSessionId, emulator: TerminalEmulator) -> Self {
        Self {
            emulator,
            session_id,
            active: true,
        }
    }

    /// Feed data from PTY to emulator
    pub fn feed(&mut self, data: &[u8]) {
        self.emulator.feed(data);
    }

    /// Get a reference to the emulator
    pub fn emulator(&self) -> &TerminalEmulator {
        &self.emulator
    }

    /// Get a mutable reference to the emulator
    pub fn emulator_mut(&mut self) -> &mut TerminalEmulator {
        &mut self.emulator
    }

    /// Get the session ID
    pub fn session_id(&self) -> &TerminalSessionId {
        &self.session_id
    }

    /// Check if session is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Mark session as inactive
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.emulator.resize(cols, rows);
    }
}

/// Terminal session manager that handles multiple sessions
pub struct TerminalSessionManager {
    /// Backend for actual terminal I/O
    backend: RealTerminalBackend,
    /// Active sessions
    sessions: HashMap<TerminalSessionId, TerminalSession>,
    /// Reader handle for polling output
    reader_handle: Option<thread::JoinHandle<()>>,
    /// Control flag for reader thread
    should_run: Arc<Mutex<bool>>,
}

impl TerminalSessionManager {
    /// Create a new terminal session manager
    pub fn new() -> Self {
        Self {
            backend: RealTerminalBackend::new(),
            sessions: HashMap::new(),
            reader_handle: None,
            should_run: Arc::new(Mutex::new(false)),
        }
    }

    /// Create a new terminal session with the given profile
    pub fn create_session(&mut self, profile: TerminalLaunchProfile) -> Result<TerminalSessionId, String> {
        let session_id = self.backend.create_session(profile)?;
        
        let emulator = TerminalEmulator::with_size(80, 24);
        let session = TerminalSession::new(session_id.clone(), emulator);
        
        self.sessions.insert(session_id.clone(), session);
        
        // Start reader thread if not running
        self.start_reader();
        
        Ok(session_id)
    }

    /// Write input to a terminal session
    pub fn write_input(&self, session_id: &TerminalSessionId, data: &[u8]) -> Result<(), String> {
        self.backend.write_input(session_id, data)
    }

    /// Resize a terminal session
    pub fn resize(&mut self, session_id: &TerminalSessionId, cols: u16, rows: u16) -> Result<(), String> {
        self.backend.resize(session_id, cols, rows)?;
        
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.resize(cols as usize, rows as usize);
        }
        
        Ok(())
    }

    /// Kill a terminal session
    pub fn kill(&mut self, session_id: &TerminalSessionId) -> Result<(), String> {
        self.backend.kill(session_id)?;
        self.sessions.remove(session_id);
        Ok(())
    }

    /// Get a session by ID
    pub fn get(&self, session_id: &TerminalSessionId) -> Option<&TerminalSession> {
        self.sessions.get(session_id)
    }

    /// Get a mutable session by ID
    pub fn get_mut(&mut self, session_id: &TerminalSessionId) -> Option<&mut TerminalSession> {
        self.sessions.get_mut(session_id)
    }

    /// Get all active session IDs
    pub fn session_ids(&self) -> Vec<TerminalSessionId> {
        self.sessions.keys().cloned().collect()
    }

    /// Start the output reader thread
    fn start_reader(&mut self) {
        // Check if already running
        {
            let should_run = self.should_run.lock().unwrap();
            if *should_run {
                return;
            }
        }

        let backend = self.backend.clone();
        let sessions = Arc::new(Mutex::new(HashMap::<TerminalSessionId, ()>::new()));
        let should_run = Arc::clone(&self.should_run);
        
        // Mark as running
        {
            let mut sr = should_run.lock().unwrap();
            *sr = true;
        }

        let reader_sessions = Arc::clone(&sessions);
        self.reader_handle = Some(thread::spawn(move || {
            let mut buf = [0u8; 4096];
            
            loop {
                // Check if we should stop
                {
                    let should_run = should_run.lock().unwrap();
                    if !*should_run {
                        break;
                    }
                }
                
                // Poll each session for output
                // Note: In a real implementation, we would use async I/O
                // For now, we just sleep and let the emulator receive data
                thread::sleep(Duration::from_millis(16));
            }
        }));
    }

    /// Stop the reader thread
    pub fn stop_reader(&mut self) {
        {
            let mut should_run = self.should_run.lock().unwrap();
            *should_run = false;
        }
        
        if let Some(handle) = self.reader_handle.take() {
            let _ = handle.join();
        }
    }

    /// Feed output data to a session
    /// This is called by the event loop when output is available
    pub fn feed_output(&mut self, session_id: &TerminalSessionId, data: &[u8]) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.feed(data);
        }
    }

    /// Poll for output from a session
    pub fn poll_output(&mut self, session_id: &TerminalSessionId) -> Result<Vec<u8>, String> {
        let mut buf = [0u8; 4096];
        let mut all_data = Vec::new();
        
        loop {
            match self.backend.read_output(session_id, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    all_data.extend_from_slice(&buf[..n]);
                    if n < buf.len() {
                        break;
                    }
                }
                Err(e) => return Err(e),
            }
        }
        
        // Feed to emulator
        if !all_data.is_empty() {
            self.feed_output(session_id, &all_data);
        }
        
        Ok(all_data)
    }
}

impl Default for TerminalSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TerminalSessionManager {
    fn drop(&mut self) {
        self.stop_reader();
    }
}

/// Convert keyboard events to PTY input bytes
pub fn keyboard_to_pty(key: &str, ctrl: bool, shift: bool, alt: bool) -> Vec<u8> {
    // Handle special keys first
    match key {
        "Enter" => return vec![0x0D],
        "Tab" => return vec![0x09],
        "Escape" => return vec![0x1B],
        "Backspace" => return vec![0x7F],
        "ArrowUp" => return escape_sequence("A", ctrl, shift, alt),
        "ArrowDown" => return escape_sequence("B", ctrl, shift, alt),
        "ArrowRight" => return escape_sequence("C", ctrl, shift, alt),
        "ArrowLeft" => return escape_sequence("D", ctrl, shift, alt),
        "Home" => return escape_sequence("H", ctrl, shift, alt),
        "End" => return escape_sequence("F", ctrl, shift, alt),
        "PageUp" => return escape_sequence("5~", ctrl, shift, alt),
        "PageDown" => return escape_sequence("6~", ctrl, shift, alt),
        "Insert" => return escape_sequence("2~", ctrl, shift, alt),
        "Delete" => return escape_sequence("3~", ctrl, shift, alt),
        "F1" => return escape_sequence("OP", ctrl, shift, alt),
        "F2" => return escape_sequence("OQ", ctrl, shift, alt),
        "F3" => return escape_sequence("OR", ctrl, shift, alt),
        "F4" => return escape_sequence("OS", ctrl, shift, alt),
        "F5" => return escape_sequence("[15~", ctrl, shift, alt),
        "F6" => return escape_sequence("[17~", ctrl, shift, alt),
        "F7" => return escape_sequence("[18~", ctrl, shift, alt),
        "F8" => return escape_sequence("[19~", ctrl, shift, alt),
        "F9" => return escape_sequence("[20~", ctrl, shift, alt),
        "F10" => return escape_sequence("[21~", ctrl, shift, alt),
        "F11" => return escape_sequence("[23~", ctrl, shift, alt),
        "F12" => return escape_sequence("[24~", ctrl, shift, alt),
        _ => {}
    }

    // Handle regular characters
    let mut result = Vec::new();
    
    // Apply modifiers for control characters
    if ctrl && key.len() == 1 {
        if let Some(c) = key.chars().next() {
            if c.is_ascii_alphabetic() {
                // Ctrl+A = 1, Ctrl+B = 2, etc.
                let ctrl_char = (c.to_ascii_uppercase() as u8) - b'A' + 1;
                return vec![ctrl_char];
            } else if c == '[' {
                return vec![0x1B]; // Ctrl+[ = Escape
            } else if c == '\\' {
                return vec![0x1C]; // Ctrl+\ = FS
            } else if c == ']' {
                return vec![0x1D]; // Ctrl+] = GS
            } else if c == '^' {
                return vec![0x1E]; // Ctrl+^ = RS
            } else if c == '_' {
                return vec![0x1F]; // Ctrl+_ = US
            }
        }
    }

    // Handle alt modifier
    if alt {
        result.push(0x1B);
    }
    
    // Handle shift for uppercase
    if shift && key.len() == 1 {
        if let Some(c) = key.chars().next() {
            if c.is_ascii_lowercase() {
                result.push(c.to_ascii_uppercase() as u8);
                return result;
            }
        }
    }
    
    // Regular character
    for c in key.chars() {
        let mut buf = [0u8; 4];
        let encoded = c.encode_utf8(&mut buf);
        result.extend_from_slice(encoded.as_bytes());
    }
    
    result
}

/// Generate escape sequence for special keys
fn escape_sequence(suffix: &str, ctrl: bool, _shift: bool, alt: bool) -> Vec<u8> {
    let mut result = Vec::new();
    
    if alt {
        result.push(0x1B);
    }
    
    result.push(0x1B);
    result.push(b'[');
    
    // Add modifier prefix for Ctrl
    if ctrl {
        result.push(b'1');
        result.push(b';');
        result.push(b'5'); // Ctrl modifier
    }
    
    result.extend_from_slice(suffix.as_bytes());
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyboard_enter() {
        let result = keyboard_to_pty("Enter", false, false, false);
        assert_eq!(result, vec![0x0D]);
    }

    #[test]
    fn test_keyboard_escape() {
        let result = keyboard_to_pty("Escape", false, false, false);
        assert_eq!(result, vec![0x1B]);
    }

    #[test]
    fn test_keyboard_ctrl_c() {
        let result = keyboard_to_pty("c", true, false, false);
        assert_eq!(result, vec![0x03]); // Ctrl+C
    }

    #[test]
    fn test_keyboard_arrow_up() {
        let result = keyboard_to_pty("ArrowUp", false, false, false);
        assert_eq!(result, vec![0x1B, 0x5B, 0x41]); // ESC[A
    }
}
