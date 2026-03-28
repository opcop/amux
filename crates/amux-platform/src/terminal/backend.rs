use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use amux_core::{ShellKind, TerminalLaunchProfile, TerminalSessionId, WorkspaceTarget};

use crate::terminal_output::{OutputLine, TerminalOutputManager};

pub type TerminalLaunchSpec = TerminalLaunchProfile;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalSessionKind {
    WindowsConPty,
    Wsl,
    UnixPty,
    Mock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSessionMetadata {
    pub id: TerminalSessionId,
    pub kind: TerminalSessionKind,
    pub target: WorkspaceTarget,
    pub shell: ShellKind,
    pub cwd: Option<String>,
    pub title: Option<String>,
}

pub trait TerminalBackend: Send + Sync {
    fn create_session(&self, spec: TerminalLaunchSpec) -> Result<TerminalSessionId, String>;
    fn write_input(&self, id: &TerminalSessionId, data: &[u8]) -> Result<(), String>;
    fn resize(&self, id: &TerminalSessionId, cols: u16, rows: u16) -> Result<(), String>;
    fn kill(&self, id: &TerminalSessionId) -> Result<(), String>;
    fn metadata(&self, id: &TerminalSessionId) -> Result<TerminalSessionMetadata, String>;
}

/// A live PTY session with separated reader/writer to avoid mutex deadlocks.
///
/// The reader runs in its own thread and pushes output to `output_buf`.
/// The writer is behind the shared `state` mutex but never blocks on I/O.
struct PtySession {
    #[allow(dead_code)]
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    metadata: TerminalSessionMetadata,
    /// Output buffer filled by the reader thread — never blocks
    output_buf: Arc<Mutex<Vec<u8>>>,
    /// Signal the reader thread to stop
    reader_alive: Arc<Mutex<bool>>,
    #[allow(dead_code)]
    reader_handle: Option<thread::JoinHandle<()>>,
}

impl PtySession {
    fn new(
        master: Box<dyn portable_pty::MasterPty + Send>,
        metadata: TerminalSessionMetadata,
    ) -> Result<Self, String> {
        let mut reader = master
            .try_clone_reader()
            .map_err(|e| format!("failed to clone PTY reader: {}", e))?;
        let writer = master
            .take_writer()
            .map_err(|e| format!("failed to take PTY writer: {}", e))?;

        let output_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let reader_alive = Arc::new(Mutex::new(true));

        // Spawn a dedicated reader thread that never holds the main state mutex
        let buf_clone = Arc::clone(&output_buf);
        let alive_clone = Arc::clone(&reader_alive);

        let reader_handle = thread::spawn(move || {
            let mut tmp = [0u8; 4096];
            loop {
                // Check if we should stop
                if !*alive_clone.lock().unwrap() {
                    break;
                }
                match reader.read(&mut tmp) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        if let Ok(mut buf) = buf_clone.lock() {
                            buf.extend_from_slice(&tmp[..n]);
                        }
                    }
                    Err(e) => {
                        // WouldBlock is expected on some platforms
                        if e.kind() == std::io::ErrorKind::WouldBlock {
                            thread::sleep(std::time::Duration::from_millis(5));
                            continue;
                        }
                        break; // Real error → session ended
                    }
                }
            }
        });

        Ok(Self {
            master,
            writer,
            metadata,
            output_buf,
            reader_alive,
            reader_handle: Some(reader_handle),
        })
    }

    fn write(&mut self, data: &[u8]) -> Result<(), String> {
        self.writer
            .write_all(data)
            .map_err(|e| format!("failed to write to PTY: {}", e))
    }

    /// Drain all pending output from the reader thread (never blocks)
    fn drain_output(&self) -> Vec<u8> {
        let mut buf = self.output_buf.lock().unwrap();
        std::mem::take(&mut *buf)
    }

    fn stop_reader(&mut self) {
        if let Ok(mut alive) = self.reader_alive.lock() {
            *alive = false;
        }
    }
}

/// Real terminal backend using portable-pty for cross-platform PTY support
#[derive(Clone)]
pub struct RealTerminalBackend {
    state: Arc<Mutex<RealTerminalState>>,
    output_manager: TerminalOutputManager,
}

struct RealTerminalState {
    sessions: Vec<PtySession>,
    next_id: usize,
}

impl std::fmt::Debug for RealTerminalBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RealTerminalBackend")
            .field("state", &"<hidden>")
            .finish()
    }
}

impl RealTerminalBackend {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RealTerminalState {
                sessions: Vec::new(),
                next_id: 1,
            })),
            output_manager: TerminalOutputManager::new(),
        }
    }

    pub fn with_output_manager(output_manager: TerminalOutputManager) -> Self {
        Self {
            state: Arc::new(Mutex::new(RealTerminalState {
                sessions: Vec::new(),
                next_id: 1,
            })),
            output_manager,
        }
    }

    /// Get the output manager for this backend
    pub fn output_manager(&self) -> &TerminalOutputManager {
        &self.output_manager
    }

    /// Read available output from a session (never blocks — drains the reader buffer)
    pub fn read_output(&self, id: &TerminalSessionId, buf: &mut [u8]) -> Result<usize, String> {
        let state = self.state.lock()
            .map_err(|_| "terminal state mutex poisoned".to_string())?;

        let session = state.sessions.iter()
            .find(|s| s.metadata.id == *id)
            .ok_or_else(|| format!("terminal session not found: {}", id))?;

        let data = session.drain_output();
        if data.is_empty() {
            return Ok(0);
        }

        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);

        // Feed to output collector
        if let Some(output) = self.output_manager.get_output(id) {
            output.append_raw(&data[..n], false);
        }

        // If there was more data than buf can hold, push remainder back
        if data.len() > buf.len() {
            if let Ok(mut ob) = session.output_buf.lock() {
                let remainder = &data[buf.len()..];
                let mut new_buf = remainder.to_vec();
                new_buf.append(&mut *ob);
                *ob = new_buf;
            }
        }

        Ok(n)
    }

    /// Get recent output lines for a session
    pub fn get_recent_output(&self, id: &TerminalSessionId, count: usize) -> Vec<OutputLine> {
        self.output_manager.get_recent_lines(id, count)
    }

    fn next_session_id(state: &mut RealTerminalState) -> TerminalSessionId {
        let id = TerminalSessionId::new(format!("pty-session-{}", state.next_id));
        state.next_id += 1;
        id
    }

    fn build_pty_command(spec: &TerminalLaunchSpec) -> Result<(String, Vec<String>, Option<String>), String> {
        // On non-Windows platforms, always use the system shell
        if !cfg!(target_os = "windows") {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            return Ok((shell, vec!["-l".to_string()], spec.cwd.clone()));
        }

        match (&spec.target, &spec.shell) {
            (WorkspaceTarget::WindowsPath { .. }, ShellKind::PowerShell) => {
                Ok((
                    "powershell.exe".to_string(),
                    vec!["-NoLogo".to_string()],
                    spec.cwd.clone(),
                ))
            }
            (WorkspaceTarget::WindowsPath { .. }, ShellKind::Cmd) => {
                Ok((
                    "cmd.exe".to_string(),
                    Vec::new(),
                    spec.cwd.clone(),
                ))
            }
            (WorkspaceTarget::WslPath { distro, .. }, _) | (WorkspaceTarget::WindowsPath { .. }, ShellKind::WslDistro(distro)) => {
                let mut args = vec![
                    "-d".to_string(),
                    distro.clone(),
                ];
                if let Some(cwd) = &spec.cwd {
                    args.push("--cd".to_string());
                    args.push(cwd.clone());
                }
                args.push("--".to_string());
                args.push("bash".to_string());
                Ok(("wsl.exe".to_string(), args, None))
            }
            _ => {
                // Fallback: use system shell
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
                Ok((shell, Vec::new(), spec.cwd.clone()))
            }
        }
    }

    fn create_pty(&self, spec: &TerminalLaunchSpec) -> Result<PtySession, String> {
        let (program, args, _cwd) = Self::build_pty_command(spec)?;

        let pty_pair = portable_pty::native_pty_system()
            .openpty(portable_pty::PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("failed to open PTY: {}", e))?;

        let mut cmd = portable_pty::CommandBuilder::new(&program);
        for arg in &args {
            cmd.arg(arg);
        }

        if let Some(cwd) = &spec.cwd {
            cmd.cwd(cwd);
        }

        for (key, value) in &spec.env {
            cmd.env(key, value);
        }

        let _child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("failed to spawn PTY child: {}", e))?;

        let session_id = {
            let mut state = self.state.lock()
                .map_err(|_| "terminal state mutex poisoned".to_string())?;
            Self::next_session_id(&mut state)
        };

        let metadata = TerminalSessionMetadata {
            id: session_id,
            kind: if matches!(spec.target, WorkspaceTarget::WslPath { .. }) {
                TerminalSessionKind::Wsl
            } else {
                TerminalSessionKind::WindowsConPty
            },
            target: spec.target.clone(),
            shell: spec.shell.clone(),
            cwd: spec.cwd.clone(),
            title: spec.title.clone(),
        };

        PtySession::new(pty_pair.master, metadata)
    }
}

impl Default for RealTerminalBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalBackend for RealTerminalBackend {
    fn create_session(&self, spec: TerminalLaunchSpec) -> Result<TerminalSessionId, String> {
        let session = self.create_pty(&spec)?;
        let id = session.metadata.id.clone();

        // Register output collector
        self.output_manager.register_session(id.clone());

        let mut state = self.state.lock()
            .map_err(|_| "terminal state mutex poisoned".to_string())?;
        state.sessions.push(session);

        Ok(id)
    }

    fn write_input(&self, id: &TerminalSessionId, data: &[u8]) -> Result<(), String> {
        let mut state = self.state.lock()
            .map_err(|_| "terminal state mutex poisoned".to_string())?;

        let session = state.sessions.iter_mut()
            .find(|s| s.metadata.id == *id)
            .ok_or_else(|| format!("terminal session not found: {}", id))?;

        session.write(data)
    }

    fn resize(&self, id: &TerminalSessionId, cols: u16, rows: u16) -> Result<(), String> {
        let mut state = self.state.lock()
            .map_err(|_| "terminal state mutex poisoned".to_string())?;

        let session = state.sessions.iter_mut()
            .find(|s| s.metadata.id == *id)
            .ok_or_else(|| format!("terminal session not found: {}", id))?;

        session.master
            .resize(portable_pty::PtySize {
                cols,
                rows,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("failed to resize PTY: {}", e))
    }

    fn kill(&self, id: &TerminalSessionId) -> Result<(), String> {
        let mut state = self.state.lock()
            .map_err(|_| "terminal state mutex poisoned".to_string())?;

        let pos = state.sessions.iter()
            .position(|s| s.metadata.id == *id)
            .ok_or_else(|| format!("terminal session not found: {}", id))?;

        let mut session = state.sessions.remove(pos);
        session.stop_reader();

        self.output_manager.unregister_session(id);

        Ok(())
    }

    fn metadata(&self, id: &TerminalSessionId) -> Result<TerminalSessionMetadata, String> {
        let state = self.state.lock()
            .map_err(|_| "terminal state mutex poisoned".to_string())?;

        state.sessions.iter()
            .find(|s| s.metadata.id == *id)
            .map(|s| s.metadata.clone())
            .ok_or_else(|| format!("terminal session not found: {}", id))
    }
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MockTerminalRecord {
    pub metadata: TerminalSessionMetadata,
    pub writes: Vec<Vec<u8>>,
    pub last_size: Option<(u16, u16)>,
    pub killed: bool,
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryTerminalBackend {
    state: Arc<Mutex<Vec<MockTerminalRecord>>>,
}

impl InMemoryTerminalBackend {
    pub fn records(&self) -> Vec<MockTerminalRecord> {
        self.state
            .lock()
            .expect("terminal backend mutex poisoned")
            .clone()
    }

    fn next_id(records: &[MockTerminalRecord]) -> TerminalSessionId {
        TerminalSessionId::new(format!("session-{}", records.len() + 1))
    }
}

impl TerminalBackend for InMemoryTerminalBackend {
    fn create_session(&self, spec: TerminalLaunchSpec) -> Result<TerminalSessionId, String> {
        let mut records = self
            .state
            .lock()
            .map_err(|_| "terminal backend mutex poisoned".to_string())?;
        let id = Self::next_id(&records);
        records.push(MockTerminalRecord {
            metadata: TerminalSessionMetadata {
                id: id.clone(),
                kind: TerminalSessionKind::Mock,
                target: spec.target,
                shell: spec.shell,
                cwd: spec.cwd,
                title: spec.title,
            },
            writes: Vec::new(),
            last_size: None,
            killed: false,
            env: spec.env,
        });
        Ok(id)
    }

    fn write_input(&self, id: &TerminalSessionId, data: &[u8]) -> Result<(), String> {
        let mut records = self
            .state
            .lock()
            .map_err(|_| "terminal backend mutex poisoned".to_string())?;
        let Some(record) = records.iter_mut().find(|record| &record.metadata.id == id) else {
            return Err(format!("terminal session not found: {id}"));
        };
        record.writes.push(data.to_vec());
        Ok(())
    }

    fn resize(&self, id: &TerminalSessionId, cols: u16, rows: u16) -> Result<(), String> {
        let mut records = self
            .state
            .lock()
            .map_err(|_| "terminal backend mutex poisoned".to_string())?;
        let Some(record) = records.iter_mut().find(|record| &record.metadata.id == id) else {
            return Err(format!("terminal session not found: {id}"));
        };
        record.last_size = Some((cols, rows));
        Ok(())
    }

    fn kill(&self, id: &TerminalSessionId) -> Result<(), String> {
        let mut records = self
            .state
            .lock()
            .map_err(|_| "terminal backend mutex poisoned".to_string())?;
        let Some(record) = records.iter_mut().find(|record| &record.metadata.id == id) else {
            return Err(format!("terminal session not found: {id}"));
        };
        record.killed = true;
        Ok(())
    }

    fn metadata(&self, id: &TerminalSessionId) -> Result<TerminalSessionMetadata, String> {
        let records = self
            .state
            .lock()
            .map_err(|_| "terminal backend mutex poisoned".to_string())?;
        let Some(record) = records.iter().find(|record| &record.metadata.id == id) else {
            return Err(format!("terminal session not found: {id}"));
        };
        Ok(record.metadata.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use amux_core::{ShellKind, WorkspaceTarget};

    use super::{InMemoryTerminalBackend, TerminalBackend, TerminalLaunchSpec};

    #[test]
    fn mock_backend_tracks_session_lifecycle() {
        let backend = InMemoryTerminalBackend::default();
        let session = backend
            .create_session(TerminalLaunchSpec {
                target: WorkspaceTarget::WindowsPath {
                    path: PathBuf::from("D:/repo/amux"),
                },
                shell: ShellKind::PowerShell,
                cwd: Some("D:/repo/amux".into()),
                env: BTreeMap::from([(String::from("TERM"), String::from("xterm-256color"))]),
                title: Some("Main".into()),
            })
            .expect("session should be created");

        backend
            .write_input(&session, b"dir\n")
            .expect("write should succeed");
        backend.resize(&session, 120, 40).expect("resize should succeed");
        backend.kill(&session).expect("kill should succeed");

        let records = backend.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].writes.len(), 1);
        assert_eq!(records[0].last_size, Some((120, 40)));
        assert!(records[0].killed);
    }

    #[test]
    fn real_backend_can_be_created() {
        use super::RealTerminalBackend;

        // Just verify we can create the backend
        let _backend = RealTerminalBackend::new();
    }
}
