//! Alacritty-based terminal view
//!
//! Wraps `alacritty_terminal::Term` to provide a terminal emulator with full
//! VT100/xterm escape sequence support.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver as StdReceiver, Sender as StdSender, channel as std_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use alacritty_terminal::event::{Event as AlacrittyEvent, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty::{self, ChildEvent, EventedPty, EventedReadWrite};

use crate::terminal::osc_intercept::{OscEvent, OscInterceptor};

/// Event listener that bridges alacritty events to our system
#[derive(Clone)]
pub struct AmuEventProxy {
    pub title: Arc<Mutex<Option<String>>>,
    pub bell: Arc<std::sync::atomic::AtomicBool>,
    pub child_exited: Arc<std::sync::atomic::AtomicBool>,
    /// Set true when PTY has new output — cleared by `take_dirty()`
    pub dirty: Arc<std::sync::atomic::AtomicBool>,
    /// Set true when OSC 0/2 sets a new window title — cleared by
    /// `take_title_changed()`. Used as a proxy for "the shell just
    /// printed a new prompt" to trigger CWD cache refresh.
    pub title_changed: Arc<std::sync::atomic::AtomicBool>,
    /// Sender cloned into the `FilterPty` reader. Each OSC 7 / 133
    /// sequence the interceptor extracts from the PTY stream is
    /// pushed here. The main thread drains via `take_osc_events()`.
    ///
    /// `Arc<Mutex<Sender>>` rather than a bare `Sender` because
    /// `AmuEventProxy` is `Clone` (alacritty needs it cheap-to-clone
    /// for internal plumbing) and `Sender` is already cheaply clonable
    /// but we want the option of replacing the channel if we ever
    /// need to (e.g. in tests). Wrapping is cheap.
    pub osc_event_tx: StdSender<OscEvent>,
}

impl EventListener for AmuEventProxy {
    fn send_event(&self, event: AlacrittyEvent) {
        match event {
            AlacrittyEvent::Title(title) => {
                if let Ok(mut t) = self.title.lock() {
                    *t = Some(title);
                }
                self.title_changed.store(true, std::sync::atomic::Ordering::Relaxed);
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
    /// Receiver for OSC events produced by the `FilterPty` reader.
    /// Drained on every `take_osc_events` call from the main thread.
    /// On Windows the filter path is skipped, so this receiver never
    /// produces any events (interceptor is Unix-only for now).
    osc_event_rx: StdReceiver<OscEvent>,
    // On Unix the event loop handle's pty type is FilterPty; on
    // Windows we fall through to raw tty::Pty. Boxing lets both
    // variants share a single struct field without a platform-
    // specific Self type. The Box erases the pty type to keep the
    // field signature identical across platforms.
    #[cfg(unix)]
    event_loop_handle: Option<JoinHandle<(EventLoop<FilterPty, AmuEventProxy>, alacritty_terminal::event_loop::State)>>,
    #[cfg(not(unix))]
    event_loop_handle: Option<JoinHandle<(EventLoop<tty::Pty, AmuEventProxy>, alacritty_terminal::event_loop::State)>>,
    /// Channel sender to signal shutdown to the event loop
    event_loop_sender: EventLoopSender,
    /// Child process PID (for reading /proc/PID/cwd on Linux)
    child_pid: Option<u32>,
}

impl Drop for AlacrittyTerminal {
    fn drop(&mut self) {
        // First kill the child process to ensure the PTY read unblocks.
        // Without this, the event loop thread can get stuck on a blocking
        // PTY read, causing orphaned child processes and leaked file
        // descriptors when terminals are closed rapidly.
        self.kill_child();
        // Then signal the event loop to shut down.
        let _ = self.event_loop_sender.send(Msg::Shutdown);
        // Give the thread a moment to exit cleanly before dropping the
        // handle. Use a short sleep rather than join, to avoid blocking
        // the UI thread if the event loop is slow to respond.
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
        Self::with_scrollback(cols, rows, cell_width, cell_height, shell, args, cwd, 10000, &HashMap::new())
    }

    /// Create with custom scrollback size
    pub fn with_scrollback(
        cols: u16,
        rows: u16,
        cell_width: u16,
        cell_height: u16,
        shell: &str,
        args: &[String],
        cwd: Option<&str>,
        scrollback_lines: usize,
        extra_env: &HashMap<String, String>,
    ) -> Result<Self, String> {
        // OSC event channel: FilterPty's reader pushes events to the
        // sender stored on AmuEventProxy; AlacrittyTerminal holds the
        // receiver and drains via take_osc_events(). Unbounded —
        // realistic OSC rates are small (≤1 per prompt cycle) so
        // backpressure isn't a concern, and we must never block the
        // reader thread (spec §9 "Never").
        let (osc_event_tx, osc_event_rx) = std_channel::<OscEvent>();

        let event_proxy = AmuEventProxy {
            title: Arc::new(Mutex::new(None)),
            bell: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            child_exited: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            dirty: Arc::new(std::sync::atomic::AtomicBool::new(true)), // dirty on creation
            title_changed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            osc_event_tx,
        };

        let size = TermSize { cols, rows, cell_width, cell_height };
        let mut config = TermConfig::default();
        config.scrolling_history = scrollback_lines;
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
        // Pass terminal env vars through to WSL sessions via WSLENV.
        // Append to existing WSLENV if set, so user values aren't lost.
        let wslenv_extra = "LS_COLORS:TERM:COLORTERM:TERM_PROGRAM:AMUX:AMUX_PANE_ID:AMUX_WORKSPACE:AMUX_VERSION";
        let wslenv = match std::env::var("WSLENV") {
            Ok(existing) if !existing.is_empty() => format!("{}:{}", existing, wslenv_extra),
            _ => wslenv_extra.to_string(),
        };
        env.insert("WSLENV".to_string(), wslenv);

        // Inject AMUX_* environment variables for agent bridge.
        // AMUX=1 prevents nested multiplexer instances.
        // AMUX_SOCKET_PATH enables external tools to send notifications.
        env.insert("AMUX".to_string(), "1".to_string());
        env.insert("AMUX_VERSION".to_string(), env!("CARGO_PKG_VERSION").to_string());
        let socket_path = crate::socket_notify::socket_path();
        env.insert("AMUX_SOCKET_PATH".to_string(), socket_path.to_string_lossy().to_string());
        for (k, v) in extra_env {
            env.insert(k.clone(), v.clone());
        }

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
        let child_pid: Option<u32> = pty.child_watcher().pid().map(|p| p.get());

        // On Unix, wrap the Pty in FilterPty so reads route through
        // the OSC interceptor. On Windows the filter path isn't
        // implemented yet (see spec §10 risk register); we pass the
        // raw Pty through and osc_event_rx stays dormant.
        #[cfg(unix)]
        let event_loop = {
            let filter_pty = FilterPty::new(pty, event_proxy.osc_event_tx.clone())
                .map_err(|e| format!("failed to wrap pty with OSC filter: {}", e))?;
            EventLoop::new(
                term.clone(),
                event_proxy.clone(),
                filter_pty,
                pty_config.drain_on_exit,
                false,
            )
            .map_err(|e| format!("failed to create event loop: {}", e))?
        };
        #[cfg(not(unix))]
        let event_loop = EventLoop::new(
            term.clone(),
            event_proxy.clone(),
            pty,
            pty_config.drain_on_exit,
            false,
        )
        .map_err(|e| format!("failed to create event loop: {}", e))?;

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
            osc_event_rx,
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

    /// Check and clear title-changed flag. True means the shell set a
    /// new window title (OSC 0/2) since the last check — in most shell
    /// configs this fires at every prompt, making it a reliable proxy
    /// for "the user just got a new prompt, CWD may have changed."
    pub fn take_title_changed(&self) -> bool {
        self.event_proxy.title_changed.swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    /// Check if child process has exited
    pub fn child_exited(&self) -> bool {
        self.event_proxy.child_exited.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the current working directory of the child process.
    ///
    /// Per-platform implementations:
    /// * **Linux** — read `/proc/<pid>/cwd` symlink.
    /// * **macOS** — call `proc_pidinfo(PROC_PIDVNODEPATHINFO)`. This is
    ///   the native equivalent of `/proc/<pid>/cwd`; macOS doesn't
    ///   expose `/proc`, so prior versions returned `None` here and
    ///   every cwd-dependent feature (Launch Claude from current dir,
    ///   new tab inheriting cwd, split pane inheriting cwd, file
    ///   picker cwd) silently fell back to amux's launch directory.
    /// * **Windows** — delegates to `win_process_cwd`, a NtQueryInformation
    ///   call.
    pub fn current_cwd(&self) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            let pid = self.child_pid?;
            let link = std::fs::read_link(format!("/proc/{}/cwd", pid)).ok()?;
            Some(link.to_string_lossy().to_string())
        }
        #[cfg(target_os = "macos")]
        {
            // SAFETY: `proc_pidinfo` writes at most `size_of::<proc_vnodepathinfo>()`
            // bytes into a freshly-zeroed local struct. The buffer is
            // valid for the duration of the call and we read from it
            // only after a non-negative return value confirms the kernel
            // populated it.
            let pid = self.child_pid? as libc::c_int;
            let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
            let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
            let n = unsafe {
                libc::proc_pidinfo(
                    pid,
                    libc::PROC_PIDVNODEPATHINFO,
                    0,
                    &mut info as *mut _ as *mut libc::c_void,
                    size,
                )
            };
            if n <= 0 {
                return None;
            }
            // `vip_path` is declared in the libc crate as `[[c_char; 32]; 32]`
            // for historical rustc-version-compat reasons, but its runtime
            // layout is a flat NUL-terminated 1024-byte path buffer
            // (MAXPATHLEN on Darwin). Read it as bytes up to the first NUL.
            let path_ptr = info.pvi_cdir.vip_path.as_ptr() as *const u8;
            let path_bytes: &[u8] = unsafe { std::slice::from_raw_parts(path_ptr, 1024) };
            let end = path_bytes.iter().position(|&b| b == 0).unwrap_or(path_bytes.len());
            if end == 0 {
                return None;
            }
            std::str::from_utf8(&path_bytes[..end]).ok().map(str::to_string)
        }
        #[cfg(target_os = "windows")]
        {
            let pid = self.child_pid?;
            crate::terminal::win_process_cwd(pid)
        }
        #[cfg(target_os = "unknown")]
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

    /// Clear the terminal buffer (scrollback + visible screen).
    /// Sends the "Erase in Display: All" + "Erase Saved Lines" escape
    /// sequences, matching `Ctrl+L` behavior in most shells.
    pub fn clear_buffer(&self) {
        use alacritty_terminal::vte::ansi::Handler;
        self.with_term_mut(|t| {
            t.clear_screen(alacritty_terminal::vte::ansi::ClearMode::Saved);
            t.clear_screen(alacritty_terminal::vte::ansi::ClearMode::All);
        });
    }

    /// Whether scrolled up
    pub fn is_scrolled(&self) -> bool {
        let term = self.term.lock_unfair();
        term.grid().display_offset() > 0
    }

    /// Kill the child process associated with this PTY.
    /// This is called during Drop to ensure the PTY read unblocks and
    /// the event loop thread can exit cleanly.
    pub fn kill_child(&self) {
        if let Some(pid) = self.child_pid {
            #[cfg(not(target_os = "windows"))]
            {
                // Send SIGHUP to the process group so all child processes
                // (including any nested shells or commands) are terminated.
                let _ = unsafe { libc::kill(-(pid as i32), libc::SIGTERM) };
            }
            #[cfg(target_os = "windows")]
            {
                // On Windows we use taskkill to terminate the process tree.
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .output();
            }
        }
    }

    /// Scroll info for rendering scrollbar: (display_offset, total_history_lines, visible_rows)
    pub fn scroll_info(&self) -> (usize, usize, usize) {
        use alacritty_terminal::grid::Dimensions;
        let term = self.term.lock_unfair();
        let offset = term.grid().display_offset();
        let history = term.grid().history_size();
        let visible = term.screen_lines();
        (offset, history, visible)
    }

    /// Read the text content of the line where the cursor is currently positioned.
    /// Used for command interception — always reads the actual input line,
    /// regardless of where it is on screen.
    pub fn cursor_line_text(&self) -> String {
        use alacritty_terminal::index::Column;

        self.with_term(|t| {
            let grid = t.grid();
            let cursor_line = t.grid().cursor.point.line;
            let cols = t.grid().columns();
            let mut text = String::new();
            for col in 0..cols {
                let cell = &grid[cursor_line][Column(col)];
                if cell.c != ' ' && cell.c != '\0' {
                    while text.len() < col {
                        text.push(' ');
                    }
                    text.push(cell.c);
                }
            }
            text.trim_end().to_string()
        })
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

    /// Drain any OSC events (OSC 7 / OSC 133) that the filter
    /// recorded since the last call. The caller — typically
    /// `TerminalManager::poll_activity` — routes these to the
    /// appropriate tab state (cwd cache, shell integration phase).
    ///
    /// Non-blocking. Safe to call every render tick.
    pub fn take_osc_events(&self) -> Vec<OscEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.osc_event_rx.try_recv() {
            events.push(event);
        }
        events
    }
}

// ─── PTY wrapper with OSC interception ─────────────────────────
//
// The spec (`plans/osc-integration-spec.md` §4) calls for inserting an
// OSC byte-filter between the PTY read and alacritty's VTE parser.
// Rather than rewriting the full read loop, we wrap alacritty's
// `tty::Pty` in a thin `FilterPty` that:
//
// * delegates `register / reregister / deregister / next_child_event`
//   and writer access to the inner Pty unchanged;
// * replaces the inner Pty's reader with a `FilterReader` that owns a
//   duplicated file descriptor, runs `OscInterceptor::process` over
//   every read, pushes extracted events through a channel, and
//   returns only the filtered bytes to alacritty.
//
// This keeps alacritty's event loop, resize handling, and child exit
// plumbing untouched. The only behavioral change is that
// `reader().read()` returns shorter byte slices when OSC 7 / 133
// sequences are present.

/// Duplicate a file descriptor into a fresh `File`. Used to hand a
/// second reader over the PTY master to the `FilterReader` so reads
/// via the wrapper consume from the same kernel buffer as the inner
/// Pty would, without borrowing through the Pty.
///
/// On Windows the equivalent is obtained via `try_clone` on the
/// handle (not currently needed — amux only goes through this path
/// on Unix). For Windows we fall back to passing through unmodified
/// (OSC interception is ignored on Windows in this iteration).
#[cfg(unix)]
fn dup_file(file: &std::fs::File) -> std::io::Result<std::fs::File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let fd = unsafe { libc::dup(file.as_raw_fd()) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

/// Reader that runs every byte chunk through `OscInterceptor` before
/// handing it to alacritty. Owns its own duped File on the PTY
/// master; reads from the kernel's shared buffer for that fd.
#[cfg(unix)]
pub struct FilterReader {
    inner: std::fs::File,
    filter: OscInterceptor,
    event_tx: StdSender<OscEvent>,
    /// Bytes filtered from a previous read that couldn't all fit in
    /// the caller's buffer. Drained first on the next read before we
    /// touch the kernel again.
    pending: Vec<u8>,
    pending_pos: usize,
}

#[cfg(unix)]
impl FilterReader {
    fn new(
        inner: std::fs::File,
        event_tx: StdSender<OscEvent>,
    ) -> Self {
        Self {
            inner,
            filter: OscInterceptor::new(),
            event_tx,
            pending: Vec::new(),
            pending_pos: 0,
        }
    }

    fn drain_pending(&mut self, buf: &mut [u8]) -> usize {
        if self.pending_pos >= self.pending.len() {
            return 0;
        }
        let available = &self.pending[self.pending_pos..];
        let n = available.len().min(buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        self.pending_pos += n;
        if self.pending_pos >= self.pending.len() {
            // Everything drained — reset for reuse.
            self.pending.clear();
            self.pending_pos = 0;
        }
        n
    }
}

#[cfg(unix)]
impl std::io::Read for FilterReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Hot path: leftover from a previous call.
        let drained = self.drain_pending(buf);
        if drained > 0 {
            return Ok(drained);
        }

        // Nothing queued — pull from the inner PTY. The buffer size
        // matches alacritty's `READ_BUFFER_SIZE` ceiling behavior:
        // we fill whatever the caller asked for, not more.
        //
        // Scratch is on the stack so we don't allocate per read.
        // 8 KB covers typical PTY chunks; anything larger will loop
        // on the caller's side.
        let mut scratch = [0u8; 8192];
        let to_read = buf.len().min(scratch.len());
        let got = match self.inner.read(&mut scratch[..to_read]) {
            Ok(0) => return Ok(0),
            Ok(n) => n,
            Err(e) => return Err(e),
        };

        let (filtered, events) = self.filter.process(&scratch[..got]);

        // Forward every OSC event to the main-thread drain channel.
        // Channel full / disconnected is silently ignored — the
        // reader thread must never block on event routing per
        // `plans/osc-integration-spec.md` §9 "Never".
        for event in events {
            let _ = self.event_tx.send(event);
        }

        if filtered.is_empty() {
            // Entire chunk was OSC — nothing for alacritty to parse
            // right now. Ok(0) here tells alacritty's read loop
            // "nothing more for now" (see alacritty's `event_loop.rs`
            // comment on Ok(0)). Next poll cycle picks up if the PTY
            // has more data.
            return Ok(0);
        }

        if filtered.len() <= buf.len() {
            buf[..filtered.len()].copy_from_slice(&filtered);
            Ok(filtered.len())
        } else {
            // Rare: filtered output larger than caller's buffer.
            // (Can happen when inner read was bigger than buf and
            // everything passed through.) Fill buf and stash the
            // rest for the next call.
            buf.copy_from_slice(&filtered[..buf.len()]);
            self.pending = filtered[buf.len()..].to_vec();
            self.pending_pos = 0;
            Ok(buf.len())
        }
    }
}

/// Wrapper around `tty::Pty` that reroutes reads through the OSC
/// interceptor. Delegates everything else untouched — register,
/// reregister, deregister, writer, child event, resize.
#[cfg(unix)]
pub struct FilterPty {
    inner: tty::Pty,
    filter_reader: FilterReader,
}

#[cfg(unix)]
impl FilterPty {
    pub fn new(
        mut pty: tty::Pty,
        event_tx: StdSender<OscEvent>,
    ) -> std::io::Result<Self> {
        // Duplicate the master fd so the filter reader owns its own
        // `File` handle. Both handles point to the same kernel fd;
        // reads on either advance the shared kernel buffer.
        let duped = dup_file(pty.reader())?;
        Ok(Self {
            inner: pty,
            filter_reader: FilterReader::new(duped, event_tx),
        })
    }
}

#[cfg(unix)]
impl EventedReadWrite for FilterPty {
    type Reader = FilterReader;
    type Writer = std::fs::File;

    unsafe fn register(
        &mut self,
        poll: &Arc<polling::Poller>,
        interest: polling::Event,
        poll_opts: polling::PollMode,
    ) -> std::io::Result<()> {
        unsafe { self.inner.register(poll, interest, poll_opts) }
    }

    fn reregister(
        &mut self,
        poll: &Arc<polling::Poller>,
        interest: polling::Event,
        poll_opts: polling::PollMode,
    ) -> std::io::Result<()> {
        self.inner.reregister(poll, interest, poll_opts)
    }

    fn deregister(&mut self, poll: &Arc<polling::Poller>) -> std::io::Result<()> {
        self.inner.deregister(poll)
    }

    fn reader(&mut self) -> &mut FilterReader {
        &mut self.filter_reader
    }

    fn writer(&mut self) -> &mut std::fs::File {
        self.inner.writer()
    }
}

#[cfg(unix)]
impl EventedPty for FilterPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.inner.next_child_event()
    }
}

#[cfg(unix)]
impl OnResize for FilterPty {
    fn on_resize(&mut self, window_size: WindowSize) {
        self.inner.on_resize(window_size);
    }
}
