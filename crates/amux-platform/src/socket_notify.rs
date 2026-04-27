//! Unix socket notification listener for external tool integration.
//!
//! Creates a local socket at `~/.amux/notify.sock` that external tools
//! (Claude Code hooks, OpenCode plugins, custom scripts) can write
//! notifications to. The protocol is simple pipe-delimited text:
//!
//! ```text
//! type|paneID|title|body
//! ```
//!
//! Fields:
//! - `type`: "notification", "agent_status", "cwd_change", "toast"
//! - `paneID`: the pane ID from AMUX_PANE_ID env var
//! - `title`: short title for the notification
//! - `body`: optional detail text
//!
//! Each terminal pane is injected with `AMUX_SOCKET_PATH` so child
//! processes know where to connect. External tools write one line per
//! notification. Max message size: 64KB.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

/// A notification received from an external tool via the socket.
#[derive(Clone, Debug)]
pub struct SocketNotification {
    pub kind: String,
    pub pane_id: String,
    pub title: String,
    pub body: String,
}

impl SocketNotification {
    fn parse(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() < 3 {
            return None;
        }
        Some(Self {
            kind: parts[0].trim().to_string(),
            pane_id: parts[1].trim().to_string(),
            title: parts[2].trim().to_string(),
            body: parts.get(3).map(|s| s.trim().to_string()).unwrap_or_default(),
        })
    }
}

/// Socket path for the notification listener.
pub fn socket_path() -> PathBuf {
    crate::dirs::amux_home_dir().join("notify.sock")
}

/// Start a background listener thread. Returns a receiver for
/// `SocketNotification` events that the main loop should drain.
///
/// On Unix (macOS/Linux): binds a Unix domain socket.
/// On Windows: creates a named pipe server.
pub fn start_listener() -> Option<mpsc::Receiver<SocketNotification>> {
    let path = socket_path();

    // Remove stale socket file if it exists
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(&path);
    }

    let (tx, rx) = mpsc::channel::<SocketNotification>();

    #[cfg(unix)]
    {
        let listener = match std::os::unix::net::UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[amux] socket notify: failed to bind {}: {}", path.display(), e);
                return None;
            }
        };
        // Restrict permissions so only the current user can write
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }

        eprintln!("[amux] socket notify: listening on {}", path.display());

        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let tx = tx.clone();
                        thread::spawn(move || {
                            let reader = BufReader::new(&stream);
                            for line in reader.lines() {
                                match line {
                                    Ok(line) if !line.is_empty() => {
                                        if let Some(notif) = SocketNotification::parse(&line) {
                                            let _ = tx.send(notif);
                                        }
                                    }
                                    Err(_) | Ok(_) => break,
                                }
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("[amux] socket notify: accept error: {}", e);
                    }
                }
            }
        });
    }

    #[cfg(not(unix))]
    {
        // Windows: named pipe server. For now, return None —
        // Windows support can be added later with tokio::net::windows::named_pipe.
        eprintln!("[amux] socket notify: Windows named pipe not yet implemented");
        return None;
    }

    Some(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_notification() {
        let n = SocketNotification::parse("toast|pane-1|Claude Code|Task completed").unwrap();
        assert_eq!(n.kind, "toast");
        assert_eq!(n.pane_id, "pane-1");
        assert_eq!(n.title, "Claude Code");
        assert_eq!(n.body, "Task completed");
    }

    #[test]
    fn parse_minimal_notification() {
        let n = SocketNotification::parse("notification|pane-2|Done").unwrap();
        assert_eq!(n.kind, "notification");
        assert_eq!(n.pane_id, "pane-2");
        assert_eq!(n.title, "Done");
        assert_eq!(n.body, "");
    }

    #[test]
    fn parse_body_with_pipes() {
        let n = SocketNotification::parse("toast|pane-1|Log|cargo test | grep fail").unwrap();
        assert_eq!(n.body, "cargo test | grep fail");
    }

    #[test]
    fn parse_rejects_short_lines() {
        assert!(SocketNotification::parse("toast").is_none());
        assert!(SocketNotification::parse("toast|pane-1").is_none());
    }

    #[test]
    fn parse_trims_whitespace() {
        let n = SocketNotification::parse("  toast  | pane-1 | Title | Body  ").unwrap();
        assert_eq!(n.kind, "toast");
        assert_eq!(n.pane_id, "pane-1");
        assert_eq!(n.title, "Title");
        assert_eq!(n.body, "Body");
    }
}
