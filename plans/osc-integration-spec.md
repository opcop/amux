# OSC Interception — Spec

**Status**: draft, awaiting approval
**Owner**: amux-platform terminal backend
**Scope**: OSC 7 (working directory) + OSC 133 (shell integration) interception, landing in one coordinated change since they share the same byte-level interceptor infrastructure.
**Depends on**: `alacritty_terminal 0.25`'s `Processor` / `Handler` trait; no new external deps.

## 1. Objective

Add a pre-parser layer between the PTY byte stream and alacritty's VTE processor that:

1. **Extracts OSC 7** — working-directory signal `ESC ] 7 ; file://host/path ST` — and pipes it directly into each `PaneTab`'s cached cwd. Removes the current heuristic chain (title extraction → `proc_pidinfo(zsh_pid)` syscall → saved spawn cwd fallback).

2. **Extracts OSC 133** — shell integration markers for command lifecycle: `133;A` (prompt start), `133;B` (command start), `133;C` (command executing), `133;D;<exit_code>` (command finished). Feeds the event stream into `PaneTab.agent_status` detection so status updates switch from "last-5-lines regex on terminal output" to "shell tells us authoritatively."

Both wins come from one piece of infrastructure — a byte-level state machine that filters the raw PTY output before alacritty sees it. Everything non-OSC-7/133 passes through unchanged.

**Non-goals this round:**
- OSC 9 (iTerm2 notification), OSC 9;4 (ConEmu progress), OSC 777 (desktop notification) — easy to add once interceptor lands, but not justified by amux's current feature set.
- Replacing the existing regex-based agent detection outright — it stays as the fallback for shells without OSC 133.
- Prompt parsing (`extract_cwd_from_prompt`) removal — it stays as the fallback for shells without OSC 7.
- Per-command output capture for structured agent session replay — separate feature.

## 2. Target users

- Any amux user with a modern shell (zsh / fish / bash + starship / oh-my-posh / p10k) that emits OSC 7 by default. Cwd tracking becomes instant and accurate instead of needing title-change syscalls.
- Agent users running Claude Code / Cursor / Codex through amux — their shells' OSC 133 emits clean command-lifecycle events, and amux can detect "agent finished running a command" without regex-matching output.

## 3. User-visible behavior

### Nothing visible *changes* for shells without OSC support
All existing heuristic chains (title → process cwd → saved cwd; last-5-lines regex for agent status) stay live as fallbacks. Users on non-integrated shells see no regression.

### Cwd tracking (OSC 7)
- `amux preview` / Ctrl+P / right-click "Preview File" resolve the picker base dir from the shell-reported cwd within one frame of any `cd`.
- File-drop / sidebar markdown shortcuts (if added) see the same fresh cwd.
- No more silent drift when the terminal title changes but cwd didn't (or vice versa).

### Agent status (OSC 133)
- Tab title dot turns green ("waiting for input") within one frame of `CommandFinished` being emitted, regardless of what text the agent printed.
- Exit code propagates — non-zero exit can render as red ("error") without matching "Error:" in output.
- Running state (`CommandExecuting`) flickers yellow immediately on command launch.

### Backward compat
- Shell emits OSC 7 only → cwd via OSC 7, status via regex (unchanged).
- Shell emits OSC 133 only → cwd via regex/title (unchanged), status via OSC 133.
- Shell emits neither → both via existing heuristics (full backward compat).
- Shell emits both → full integration path, both fallbacks dormant.

## 4. Architecture

### Where the interceptor sits

Current amux terminal read loop (simplified):

```
PTY → alacritty_terminal::EventLoop → Processor.advance(&mut Term, bytes) → Term handles everything
```

The problem: `alacritty_terminal 0.25`'s default `Term` handler silently drops OSC 7 (and never had OSC 133 support). The `Handler::osc_dispatch` method is called but does nothing for those sequences. Our only hooks today are:

- `EventProxy::send_event` — fires on title/bell/cursor events, no OSC info
- `Term` state readback — post-hoc, no lifecycle signals

New amux read loop:

```
PTY → read bytes
    ↓
OscInterceptor::process(bytes) → (filtered_bytes, Vec<OscEvent>)
    ↓                                    ↓
Processor.advance(&mut Term, filtered_bytes)   OscEvent dispatcher:
                                          • WorkingDirectory → tab.cached_cwd
                                          • CommandFinished(exit) → tab.agent_status
                                          • PromptStart → ...
```

We own the read loop. `EventLoop::spawn` (alacritty's built-in) no longer fits because it reads PTY + advances Processor in one atomic step we can't split. We replace it with a lightweight reader thread that does: read → intercept → advance.

### Core types (new module: `crates/amux-platform/src/terminal/osc_intercept.rs`)

```rust
/// Byte-stream state machine that extracts OSC 7 / 133 sequences
/// before they reach alacritty's VTE parser.
#[derive(Default)]
pub struct OscInterceptor {
    buffer: Vec<u8>,    // accumulates OSC payload
    state: ParseState,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    #[default] Ground,     // passthrough
    Escape,                // saw 0x1B
    OscStart,              // saw ESC ]
    OscPayload,            // accumulating until ST
    OscEscape,             // saw ESC inside OSC (checking for ST = ESC \)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscEvent {
    WorkingDirectory(String),
    PromptStart,
    CommandStart,
    CommandExecuting,
    CommandFinished(Option<i32>),
}

impl OscInterceptor {
    /// Feed raw PTY bytes; get filtered passthrough bytes + extracted events.
    /// Non-OSC data and uninteresting OSCs (0/2 title, 4/10/11 colors, etc.)
    /// pass through untouched so alacritty sees them.
    pub fn process(&mut self, input: &[u8]) -> (Vec<u8>, Vec<OscEvent>);
}
```

Buffer cap: 64 KB (`MAX_OSC_BUFFER`) to bound memory on malformed input; oversize buffer → abort current OSC, flush raw bytes to passthrough.

### Integration with `AlacrittyTerminal`

Three touchpoints:

1. **`AlacrittyTerminal::spawn_with_event_loop`** (or wherever the read loop lives) replaces `EventLoop::spawn` with `spawn_reader_thread` that owns:
   - PTY read buffer
   - `OscInterceptor`
   - `Processor` reference
   - Channel back to main thread for `OscEvent`s

2. **New field on `AmuEventProxy`**: `osc_event_tx: Option<Sender<OscEvent>>` — reader thread pushes OSC events here; main-thread `poll_activity` drains and applies.

3. **New fields on `PaneTab`**:
   - `shell_reported_cwd: Option<String>` — the last OSC 7 we saw (unconditional, overrides cached_cwd/saved_cwd when present)
   - `shell_integration_phase: CommandPhase` — last OSC 133 state (Idle / Prompt / Executing / Finished(exit_code))

### Cwd resolution chain (updated)

Current `active_tab_live_cwd`:
```
cached_cwd → proc_pidinfo → saved_cwd
```

New chain:
```
shell_reported_cwd (OSC 7)    ← authoritative when present
    ↓ fallback
cached_cwd (title-change triggered proc_pidinfo)
    ↓ fallback
proc_pidinfo direct syscall
    ↓ fallback
saved spawn cwd
```

OSC 7 is strictly additive — every existing source keeps working.

### Agent status detection (updated)

Current `detect_agent_status(kind, last_lines, exited)`:
```
regex match on last 5 lines → Waiting / Executing / Error / Finished / Unknown
```

New path (takes precedence when OSC 133 has fired at least once):
```
shell_integration_phase:
  Prompt    → Waiting       (shell is ready for input, agent idle)
  Executing → Running       (command is running)
  Finished(0)   → Finished  (success, brief flash green)
  Finished(!=0) → Error     (exit code propagates)
  Idle (never saw 133) → fall through to regex
```

Opt-in per tab: once we see the first OSC 133 from this tab's shell, the OSC path becomes authoritative. Before that, regex stays in charge. This gives each tab its own integration state without a global setting.

## 5. Project structure

```
crates/amux-platform/src/terminal/
├── osc_intercept.rs                (NEW, ~500 LOC)
│   ├── OscInterceptor state machine
│   ├── OscEvent enum
│   ├── sequence parsers (parse_133_payload, etc.)
│   └── unit tests (20+ cases: each OSC, malformed, partial, ST variants)
├── alacritty_view.rs               (edited, ~80 LOC diff)
│   ├── AmuEventProxy adds osc_event_tx
│   └── reader thread swaps EventLoop::spawn for custom loop
├── manager.rs                      (edited, ~60 LOC diff)
│   ├── PaneTab adds shell_reported_cwd + shell_integration_phase
│   ├── active_tab_live_cwd: prepend OSC 7 source
│   └── detect_agent_status: branch on shell_integration_phase first
└── backend.rs                      (edited, ~40 LOC diff)
    └── read loop wires OscInterceptor between pty and processor

apps/desktop/src/
└── gpui_entry.rs                   (unchanged)
```

Total: ~680 LOC new, ~180 LOC edited. One new file, three edited.

### Module responsibilities

- **`osc_intercept.rs`**: pure byte-state-machine. No I/O, no platform deps. All tests run without a PTY.
- **`alacritty_view.rs`**: owns the reader thread lifecycle; routes bytes through the interceptor; forwards events.
- **`manager.rs`**: consumes events and updates tab state. Kept separate so interceptor stays unit-testable.

## 6. Commands / keystrokes

No new user-visible commands or keystrokes. OSC integration is invisible plumbing that makes existing features (preview picker cwd, agent status dots) more reliable.

## 7. Testing strategy

### Unit tests — `osc_intercept.rs`

Byte-level parser tests (no PTY, no alacritty):

**OSC 7:**
- `ESC ] 7 ; file://hostname/home/user/project ST` → `WorkingDirectory("/home/user/project")`
- `ESC ] 7 ; file:///home/user/project ST` → authority-less form
- Percent-encoded path: `file:///home/user/my%20project` → `"/home/user/my project"`
- Non-file URI: `http://example.com` → silently ignored (no event, passes through)
- Malformed prefix: `ESC ] 7 ; garbage ST` → passes through

**OSC 133:**
- `ESC ] 133 ; A ST` → `PromptStart`
- `ESC ] 133 ; B ST` → `CommandStart`
- `ESC ] 133 ; C ST` → `CommandExecuting`
- `ESC ] 133 ; D ST` → `CommandFinished(None)` — no exit code
- `ESC ] 133 ; D ; 0 ST` → `CommandFinished(Some(0))`
- `ESC ] 133 ; D ; 1 ST` → `CommandFinished(Some(1))`
- `ESC ] 133 ; D ; -1 ST` → gracefully handles signed variants
- `ESC ] 133 ; X ST` (unknown subcommand) → passes through unchanged

**ST variants:**
- BEL terminator: `ESC ] 7 ; file:///path \x07` → parses correctly
- String Terminator: `ESC ] 7 ; file:///path ESC \` → parses correctly

**Pass-through correctness:**
- OSC 0 / 2 (title) — unchanged bytes, no events (alacritty still sees them)
- OSC 4 / 10 / 11 (color palette) — unchanged
- OSC 52 (clipboard) — unchanged
- Plain ASCII run with embedded `ESC ] 7` mid-stream — OSC extracted, surrounding bytes intact byte-for-byte

**Adversarial:**
- OSC buffer overflow (> 64 KB payload) — abort + resume in Ground state, no crash
- Partial OSC split across two `process()` calls — state preserved, event fires when ST arrives
- Nested ESCs / double ST — state machine doesn't deadlock

Target: 25+ tests, each pinning one invariant.

### Integration tests — `crates/amux-platform/tests/osc_smoke.rs` (new file)

- Spawn real `/bin/sh`, emit OSC 7, verify `active_cwd` returns the new path within 100ms.
- Spawn real shell with OSC 133 emitter (small script: `printf '\e]133;A\e\\'` etc.), run `false`, verify `CommandFinished(Some(1))` observed.
- Spawn shell that emits neither — verify regex fallback still works (agent status dot eventually changes).

### Regression locks
- All 157 existing unit tests stay green.
- `cargo test --workspace` green.
- `pty_smoke.rs` integration test unchanged.
- `render_grid_layout.rs` tests unchanged (terminal rendering invariants still hold).

## 8. Code style

- Match existing amux conventions (Rust 2024, no ornamental comments, `pub(crate)` by default).
- `OscInterceptor::process` is pure — no I/O, no allocations beyond the filtered output vec and buffer reuse.
- State machine uses `match` on `ParseState`, one arm per state, no nested matches.
- Percent-decoding isolated to a small helper with its own tests.
- No `unsafe`. No `unwrap` in the parse path — malformed input silently aborts and resumes.

## 9. Boundaries

### Always
- Preserve every existing fallback. Removing any backward-compat path is out of scope.
- Keep `OscInterceptor` platform-agnostic. All platform-specific code (PTY I/O, process cwd) stays in existing files.
- New event types introduced here are `#[non_exhaustive]` so we can add variants later without breaking.
- `amux-platform` must stay `no-gpui`: OSC types cross the crate boundary by value or through channels, not via GPUI entities.

### Ask first
- Before touching the reader-thread lifetime / shutdown path — PTY reader is the fiddly part of any terminal emulator and misordering teardown leaks threads.
- Before changing `AlacrittyTerminal`'s public API — multiple consumers (gpui_terminal, preview_open, etc.) read from it.
- Before introducing any blocking syscall on the reader thread. Currently it reads + parses + sends — fast. A `proc_pidinfo` call added here would serialize PTY output.
- Before touching the regex-based agent status detection logic — it stays as the fallback.

### Never
- Never alter alacritty's VTE parser behavior. We inject *before* it; we don't fork it.
- Never drop passthrough bytes. If the interceptor can't parse something, the bytes flow through to alacritty unchanged. An unknown OSC is not our problem.
- Never block the reader thread on a channel full-send. If the event consumer is slow, drop or coalesce; reader stalling stalls PTY output.
- Never enable OSC 133 path globally based on a config flag. It's automatic-per-tab based on actual shell behavior.
- Never percent-decode OSC 133 payloads — subcommands are pure ASCII; decoding would create fake values.

## 10. Risk register

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Reader thread panics on malformed bytes | low | `OscInterceptor` has unit tests for every malformed case documented by termy + xterm spec; extra fuzzer run before merge. |
| OSC 7 path encoding broken on Windows (backslash / drive letters) | medium | WSL already returns `/mnt/c/...` — we decode URI then pass through existing `maybe_convert_wsl_path`. Integration test on Windows. |
| Percent-decoding hot path — perf on noisy OSC streams | low | OSCs are rare (≤1 per prompt cycle); short paths; profile if metrics show it. |
| OSC 133 exit code parsing wrong on shells that emit `D` without exit | low | Spec-compliant: `D` alone = `CommandFinished(None)`; tested. |
| Channel overflow (OSC events faster than main thread drains) | low | Use `crossbeam-channel` bounded queue at 1024; drop on overflow with eprintln warning. Alternative: coalesce consecutive events. |
| Breaking existing OSC 0/2 title handling | medium | Interceptor only extracts 7/133; everything else passes through byte-for-byte. Unit test pins title passthrough. |
| alacritty's processor state gets confused by our filtering | low | `OscInterceptor` always removes the complete OSC (from `ESC ]` to `ST`) atomically. No partial removal. |
| Reader thread teardown races with tab close | medium | Reader exits on channel disconnect. Add unit test for spawn → drop → verify no thread leak. |

## 11. Implementation plan

Suggested ordering, each step independently verifiable:

1. **`OscInterceptor` state machine** (day 1)
   - Byte state machine + parse helpers + event types.
   - 25+ unit tests pass.
   - Zero integration yet; stands alone.

2. **Reader-thread rewire in `alacritty_view.rs`** (day 1)
   - Replace `EventLoop::spawn` with manual reader thread.
   - Interceptor inserted between read and `Processor.advance`.
   - No behavior change yet — OSC events collected but not consumed.
   - Existing tests stay green.

3. **OSC 7 wiring** (day 0.5)
   - `PaneTab.shell_reported_cwd` field + setter.
   - Event dispatcher in `poll_activity` updates it.
   - `active_tab_live_cwd` prepends OSC 7 to the fallback chain.
   - Integration smoke test: shell emits OSC 7 → `active_cwd` sees it.

4. **OSC 133 wiring** (day 0.5)
   - `PaneTab.shell_integration_phase` field.
   - Event dispatcher updates the phase.
   - `detect_agent_status` checks `shell_integration_phase` first, falls through to regex.
   - Integration smoke test: scripted OSC 133 emission → observable agent_status transitions.

5. **Polish + regression pass** (day 0.5)
   - `cargo test --workspace` green.
   - Manual: open preview on a fresh `cd` to a git repo outside home — picker base dir is correct.
   - Manual: run `claude` inside an amux tab, observe dot transitions matching claude prompt cycles.
   - Documentation: `CLAUDE.md` note on supported OSC sequences + fallbacks.

Total: ~3 days of focused work. Splittable if integration surprises surface in step 2.

## 12. Open questions

- **OSC 133 scope subcommands**: do we want to parse the optional parameters (`133;D;exit_code;command_duration_ms` on some shells)? Starter scope is exit_code only; duration added iff demand surfaces.
- **OSC 7 + WSL**: when the shell runs in WSL and emits `file:///home/user/...`, amux already converts via `maybe_convert_wsl_path`. Need an integration test on Windows to prove the path round-trips.

These don't block the initial ship — safe defaults (ignore extra 133 params, WSL handling reuses existing code) cover both.
