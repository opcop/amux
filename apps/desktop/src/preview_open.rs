//! File preview open path: CWD resolution, WSL path conversion,
//! cursor-path extraction, file picker and preview launch.
//!
//! Everything the user triggers to *open* a file for preview lives
//! here — Ctrl+P, `amux preview`, right-click on a path in the
//! terminal, and the dispatch that runs after the user picks a
//! file. The rendering side of preview tabs still lives in
//! `crate::gpui_preview` (which this module calls into).
//!
//! ## Shape
//!
//! Free functions taking `&GpuiShellView` / `&mut GpuiShellView`,
//! same convention as `crate::search` and `crate::menu`. The pure
//! helpers (`extract_cwd_from_prompt_line`, `expand_tilde`) do not
//! touch the view at all and are directly unit-tested.
//!
//! ## CWD resolution chain
//!
//! Getting the "right" current directory for the active pane is
//! trickier than it sounds — especially under WSL, where the
//! process table lies and the real cwd only shows up in the
//! prompt. `resolve_active_cwd` walks:
//!
//!   1. Parse the terminal's current prompt line (most reliable).
//!   2. Live process cwd via sysinfo / `/proc/PID/cwd`.
//!   3. Saved spawn-time cwd from the tab record.
//!
//! `resolve_best_cwd` layers a git-root walk on top so file
//! pickers default to the repo root instead of a subdirectory.

#![cfg(feature = "gpui")]

use crate::gpui_entry::GpuiShellView;

// ─── Pure helpers ──────────────────────────────────────────────

/// Cheap "could this be a path?" test. Used by both the title
/// parser and the prompt-char branches of the prompt-line parser.
/// Accepts absolute Unix paths, `~/…` home-relative paths, `~`
/// alone, and Windows drive-letter paths (`C:\…`, `d:/…`).
fn looks_like_path(s: &str) -> bool {
    if s.is_empty() { return false; }
    if s.starts_with('/') || s.starts_with("~/") || s == "~" { return true; }
    let bytes = s.as_bytes();
    if bytes.len() >= 3 && bytes[1] == b':' {
        let first = bytes[0] as char;
        if first.is_ascii_alphabetic()
            && (bytes[2] == b'\\' || bytes[2] == b'/')
        {
            return true;
        }
    }
    false
}

/// Extract the working directory from a terminal title. Many shells
/// put `$PWD` in the title (via `PROMPT_COMMAND` on bash, `precmd`
/// on zsh, default fish behavior) — this gives us a reliable CWD
/// signal without needing OSC 7 parsing, which alacritty_terminal
/// 0.25 doesn't expose.
///
/// Handles the common title layouts:
///   `~/proj`                    (bare path)
///   `user@host: ~/proj`         (xterm default)
///   `zsh: ~/proj`               (zsh title prompt)
///   `shell — ~/proj`            (em-dash separator)
///   `nvim — ~/proj/file.rs`     (editor title, may return file path
///                                — resolve_active_cwd's is_dir()
///                                check rejects it and falls through)
///
/// The strategy is deliberately simple: try the whole title, then
/// the tail after each common separator. Return the first candidate
/// that `looks_like_path`. Validation against the real filesystem
/// is done by the caller.
pub(crate) fn extract_cwd_from_title(title: &str) -> Option<String> {
    let title = title.trim();
    if title.is_empty() { return None; }

    // Candidate substrings: whole title first, then the tail after
    // each common separator. Using `rsplit_once` so we pick up the
    // right-hand side (where titles typically put the path).
    let mut candidates: Vec<&str> = Vec::with_capacity(6);
    candidates.push(title);
    for sep in [": ", " — ", " - ", " | ", "> "] {
        if let Some((_, tail)) = title.rsplit_once(sep) {
            candidates.push(tail.trim());
        }
    }

    for cand in candidates {
        if looks_like_path(cand) {
            return Some(expand_tilde(cand));
        }
    }
    None
}

/// Extract the working directory from a terminal prompt line.
/// Pure — no terminal access, trivially testable.
///
/// Supports:
///   PowerShell:    "PS C:\Users\foo\project> amux preview"
///   Bash/Zsh:      "user@host:~/project$ amux preview"
///   Zsh short:     "~/project% amux preview"
///   Starship/fish: "~/project ❯ amux preview"   (also »  ▶)
pub(crate) fn extract_cwd_from_prompt_line(line: &str) -> Option<String> {
    // PowerShell: "PS C:\path> ..." or "PS D:\path>"
    if let Some(ps_start) = line.find("PS ") {
        let after_ps = &line[ps_start + 3..];
        if let Some(gt) = after_ps.find('>') {
            let path = after_ps[..gt].trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }

    // Bash/Zsh: "user@host:~/dir$ cmd" or "user@host:/path$" (no command typed)
    if let Some(colon) = line.find(':') {
        if line[..colon].contains('@') {
            let after_colon = &line[colon + 1..];
            // Find $ or % that ends the path — with or without trailing space
            let end = after_colon.find("$ ")
                .or_else(|| after_colon.find("% "))
                .or_else(|| {
                    // No space after $ — prompt with nothing typed, or $ at end of line
                    let trimmed = after_colon.trim_end();
                    if trimmed.ends_with('$') {
                        Some(trimmed.len() - 1)
                    } else if trimmed.ends_with('%') {
                        Some(trimmed.len() - 1)
                    } else {
                        None
                    }
                });
            if let Some(pos) = end {
                let path = after_colon[..pos].trim();
                if !path.is_empty() {
                    return Some(expand_tilde(path));
                }
            }
        }
    }

    // Simple zsh: "~/project% cmd" or "/path%" (no command)
    if let Some(pct) = line.find('%') {
        if pct < 120 {
            let path = line[..pct].trim();
            if !path.is_empty() && (path.starts_with('/') || path.starts_with('~') || path.starts_with('\\')) {
                return Some(expand_tilde(path));
            }
        }
    }

    // Starship / oh-my-zsh / fish: path followed by a known prompt
    // character (❯ » ▶). The path is the last whitespace-separated
    // token before the prompt char. We intentionally don't include
    // plain `>` here because it collides with shell redirection.
    for prompt_char in ['❯', '»', '▶'] {
        let mut buf = [0u8; 4];
        let pat = prompt_char.encode_utf8(&mut buf);
        if let Some(idx) = line.find(&*pat) {
            let before = line[..idx].trim_end();
            if let Some(token) = before.split_whitespace().last() {
                if looks_like_path(token) {
                    return Some(expand_tilde(token));
                }
            }
        }
    }

    None
}

/// Expand a leading `~` to `$HOME` (falling back to `$USERPROFILE`
/// on Windows). Returns the input unchanged if it doesn't start
/// with `~` or if neither env var is set.
pub(crate) fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Some(home) = std::env::var("HOME").ok()
            .or_else(|| std::env::var("USERPROFILE").ok())
        {
            return path.replacen('~', &home, 1);
        }
    }
    path.to_string()
}

// ─── WSL path conversion ───────────────────────────────────────

/// Convert a WSL Linux path to a Windows-accessible path.
/// Two cases:
///   /mnt/d/repo/...  → D:\repo\...        (WSL drive mount → native Windows path)
///   /home/user/...   → \\wsl$\Distro\...  (WSL-native → UNC path)
/// On Linux / macOS builds, this is a no-op.
pub(crate) fn maybe_convert_wsl_path(view: &GpuiShellView, path: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        if !path.starts_with('/') {
            return path.to_string();
        }

        // Case 1: /mnt/X/... → X:\...  (drive mount)
        if path.starts_with("/mnt/") && path.len() >= 6 {
            let drive_letter = path.as_bytes()[5]; // the char after "/mnt/"
            if drive_letter.is_ascii_alphabetic()
                && (path.len() == 6 || path.as_bytes()[6] == b'/')
            {
                let rest = if path.len() > 6 { &path[6..] } else { "" };
                let drive = (drive_letter as char).to_uppercase().next().unwrap_or('C');
                let win_path = format!("{}:{}", drive, rest.replace('/', "\\"));
                return win_path;
            }
        }

        // Case 2: /home/... or other WSL-native path → \\wsl$\Distro\...
        let distro = detect_pane_wsl_distro(view)
            .or_else(|| amux_platform::get_default_distro());
        if let Some(distro) = distro {
            return amux_platform::windows::paths::wsl_unc_path(&distro, path);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = view; // suppress unused warning on non-Windows
    }
    path.to_string()
}

/// Check if the active pane is running in WSL and return the distro name.
#[cfg(target_os = "windows")]
fn detect_pane_wsl_distro(view: &GpuiShellView) -> Option<String> {
    let (shell, args) = view.terminal_manager().active_shell_cmd()?;
    if !shell.to_lowercase().contains("wsl") {
        return None;
    }
    for (i, arg) in args.iter().enumerate() {
        if (arg == "-d" || arg == "--distribution") && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }
    amux_platform::get_default_distro()
}

// ─── CWD resolution ────────────────────────────────────────────

/// Extract the working directory from the terminal's current
/// prompt line. Thin wrapper around `extract_cwd_from_prompt_line`
/// — the view access is here, the parsing is pure.
pub(crate) fn extract_cwd_from_prompt(view: &GpuiShellView) -> Option<String> {
    let term = view.terminal_manager().active_terminal_ref()?;
    let line = term.cursor_line_text();
    extract_cwd_from_prompt_line(&line)
}

/// Extract the working directory from the active pane's title
/// (set via OSC 0/2). Many shells put `$PWD` in the title by
/// default or via `PROMPT_COMMAND`; this lets us pick it up without
/// needing OSC 7 support in the underlying terminal emulator.
pub(crate) fn extract_cwd_from_active_title(view: &GpuiShellView) -> Option<String> {
    let term = view.terminal_manager().active_terminal_ref()?;
    let title = term.title()?;
    extract_cwd_from_title(&title)
}

/// Best-effort resolve the current working directory of the
/// active pane. Tries multiple sources in order; the first one
/// whose result exists as a directory wins.
///   1. Terminal title (OSC 0/2). Fast and very accurate when the
///      shell cooperates (default on many bash/zsh configs).
///   2. Parse cwd from the terminal prompt line (PowerShell,
///      Bash/Zsh, Starship/fish).
///   3. Live process cwd (sysinfo on Windows, /proc on Linux).
///   4. Saved spawn-time cwd from the tab (stale after `cd`).
pub(crate) fn resolve_active_cwd(view: &GpuiShellView) -> Option<String> {
    if let Some(cwd) = extract_cwd_from_active_title(view) {
        let resolved = maybe_convert_wsl_path(view, &cwd);
        if std::path::Path::new(&resolved).is_dir() {
            return Some(resolved);
        }
    }

    if let Some(cwd) = extract_cwd_from_prompt(view) {
        let resolved = maybe_convert_wsl_path(view, &cwd);
        if std::path::Path::new(&resolved).is_dir() {
            return Some(resolved);
        }
    }

    if let Some(cwd) = view.terminal_manager().active_process_cwd() {
        let resolved = maybe_convert_wsl_path(view, &cwd);
        if std::path::Path::new(&resolved).is_dir() {
            return Some(resolved);
        }
    }

    if let Some(cwd) = view.terminal_manager().active_saved_cwd() {
        let resolved = maybe_convert_wsl_path(view, &cwd);
        if std::path::Path::new(&resolved).is_dir() {
            return Some(resolved);
        }
    }

    None
}

/// Resolve the "best" CWD for a file picker: same chain as
/// `resolve_active_cwd`, plus a git-root walk so the picker
/// defaults to the repo root instead of a subdirectory.
pub(crate) fn resolve_best_cwd(view: &GpuiShellView) -> Option<String> {
    if let Some(cwd) = resolve_active_cwd(view) {
        if std::path::Path::new(&cwd).join(".git").exists() {
            return Some(cwd);
        }
        let mut dir = std::path::PathBuf::from(&cwd);
        for _ in 0..10 {
            if dir.join(".git").exists() {
                return Some(dir.to_string_lossy().to_string());
            }
            if !dir.pop() { break; }
        }
    }

    // Fallback: GUI process CWD (often the launch folder).
    if let Ok(gui_cwd) = std::env::current_dir() {
        if gui_cwd.join(".git").exists() {
            return Some(gui_cwd.to_string_lossy().to_string());
        }
        let mut dir = gui_cwd.clone();
        for _ in 0..10 {
            if dir.join(".git").exists() {
                return Some(dir.to_string_lossy().to_string());
            }
            if !dir.pop() { break; }
        }
        return Some(gui_cwd.to_string_lossy().to_string());
    }

    resolve_active_cwd(view)
}

// ─── Terminal cell → path extraction ───────────────────────────

/// Decode an OSC 8 `file://[host]/abs/path` URI to a local path.
/// Strips the scheme and optional host, percent-decodes `%XX`
/// escapes. Returns `None` for non-`file://` URIs (http/https etc.
/// aren't opened as file previews in v1).
fn decode_file_uri(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    // Optional authority: `file://host/path` or `file:///path`.
    let path_part = match rest.find('/') {
        Some(idx) => &rest[idx..],
        None => rest,
    };
    // Percent-decode in place. Ignore malformed escapes (leave raw).
    let bytes = path_part.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

/// Per-row visual segment re-exported from `gpui_entry` so this
/// module's type signatures don't depend on the feature-gated UI
/// struct definition.
pub(crate) type HoverSegment = crate::gpui_entry::HoverSegment;

/// Where a path candidate came from. Higher discriminant = higher
/// priority during resolution: we prefer an OSC 8 hyperlink over a
/// markdown link over a quoted string over a bareword. When several
/// sources produce the same existing file, the highest-priority
/// source wins — that's typically the most specific / least-noisy
/// interpretation of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub(crate) enum CandidateSource {
    Bareword = 0,
    Quoted = 1,
    Markdown = 2,
    Hyperlink = 3,
}

/// What kind of target a candidate represents. Drives resolution:
/// `File` goes through CWD × `exists()`, `Url` bypasses FS checks
/// entirely (URLs self-validate by syntax).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateKind {
    File,
    Url,
}

/// A single plausible path interpretation at a click position.
///
/// `display` is the "cleaned" form (no `:L:C`, no backticks, no
/// quotes) used as the primary lookup key. `with_suffix` is the
/// original un-stripped form kept as a fallback — if a file is
/// literally named `foo.tar.gz:42`, stripping would discard it.
/// `segments` is the visual highlight range; `source` drives
/// resolution priority; `kind` splits file vs URL handling.
#[derive(Debug, Clone)]
pub(crate) struct PathCandidate {
    pub display: String,
    pub with_suffix: Option<String>,
    pub segments: Vec<HoverSegment>,
    pub source: CandidateSource,
    pub kind: CandidateKind,
}

/// Result of successfully resolving a click to a real file.
/// `absolute` is ready to hand to `open_preview_file`; `segments`
/// is the visual range for the hover underline.
#[derive(Debug, Clone)]
pub(crate) struct PathHit {
    pub absolute: String,
    pub segments: Vec<HoverSegment>,
}

/// What a successfully-resolved click points at.
#[derive(Debug, Clone)]
pub(crate) enum ClickKind {
    /// Absolute filesystem path, validated to exist.
    File(String),
    /// URL with a supported scheme (http/https). No validation —
    /// syntax is the contract.
    Url(String),
}

/// Unified result for any click in terminal output. The hover path
/// reads `segments` to draw the underline; the click path branches
/// on `kind` to decide whether to open a preview or launch a URL.
#[derive(Debug, Clone)]
pub(crate) struct ClickHit {
    pub kind: ClickKind,
    pub segments: Vec<HoverSegment>,
}

// ─── URL helpers ───────────────────────────────────────────────

/// Return true if `s` starts with a supported URL scheme. v1
/// supports `http://` and `https://` only — other schemes
/// (`mailto:`, `ftp://`, `ssh://`, etc.) fall through and are
/// treated as file candidates by the rest of the pipeline.
pub(crate) fn has_url_scheme(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// Strip trailing punctuation that is almost always a sentence
/// terminator rather than a real part of the URL. Runs in a loop
/// so stacked punctuation (`…foo.com).` → `…foo.com`) gets fully
/// trimmed.
///
/// Rules:
///   * Always trim: `.,;:!?"'>`
///   * Trim `)` only when it would leave parens unbalanced in the
///     remaining prefix — this preserves Wikipedia-style URLs
///     ending in `_(programming_language)` while still removing
///     the stray `)` from `(see http://foo.com)`.
pub(crate) fn trim_url_trailing(raw: &str) -> &str {
    let mut end = raw.len();
    loop {
        let s = &raw[..end];
        let last = match s.chars().next_back() {
            Some(c) => c,
            None => break,
        };
        let is_sentence_punct =
            matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | '"' | '\'' | '>' | ']' | '}');
        let is_unbalanced_close_paren = last == ')' && {
            let opens = s.chars().filter(|&c| c == '(').count();
            let closes = s.chars().filter(|&c| c == ')').count();
            closes > opens
        };
        if !(is_sentence_punct || is_unbalanced_close_paren) {
            break;
        }
        end -= last.len_utf8();
    }
    &raw[..end]
}

/// Open a URL in the user's system default browser/handler.
/// Runs asynchronously via a spawned child process — failure is
/// logged but otherwise ignored, matching the "best-effort click"
/// contract the rest of the pipeline uses.
pub(crate) fn open_url_external(url: &str) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    if let Err(e) = result {
        eprintln!("[amux] failed to open url {url:?}: {e}");
    }
}

/// Extract the hyperlink (OSC 8) at a given cell together with
/// per-row `[start..=end]` segments covering all cells that share
/// the same hyperlink id — including cells on adjacent rows when
/// the CLI-emitted link wraps across the terminal's right edge.
/// Returns `(uri, segments)`.
fn extract_hyperlink_multirow(
    term: &amux_platform::terminal::alacritty_view::AlacrittyTerminal,
    col: usize,
    row: usize,
) -> Option<(String, Vec<HoverSegment>)> {
    term.with_term(|t| {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Line, Column};

        let grid = t.grid();
        let cols = grid.columns();
        let rows = grid.screen_lines();
        if row >= rows || col >= cols { return None; }

        let hl = grid[Line(row as i32)][Column(col)].hyperlink()?;
        let id = hl.id().to_string();
        let uri = hl.uri().to_string();
        let has_id = |r: i32, c: usize| -> bool {
            grid[Line(r)][Column(c)].hyperlink().map(|h| h.id() == id).unwrap_or(false)
        };

        // Current row: expand left/right from click.
        let mut cur_start = col;
        while cur_start > 0 && has_id(row as i32, cur_start - 1) { cur_start -= 1; }
        let mut cur_end = col;
        while cur_end + 1 < cols && has_id(row as i32, cur_end + 1) { cur_end += 1; }
        let mut segments: Vec<HoverSegment> = vec![(row, cur_start, cur_end)];

        // Extend upward while the top segment sits at col 0 and the
        // previous row's last cell carries the same id.
        let mut r = row as i32;
        while r > 0 && segments[0].1 == 0 && has_id(r - 1, cols - 1) {
            let pr = r - 1;
            let mut s = cols - 1;
            while s > 0 && has_id(pr, s - 1) { s -= 1; }
            segments.insert(0, (pr as usize, s, cols - 1));
            r = pr;
            if s != 0 { break; }
        }

        // Extend downward while the bottom segment sits at cols-1
        // and the next row's first cell carries the same id.
        let mut r = row as i32;
        while ((r + 1) as usize) < rows
            && segments.last().unwrap().2 == cols - 1
            && has_id(r + 1, 0)
        {
            let nr = r + 1;
            let mut e = 0;
            while e + 1 < cols && has_id(nr, e + 1) { e += 1; }
            segments.push((nr as usize, 0, e));
            r = nr;
            if e != cols - 1 { break; }
        }

        Some((uri, segments))
    })
}

/// Walk `candidates` in priority order and return the first one
/// whose cleaned-or-raw form resolves to an existing file under
/// any CWD. Pure: `exists` is injected so tests can provide a fake
/// filesystem. The contract is strict — **only real files win**.
///
/// For each candidate we try both `display` and `with_suffix`.
/// Absolute paths bypass the CWD list entirely. Relative paths are
/// joined with each entry in `cwds`; the first hit wins.
///
/// The caller provides `cwds` already deduped and in the order it
/// wants probed — typically `[pane_cwd, pane_git_root, gui_cwd,
/// gui_git_root]`.
pub(crate) fn pick_existing<F: Fn(&str) -> bool>(
    candidates: &[PathCandidate],
    cwds: &[String],
    exists: F,
) -> Option<PathHit> {
    for cand in candidates {
        // URL candidates never resolve as files — skip them here.
        // `resolve_click_at_term` handles URL candidates separately.
        if matches!(cand.kind, CandidateKind::Url) { continue; }
        // Try each form: cleaned first (it's the common case), then
        // the un-stripped form as a fallback for files literally
        // named with `:N` at the end.
        let forms: Vec<&str> = std::iter::once(cand.display.as_str())
            .chain(cand.with_suffix.as_deref())
            .collect();
        for form in forms {
            if form.is_empty() { continue; }
            if std::path::Path::new(form).is_absolute() {
                if exists(form) {
                    return Some(PathHit {
                        absolute: form.to_string(),
                        segments: cand.segments.clone(),
                    });
                }
                continue;
            }
            for cwd in cwds {
                let joined = std::path::PathBuf::from(cwd)
                    .join(form)
                    .to_string_lossy()
                    .to_string();
                if exists(&joined) {
                    return Some(PathHit {
                        absolute: joined,
                        segments: cand.segments.clone(),
                    });
                }
            }
        }
    }
    None
}

/// Build the CWD list used by `pick_existing`. Order:
///   1. Pane's active cwd (parsed from prompt / `/proc` / saved).
///   2. Git root walking up from pane cwd (if found within 10 levels).
///   3. GUI process cwd.
///   4. Git root walking up from GUI cwd.
/// Duplicates are removed while preserving the first occurrence.
fn collect_cwd_bases(view: &GpuiShellView) -> Vec<String> {
    let mut bases: Vec<String> = Vec::new();
    let push = |b: &mut Vec<String>, p: String| {
        if !b.contains(&p) { b.push(p); }
    };

    if let Some(cwd) = resolve_active_cwd(view) {
        push(&mut bases, cwd.clone());
        let mut dir = std::path::PathBuf::from(&cwd);
        for _ in 0..10 {
            if dir.join(".git").exists() {
                push(&mut bases, dir.to_string_lossy().to_string());
                break;
            }
            if !dir.pop() { break; }
        }
    }
    if let Ok(gui_cwd) = std::env::current_dir() {
        push(&mut bases, gui_cwd.to_string_lossy().to_string());
        let mut dir = gui_cwd;
        for _ in 0..10 {
            if dir.join(".git").exists() {
                push(&mut bases, dir.to_string_lossy().to_string());
                break;
            }
            if !dir.pop() { break; }
        }
    }
    bases
}

/// Build a single `PathCandidate` from a raw user-selected string.
/// Used by the right-click "Open Selection" path — the user has
/// already defined the exact character range, so there's no need
/// for window-scanning; we just clean the string and emit one
/// candidate covering both stripped and raw forms.
///
/// Trim leading/trailing whitespace AND the common "wrapping"
/// characters (backticks, quotes, parens) so that selecting
/// `` `src/main.rs` `` works as naturally as selecting `src/main.rs`.
fn candidate_from_selection(s: &str) -> Option<PathCandidate> {
    let trimmed = s
        .trim()
        .trim_matches(|c: char| matches!(c, '`' | '"' | '\'' | '(' | ')' | '[' | ']' | '<' | '>'))
        .trim()
        .to_string();
    if trimmed.is_empty() { return None; }
    let display = cleanup_path_suffix(&trimmed).unwrap_or_else(|| trimmed.clone());
    if display.len() < 3 && trimmed.len() < 3 { return None; }
    let with_suffix = if trimmed != display { Some(trimmed) } else { None };
    Some(PathCandidate {
        display,
        with_suffix,
        // Selection is already visibly highlighted by the terminal
        // itself; no hover segment needed.
        segments: Vec::new(),
        // Source priority is moot for a single candidate, but use
        // a neutral tier so sorting (if ever added) doesn't matter.
        source: CandidateSource::Bareword,
        // Right-click selection path only handles files for now;
        // URL selection would be a straightforward extension but
        // isn't part of this change.
        kind: CandidateKind::File,
    })
}

/// Try to resolve a user-selected text span to a real file.
/// Skips all the grid-scanning machinery — the selection *is* the
/// range — and just runs the cleaned string through the same
/// CWD × exists() pipeline as `resolve_path_at_term`.
///
/// Used by the right-click "Open Selection as File" menu item.
/// Returns `Some(PathHit)` only when the resolved path is a real
/// file on disk.
pub(crate) fn try_resolve_selection_as_path(
    view: &GpuiShellView,
    selection: &str,
) -> Option<PathHit> {
    let cand = candidate_from_selection(selection)?;
    let cwds = collect_cwd_bases(view);
    let exists = |p: &str| -> bool {
        let converted = maybe_convert_wsl_path(view, p);
        std::path::Path::new(&converted).is_file()
    };
    pick_existing(&[cand], &cwds, exists).map(|hit| PathHit {
        absolute: maybe_convert_wsl_path(view, &hit.absolute),
        segments: hit.segments,
    })
}

/// Orchestrator: collect candidates at a click, classify as File or
/// Url, and return the first resolvable hit. URL candidates win
/// instantly (syntax is the contract); file candidates go through
/// the CWD × `exists()` pipeline.
///
/// Candidates are pre-sorted by source priority descending. That
/// means an OSC 8 hyperlink wins over a markdown link over a
/// bareword, whether the target is a file or a URL.
///
/// The underline appears **iff** this returns `Some`. For files,
/// the absolute path is ready to hand to `open_preview_file`; for
/// URLs, the URL is ready to pass to `open_url_external`.
pub(crate) fn resolve_click_at_term(
    term: &amux_platform::terminal::alacritty_view::AlacrittyTerminal,
    view: &GpuiShellView,
    col: usize,
    row: usize,
) -> Option<ClickHit> {
    let candidates = collect_candidates_at_term(term, col, row);
    if candidates.is_empty() { return None; }

    // URL candidates resolve by inspection — no FS, no CWD. Walk
    // the (already-sorted) list once; the highest-priority URL
    // wins. Markdown > bareword still holds because sorting was
    // done by the collector.
    for cand in &candidates {
        if matches!(cand.kind, CandidateKind::Url) {
            return Some(ClickHit {
                kind: ClickKind::Url(cand.display.clone()),
                segments: cand.segments.clone(),
            });
        }
    }

    // File candidates go through the existing CWD × exists()
    // pipeline. pick_existing skips Url candidates internally.
    let cwds = collect_cwd_bases(view);
    let exists = |p: &str| -> bool {
        let converted = maybe_convert_wsl_path(view, p);
        std::path::Path::new(&converted).is_file()
    };
    pick_existing(&candidates, &cwds, exists).map(|hit| ClickHit {
        kind: ClickKind::File(maybe_convert_wsl_path(view, &hit.absolute)),
        segments: hit.segments,
    })
}

/// Pure multi-row scanner used by unit tests. Runs markdown / quoted
/// / bareword branches in priority order and returns the first
/// match. For bareword, applies wrap extension. The production
/// multi-candidate pipeline uses `collect_candidates_at_term`
/// instead; this function is kept because single-result tests
/// remain valuable for regression coverage on the core scanners.
#[allow(dead_code)]
fn scan_path_multirow<R, W>(
    read_row: &R,
    row_wraps: &W,
    total_rows: usize,
    cols: usize,
    row: usize,
    col: usize,
) -> Option<(String, Vec<HoverSegment>)>
where
    R: Fn(i32) -> Vec<char>,
    W: Fn(i32) -> bool,
{
    if row >= total_rows || col >= cols { return None; }
    let cur = read_row(row as i32);

    if let Some((s, e)) = scan_markdown_link(&cur, col) {
        if let Some((p, s, e)) = finalize(&cur, s, e) {
            return Some((p, vec![(row, s, e)]));
        }
    }
    if let Some((s, e)) = scan_quoted(&cur, col) {
        if let Some((p, s, e)) = finalize(&cur, s, e) {
            return Some((p, vec![(row, s, e)]));
        }
    }

    let (raw, segs) = wrap_extend_bareword_raw(read_row, row_wraps, total_rows, cols, row, col)?;
    let cleaned = cleanup_path_suffix(&raw)?;
    Some((cleaned, segs))
}

/// Bareword wrap-extension core, without suffix cleanup. Returns
/// the raw joined text across segments so the caller can generate
/// both stripped and unstripped candidates for FS validation.
fn wrap_extend_bareword_raw<R, W>(
    read_row: &R,
    row_wraps: &W,
    total_rows: usize,
    cols: usize,
    row: usize,
    col: usize,
) -> Option<(String, Vec<HoverSegment>)>
where
    R: Fn(i32) -> Vec<char>,
    W: Fn(i32) -> bool,
{
    if row >= total_rows || col >= cols { return None; }
    let cur = read_row(row as i32);
    let (cur_raw, start, end) = scan_bareword(&cur, col)?;
    let mut segments: Vec<HoverSegment> = vec![(row, start, end)];
    let mut left_raw = String::new();
    let mut right_raw = String::new();

    let mut r = row as i32;
    while segments[0].1 == 0 && r > 0 && row_wraps(r - 1) {
        let pr = r - 1;
        let prev = read_row(pr);
        let (p_raw, p_start) = match scan_bareword_trailing(&prev) {
            Some(v) => v,
            None => break,
        };
        segments.insert(0, (pr as usize, p_start, cols - 1));
        left_raw = p_raw + &left_raw;
        r = pr;
        if p_start != 0 { break; }
    }

    let mut r = row as i32;
    while segments.last().unwrap().2 == cols - 1
        && row_wraps(r)
        && ((r + 1) as usize) < total_rows
    {
        let nr = r + 1;
        let next = read_row(nr);
        let (n_raw, n_end) = match scan_bareword_leading(&next) {
            Some(v) => v,
            None => break,
        };
        segments.push((nr as usize, 0, n_end));
        right_raw.push_str(&n_raw);
        r = nr;
        if n_end != cols - 1 { break; }
    }

    Some((format!("{}{}{}", left_raw, cur_raw, right_raw), segments))
}

/// Generate every plausible path candidate at the click position.
///
/// This is the "identification" half of Tier 1's "generate many,
/// validate each" strategy. We do NOT try to pick the single best
/// interpretation here — that decision is deferred to
/// `pick_existing`, which uses the real filesystem as the tiebreaker.
///
/// Priority order (emitted first wins in case of FS tie):
///   1. OSC 8 hyperlink — when present, we return ONLY this; mixing
///      with heuristics would risk promoting a nearby bareword over
///      the CLI's explicit signal.
///   2. Markdown link — `[text](path)` at click.
///   3. Quoted string — `"..."` / `'...'` at click.
///   4. Bareword — longest path-char run around click, wrap-extended.
///
/// For each heuristic branch, both the cleaned form (`display`,
/// with `:L:C` stripped) and the raw form (`with_suffix`) are
/// stored on the candidate — the resolver tries both.
pub(crate) fn collect_candidates_at_term(
    term: &amux_platform::terminal::alacritty_view::AlacrittyTerminal,
    col: usize,
    row: usize,
) -> Vec<PathCandidate> {
    let mut out: Vec<PathCandidate> = Vec::new();

    // OSC 8 fast path. When the cell is tagged, we trust the CLI
    // completely — no heuristic candidates mixed in. The URI's
    // scheme determines whether this is a file or a URL click.
    if let Some((uri, segs)) = extract_hyperlink_multirow(term, col, row) {
        if let Some(path) = decode_file_uri(&uri) {
            out.push(PathCandidate {
                display: path,
                with_suffix: None,
                segments: segs,
                source: CandidateSource::Hyperlink,
                kind: CandidateKind::File,
            });
        } else if has_url_scheme(&uri) {
            // OSC 8 URIs don't carry stray punctuation — the
            // terminal emitted them deliberately — so no trim.
            out.push(PathCandidate {
                display: uri,
                with_suffix: None,
                segments: segs,
                source: CandidateSource::Hyperlink,
                kind: CandidateKind::Url,
            });
        }
        return out;
    }

    term.with_term(|t| {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Line, Column};
        use alacritty_terminal::term::cell::Flags;

        let grid = t.grid();
        let cols = grid.columns();
        let rows = grid.screen_lines();
        if row >= rows || col >= cols { return; }

        let read_row = |r: i32| -> Vec<char> {
            (0..cols).map(|c| grid[Line(r)][Column(c)].c).collect()
        };
        let row_wraps = |r: i32| -> bool {
            if (r as usize) >= rows { return false; }
            grid[Line(r)][Column(cols - 1)].flags.contains(Flags::WRAPLINE)
        };

        let cur = read_row(row as i32);

        // Markdown
        if let Some((s, e)) = scan_markdown_link(&cur, col) {
            if let Some((display, vs, ve)) = finalize(&cur, s, e) {
                let raw: String = cur[s..=e].iter().filter(|&&c| c != '\0').collect();
                let raw = raw.trim().to_string();
                let with_suffix = if !raw.is_empty() && raw != display { Some(raw) } else { None };
                if let Some(c) = make_candidate(display, with_suffix, vec![(row, vs, ve)], CandidateSource::Markdown) {
                    out.push(c);
                }
            }
        }

        // Quoted
        if let Some((s, e)) = scan_quoted(&cur, col) {
            if let Some((display, vs, ve)) = finalize(&cur, s, e) {
                let raw: String = cur[s..=e].iter().filter(|&&c| c != '\0').collect();
                let raw = raw.trim().to_string();
                let with_suffix = if !raw.is_empty() && raw != display { Some(raw) } else { None };
                if let Some(c) = make_candidate(display, with_suffix, vec![(row, vs, ve)], CandidateSource::Quoted) {
                    out.push(c);
                }
            }
        }

        // Bareword with wrap extension.
        if let Some((raw, segs)) =
            wrap_extend_bareword_raw(&read_row, &row_wraps, rows, cols, row, col)
        {
            let raw_trim = raw.trim().to_string();
            // URL bareword: the raw form IS the display (no :L:C
            // stripping for URLs — that suffix machinery is file
            // specific). Classify first so cleanup doesn't mangle
            // a URL's trailing port or query.
            if has_url_scheme(&raw_trim) {
                if let Some(c) = make_candidate(raw_trim.clone(), None, segs, CandidateSource::Bareword) {
                    out.push(c);
                }
            } else {
                let cleaned = cleanup_path_suffix(&raw_trim);
                match cleaned {
                    Some(display) => {
                        let with_suffix = if raw_trim != display { Some(raw_trim) } else { None };
                        if let Some(c) = make_candidate(display, with_suffix, segs, CandidateSource::Bareword) {
                            out.push(c);
                        }
                    }
                    None if raw_trim.len() >= 3 => {
                        // Cleanup rejected but raw is long enough — keep
                        // the raw form as the only candidate. Covers
                        // cases where `:L:C` stripping shortened it
                        // below the 3-char threshold.
                        if let Some(c) = make_candidate(raw_trim, None, segs, CandidateSource::Bareword) {
                            out.push(c);
                        }
                    }
                    None => {}
                }
            }
        }
    });

    // Priority descending: highest source first.
    out.sort_by(|a, b| b.source.cmp(&a.source));
    out
}

/// Build a PathCandidate, classifying File vs Url by the display
/// string's scheme. For Url candidates, trim trailing sentence
/// punctuation and shrink the visual highlight range to match so
/// the hover underline doesn't include a stray period or paren.
fn make_candidate(
    display: String,
    with_suffix: Option<String>,
    segments: Vec<HoverSegment>,
    source: CandidateSource,
) -> Option<PathCandidate> {
    if has_url_scheme(&display) {
        let trimmed = trim_url_trailing(&display).to_string();
        if trimmed.len() < 3 { return None; }
        // Shrink the last segment's end_col by the number of
        // trailing chars we removed. URLs don't wrap in practice,
        // but if they do (multi-segment hyperlinks), we only adjust
        // the tail segment — the leading rows stay full-width.
        let shrink = display.chars().count() - trimmed.chars().count();
        let segments = if shrink == 0 {
            segments
        } else {
            let mut segs = segments;
            if let Some(last) = segs.last_mut() {
                last.2 = last.2.saturating_sub(shrink);
            }
            segs
        };
        Some(PathCandidate {
            display: trimmed,
            with_suffix: None,
            segments,
            source,
            kind: CandidateKind::Url,
        })
    } else {
        Some(PathCandidate {
            display,
            with_suffix,
            segments,
            source,
            kind: CandidateKind::File,
        })
    }
}

/// Pure heuristic path scanner. Given a row of terminal cells as
/// chars and a click column, returns the path string and inclusive
/// `[start..=end]` cell range, or `None` if nothing looks like a
/// path under the cursor.
///
/// Priority order:
///   1. Markdown link `[text](path)` — click inside the `(...)`
///      part returns the parenthesised path.
///   2. Quoted string `"..."` / `'...'` — click inside the quotes
///      returns the quoted content (supports paths with spaces).
///   3. Bareword character-class scan — the original behavior.
///
/// All three branches then run the shared `:line[:col]` suffix
/// strip and backtick trim, and reject results shorter than 3
/// characters. The `start`/`end` range returned is the *visual*
/// highlight range — it matches the text the user sees, not the
/// post-strip path (which may be shorter).
// Production now uses `scan_path_multirow`; `scan_path_in_row`
// stays as the single-row pure entry point for unit tests and for
// any future caller that doesn't care about wrap extension.
#[allow(dead_code)]
fn scan_path_in_row(row: &[char], col: usize) -> Option<(String, usize, usize)> {
    if col >= row.len() { return None; }
    if row[col] == '\0' { return None; }

    if let Some((s, e)) = scan_markdown_link(row, col) {
        if let Some(out) = finalize(row, s, e) { return Some(out); }
    }
    if let Some((s, e)) = scan_quoted(row, col) {
        if let Some(out) = finalize(row, s, e) { return Some(out); }
    }
    let (_raw, s, e) = scan_bareword(row, col)?;
    finalize(row, s, e)
}

/// Scan a bareword run around `col`. Returns `(raw_string, start,
/// end)` inclusive. `raw` includes whatever chars the bareword
/// predicate allows — no suffix stripping.
fn scan_bareword(row: &[char], col: usize) -> Option<(String, usize, usize)> {
    if col >= row.len() { return None; }
    let ch = row[col];
    if ch == ' ' || ch == '\0' || !is_bareword_char(ch) { return None; }
    let mut start = col;
    while start > 0 && is_bareword_char(row[start - 1]) { start -= 1; }
    let mut end = col;
    while end + 1 < row.len() && is_bareword_char(row[end + 1]) { end += 1; }
    let s: String = row[start..=end].iter().filter(|&&c| c != '\0').collect();
    Some((s, start, end))
}

/// Find the trailing bareword run on `row` — the longest suffix of
/// bareword chars ending at the last non-`\0` cell. Used for wrap
/// extension upward from a row whose first cell is a bareword char.
/// Returns `(raw_string, start_col)`.
fn scan_bareword_trailing(row: &[char]) -> Option<(String, usize)> {
    if row.is_empty() { return None; }
    let last = row.len() - 1;
    if !is_bareword_char(row[last]) { return None; }
    let mut s = last;
    while s > 0 && is_bareword_char(row[s - 1]) { s -= 1; }
    let st: String = row[s..=last].iter().filter(|&&c| c != '\0').collect();
    Some((st, s))
}

/// Find the leading bareword run on `row` — the longest prefix of
/// bareword chars starting at col 0. Returns `(raw_string, end_col)`.
fn scan_bareword_leading(row: &[char]) -> Option<(String, usize)> {
    if row.is_empty() || !is_bareword_char(row[0]) { return None; }
    let mut e = 0;
    while e + 1 < row.len() && is_bareword_char(row[e + 1]) { e += 1; }
    let st: String = row[..=e].iter().filter(|&&c| c != '\0').collect();
    Some((st, e))
}

fn is_bareword_char(c: char) -> bool {
    // NOTE: `(` and `)` are deliberately excluded — they conflict
    // with markdown-link detection (pass 1). Paths that were wrapped
    // in literal parens like `(src/foo.rs)` now get their parens
    // trimmed by `finalize` via the bareword scan stopping at `(`.
    c.is_ascii_alphanumeric()
        || matches!(c, '/' | '\\' | '.' | '-' | '_' | ':' | '~' | '`')
}

/// Shared tail: slice `row[start..=end]`, clean via
/// `cleanup_path_suffix`, reject if too short. Returns the cleaned
/// string together with the *original* visual range so hover
/// highlighting covers what the user sees.
fn finalize(row: &[char], start: usize, end: usize) -> Option<(String, usize, usize)> {
    if end < start || end >= row.len() { return None; }
    let raw: String = row[start..=end].iter().filter(|&&c| c != '\0').collect();
    let cleaned = cleanup_path_suffix(&raw)?;
    Some((cleaned, start, end))
}

/// Strip trailing `:line[:col]` coordinates, surrounding backticks,
/// and leading/trailing whitespace from a raw path-like string.
/// Returns `None` if the cleaned result is shorter than 3 chars.
fn cleanup_path_suffix(raw: &str) -> Option<String> {
    let path = raw.trim().to_string();
    if path.len() < 3 { return None; }

    let path = if let Some(idx) = path.rfind(':') {
        let after = &path[idx + 1..];
        if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit()) {
            let base = &path[..idx];
            if let Some(idx2) = base.rfind(':') {
                let after2 = &base[idx2 + 1..];
                if !after2.is_empty() && after2.chars().all(|c| c.is_ascii_digit()) {
                    base[..idx2].to_string()
                } else {
                    base.to_string()
                }
            } else {
                base.to_string()
            }
        } else {
            path
        }
    } else {
        path
    };

    let path = path.trim_matches('`').trim().to_string();
    if path.len() < 3 { return None; }
    Some(path)
}

/// Markdown link scanner. Returns the inclusive cell range of the
/// `path` segment inside `[text](path)` when `col` falls inside it.
/// Returns `None` otherwise (click is on `[text]`, outside brackets,
/// or the row doesn't contain a complete link around `col`).
fn scan_markdown_link(row: &[char], col: usize) -> Option<(usize, usize)> {
    // Clicks directly on a bracket/paren shouldn't count as "inside
    // the path" — they're on the delimiter, not the content.
    if matches!(row[col], '[' | ']' | '(' | ')') { return None; }

    // Walk left: find `](` (an `(` whose previous cell is `]`)
    // without crossing any other `)`, `(`, or `[` first. Loop reads
    // `row[i]` before decrementing so `row[0]` is always examined.
    let mut i = col;
    let path_start = loop {
        let c = row[i];
        if c == '(' && i > 0 && row[i - 1] == ']' {
            break i + 1;
        }
        if matches!(c, ')' | '[') { return None; }
        if i == 0 { return None; }
        i -= 1;
    };

    // Walk right: find the closing `)`. Bail on a stray `(` or `[`.
    let mut j = col;
    let path_end = loop {
        if j >= row.len() { return None; }
        let c = row[j];
        if c == ')' {
            if j == 0 { return None; }
            break j - 1;
        }
        if matches!(c, '(' | '[') { return None; }
        j += 1;
    };

    if path_end < path_start { return None; }
    Some((path_start, path_end))
}

/// Quoted-string scanner. Matches `"..."` and `'...'` pairs that
/// straddle `col`. Returns the inclusive cell range of the content
/// between the quotes (not including the quotes themselves).
fn scan_quoted(row: &[char], col: usize) -> Option<(usize, usize)> {
    let q = {
        // Find nearest `"` or `'` to the left of col.
        let mut found = None;
        let mut i = col;
        loop {
            let c = row[i];
            if c == '"' || c == '\'' {
                if i == col {
                    // Click directly on the quote — treat as outside.
                    return None;
                }
                found = Some((i, c));
                break;
            }
            if i == 0 { break; }
            i -= 1;
        }
        found?
    };
    let (left_idx, quote) = q;
    // Find matching quote to the right of col.
    let mut j = col;
    while j < row.len() {
        if row[j] == quote {
            if j == col { return None; }
            if j <= left_idx + 1 { return None; }
            return Some((left_idx + 1, j - 1));
        }
        j += 1;
    }
    None
}

// ─── File picker + preview launch ──────────────────────────────

/// Open the file picker (Ctrl+P, right-click, `amux preview`).
pub(crate) fn open_file_picker(view: &mut GpuiShellView) {
    let cwd = resolve_best_cwd(view);
    view.file_picker = Some(crate::gpui_preview::FilePickerState::new(cwd));
}

/// Open the file picker scoped to a specific CWD, falling back to
/// `resolve_best_cwd` if the supplied path isn't a directory.
pub(crate) fn open_file_picker_with_cwd(view: &mut GpuiShellView, cwd: Option<String>) {
    let cwd = cwd.map(|p| maybe_convert_wsl_path(view, &p))
        .filter(|p| std::path::Path::new(p).is_dir())
        .or_else(|| resolve_best_cwd(view));
    view.file_picker = Some(crate::gpui_preview::FilePickerState::new(cwd));
}

/// Open a file for preview from the currently-open file picker,
/// resolving the picker's captured `base_dir` against relative
/// paths so the result matches the file list the user saw.
pub(crate) fn open_preview_from_picker(view: &mut GpuiShellView, index: usize) {
    let (path, base_dir) = if let Some(ref picker) = view.file_picker {
        (picker.matches.get(index).cloned(), picker.base_dir.clone())
    } else {
        (None, None)
    };
    view.file_picker = None;
    if let Some(path) = path {
        let full_path = if std::path::Path::new(&path).is_absolute() {
            path
        } else if let Some(ref base) = base_dir {
            std::path::PathBuf::from(base).join(&path).to_string_lossy().to_string()
        } else {
            path
        };
        open_preview_file(view, &full_path);
    }
}

/// Open a file for preview by path. Resolves relative paths
/// against the active pane's CWD, then loads the preview state
/// and adds a preview tab to the active pane.
pub(crate) fn open_preview_file(view: &mut GpuiShellView, path: &str) {
    let full_path = if std::path::Path::new(path).is_absolute() {
        path.to_string()
    } else {
        resolve_active_cwd(view)
            .map(|cwd| std::path::PathBuf::from(cwd).join(path).to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string())
    };
    if let Some(state) = crate::gpui_preview::PreviewState::load(&full_path) {
        let active_pid = view.terminal_manager().active_pane_id().cloned();
        if let Some(ref pid) = active_pid {
            if let Some(pane) = view.terminal_manager_mut().get_pane_mut(pid) {
                pane.add_preview_tab(&full_path);
            }
        }
        view.preview_tabs.insert(full_path, state);
    }
}

/// Open a preview for a path that may be relative to a specific
/// CWD (typically the one parsed from the `amux preview <path>`
/// command's prompt line). Falls through to
/// `resolve_active_cwd` if the supplied CWD isn't a directory.
pub(crate) fn open_preview_file_with_cwd(
    view: &mut GpuiShellView,
    path: &str,
    cwd: Option<&str>,
) {
    let full_path = if std::path::Path::new(path).is_absolute() {
        maybe_convert_wsl_path(view, path)
    } else {
        let resolved_cwd = cwd.map(|p| maybe_convert_wsl_path(view, p))
            .filter(|p| std::path::Path::new(p).is_dir())
            .or_else(|| resolve_active_cwd(view));
        resolved_cwd
            .map(|cwd| std::path::PathBuf::from(cwd).join(path).to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string())
    };
    open_preview_file(view, &full_path);
}

/// Handle a Cmd/Ctrl+click at a terminal cell. Delegates to
/// `resolve_click_at_term`, then dispatches by kind:
///
/// * `File` → `open_preview_file` (existing preview pipeline)
/// * `Url`  → `open_url_external` (system default browser)
///
/// Returns `true` iff a hit was found and dispatched. Callers use
/// the return to decide whether to consume the click or fall
/// through to normal selection.
pub(crate) fn try_preview_path_at(view: &mut GpuiShellView, col: usize, row: usize) -> bool {
    let Some(term) = view.terminal_manager().active_terminal_ref() else { return false; };
    let Some(hit) = resolve_click_at_term(term, view, col, row) else { return false; };
    match hit.kind {
        ClickKind::File(absolute) => {
            open_preview_file(view, &absolute);
        }
        ClickKind::Url(url) => {
            open_url_external(&url);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_powershell() {
        let out = extract_cwd_from_prompt_line("PS C:\\Users\\foo\\project> ");
        assert_eq!(out.as_deref(), Some("C:\\Users\\foo\\project"));
    }

    #[test]
    fn prompt_bash_with_tilde() {
        // HOME is required for the tilde expansion path; inject it.
        // SAFETY: test is single-threaded and the env var is only
        // read by expand_tilde below.
        unsafe { std::env::set_var("HOME", "/home/tester") };
        let out = extract_cwd_from_prompt_line("tester@box:~/project$ ");
        assert_eq!(out.as_deref(), Some("/home/tester/project"));
    }

    #[test]
    fn prompt_bash_absolute() {
        let out = extract_cwd_from_prompt_line("tester@box:/etc/nginx$ vim");
        assert_eq!(out.as_deref(), Some("/etc/nginx"));
    }

    #[test]
    fn prompt_zsh_percent() {
        unsafe { std::env::set_var("HOME", "/home/tester") };
        let out = extract_cwd_from_prompt_line("~/src% ");
        assert_eq!(out.as_deref(), Some("/home/tester/src"));
    }

    #[test]
    fn prompt_unrecognised_returns_none() {
        assert_eq!(extract_cwd_from_prompt_line("nothing prompty here"), None);
    }

    // ─── Title-based CWD parsing ────────────────────────────────

    #[test]
    fn title_bare_absolute() {
        assert_eq!(
            extract_cwd_from_title("/home/alice/proj"),
            Some("/home/alice/proj".to_string())
        );
    }

    #[test]
    fn title_bare_tilde_expands() {
        unsafe { std::env::set_var("HOME", "/home/carol") };
        assert_eq!(
            extract_cwd_from_title("~/src"),
            Some("/home/carol/src".to_string())
        );
    }

    #[test]
    fn title_user_at_host_colon_path() {
        // xterm bash default: "user@host: ~/dir"
        unsafe { std::env::set_var("HOME", "/home/bob") };
        assert_eq!(
            extract_cwd_from_title("bob@laptop: ~/projects/amux"),
            Some("/home/bob/projects/amux".to_string())
        );
    }

    #[test]
    fn title_em_dash_separator() {
        unsafe { std::env::set_var("HOME", "/Users/dan") };
        assert_eq!(
            extract_cwd_from_title("zsh — ~/work"),
            Some("/Users/dan/work".to_string())
        );
    }

    #[test]
    fn title_zsh_colon_prefix() {
        unsafe { std::env::set_var("HOME", "/home/e") };
        assert_eq!(
            extract_cwd_from_title("zsh: ~/notes"),
            Some("/home/e/notes".to_string())
        );
    }

    #[test]
    fn title_windows_drive() {
        assert_eq!(
            extract_cwd_from_title("PS: C:\\Users\\foo\\proj"),
            Some("C:\\Users\\foo\\proj".to_string())
        );
    }

    #[test]
    fn title_bare_text_returns_none() {
        assert_eq!(extract_cwd_from_title("Claude Code"), None);
        assert_eq!(extract_cwd_from_title("bash"), None);
        assert_eq!(extract_cwd_from_title(""), None);
    }

    #[test]
    fn title_vim_with_file_returns_path_caller_validates() {
        // `extract_cwd_from_title` returns a path-like string — the
        // caller's is_dir() check is what actually filters out file
        // paths. We only assert that the parse succeeds.
        unsafe { std::env::set_var("HOME", "/home/f") };
        assert_eq!(
            extract_cwd_from_title("nvim — ~/proj/file.rs"),
            Some("/home/f/proj/file.rs".to_string())
        );
    }

    // ─── Starship / fish / oh-my-zsh prompts ────────────────────

    #[test]
    fn prompt_starship_angle() {
        unsafe { std::env::set_var("HOME", "/home/g") };
        let out = extract_cwd_from_prompt_line("~/proj ❯ cargo build");
        assert_eq!(out.as_deref(), Some("/home/g/proj"));
    }

    #[test]
    fn prompt_ohmyzsh_double_angle() {
        unsafe { std::env::set_var("HOME", "/home/h") };
        let out = extract_cwd_from_prompt_line("~/repo » ls");
        assert_eq!(out.as_deref(), Some("/home/h/repo"));
    }

    #[test]
    fn prompt_triangle() {
        let out = extract_cwd_from_prompt_line("/var/log ▶ tail");
        assert_eq!(out.as_deref(), Some("/var/log"));
    }

    #[test]
    fn prompt_starship_no_command_yet() {
        unsafe { std::env::set_var("HOME", "/home/i") };
        let out = extract_cwd_from_prompt_line("~/work ❯ ");
        assert_eq!(out.as_deref(), Some("/home/i/work"));
    }

    #[test]
    fn prompt_starship_token_before_is_not_path() {
        // Prompt with a non-path token (like a git branch) should
        // NOT match — we require the token right before ❯ to look
        // like a path.
        let out = extract_cwd_from_prompt_line("some status text ❯ cmd");
        assert_eq!(out, None);
    }

    #[test]
    fn expand_tilde_noop_on_absolute() {
        assert_eq!(expand_tilde("/etc/passwd"), "/etc/passwd");
    }

    #[test]
    fn expand_tilde_respects_home() {
        unsafe { std::env::set_var("HOME", "/home/alice") };
        assert_eq!(expand_tilde("~/x"), "/home/alice/x");
    }

    // ─── Heuristic path scanner ────────────────────────────────

    fn row(s: &str) -> Vec<char> {
        // Pad with spaces so tests can address stable indices past
        // the visible content, matching real terminal rows.
        let mut v: Vec<char> = s.chars().collect();
        v.resize(80, ' ');
        v
    }

    #[test]
    fn bareword_simple_path() {
        let r = row("open src/auth.rs and go");
        // click inside "auth.rs"
        let (p, _, _) = scan_path_in_row(&r, 12).unwrap();
        assert_eq!(p, "src/auth.rs");
    }

    #[test]
    fn bareword_strips_line_col_suffix() {
        let r = row("see src/auth.rs:42:5 now");
        let (p, _, _) = scan_path_in_row(&r, 10).unwrap();
        assert_eq!(p, "src/auth.rs");
    }

    #[test]
    fn markdown_link_path() {
        let r = row("see [the file](src/auth.rs) for more");
        // Click somewhere inside "auth.rs".
        let click = "see [the file](src/".len() + 1;
        let (p, s, e) = scan_path_in_row(&r, click).unwrap();
        assert_eq!(p, "src/auth.rs");
        // Range covers only the path segment, not the brackets.
        let span: String = r[s..=e].iter().collect::<String>().trim().to_string();
        assert_eq!(span, "src/auth.rs");
    }

    #[test]
    fn markdown_link_with_line_col() {
        let r = row("goto [here](foo/bar.rs:12:3) pls");
        let click = "goto [here](foo/".len() + 1;
        let (p, _, _) = scan_path_in_row(&r, click).unwrap();
        assert_eq!(p, "foo/bar.rs");
    }

    #[test]
    fn markdown_click_on_text_part_ignored() {
        // Click inside "[the file]" — not in the path. Should fall
        // through to bareword, which finds nothing useful.
        let r = row("see [the file](src/auth.rs) now");
        let click = "see [the ".len();
        let got = scan_path_in_row(&r, click);
        // "file" has no path chars → bareword scan gives "file",
        // which is < 3 chars after stripping? Actually it's 4. So
        // scanner may return Some("file", _, _). We assert it does
        // NOT return the markdown path — that's the real invariant.
        if let Some((p, _, _)) = got {
            assert_ne!(p, "src/auth.rs");
        }
    }

    #[test]
    fn quoted_path_with_spaces() {
        let r = row("cat \"my notes/todo.md\" now");
        let click = "cat \"my not".len();
        let (p, _, _) = scan_path_in_row(&r, click).unwrap();
        assert_eq!(p, "my notes/todo.md");
    }

    #[test]
    fn single_quoted_path() {
        let r = row("see 'a b/c.rs' here");
        let click = "see 'a b/".len();
        let (p, _, _) = scan_path_in_row(&r, click).unwrap();
        assert_eq!(p, "a b/c.rs");
    }

    #[test]
    fn quoted_click_on_quote_itself_falls_back() {
        // Clicking on the opening quote should fall through, not
        // return an empty string.
        let r = row("see \"src/main.rs\" here");
        let click = "see ".len();
        // row[click] == '"'. Quoted scan returns None, bareword
        // scan rejects '"' (not a bareword char), result: None.
        assert_eq!(scan_path_in_row(&r, click), None);
    }

    #[test]
    fn backtick_wrapped_path() {
        let r = row("edit `src/main.rs` ok");
        let click = "edit `src/m".len();
        let (p, _, _) = scan_path_in_row(&r, click).unwrap();
        assert_eq!(p, "src/main.rs");
    }

    #[test]
    fn cjk_text_not_included() {
        let r = row("输出到 docs/file.rs 完成");
        let click = r.iter().position(|&c| c == 'd').unwrap() + 2;
        let (p, _, _) = scan_path_in_row(&r, click).unwrap();
        assert_eq!(p, "docs/file.rs");
    }

    #[test]
    fn click_on_space_returns_none() {
        let r = row("foo   bar");
        assert_eq!(scan_path_in_row(&r, 4), None);
    }

    #[test]
    fn too_short_rejected() {
        let r = row("ab cd");
        // "ab" is only 2 chars.
        assert_eq!(scan_path_in_row(&r, 0), None);
    }

    // ─── Multi-row wrap extension ───────────────────────────────

    /// Build a fixed-width grid from text rows. Each input row is
    /// padded/truncated to exactly `cols` chars. `wraps` gives the
    /// WRAPLINE flag per row (true = row continues into the next).
    fn multirow_fixture(
        texts: &[&str],
        wraps: &[bool],
        cols: usize,
    ) -> (Vec<Vec<char>>, Vec<bool>) {
        assert_eq!(texts.len(), wraps.len());
        let rows: Vec<Vec<char>> = texts
            .iter()
            .map(|s| {
                let mut v: Vec<char> = s.chars().collect();
                v.resize(cols, ' ');
                v.truncate(cols);
                v
            })
            .collect();
        (rows, wraps.to_vec())
    }

    fn scan_mr(
        rows: &[Vec<char>],
        wraps: &[bool],
        row: usize,
        col: usize,
    ) -> Option<(String, Vec<HoverSegment>)> {
        let cols = rows[0].len();
        let total_rows = rows.len();
        let read_row = |r: i32| -> Vec<char> { rows[r as usize].clone() };
        let row_wraps = |r: i32| -> bool {
            if (r as usize) >= wraps.len() { false } else { wraps[r as usize] }
        };
        scan_path_multirow(&read_row, &row_wraps, total_rows, cols, row, col)
    }

    #[test]
    fn mr_single_row_no_wrap() {
        // No WRAPLINE → behaves like the single-row scanner.
        let (rows, wraps) = multirow_fixture(
            &["open src/main.rs please"],
            &[false],
            80,
        );
        let (p, segs) = scan_mr(&rows, &wraps, 0, 7).unwrap();
        assert_eq!(p, "src/main.rs");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].0, 0);
    }

    #[test]
    fn mr_bareword_wraps_right_edge() {
        // cols = 20. Top row is exactly 20 chars ending in a path
        // char (WRAPLINE-style fill). Bottom row starts with path
        // chars. Click inside row 0 should yield the full path.
        let top =    "see src/very/long/pa"; // 20 chars, ends in 'a'
        let bottom = "th/file.rs here     "; // 20 chars
        assert_eq!(top.chars().count(), 20);
        assert_eq!(bottom.chars().count(), 20);
        let (rows, wraps) = multirow_fixture(&[top, bottom], &[true, false], 20);
        let click_col = "see src/v".len();
        let (p, segs) = scan_mr(&rows, &wraps, 0, click_col).unwrap();
        assert_eq!(p, "src/very/long/path/file.rs");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].0, 0);
        assert_eq!(segs[0].2, 19);
        assert_eq!(segs[1].0, 1);
        assert_eq!(segs[1].1, 0);
    }

    #[test]
    fn mr_click_on_second_row_still_resolves_full_path() {
        let top =    "aaa src/very/long/pa"; // 20 chars, ends in 'a'
        let bottom = "th/file.rs   more   "; // 20 chars
        assert_eq!(top.chars().count(), 20);
        assert_eq!(bottom.chars().count(), 20);
        let (rows, wraps) = multirow_fixture(&[top, bottom], &[true, false], 20);
        // Click on row 1 inside "th/file.rs" (col 2 is '/').
        let click_col = 2;
        let (p, segs) = scan_mr(&rows, &wraps, 1, click_col).unwrap();
        assert_eq!(p, "src/very/long/path/file.rs");
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn mr_wrap_blocked_without_wrapline_flag() {
        // Same layout but wraps[0] = false → no continuation.
        let top =    "see src/very/long/p";
        let bottom = "ath/file.rs here   ";
        let (rows, wraps) = multirow_fixture(&[top, bottom], &[false, false], 20);
        let click_col = "see src/v".len();
        let (p, segs) = scan_mr(&rows, &wraps, 0, click_col).unwrap();
        // Only what's on row 0 (up to end of row).
        assert_eq!(p, "src/very/long/p");
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn mr_three_row_wrap() {
        // Pathological long path across 3 rows. cols = 10.
        let r0 = "xx src/aaa"; // last char is path-char
        let r1 = "/bbbb/cccc"; // full row bareword
        let r2 = "/final.rs ";
        let (rows, wraps) = multirow_fixture(&[r0, r1, r2], &[true, true, false], 10);
        // Click on row 1 in the middle.
        let (p, segs) = scan_mr(&rows, &wraps, 1, 5).unwrap();
        assert_eq!(p, "src/aaa/bbbb/cccc/final.rs");
        assert_eq!(segs.len(), 3);
    }

    // ─── pick_existing (FS validator) ───────────────────────────

    fn cand(
        display: &str,
        with_suffix: Option<&str>,
        source: CandidateSource,
    ) -> PathCandidate {
        PathCandidate {
            display: display.to_string(),
            with_suffix: with_suffix.map(String::from),
            segments: vec![(0, 0, display.len().saturating_sub(1))],
            source,
            kind: CandidateKind::File,
        }
    }

    /// Build a `Fn(&str) -> bool` exists predicate from a fixed set.
    fn fake_fs<'a>(existing: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
        move |p: &str| existing.iter().any(|e| *e == p)
    }

    #[test]
    fn pick_absolute_hit() {
        let c = [cand("/etc/hosts", None, CandidateSource::Bareword)];
        let fs = fake_fs(&["/etc/hosts"]);
        let hit = pick_existing(&c, &[], fs).unwrap();
        assert_eq!(hit.absolute, "/etc/hosts");
    }

    #[test]
    fn pick_relative_joined_with_cwd() {
        let c = [cand("src/main.rs", None, CandidateSource::Bareword)];
        let cwds = vec!["/home/u/proj".to_string()];
        let fs = fake_fs(&["/home/u/proj/src/main.rs"]);
        let hit = pick_existing(&c, &cwds, fs).unwrap();
        assert_eq!(hit.absolute, "/home/u/proj/src/main.rs");
    }

    #[test]
    fn pick_tries_cwds_in_order() {
        // First cwd has no match; second cwd does.
        let c = [cand("foo.rs", None, CandidateSource::Bareword)];
        let cwds = vec!["/a".to_string(), "/b".to_string()];
        let fs = fake_fs(&["/b/foo.rs"]);
        let hit = pick_existing(&c, &cwds, fs).unwrap();
        assert_eq!(hit.absolute, "/b/foo.rs");
    }

    #[test]
    fn pick_prefers_higher_priority_source() {
        // Two candidates for the same click: bareword (low) and
        // markdown (high). Both resolve. Markdown wins because
        // `collect_candidates_at_term` sorts by source descending,
        // so `pick_existing` sees it first.
        let mut cs = vec![
            cand("bad.rs", None, CandidateSource::Bareword),
            cand("good.rs", None, CandidateSource::Markdown),
        ];
        cs.sort_by(|a, b| b.source.cmp(&a.source));
        let cwds = vec!["/repo".to_string()];
        let fs = fake_fs(&["/repo/bad.rs", "/repo/good.rs"]);
        let hit = pick_existing(&cs, &cwds, fs).unwrap();
        assert_eq!(hit.absolute, "/repo/good.rs");
    }

    #[test]
    fn pick_falls_back_to_with_suffix() {
        // `display` doesn't exist, but `with_suffix` (raw form) does.
        // This covers files literally named with `:N` at the end.
        let c = [cand(
            "foo.tar.gz",
            Some("foo.tar.gz:42"),
            CandidateSource::Bareword,
        )];
        let cwds = vec!["/tmp".to_string()];
        let fs = fake_fs(&["/tmp/foo.tar.gz:42"]);
        let hit = pick_existing(&c, &cwds, fs).unwrap();
        assert_eq!(hit.absolute, "/tmp/foo.tar.gz:42");
    }

    #[test]
    fn pick_prefers_display_over_with_suffix_when_both_exist() {
        // Both forms exist — display (the cleaned form) comes first
        // in the forms iteration so it wins. Matches user intent
        // for normal `:L:C` line numbers.
        let c = [cand(
            "src/auth.rs",
            Some("src/auth.rs:42:5"),
            CandidateSource::Bareword,
        )];
        let cwds = vec!["/repo".to_string()];
        let fs = fake_fs(&["/repo/src/auth.rs", "/repo/src/auth.rs:42:5"]);
        let hit = pick_existing(&c, &cwds, fs).unwrap();
        assert_eq!(hit.absolute, "/repo/src/auth.rs");
    }

    #[test]
    fn pick_none_when_nothing_exists() {
        let c = [cand("ghost.rs", None, CandidateSource::Bareword)];
        let cwds = vec!["/a".to_string(), "/b".to_string()];
        let fs = fake_fs(&[]);
        assert!(pick_existing(&c, &cwds, fs).is_none());
    }

    #[test]
    fn pick_empty_candidates_returns_none() {
        let fs = fake_fs(&["/a"]);
        assert!(pick_existing(&[], &[], fs).is_none());
    }

    #[test]
    fn pick_skips_url_candidates() {
        // A Url candidate mixed in must not be treated as a file
        // lookup. The only other candidate is a non-existent file,
        // so the overall result is None (URLs are handled by the
        // dedicated branch in resolve_click_at_term).
        let url_cand = PathCandidate {
            display: "https://example.com".to_string(),
            with_suffix: None,
            segments: vec![],
            source: CandidateSource::Bareword,
            kind: CandidateKind::Url,
        };
        let file_cand = cand("missing.rs", None, CandidateSource::Bareword);
        let cwds = vec!["/ws".to_string()];
        let fs = fake_fs(&[]);
        assert!(pick_existing(&[url_cand, file_cand], &cwds, fs).is_none());
    }

    // ─── URL helpers ─────────────────────────────────────────────

    #[test]
    fn url_scheme_detection() {
        assert!(has_url_scheme("https://example.com"));
        assert!(has_url_scheme("http://example.com/path?q=1"));
        assert!(!has_url_scheme("ftp://example.com"));
        assert!(!has_url_scheme("mailto:a@b.com"));
        assert!(!has_url_scheme("src/main.rs"));
        assert!(!has_url_scheme("/etc/hosts"));
        assert!(!has_url_scheme(""));
    }

    #[test]
    fn url_trim_sentence_punct() {
        assert_eq!(trim_url_trailing("https://example.com."), "https://example.com");
        assert_eq!(trim_url_trailing("https://example.com,"), "https://example.com");
        assert_eq!(trim_url_trailing("https://example.com!"), "https://example.com");
        assert_eq!(trim_url_trailing("https://example.com?"), "https://example.com");
    }

    #[test]
    fn url_trim_stacked_punct() {
        // `.).` should strip all three from the tail in order.
        assert_eq!(
            trim_url_trailing("https://example.com).."),
            "https://example.com"
        );
    }

    #[test]
    fn url_preserves_balanced_trailing_paren() {
        // Wikipedia-style URL with a literal `)` in the path
        // must not have its tail paren stripped — the parens
        // balance.
        let url = "https://en.wikipedia.org/wiki/Rust_(programming_language)";
        assert_eq!(trim_url_trailing(url), url);
    }

    #[test]
    fn url_trims_unbalanced_closing_paren() {
        // "(see https://foo.com)" as written in prose: the bareword
        // is "https://foo.com)" with one unbalanced `)` — strip it.
        assert_eq!(trim_url_trailing("https://foo.com)"), "https://foo.com");
    }

    #[test]
    fn url_preserves_path_with_balanced_parens_in_middle() {
        // Parens in the middle of the URL with a non-paren tail
        // survive untouched.
        let url = "https://a.com/foo(bar)/baz";
        assert_eq!(trim_url_trailing(url), url);
    }

    #[test]
    fn url_trim_preserves_query_string() {
        let url = "https://a.com/search?q=rust&lang=en";
        assert_eq!(trim_url_trailing(url), url);
    }

    #[test]
    fn url_trim_empty_string() {
        assert_eq!(trim_url_trailing(""), "");
    }

    #[test]
    fn make_candidate_classifies_url() {
        let c = make_candidate(
            "https://example.com".to_string(),
            None,
            vec![(0, 0, 18)],
            CandidateSource::Bareword,
        )
        .unwrap();
        assert_eq!(c.kind, CandidateKind::Url);
        assert_eq!(c.display, "https://example.com");
    }

    #[test]
    fn make_candidate_classifies_file() {
        let c = make_candidate(
            "src/main.rs".to_string(),
            None,
            vec![(0, 0, 10)],
            CandidateSource::Bareword,
        )
        .unwrap();
        assert_eq!(c.kind, CandidateKind::File);
    }

    #[test]
    fn make_candidate_shrinks_range_on_url_trim() {
        // Display includes a trailing `.` that trim will drop.
        // The visual range must shrink by 1 so the underline
        // doesn't cover the period.
        let c = make_candidate(
            "https://example.com.".to_string(),
            None,
            vec![(5, 10, 29)], // 20 chars long
            CandidateSource::Bareword,
        )
        .unwrap();
        assert_eq!(c.display, "https://example.com");
        assert_eq!(c.segments.len(), 1);
        assert_eq!(c.segments[0], (5, 10, 28)); // end_col -1
    }

    // ─── Selection-to-candidate helper ──────────────────────────

    #[test]
    fn selection_plain_path() {
        let c = candidate_from_selection("src/main.rs").unwrap();
        assert_eq!(c.display, "src/main.rs");
        assert!(c.with_suffix.is_none());
        assert!(c.segments.is_empty());
    }

    #[test]
    fn selection_strips_line_col_suffix() {
        let c = candidate_from_selection("src/auth.rs:42:5").unwrap();
        assert_eq!(c.display, "src/auth.rs");
        assert_eq!(c.with_suffix.as_deref(), Some("src/auth.rs:42:5"));
    }

    #[test]
    fn selection_trims_wrapping_backticks() {
        let c = candidate_from_selection("`src/lib.rs`").unwrap();
        assert_eq!(c.display, "src/lib.rs");
    }

    #[test]
    fn selection_trims_wrapping_quotes() {
        let c = candidate_from_selection("\"src/main.rs\"").unwrap();
        assert_eq!(c.display, "src/main.rs");
        let c = candidate_from_selection("'src/main.rs'").unwrap();
        assert_eq!(c.display, "src/main.rs");
    }

    #[test]
    fn selection_trims_wrapping_parens_and_brackets() {
        let c = candidate_from_selection("(src/main.rs)").unwrap();
        assert_eq!(c.display, "src/main.rs");
        let c = candidate_from_selection("[src/main.rs]").unwrap();
        assert_eq!(c.display, "src/main.rs");
        let c = candidate_from_selection("<src/main.rs>").unwrap();
        assert_eq!(c.display, "src/main.rs");
    }

    #[test]
    fn selection_trims_whitespace() {
        let c = candidate_from_selection("   src/main.rs   ").unwrap();
        assert_eq!(c.display, "src/main.rs");
    }

    #[test]
    fn selection_with_spaces_in_path_preserved() {
        // User selects a path containing literal spaces — we
        // shouldn't mangle it. Quoted wrappers still get stripped.
        let c = candidate_from_selection("\"my notes/todo.md\"").unwrap();
        assert_eq!(c.display, "my notes/todo.md");
    }

    #[test]
    fn selection_empty_returns_none() {
        assert!(candidate_from_selection("").is_none());
        assert!(candidate_from_selection("   ").is_none());
        assert!(candidate_from_selection("``").is_none());
    }

    #[test]
    fn selection_too_short_returns_none() {
        assert!(candidate_from_selection("a").is_none());
        assert!(candidate_from_selection("ab").is_none());
    }

    #[test]
    fn pick_higher_priority_missing_falls_through() {
        // Markdown candidate doesn't exist; bareword does. The
        // bareword (lower priority) should still win because there's
        // no real file at the higher-priority location.
        let mut cs = vec![
            cand("does_not_exist.rs", None, CandidateSource::Markdown),
            cand("real.rs", None, CandidateSource::Bareword),
        ];
        cs.sort_by(|a, b| b.source.cmp(&a.source));
        let cwds = vec!["/w".to_string()];
        let fs = fake_fs(&["/w/real.rs"]);
        let hit = pick_existing(&cs, &cwds, fs).unwrap();
        assert_eq!(hit.absolute, "/w/real.rs");
    }

    #[test]
    fn mr_markdown_does_not_cross_wrap() {
        // Markdown links don't participate in wrap extension — they
        // need complete `[...](...)` on a single row.
        let r0 = "see [t](src/a"; // cols=13, wraps
        let r1 = "b/c.rs) now  ";
        let (rows, wraps) = multirow_fixture(&[r0, r1], &[true, false], 13);
        // Click inside "src/a" on row 0. Markdown scanner bails
        // because the `)` isn't on this row; bareword takes over
        // and extends into row 1 via WRAPLINE. Extension stops at
        // `)` since `)` isn't a bareword char.
        let click_col = "see [t](".len() + 1;
        let (p, _) = scan_mr(&rows, &wraps, 0, click_col).unwrap();
        assert_eq!(p, "src/ab/c.rs");
    }
}
