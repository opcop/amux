//! Terminal output collector for capturing and managing PTY output.
//!
//! This module provides the infrastructure for collecting terminal output
//! from PTY sessions and making it available for UI rendering.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use amux_core::TerminalSessionId;

/// Maximum number of lines to keep in the output buffer
const MAX_OUTPUT_LINES: usize = 10000;

/// Maximum characters per line before wrapping
const MAX_LINE_WIDTH: usize = 500;

/// A single line of terminal output with metadata
#[derive(Clone, Debug)]
pub struct OutputLine {
    /// The text content of the line
    pub text: String,
    /// Whether this line contains user input (echo)
    pub is_input: bool,
    /// Timestamp when this line was added (milliseconds since session start)
    pub timestamp_ms: u64,
}

impl OutputLine {
    pub fn new(text: String, is_input: bool, timestamp_ms: u64) -> Self {
        Self {
            text,
            is_input,
            timestamp_ms,
        }
    }
}

/// Collected terminal output for a session
#[derive(Clone, Debug)]
pub struct TerminalOutput {
    /// Session ID this output belongs to
    pub session_id: TerminalSessionId,
    /// All output lines
    lines: Arc<Mutex<VecDeque<OutputLine>>>,
    /// Total bytes received
    total_bytes: Arc<Mutex<usize>>,
    /// Session start time (for timestamp calculation)
    start_time_ms: u64,
}

impl TerminalOutput {
    /// Create a new output collector for a session
    pub fn new(session_id: TerminalSessionId, start_time_ms: u64) -> Self {
        Self {
            session_id,
            lines: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_OUTPUT_LINES))),
            total_bytes: Arc::new(Mutex::new(0)),
            start_time_ms,
        }
    }

    /// Append raw output data to the terminal output
    pub fn append_raw(&self, data: &[u8], is_input: bool) {
        let timestamp_ms = current_time_ms() - self.start_time_ms;

        // Convert bytes to string, handling common encodings
        let text = decode_output(data);

        // Split into lines and process
        for line in text.lines() {
            self.append_line(line.to_string(), is_input, timestamp_ms);
        }

        // Update total byte count
        if let Ok(mut count) = self.total_bytes.lock() {
            *count += data.len();
        }
    }

    /// Append a single line to the output
    fn append_line(&self, text: String, is_input: bool, timestamp_ms: u64) {
        if let Ok(mut lines) = self.lines.lock() {
            // Trim trailing empty lines if this is also empty
            if text.is_empty() && lines.back().map(|l| l.text.is_empty()).unwrap_or(false) {
                return;
            }

            // Wrap long lines
            let wrapped_lines = wrap_text(&text, MAX_LINE_WIDTH);

            for line_text in wrapped_lines {
                if lines.len() >= MAX_OUTPUT_LINES {
                    lines.pop_front();
                }
                lines.push_back(OutputLine::new(line_text, is_input, timestamp_ms));
            }
        }
    }

    /// Get recent lines for display
    pub fn recent_lines(&self, count: usize) -> Vec<OutputLine> {
        let lines = match self.lines.lock() {
            Ok(l) => l,
            Err(_) => return Vec::new(),
        };

        let start = if lines.len() > count {
            lines.len() - count
        } else {
            0
        };

        lines.range(start..).cloned().collect()
    }

    /// Get all lines
    pub fn all_lines(&self) -> Vec<OutputLine> {
        match self.lines.lock() {
            Ok(lines) => lines.iter().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Get total bytes received
    pub fn total_bytes(&self) -> usize {
        match self.total_bytes.lock() {
            Ok(count) => *count,
            Err(_) => 0,
        }
    }

    /// Clear all output
    pub fn clear(&self) {
        if let Ok(mut lines) = self.lines.lock() {
            lines.clear();
        }
        if let Ok(mut count) = self.total_bytes.lock() {
            *count = 0;
        }
    }
}

/// Output manager that tracks output for all terminal sessions
#[derive(Clone, Default)]
pub struct TerminalOutputManager {
    outputs: Arc<Mutex<Vec<TerminalOutput>>>,
}

impl TerminalOutputManager {
    /// Create a new output manager
    pub fn new() -> Self {
        Self {
            outputs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register a new terminal session
    pub fn register_session(&self, session_id: TerminalSessionId) -> TerminalOutput {
        let output = TerminalOutput::new(session_id.clone(), current_time_ms());

        if let Ok(mut outputs) = self.outputs.lock() {
            // Remove any existing output for this session
            outputs.retain(|o| o.session_id != session_id);
            outputs.push(output.clone());
        }

        output
    }

    /// Get output for a session
    pub fn get_output(&self, session_id: &TerminalSessionId) -> Option<TerminalOutput> {
        match self.outputs.lock() {
            Ok(outputs) => outputs
                .iter()
                .find(|o| o.session_id == *session_id)
                .cloned(),
            Err(_) => None,
        }
    }

    /// Unregister a terminal session
    pub fn unregister_session(&self, session_id: &TerminalSessionId) {
        if let Ok(mut outputs) = self.outputs.lock() {
            outputs.retain(|o| o.session_id != *session_id);
        }
    }

    /// Get recent lines for a session
    pub fn get_recent_lines(
        &self,
        session_id: &TerminalSessionId,
        count: usize,
    ) -> Vec<OutputLine> {
        self.get_output(session_id)
            .map(|o| o.recent_lines(count))
            .unwrap_or_default()
    }
}

/// Decode raw bytes to string, handling common terminal encodings
fn decode_output(data: &[u8]) -> String {
    // Try UTF-8 first
    if let Ok(s) = std::str::from_utf8(data) {
        return s.to_string();
    }

    // Try lossy UTF-8
    String::from_utf8_lossy(data).to_string()
}

/// Wrap text to fit within a maximum width
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if text.len() <= max_width {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.len() + word.len() + 1 <= max_width {
            if !current_line.is_empty() {
                current_line.push(' ');
            }
            current_line.push_str(word);
        } else {
            if !current_line.is_empty() {
                lines.push(current_line.clone());
                current_line.clear();
            }
            // If a single word is longer than max_width, break it
            if word.len() > max_width {
                let mut remaining = word;
                while remaining.len() > max_width {
                    lines.push(remaining[..max_width].to_string());
                    remaining = &remaining[max_width..];
                }
                current_line = remaining.to_string();
            } else {
                current_line = word.to_string();
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Get current time in milliseconds
fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// PTY reader that continuously reads output and feeds it to an output collector
pub struct PtyReader {
    session_id: TerminalSessionId,
    output: TerminalOutput,
}

impl PtyReader {
    /// Create a new PTY reader
    pub fn new(session_id: TerminalSessionId, output: TerminalOutput) -> Self {
        Self { session_id, output }
    }

    /// Process raw PTY data
    pub fn process_data(&self, data: &[u8]) {
        self.output.append_raw(data, false);
    }

    /// Get the session ID
    pub fn session_id(&self) -> &TerminalSessionId {
        &self.session_id
    }

    /// Get recent output lines
    pub fn recent_lines(&self, count: usize) -> Vec<OutputLine> {
        self.output.recent_lines(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_utf8() {
        let data = b"Hello, World!";
        assert_eq!(decode_output(data), "Hello, World!");
    }

    #[test]
    fn test_wrap_text() {
        let text = "This is a very long line that needs to be wrapped";
        let lines = wrap_text(text, 20);
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(line.len() <= 20);
        }
    }

    #[test]
    fn test_output_collector() {
        let output = TerminalOutput::new(
            amux_core::TerminalSessionId::new("test-session"),
            0,
        );

        output.append_raw(b"Line 1\nLine 2\nLine 3", false);
        let lines = output.recent_lines(10);

        assert!(lines.len() >= 3);
    }
}
