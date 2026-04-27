//! End-to-end smoke test for the live terminal stack.
//!
//! Spawns a real `/bin/sh` PTY via `RealTerminalBackend`, writes a
//! command, and drains the reader thread until the expected marker
//! shows up in the collected output. Exercises the full production
//! path: `portable-pty` master/slave split, the dedicated reader
//! thread in `backend.rs`, the output buffer drain, and
//! `TerminalOutputManager` line collection.
//!
//! Scoped to Unix for now — Windows uses a different spawn path
//! (ConPTY via pwsh/powershell) and deserves its own smoke test.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use amux_core::{ShellKind, TerminalLaunchProfile, WorkspaceTarget};
use amux_platform::terminal::{RealTerminalBackend, TerminalBackend};

#[test]
fn real_backend_round_trips_stdin_to_pty_output() {
    let backend = RealTerminalBackend::new();

    let spec = TerminalLaunchProfile {
        target: WorkspaceTarget::LocalPath {
            path: PathBuf::from("/tmp"),
        },
        // Plain /bin/sh keeps the output predictable: no prompt, no
        // MOTD, no history. `build_unix_command` maps Custom to an
        // empty arg vec so the shell runs in batch mode reading stdin.
        shell: ShellKind::Custom("/bin/sh".into()),
        cwd: Some("/tmp".into()),
        env: BTreeMap::new(),
        title: None,
    };

    let id = backend
        .create_session(spec)
        .expect("create_session should spawn a PTY");

    // Use a unique marker so we can't mistake incidental output for
    // the real thing. `printf` has no newline by default — we add it
    // explicitly so the reader thread flushes a complete line.
    let marker = "amux-smoke-marker-42";
    let script = format!("printf '{marker}\\n'\nexit 0\n");
    backend
        .write_input(&id, script.as_bytes())
        .expect("write_input should succeed");

    let mut collected = Vec::<u8>::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let mut buf = [0u8; 4096];
        match backend.read_output(&id, &mut buf) {
            Ok(0) => thread::sleep(Duration::from_millis(25)),
            Ok(n) => collected.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
        if String::from_utf8_lossy(&collected).contains(marker) {
            break;
        }
    }

    let text = String::from_utf8_lossy(&collected).to_string();
    assert!(
        text.contains(marker),
        "expected marker {marker:?} in PTY output within 5s; got {text:?}",
    );

    let _ = backend.kill(&id);
}
