//! End-to-end test for the OSC interceptor → PTY reader pipeline.
//!
//! Spawns a real `/bin/sh -c 'printf ...'` session via
//! `AlacrittyTerminal`, which runs the full production path:
//! `portable-pty` master/slave split → `FilterPty::reader()` →
//! `OscInterceptor::process` → `AmuEventProxy.osc_event_tx`. Drains
//! the event channel via `take_osc_events()` and verifies the OSC 7
//! payload emitted by the child process shows up as a
//! `WorkingDirectory` event.
//!
//! Without this test, the Step 2 interceptor + Step 3 event wiring
//! could silently break at the kernel/fd-dup layer and only surface
//! on manual inspection. Having it run under `cargo test` makes the
//! integration contract enforceable.
//!
//! Unix only — the `FilterPty` wrapper is Unix-specific (see the
//! `#[cfg(unix)]` gate in `alacritty_view.rs`). Windows OSC support
//! is a separate iteration per the spec §10 risk register.

#![cfg(unix)]

use std::thread;
use std::time::{Duration, Instant};

use amux_platform::terminal::alacritty_view::AlacrittyTerminal;
use amux_platform::terminal::osc_intercept::OscEvent;

#[test]
fn osc7_from_shell_reaches_event_channel() {
    // `/bin/sh -c` with an inline script that emits OSC 7 in BEL-
    // terminated form, then exits. The child process writes the
    // sequence to stdout, which flows through the PTY master →
    // `FilterPty::reader()` → interceptor.
    //
    // The escape sequence below is literally:
    //   ESC ] 7 ; file:///tmp BEL
    // in the shell's single-quoted `$' '` form.
    let term = AlacrittyTerminal::new(
        80,
        24,
        8,
        16,
        "/bin/sh",
        &[
            "-c".to_string(),
            // printf supports \NNN octal escapes; 033 = ESC, 007 = BEL.
            r"printf '\033]7;file:///tmp\007'".to_string(),
        ],
        Some("/tmp"),
    )
    .expect("spawn AlacrittyTerminal with /bin/sh -c printf");

    // Give the reader thread time to run, filter the OSC, push an
    // event, and for the child to exit. 2s is plenty for a `printf`
    // + exit round-trip even on loaded CI.
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut events: Vec<OscEvent> = Vec::new();
    while Instant::now() < deadline {
        events.extend(term.take_osc_events());
        if events
            .iter()
            .any(|e| matches!(e, OscEvent::WorkingDirectory(p) if p == "/tmp"))
        {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    assert!(
        events.iter().any(|e| matches!(e, OscEvent::WorkingDirectory(p) if p == "/tmp")),
        "expected OscEvent::WorkingDirectory(\"/tmp\") in {events:?} within 2s",
    );
}

#[test]
fn osc133_prompt_cycle_from_shell_reaches_event_channel() {
    // Same pipeline test, but for OSC 133 — simulates the prompt
    // cycle `vscode-shell-integration` scripts emit:
    //   OSC 133;A (prompt start) → OSC 133;D;0 (command finished, exit 0)
    // Verifies both variants make it through.
    let term = AlacrittyTerminal::new(
        80,
        24,
        8,
        16,
        "/bin/sh",
        &[
            "-c".to_string(),
            r"printf '\033]133;A\007\033]133;D;0\007'".to_string(),
        ],
        Some("/tmp"),
    )
    .expect("spawn AlacrittyTerminal with 133 sequence");

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut events: Vec<OscEvent> = Vec::new();
    while Instant::now() < deadline {
        events.extend(term.take_osc_events());
        let has_prompt = events.iter().any(|e| matches!(e, OscEvent::PromptStart));
        let has_finish = events
            .iter()
            .any(|e| matches!(e, OscEvent::CommandFinished(Some(0))));
        if has_prompt && has_finish {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    assert!(
        events.iter().any(|e| matches!(e, OscEvent::PromptStart)),
        "expected PromptStart in {events:?}",
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, OscEvent::CommandFinished(Some(0)))),
        "expected CommandFinished(Some(0)) in {events:?}",
    );
}
