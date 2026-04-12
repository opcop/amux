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

/// Extract the working directory from a terminal prompt line.
/// Pure — no terminal access, trivially testable.
///
/// Supports:
///   PowerShell:  "PS C:\Users\foo\project> amux preview"  → "C:\Users\foo\project"
///   Bash/Zsh:    "user@host:~/project$ amux preview"      → "/home/user/project"
///   Zsh:         "~/project% amux preview"                 → "/home/user/project"
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

/// Best-effort resolve the current working directory of the
/// active pane. Tries multiple sources in order:
///   1. Parse cwd from the terminal prompt line (PowerShell,
///      Bash/Zsh); most reliable because prompts always show the
///      live cwd.
///   2. Live process cwd (sysinfo on Windows, /proc on Linux).
///   3. Saved spawn-time cwd from the tab (stale after `cd`).
pub(crate) fn resolve_active_cwd(view: &GpuiShellView) -> Option<String> {
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

/// Extract a file-path-like string from the terminal grid at the
/// given cell position. Scans left and right for path characters,
/// strips trailing `:line[:col]` coordinates and surrounding
/// backticks. Returns `None` if the extracted text is shorter
/// than 3 characters (too short to be a useful path).
pub(crate) fn extract_path_at_cursor(
    view: &GpuiShellView,
    col: usize,
    row: usize,
) -> Option<String> {
    let term = view.terminal_manager().active_terminal_ref()?;
    term.with_term(|t| {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Line, Column};

        let grid = t.grid();
        let cols = grid.columns();
        let rows = grid.screen_lines();
        if row >= rows || col >= cols { return None; }

        let line = Line(row as i32);

        let ch = grid[line][Column(col)].c;
        if ch == ' ' || ch == '\0' { return None; }

        // Use is_ascii_alphanumeric (not is_alphanumeric) so CJK
        // characters don't get included — "输出到docs/file.rs"
        // should extract "docs/file.rs".
        let is_path_char = |c: char| -> bool {
            c.is_ascii_alphanumeric()
                || matches!(c, '/' | '\\' | '.' | '-' | '_' | ':' | '~' | '(' | ')' | '`')
        };

        let mut start = col;
        while start > 0 {
            let c = grid[line][Column(start - 1)].c;
            if !is_path_char(c) { break; }
            start -= 1;
        }

        let mut end = col;
        while end + 1 < cols {
            let c = grid[line][Column(end + 1)].c;
            if !is_path_char(c) { break; }
            end += 1;
        }

        let mut path = String::new();
        for c in start..=end {
            let ch = grid[line][Column(c)].c;
            if ch != '\0' {
                path.push(ch);
            }
        }

        let path = path.trim().to_string();
        if path.len() < 3 { return None; }

        // Strip trailing `:line` or `:line:col` (e.g., "src/auth.rs:42:5")
        let path = if let Some(idx) = path.rfind(':') {
            let after = &path[idx + 1..];
            if after.chars().all(|c| c.is_ascii_digit()) {
                let base = &path[..idx];
                if let Some(idx2) = base.rfind(':') {
                    let after2 = &base[idx2 + 1..];
                    if after2.chars().all(|c| c.is_ascii_digit()) {
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

        let path = path.trim_matches('`').to_string();
        if path.len() < 3 { return None; }

        Some(path)
    })
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

/// Try to preview a file path at the given terminal cell
/// position. Extracts the path-like text, searches several
/// candidate base directories (pane cwd, git root, GUI cwd) for
/// an existing file, and opens a preview if one is found and has
/// a recognised text extension. Returns `true` if a preview was
/// actually opened.
pub(crate) fn try_preview_path_at(view: &mut GpuiShellView, col: usize, row: usize) -> bool {
    let path = match extract_path_at_cursor(view, col, row) {
        Some(p) => p,
        None => return false,
    };

    let converted = maybe_convert_wsl_path(view, &path);

    let mut candidates: Vec<String> = Vec::new();

    if std::path::Path::new(&converted).is_absolute() {
        candidates.push(converted.clone());
    }

    if let Some(cwd) = resolve_active_cwd(view) {
        candidates.push(
            std::path::PathBuf::from(&cwd)
                .join(&path)
                .to_string_lossy()
                .to_string(),
        );
    }

    // Git repo root detection: walk up from known paths to find .git
    if let Some(cwd) = resolve_active_cwd(view) {
        let mut dir = std::path::PathBuf::from(&cwd);
        for _ in 0..10 {
            if dir.join(".git").exists() {
                candidates.push(dir.join(&path).to_string_lossy().to_string());
                break;
            }
            if !dir.pop() { break; }
        }
    }

    // Try GUI process CWD as fallback
    if let Ok(gui_cwd) = std::env::current_dir() {
        candidates.push(gui_cwd.join(&path).to_string_lossy().to_string());
        let mut dir = gui_cwd;
        for _ in 0..10 {
            if dir.join(".git").exists() {
                candidates.push(dir.join(&path).to_string_lossy().to_string());
                break;
            }
            if !dir.pop() { break; }
        }
    }

    let resolved = match candidates.iter().find(|p| std::path::Path::new(p).exists()) {
        Some(p) => p.clone(),
        None => return false,
    };

    // Check if it's a previewable file type
    let ext = std::path::Path::new(&resolved).extension().and_then(|e| e.to_str());
    let is_previewable = matches!(
        ext,
        Some("md" | "markdown" | "txt" | "rs" | "js" | "ts" | "py" | "toml"
            | "json" | "yaml" | "yml" | "sh" | "bash" | "css" | "html"
            | "tsx" | "jsx" | "go" | "c" | "cpp" | "h" | "hpp" | "java"
            | "rb" | "php" | "swift" | "kt" | "lua" | "sql" | "xml"
            | "ini" | "cfg" | "conf" | "log" | "vim")
    );
    if !is_previewable { return false; }

    open_preview_file(view, &resolved);
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

    #[test]
    fn expand_tilde_noop_on_absolute() {
        assert_eq!(expand_tilde("/etc/passwd"), "/etc/passwd");
    }

    #[test]
    fn expand_tilde_respects_home() {
        unsafe { std::env::set_var("HOME", "/home/alice") };
        assert_eq!(expand_tilde("~/x"), "/home/alice/x");
    }
}
