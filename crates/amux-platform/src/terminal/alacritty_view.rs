//! Alacritty-based terminal view
//!
//! Wraps `alacritty_terminal::Term` to provide a terminal emulator with full
//! VT100/xterm escape sequence support.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use alacritty_terminal::event::{Event as AlacrittyEvent, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty;

/// Event listener that bridges alacritty events to our system
#[derive(Clone)]
pub struct AmuEventProxy {
    pub title: Arc<Mutex<Option<String>>>,
    pub bell: Arc<std::sync::atomic::AtomicBool>,
    pub child_exited: Arc<std::sync::atomic::AtomicBool>,
    /// Set true when PTY has new output — cleared by `take_dirty()`
    pub dirty: Arc<std::sync::atomic::AtomicBool>,
}

impl EventListener for AmuEventProxy {
    fn send_event(&self, event: AlacrittyEvent) {
        match event {
            AlacrittyEvent::Title(title) => {
                if let Ok(mut t) = self.title.lock() {
                    *t = Some(title);
                }
            }
            AlacrittyEvent::Bell => {
                self.bell.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            AlacrittyEvent::Exit => {
                self.child_exited.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            AlacrittyEvent::Wakeup => {
                self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

/// Terminal size for alacritty
pub struct TermSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl Default for TermSize {
    fn default() -> Self {
        Self { cols: 120, rows: 40, cell_width: 8, cell_height: 20 }
    }
}

impl TermSize {
    fn to_window_size(&self) -> WindowSize {
        WindowSize {
            num_lines: self.rows,
            num_cols: self.cols,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
        }
    }
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize { self.rows as usize }
    fn screen_lines(&self) -> usize { self.rows as usize }
    fn columns(&self) -> usize { self.cols as usize }
}

/// Wrapper around alacritty_terminal
pub struct AlacrittyTerminal {
    term: Arc<FairMutex<Term<AmuEventProxy>>>,
    notifier: Notifier,
    event_proxy: AmuEventProxy,
    cols: u16,
    rows: u16,
    cell_width: u16,
    cell_height: u16,
    event_loop_handle: Option<JoinHandle<(EventLoop<tty::Pty, AmuEventProxy>, alacritty_terminal::event_loop::State)>>,
    /// Channel sender to signal shutdown to the event loop
    event_loop_sender: EventLoopSender,
    /// Child process PID (for reading /proc/PID/cwd on Linux)
    child_pid: Option<u32>,
}

impl Drop for AlacrittyTerminal {
    fn drop(&mut self) {
        // Signal the event loop to shut down. The thread will exit on its own
        // after processing the Shutdown message. We don't join here to avoid
        // blocking the UI thread if the event loop is stuck on a PTY read.
        let _ = self.event_loop_sender.send(Msg::Shutdown);
        // Detach the thread handle — it will clean up when it exits.
        drop(self.event_loop_handle.take());
    }
}

impl AlacrittyTerminal {
    /// Create and spawn a new terminal session
    pub fn new(
        cols: u16,
        rows: u16,
        cell_width: u16,
        cell_height: u16,
        shell: &str,
        args: &[String],
        cwd: Option<&str>,
    ) -> Result<Self, String> {
        let event_proxy = AmuEventProxy {
            title: Arc::new(Mutex::new(None)),
            bell: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            child_exited: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            dirty: Arc::new(std::sync::atomic::AtomicBool::new(true)), // dirty on creation
        };

        let size = TermSize { cols, rows, cell_width, cell_height };
        let config = TermConfig::default();
        let term = Term::new(config, &size, event_proxy.clone());
        let term = Arc::new(FairMutex::new(term));

        // Create PTY
        let mut env = HashMap::new();
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        env.insert("COLORTERM".to_string(), "truecolor".to_string());
        env.insert("TERM_PROGRAM".to_string(), "amux".to_string());
        // Inherit locale from parent or default to UTF-8
        if let Ok(lang) = std::env::var("LANG") {
            env.insert("LANG".to_string(), lang);
        } else {
            env.insert("LANG".to_string(), "en_US.UTF-8".to_string());
        }
        for var in &["LC_ALL", "LC_CTYPE", "LC_MESSAGES"] {
            if let Ok(val) = std::env::var(var) {
                env.insert(var.to_string(), val);
            }
        }
        // Set LS_COLORS without background colors — only foreground colors
        env.insert("LS_COLORS".to_string(),
            "di=1;34:ln=1;36:so=1;35:pi=33:ex=1;32:bd=1;33:cd=1;33:su=1;31:sg=1;33:tw=1;34:ow=1;34:*.tar=1;31:*.gz=1;31:*.zip=1;31:*.rpm=1;31:*.deb=1;31".to_string()
        );

        let pty_config = tty::Options {
            shell: Some(tty::Shell::new(shell.to_string(), args.to_vec())),
            working_directory: cwd.map(|s| std::path::PathBuf::from(s)),
            drain_on_exit: true,
            env,
            #[cfg(target_os = "windows")]
            escape_args: true,
        };

        let window_size = size.to_window_size();
        let pty = tty::new(&pty_config, window_size, 0)
            .map_err(|e| format!("failed to create PTY: {}", e))?;

        // Capture child PID before pty is moved into event loop
        #[cfg(not(target_os = "windows"))]
        let child_pid = Some(pty.child().id());
        #[cfg(target_os = "windows")]
        let child_pid: Option<u32> = None;

        // Spawn the event loop
        let event_loop = EventLoop::new(
            term.clone(),
            event_proxy.clone(),
            pty,
            pty_config.drain_on_exit,
            false,
        ).map_err(|e| format!("failed to create event loop: {}", e))?;

        let sender = event_loop.channel();
        let notifier = Notifier(sender.clone());
        let handle = event_loop.spawn();

        Ok(Self {
            term,
            notifier,
            event_proxy,
            cols,
            rows,
            cell_width,
            cell_height,
            event_loop_handle: Some(handle),
            event_loop_sender: sender,
            child_pid,
        })
    }

    /// Access the term for rendering (via callback to avoid exposing guard type)
    pub fn with_term<R>(&self, f: impl FnOnce(&Term<AmuEventProxy>) -> R) -> R {
        let term = self.term.lock_unfair();
        f(&term)
    }

    /// Access the term mutably
    pub fn with_term_mut<R>(&self, f: impl FnOnce(&mut Term<AmuEventProxy>) -> R) -> R {
        let mut term = self.term.lock();
        f(&mut term)
    }

    /// Send keyboard input to PTY
    pub fn send_input(&self, data: &[u8]) {
        self.notifier.notify(data.to_vec());
    }

    /// Send text to PTY, wrapping with bracketed paste escape sequences if the
    /// terminal has bracketed paste mode enabled.
    pub fn send_paste_input(&self, text: &str) {
        let bracketed = self.with_term(|t| {
            t.mode().contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE)
        });
        if bracketed {
            self.send_input(b"\x1b[200~");
        }
        self.send_input(text.as_bytes());
        if bracketed {
            self.send_input(b"\x1b[201~");
        }
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: u16, rows: u16) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;

        let window_size = WindowSize {
            num_lines: rows,
            num_cols: cols,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
        };

        let size = TermSize { cols, rows, cell_width: self.cell_width, cell_height: self.cell_height };
        let mut term = self.term.lock();
        term.resize(size);
        let _ = self.notifier.0.send(Msg::Resize(window_size));
    }

    /// Get terminal title
    pub fn title(&self) -> Option<String> {
        self.event_proxy.title.lock().ok().and_then(|t| t.clone())
    }

    /// Check and clear bell flag (visual bell support)
    pub fn take_bell(&self) -> bool {
        self.event_proxy.bell.swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    /// Check and clear dirty flag (true = PTY had new output since last check)
    pub fn take_dirty(&self) -> bool {
        self.event_proxy.dirty.swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    /// Check if child process has exited
    pub fn child_exited(&self) -> bool {
        self.event_proxy.child_exited.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the current working directory of the child process by reading /proc/PID/cwd.
    /// Returns None on Windows or if the process has exited.
    pub fn current_cwd(&self) -> Option<String> {
        #[cfg(not(target_os = "windows"))]
        {
            let pid = self.child_pid?;
            let link = std::fs::read_link(format!("/proc/{}/cwd", pid)).ok()?;
            Some(link.to_string_lossy().to_string())
        }
        #[cfg(target_os = "windows")]
        {
            None
        }
    }

    /// Get current dimensions
    pub fn dimensions(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// Scroll up
    pub fn scroll_up(&self, lines: usize) {
        let mut term = self.term.lock();
        term.scroll_display(alacritty_terminal::grid::Scroll::Delta(lines as i32));
    }

    /// Scroll down
    pub fn scroll_down(&self, lines: usize) {
        let mut term = self.term.lock();
        term.scroll_display(alacritty_terminal::grid::Scroll::Delta(-(lines as i32)));
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&self) {
        let mut term = self.term.lock();
        term.scroll_display(alacritty_terminal::grid::Scroll::Bottom);
    }

    /// Whether scrolled up
    pub fn is_scrolled(&self) -> bool {
        let term = self.term.lock_unfair();
        term.grid().display_offset() > 0
    }

    /// Read the last N non-empty lines from the terminal screen.
    /// Used for agent status detection (lightweight, no allocation for empty lines).
    pub fn last_lines(&self, n: usize) -> Vec<String> {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Line, Column};

        self.with_term(|t| {
            let grid = t.grid();
            let screen_lines = grid.screen_lines() as i32;
            let cols = grid.columns();
            let mut result = Vec::new();

            // Scan from bottom of screen upward, collecting non-empty lines
            for row_idx in (0..screen_lines).rev() {
                let line = Line(row_idx);
                let mut text = String::new();
                for col in 0..cols {
                    let cell = &grid[line][Column(col)];
                    if cell.c != ' ' && cell.c != '\0' {
                        // Extend to include this column
                        while text.len() < col {
                            text.push(' ');
                        }
                        text.push(cell.c);
                    }
                }
                let trimmed = text.trim_end().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                    if result.len() >= n {
                        break;
                    }
                }
            }
            result.reverse(); // return in top-to-bottom order
            result
        })
    }
}
