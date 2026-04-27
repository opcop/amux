//! Byte-level OSC sequence interceptor sitting between the PTY read
//! buffer and alacritty's VTE parser. See
//! `plans/osc-integration-spec.md` for the full design.
//!
//! Extracts two sequence families alacritty's current `Handler` drops:
//! * **OSC 7** — `ESC ] 7 ; file://host/path ST` — working-directory
//!   signal emitted by modern shells after every `cd`.
//! * **OSC 133** — shell integration lifecycle: `133;A` prompt start,
//!   `133;B` command start, `133;C` command executing, `133;D[;exit]`
//!   command finished. Emitted by shells with `vscode-shell-integration`,
//!   Kitty, WezTerm, iTerm2 shell scripts, etc.
//!
//! Every other OSC (0/2 title, 4/10/11 palette, 52 clipboard, 9 / 9;4
//! / 777 notifications that we don't handle yet) passes through
//! byte-for-byte so alacritty sees it unchanged. Non-OSC bytes flow
//! through without even entering the state machine.
//!
//! Scope boundaries this module holds to (spec §9 "Never"):
//! * No I/O. `process(bytes)` is a pure function of state + input.
//! * No `unsafe`, no `unwrap` in parse paths. Malformed payloads
//!   abort the current OSC silently, emit whatever we had as
//!   passthrough, and resume `Ground` state.
//! * No allocation growth beyond the output `Vec` and a reused
//!   payload buffer capped at `MAX_OSC_BUFFER`.

/// Events extracted from OSC sequences we care about. Everything
/// else passes through the output stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscEvent {
    /// OSC 7 — cwd update. Payload is the path part of a `file://`
    /// URL, percent-decoded. Host component is stripped (we treat
    /// both `file:///abs` and `file://host/abs` as the same thing).
    WorkingDirectory(String),
    /// OSC 133;A — prompt about to be printed / shell ready for
    /// input. Treat as "idle from the shell's perspective".
    PromptStart,
    /// OSC 133;B — command line has been entered, shell is about to
    /// execute. Rare in practice (most shells only emit A and D).
    CommandStart,
    /// OSC 133;C — command is now executing. Analogous to
    /// `CommandStart` but emitted at a different point in the cycle
    /// depending on the shell.
    CommandExecuting,
    /// OSC 133;D[;exit_code] — command finished. Exit code is
    /// `None` when the shell didn't report one (zsh sometimes omits,
    /// bash + integrations always include).
    CommandFinished(Option<i32>),
}

/// Byte-stream state machine for OSC extraction. Constructed once per
/// terminal; `process` is called with every chunk of PTY output.
#[derive(Debug, Default)]
pub struct OscInterceptor {
    /// Accumulated OSC payload — bytes between `ESC ]` and the
    /// terminator. Cleared after every OSC dispatch (intercept or
    /// passthrough) or overflow abort.
    buffer: Vec<u8>,
    state: ParseState,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    /// Passthrough: bytes copy straight to output.
    #[default]
    Ground,
    /// Saw `0x1B` (ESC) — might be the start of an OSC or any other
    /// escape sequence (CSI, SS3, single-char escape).
    Escape,
    /// Saw `ESC ]` — next byte is the first of the OSC payload.
    OscStart,
    /// Accumulating payload bytes until we see a terminator (BEL or
    /// ST = `ESC \`).
    OscPayload,
    /// Saw `ESC` while inside an OSC — might be ST completion (next
    /// byte `\`) or a literal ESC that happens to be inside the
    /// payload (rare but not forbidden by xterm's grammar).
    OscEscape,
}

/// Cap on the OSC payload buffer. Realistic OSCs (title, cwd, even
/// OSC 52 clipboard) fit in well under this. Beyond 64 KB we bail —
/// either a pathological app or malicious input. Aborting emits the
/// partial payload as passthrough, which alacritty also handles
/// tolerantly.
const MAX_OSC_BUFFER: usize = 64 * 1024;

impl OscInterceptor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw PTY bytes through the interceptor.
    ///
    /// Returns `(passthrough_bytes, events)`. The caller forwards
    /// `passthrough_bytes` to alacritty's `Processor.advance` and
    /// consumes `events` via the app's event bus.
    ///
    /// State is preserved across calls — a partial OSC split across
    /// two read chunks is accumulated until the terminator arrives.
    pub fn process(&mut self, input: &[u8]) -> (Vec<u8>, Vec<OscEvent>) {
        let mut output = Vec::with_capacity(input.len());
        let mut events = Vec::new();

        for &byte in input {
            match self.state {
                ParseState::Ground => {
                    if byte == 0x1B {
                        self.state = ParseState::Escape;
                    } else {
                        output.push(byte);
                    }
                }
                ParseState::Escape => {
                    if byte == b']' {
                        // ESC ] — OSC starts. Reset the payload
                        // buffer and enter OscStart.
                        self.buffer.clear();
                        self.state = ParseState::OscStart;
                    } else {
                        // Not an OSC — pass the ESC + this byte
                        // through unchanged so alacritty's parser
                        // can handle whatever escape sequence this
                        // is (CSI, SS3, bare ESC, etc.).
                        output.push(0x1B);
                        output.push(byte);
                        self.state = ParseState::Ground;
                    }
                }
                ParseState::OscStart => {
                    // First payload byte — push and flow to
                    // OscPayload. No "sniff ahead" to decide intercept
                    // vs passthrough; we buffer the whole payload
                    // and check the prefix at terminator time. This
                    // is simpler and the buffer cap bounds memory.
                    self.buffer.push(byte);
                    self.state = ParseState::OscPayload;
                }
                ParseState::OscPayload => {
                    if byte == 0x07 {
                        // BEL terminator — dispatch.
                        self.dispatch_osc(&mut output, &mut events);
                        self.state = ParseState::Ground;
                    } else if byte == 0x1B {
                        // Might be the start of ST (ESC \). Defer
                        // until we see the next byte.
                        self.state = ParseState::OscEscape;
                    } else if self.buffer.len() < MAX_OSC_BUFFER {
                        self.buffer.push(byte);
                    } else {
                        // Overflow — flush whatever we have as
                        // passthrough and give up on this OSC. Safe
                        // because alacritty will re-enter its own
                        // recovery path when it sees the raw bytes.
                        self.flush_as_passthrough(&mut output);
                        self.state = ParseState::Ground;
                    }
                }
                ParseState::OscEscape => {
                    if byte == b'\\' {
                        // ESC \ = ST terminator — dispatch.
                        self.dispatch_osc(&mut output, &mut events);
                        self.state = ParseState::Ground;
                    } else if self.buffer.len() + 2 <= MAX_OSC_BUFFER {
                        // False alarm — the ESC was part of the
                        // payload. Push both bytes and resume
                        // OscPayload.
                        self.buffer.push(0x1B);
                        self.buffer.push(byte);
                        self.state = ParseState::OscPayload;
                    } else {
                        // Overflow — flush and abort.
                        self.flush_as_passthrough(&mut output);
                        self.state = ParseState::Ground;
                    }
                }
            }
        }

        (output, events)
    }

    /// Dispatch the accumulated OSC payload: if it's OSC 7 or OSC 133
    /// we emit an event and drop the bytes; otherwise we flush the
    /// payload back to the output stream as a normal OSC terminated
    /// with BEL.
    ///
    /// Normalizing the terminator to BEL (rather than preserving the
    /// original BEL vs ST choice) simplifies the implementation and
    /// is compatible with every OSC consumer we've seen — xterm,
    /// alacritty, iTerm2, Terminal.app all accept either.
    fn dispatch_osc(&mut self, output: &mut Vec<u8>, events: &mut Vec<OscEvent>) {
        if let Some(event) = parse_osc_payload(&self.buffer) {
            events.push(event);
        } else {
            self.flush_as_passthrough(output);
        }
        self.buffer.clear();
    }

    /// Emit the current payload buffer back to the output stream
    /// wrapped as `ESC ] payload BEL`. Used for both "we don't care
    /// about this OSC" and "overflow, give up" paths.
    fn flush_as_passthrough(&mut self, output: &mut Vec<u8>) {
        output.push(0x1B);
        output.push(b']');
        output.extend_from_slice(&self.buffer);
        output.push(0x07);
        self.buffer.clear();
    }
}

/// Classify an OSC payload into one of our known events, or `None`
/// if we don't recognize it (caller will pass it through as a normal
/// OSC).
fn parse_osc_payload(buffer: &[u8]) -> Option<OscEvent> {
    let payload = std::str::from_utf8(buffer).ok()?;
    if let Some(rest) = payload.strip_prefix("7;") {
        // OSC 7 — file:// URL. `parse_file_url` returns `None` for
        // non-file schemes; in that case we skip event emission so
        // the passthrough path kicks in (spec: non-file URIs pass
        // through unchanged to alacritty, which ignores them).
        return parse_file_url(rest).map(OscEvent::WorkingDirectory);
    }
    if let Some(rest) = payload.strip_prefix("133;") {
        return parse_osc_133(rest);
    }
    None
}

/// Parse a `file://[host]/path` URL into the raw filesystem path.
///
/// Strips an optional authority (`host`) and percent-decodes the
/// path. Returns `None` for URLs we can't interpret — non-file
/// schemes, empty paths, paths with invalid percent escapes. The
/// caller treats `None` as "pass the OSC through unchanged."
fn parse_file_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    // After `file://`, the next `/` begins the absolute path. Any
    // characters between `file://` and that `/` form the authority
    // (hostname); we discard it because cwd is always the local FS.
    let path_start = rest.find('/')?;
    let encoded_path = &rest[path_start..];
    let decoded = percent_decode(encoded_path)?;
    // Trim trailing terminator fragment artifacts (shouldn't happen
    // post-state-machine, but cheap to guard).
    let trimmed = decoded.trim_end_matches(|c: char| c.is_control());
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Percent-decode a UTF-8 string. Valid `%XX` escapes where `XX` is
/// a byte pair become a single byte; everything else copies through.
///
/// Returns `None` on malformed escapes (`%` without two hex digits,
/// or escape bytes that don't combine into valid UTF-8). Strict
/// handling here is fine: shells almost always emit ASCII paths, and
/// a broken escape sequence in a cwd is a good reason to reject the
/// whole event rather than trust the partial decode.
fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_digit(bytes[i + 1])?;
            let lo = hex_digit(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Parse the subcommand + params portion of an OSC 133 payload
/// (everything after the `133;` prefix).
///
/// Unknown subcommands return `None`, which lets the payload pass
/// through as a normal OSC so apps that innovate on 133 (e.g. with
/// `;E` or `;P` vendor extensions) don't silently drop state.
fn parse_osc_133(rest: &str) -> Option<OscEvent> {
    let mut chars = rest.chars();
    let subcommand = chars.next()?;
    match subcommand {
        'A' => Some(OscEvent::PromptStart),
        'B' => Some(OscEvent::CommandStart),
        'C' => Some(OscEvent::CommandExecuting),
        'D' => {
            // 133;D may stand alone or be `D;exit_code[;extras]`.
            // Extras (command duration, etc.) are ignored per spec
            // §12 open question — starter scope is exit code only.
            let exit_code = rest
                .strip_prefix("D;")
                .and_then(|params| params.split(';').next())
                .and_then(|first| first.parse::<i32>().ok());
            Some(OscEvent::CommandFinished(exit_code))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: run bytes through a fresh interceptor, return stringified
    /// output + events. Simplifies test readability.
    fn run(input: &[u8]) -> (Vec<u8>, Vec<OscEvent>) {
        let mut it = OscInterceptor::new();
        it.process(input)
    }

    // ─── Passthrough correctness ───────────────────────────────────

    #[test]
    fn plain_ascii_passes_through() {
        let (out, events) = run(b"hello world");
        assert_eq!(out, b"hello world");
        assert!(events.is_empty());
    }

    #[test]
    fn unknown_osc_passes_through_verbatim() {
        // OSC 0 (title) is not in our intercept set — it must reach
        // alacritty unchanged so the existing title-tracking code
        // keeps working. BEL terminator variant.
        let (out, events) = run(b"\x1b]0;My Title\x07");
        assert_eq!(out, b"\x1b]0;My Title\x07");
        assert!(events.is_empty());
    }

    #[test]
    fn unknown_osc_with_st_terminator_passes_through_as_bel() {
        // Input: ESC ] 2 ; title ESC \ — ST terminator form.
        // We normalize the terminator to BEL on passthrough; both
        // are accepted by alacritty.
        let (out, events) = run(b"\x1b]2;Other\x1b\\");
        assert_eq!(out, b"\x1b]2;Other\x07");
        assert!(events.is_empty());
    }

    #[test]
    fn bare_escape_passes_through() {
        // ESC not followed by ] should pass the ESC + next byte
        // through — e.g. ESC c (full reset), ESC 7 (save cursor).
        let (out, events) = run(b"\x1bc");
        assert_eq!(out, b"\x1bc");
        assert!(events.is_empty());
    }

    #[test]
    fn mixed_stream_preserves_non_osc_bytes() {
        // Plain text → OSC 7 → more text. OSC extracted; text
        // intact byte-for-byte.
        let (out, events) = run(b"before\x1b]7;file:///tmp\x07after");
        assert_eq!(out, b"beforeafter");
        assert_eq!(
            events,
            vec![OscEvent::WorkingDirectory("/tmp".to_string())]
        );
    }

    // ─── OSC 7 ─────────────────────────────────────────────────────

    #[test]
    fn osc_7_with_hostname_strips_authority() {
        let (_, events) = run(b"\x1b]7;file://localhost/home/user\x07");
        assert_eq!(
            events,
            vec![OscEvent::WorkingDirectory("/home/user".to_string())]
        );
    }

    #[test]
    fn osc_7_authority_less_form() {
        let (_, events) = run(b"\x1b]7;file:///home/user\x07");
        assert_eq!(
            events,
            vec![OscEvent::WorkingDirectory("/home/user".to_string())]
        );
    }

    #[test]
    fn osc_7_percent_decodes_spaces() {
        let (_, events) = run(b"\x1b]7;file:///home/my%20project\x07");
        assert_eq!(
            events,
            vec![OscEvent::WorkingDirectory("/home/my project".to_string())]
        );
    }

    #[test]
    fn osc_7_percent_decodes_cjk() {
        // "你好" UTF-8: e4 bd a0 e5 a5 bd → percent-encoded as
        // %E4%BD%A0%E5%A5%BD.
        let (_, events) = run(b"\x1b]7;file:///%E4%BD%A0\x07");
        assert_eq!(
            events,
            vec![OscEvent::WorkingDirectory("/\u{4f60}".to_string())]
        );
    }

    #[test]
    fn osc_7_non_file_scheme_passes_through() {
        // http:// URL in OSC 7 is nonsense. Don't emit an event;
        // the payload flows through as a normal OSC for alacritty
        // to silently discard.
        let (out, events) = run(b"\x1b]7;http://example.com/\x07");
        assert_eq!(out, b"\x1b]7;http://example.com/\x07");
        assert!(events.is_empty());
    }

    #[test]
    fn osc_7_missing_path_passes_through() {
        // `file://hostname` with no path — no slash after host.
        let (out, events) = run(b"\x1b]7;file://localhost\x07");
        assert_eq!(out, b"\x1b]7;file://localhost\x07");
        assert!(events.is_empty());
    }

    #[test]
    fn osc_7_malformed_percent_encoding_passes_through() {
        // `%` not followed by two hex digits — reject decode and
        // pass the raw payload through.
        let (out, events) = run(b"\x1b]7;file:///foo%ZZ\x07");
        assert_eq!(out, b"\x1b]7;file:///foo%ZZ\x07");
        assert!(events.is_empty());
    }

    #[test]
    fn osc_7_st_terminator_variant() {
        let (_, events) = run(b"\x1b]7;file:///tmp\x1b\\");
        assert_eq!(
            events,
            vec![OscEvent::WorkingDirectory("/tmp".to_string())]
        );
    }

    // ─── OSC 133 ───────────────────────────────────────────────────

    #[test]
    fn osc_133_prompt_start() {
        let (_, events) = run(b"\x1b]133;A\x07");
        assert_eq!(events, vec![OscEvent::PromptStart]);
    }

    #[test]
    fn osc_133_command_start() {
        let (_, events) = run(b"\x1b]133;B\x07");
        assert_eq!(events, vec![OscEvent::CommandStart]);
    }

    #[test]
    fn osc_133_command_executing() {
        let (_, events) = run(b"\x1b]133;C\x07");
        assert_eq!(events, vec![OscEvent::CommandExecuting]);
    }

    #[test]
    fn osc_133_command_finished_no_exit() {
        let (_, events) = run(b"\x1b]133;D\x07");
        assert_eq!(events, vec![OscEvent::CommandFinished(None)]);
    }

    #[test]
    fn osc_133_command_finished_zero_exit() {
        let (_, events) = run(b"\x1b]133;D;0\x07");
        assert_eq!(events, vec![OscEvent::CommandFinished(Some(0))]);
    }

    #[test]
    fn osc_133_command_finished_nonzero_exit() {
        let (_, events) = run(b"\x1b]133;D;1\x07");
        assert_eq!(events, vec![OscEvent::CommandFinished(Some(1))]);
    }

    #[test]
    fn osc_133_command_finished_with_extra_params_ignores_them() {
        // Some shells (fish, kitty) emit `D;exit;duration_ms`.
        // We parse only the first param for now (spec §12 open
        // question — duration added iff demand surfaces).
        let (_, events) = run(b"\x1b]133;D;42;1500\x07");
        assert_eq!(events, vec![OscEvent::CommandFinished(Some(42))]);
    }

    #[test]
    fn osc_133_unknown_subcommand_passes_through() {
        // Vendor extensions like `133;E` or `133;P` should pass
        // through rather than silently disappear.
        let (out, events) = run(b"\x1b]133;E;foo\x07");
        assert_eq!(out, b"\x1b]133;E;foo\x07");
        assert!(events.is_empty());
    }

    #[test]
    fn osc_133_st_terminator_variant() {
        let (_, events) = run(b"\x1b]133;A\x1b\\");
        assert_eq!(events, vec![OscEvent::PromptStart]);
    }

    // ─── Stateful / split-chunk behavior ───────────────────────────

    #[test]
    fn osc_split_across_two_process_calls() {
        // Reader thread receives PTY output in arbitrary chunks —
        // a single OSC may straddle two reads. The state machine
        // must preserve the buffer across calls.
        let mut it = OscInterceptor::new();
        let (out1, events1) = it.process(b"\x1b]7;file:///tm");
        assert!(out1.is_empty(), "partial OSC must not leak to output");
        assert!(events1.is_empty());
        let (out2, events2) = it.process(b"p\x07after");
        assert_eq!(out2, b"after");
        assert_eq!(
            events2,
            vec![OscEvent::WorkingDirectory("/tmp".to_string())]
        );
    }

    #[test]
    fn osc_split_across_st_halves() {
        // ESC arrives in one chunk, \ in the next. Exercises the
        // OscEscape → ST transition across a chunk boundary.
        let mut it = OscInterceptor::new();
        let (out1, events1) = it.process(b"\x1b]133;A\x1b");
        assert!(out1.is_empty());
        assert!(events1.is_empty());
        let (_, events2) = it.process(b"\\");
        assert_eq!(events2, vec![OscEvent::PromptStart]);
    }

    #[test]
    fn multiple_oscs_in_one_chunk() {
        // Shell prompt cycle: OSC 133;A, OSC 7, OSC 0 (title),
        // OSC 133;B often fire back-to-back. All must be extracted
        // or passed-through in order.
        let input = b"\x1b]133;A\x07\x1b]7;file:///tmp\x07\x1b]0;title\x07\x1b]133;B\x07";
        let (out, events) = run(input);
        // OSC 0 passes through; 133;A, 7, 133;B extract.
        assert_eq!(out, b"\x1b]0;title\x07");
        assert_eq!(
            events,
            vec![
                OscEvent::PromptStart,
                OscEvent::WorkingDirectory("/tmp".to_string()),
                OscEvent::CommandStart,
            ]
        );
    }

    #[test]
    fn esc_inside_osc_is_literal_not_st() {
        // Pathological input: ESC ] payload ESC a (ESC followed by
        // non-`\`) means the ESC is a literal byte, not the start
        // of ST. Our OscEscape state handles this by pushing both
        // bytes back into the payload. Since the resulting buffer
        // doesn't match any known prefix, it passes through.
        let (out, _) = run(b"\x1b]0;a\x1bb\x07");
        // The OSC body becomes "0;a" + ESC + "b", flushed as
        // ESC ] 0;a ESC b BEL.
        assert_eq!(out, b"\x1b]0;a\x1bb\x07");
    }

    // ─── Adversarial / overflow ────────────────────────────────────

    #[test]
    fn buffer_overflow_aborts_and_recovers() {
        // 100KB payload > MAX_OSC_BUFFER (64KB). Overflow should
        // flush the partial payload as passthrough and drop us back
        // into Ground so subsequent data parses normally.
        let mut input = Vec::from(b"\x1b]0;");
        input.extend(std::iter::repeat(b'a').take(100 * 1024));
        input.extend_from_slice(b"\x07after");
        let mut it = OscInterceptor::new();
        let (out, events) = it.process(&input);
        // No events for the aborted OSC.
        assert!(events.is_empty());
        // After abort, subsequent bytes flow normally.
        assert!(out.ends_with(b"after"));
    }

    #[test]
    fn invalid_utf8_payload_passes_through() {
        // OSC payload with invalid UTF-8 can't be our OSC 7/133
        // (both are ASCII-only prefixes), so it must pass through.
        let input = b"\x1b]7;\xFF\xFE\x07";
        let (out, events) = run(input);
        assert_eq!(out, input);
        assert!(events.is_empty());
    }

    // ─── End-to-end sanity ────────────────────────────────────────

    #[test]
    fn realistic_prompt_cycle_emits_expected_events() {
        // What a zsh + vscode-shell-integration prompt cycle looks
        // like post-enter-keypress:
        //   OSC 133;C (command executing)
        //   command output (plain text)
        //   OSC 133;D;0 (finished, exit 0)
        //   OSC 7 (cwd maybe changed)
        //   OSC 0 (title)
        //   OSC 133;A (prompt start again)
        let input = b"\x1b]133;C\x07output\n\x1b]133;D;0\x07\x1b]7;file:///home\x07\x1b]0;zsh\x07\x1b]133;A\x07";
        let (out, events) = run(input);
        // `output\n` + OSC 0 passthrough.
        assert_eq!(out, b"output\n\x1b]0;zsh\x07");
        assert_eq!(
            events,
            vec![
                OscEvent::CommandExecuting,
                OscEvent::CommandFinished(Some(0)),
                OscEvent::WorkingDirectory("/home".to_string()),
                OscEvent::PromptStart,
            ]
        );
    }
}
