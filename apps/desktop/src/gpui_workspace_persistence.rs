//! Workspace persistence — startup files, layout save/restore
//!
//! Manages ~/.amux directory structure, .startup file parsing,
//! workspace layout serialization, and startup command execution.

#[cfg(feature = "gpui")]
use amux_platform::terminal::manager::SplitDirection;
#[cfg(feature = "gpui")]
use serde::{Deserialize, Serialize};

/// Current on-disk schema version for workspace persistence files.
/// Bump whenever the shape of the envelope or its payload changes in
/// a way old code can't read.
#[cfg(feature = "gpui")]
const PERSISTENCE_SCHEMA_VERSION: u32 = 1;

/// Envelope around the per-workspace layouts map. Old files are a
/// bare `HashMap<String, String>`; the loader falls back to that
/// shape if deserializing as an envelope fails.
#[cfg(feature = "gpui")]
#[derive(Serialize, Deserialize)]
struct LayoutsEnvelope {
    schema_version: u32,
    layouts: std::collections::HashMap<String, String>,
}

/// Envelope around a single layout template file. Same back-compat
/// story — old files are a bare `LayoutTemplate`.
#[cfg(feature = "gpui")]
#[derive(Serialize, Deserialize)]
struct TemplateEnvelope {
    schema_version: u32,
    template: amux_platform::terminal::manager::LayoutTemplate,
}

/// Atomic file write: write to `<path>.tmp`, fsync, rename. Prevents
/// half-written files on crash or power loss — the previous version
/// stays intact until the rename succeeds.
#[cfg(feature = "gpui")]
fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
    })?;
    std::fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no file name")
        })?;
    let tmp = parent.join(format!(".{file_name}.tmp"));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Get the AMUX config directory.
///
/// Defaults to `~/.amux` (`%USERPROFILE%\.amux` on Windows) but is
/// overridable via the `AMUX_HOME` environment variable. Resolution is
/// centralized in `amux_platform::amux_home_dir()` so every layer
/// (session storage, layouts, screenshots, startup files, templates)
/// goes through the same rules and PTY children keep inheriting the
/// user's real `HOME`. See the doc comment on
/// `amux_platform::dirs::amux_home_dir` for the full rationale.
pub(crate) fn amux_base_dir() -> std::path::PathBuf {
    amux_platform::amux_home_dir()
}

/// Get the config.toml path
pub(crate) fn amux_config_path() -> std::path::PathBuf {
    amux_base_dir().join("config.toml")
}

#[cfg(feature = "gpui")]
use crate::gpui_entry::GpuiShellView;

/// How a pane declared in a `.startup` file should be spawned.
///
/// Replaces the old Windows-centric `PaneEnv { Win, Wsl }`. The
/// three variants cover every cross-platform spawn need without
/// special-casing macOS/Linux as a second-class fallback:
///
/// * **`Default`** — `default_shell()` for the current platform.
///   Means `$SHELL` on macOS/Linux (zsh/bash/fish) and PowerShell
///   (or cmd) on Windows. The common case — `[pane:1]` with no
///   `shell=` attribute resolves here.
///
/// * **`Wsl`** — `wsl.exe --cd <cwd>` into the default distribution.
///   Windows-only. On macOS/Linux a warning is logged and the pane
///   falls back to `Default` so the workspace still comes up.
///
/// * **`Explicit(name)`** — any executable by name (`bash`, `fish`,
///   `zsh`, `pwsh`, `nu`, ...). Spawned with no arguments; shells
///   detect the attached TTY and go interactive automatically. If
///   the binary isn't in `$PATH`, the spawn fails loudly via the
///   normal error-reporting channel.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug, PartialEq)]
enum PaneShell {
    Default,
    Wsl,
    Explicit(String),
}

/// Parsed pane config from .startup file
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
struct PaneStartup {
    pane_num: usize,
    title: Option<String>,
    shell: PaneShell,
    commands: Vec<String>,
}

/// Apply cross-platform fallbacks to a parsed `PaneShell`. The
/// only non-identity case today is `Wsl` on non-Windows: emit a
/// one-line warning and downgrade to `Default` so the workspace
/// still comes up. `pane_num` is only used for the warning text.
#[cfg(feature = "gpui")]
fn resolve_pane_shell(shell: &PaneShell, pane_num: usize) -> PaneShell {
    match shell {
        PaneShell::Wsl if !cfg!(target_os = "windows") => {
            eprintln!(
                "[amux] startup: shell=wsl is Windows-only; pane:{pane_num} falling back to default shell"
            );
            PaneShell::Default
        }
        other => other.clone(),
    }
}

/// Pure parser for startup-file content. Free-standing so unit
/// tests can drive it with a string literal — no filesystem, no
/// view access, no feature gates on the test side.
#[cfg(feature = "gpui")]
fn parse_startup_content(content: &str) -> Vec<PaneStartup> {
    let mut result: Vec<PaneStartup> = Vec::new();
    let mut current_pane: usize = 1;
    let mut current_title: Option<String> = None;
    let mut current_shell = PaneShell::Default;
    let mut current_cmds: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("[pane:") && trimmed.ends_with(']') {
            // Flush the previous pane.
            if !current_cmds.is_empty() {
                result.push(PaneStartup {
                    pane_num: current_pane,
                    title: current_title.take(),
                    shell: current_shell.clone(),
                    commands: std::mem::take(&mut current_cmds),
                });
            }
            // Reset attributes to per-pane defaults.
            current_shell = PaneShell::Default;
            current_title = None;

            let inner = &trimmed[6..trimmed.len() - 1]; // "1 title=xxx shell=wsl"
            let (num_str, attrs_str) = match inner.find(' ') {
                Some(pos) => (&inner[..pos], &inner[pos + 1..]),
                None => (inner, ""),
            };
            current_pane = num_str.parse().unwrap_or(1);

            for attr in attrs_str.split_whitespace() {
                if let Some(val) = attr.strip_prefix("title=") {
                    if !val.is_empty() {
                        current_title = Some(val.to_string());
                    }
                } else if let Some(val) = attr.strip_prefix("shell=") {
                    current_shell = match val.to_lowercase().as_str() {
                        "" | "default" => PaneShell::Default,
                        "wsl" => PaneShell::Wsl,
                        _ => PaneShell::Explicit(val.to_string()),
                    };
                }
                // Unrecognised attributes (including the legacy
                // `env=` from the Windows-centric design) are
                // silently ignored. See `parse_startup_file` doc.
            }
        } else {
            current_cmds.push(trimmed.to_string());
        }
    }
    if !current_cmds.is_empty() {
        result.push(PaneStartup {
            pane_num: current_pane,
            title: current_title,
            shell: current_shell,
            commands: current_cmds,
        });
    }
    result
}

#[cfg(feature = "gpui")]
impl GpuiShellView {
    /// Get the ~/.amux base directory
    pub(crate) fn amux_dir() -> std::path::PathBuf {
        amux_base_dir()
    }

    /// Get layout storage path
    pub(crate) fn layout_file_path() -> std::path::PathBuf {
        Self::amux_dir().join("layouts.json")
    }

    /// Get startup file path for a workspace
    pub(crate) fn startup_file_path(workspace_name: &str) -> std::path::PathBuf {
        let safe_name = workspace_name.replace(['/', '\\', ':', ' '], "_");
        Self::amux_dir().join("workspaces").join(format!("{}.startup", safe_name))
    }

    /// Parse a .startup file into pane commands.
    ///
    /// Format:
    /// ```text
    ///   [pane:1 title=Build]
    ///   cd ~/project
    ///   cargo watch -x check
    ///
    ///   [pane:2 title=WSL shell=wsl]         # Windows-only
    ///   cd /mnt/d/projects/myapp
    ///   claude
    ///
    ///   [pane:3 shell=fish]                  # explicit shell override
    ///   ls
    /// ```
    ///
    /// Supported attributes:
    /// * `title=<name>` — custom tab title (no whitespace in name)
    /// * `shell=default` — platform default shell (same as omitting it)
    /// * `shell=wsl` — Windows WSL; warns + falls back to default on
    ///   macOS/Linux
    /// * `shell=<cmd>` — any executable name on `$PATH` (bash, fish,
    ///   pwsh, ...)
    ///
    /// The legacy `env=` attribute is no longer recognized. This is
    /// a clean break — there's no silent compat shim, so a typo or a
    /// stale file just won't parse its pane attributes.
    fn parse_startup_file(path: &std::path::Path) -> Vec<PaneStartup> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        parse_startup_content(&content)
    }

    /// Check if workspace is "empty" (single pane, single tab, no splits).
    pub(crate) fn is_workspace_empty(&self) -> bool {
        let mgr = self.terminal_manager();
        mgr.total_panes() == 1 && mgr.total_tabs() <= 1
    }

    /// Execute startup commands for the active workspace.
    /// Creates panes as needed and sends commands to each.
    /// Supports per-pane `env=wsl` to run commands in WSL,
    /// with automatic path conversion between Windows and WSL formats.
    pub(crate) fn run_startup_commands(&mut self) {
        let ws_name = self.model.active_workspace_name
            .clone()
            .unwrap_or_else(|| self.active_workspace_id.clone());
        let path = Self::startup_file_path(&ws_name);
        let pane_cmds = Self::parse_startup_file(&path);
        if pane_cmds.is_empty() {
            return;
        }

        // Resolve to the active workspace's target path so startup
        // commands run *in* that workspace, not in amux's launch
        // directory. See `spawn_cwd` doc in gpui_entry.rs.
        let cwd = self.spawn_cwd();

        for (i, pane_cfg) in pane_cmds.iter().enumerate() {
            // Resolve the shell kind, applying cross-platform
            // fallbacks: `shell=wsl` on non-Windows downgrades to
            // Default with a warning so the workspace still opens.
            let shell = resolve_pane_shell(&pane_cfg.shell, pane_cfg.pane_num);

            // First pane already has its default shell from
            // workspace init. Subsequent panes need a split that
            // creates a fresh empty pane we then fill below.
            if i > 0 {
                let direction = if i % 2 == 1 {
                    SplitDirection::Horizontal
                } else {
                    SplitDirection::Vertical
                };
                self.terminal_manager_mut().split_active_pane(direction);
            }

            // Spawn the requested shell into the active pane.
            // `Default` for i==0 is a no-op (pane already runs the
            // default shell); for i>0 we fill the empty split. For
            // non-Default, `spawn_in_active` replaces any running
            // shell in the pane, which is what we want in both
            // cases.
            match &shell {
                PaneShell::Default => {
                    if i > 0 {
                        self.spawn_terminal_in_active();
                    }
                }
                PaneShell::Wsl => {
                    // resolve_pane_shell guarantees we only reach
                    // this arm on Windows.
                    let mut wsl_args = vec![];
                    if let Some(ref cwd_str) = cwd {
                        let wsl_path = Self::windows_path_to_wsl(cwd_str);
                        wsl_args.extend(["--cd".to_string(), wsl_path]);
                    }
                    let _ = self.terminal_manager_mut()
                        .spawn_in_active("wsl.exe", &wsl_args, None);
                }
                PaneShell::Explicit(cmd) => {
                    let _ = self.terminal_manager_mut()
                        .spawn_in_active(cmd, &[], cwd.as_deref());
                }
            }

            // Send commands. Path auto-conversion is a Windows-only
            // concern — on macOS/Linux we never rewrite paths,
            // removing the old edge case where `/mnt/...` paths in
            // macOS startup files got silently rewritten to `D:\`.
            if let Some(term) = self.terminal_manager_mut().active_terminal() {
                for cmd in &pane_cfg.commands {
                    let converted = Self::convert_command_paths(cmd, &shell);
                    let input = format!("{}\r", converted);
                    term.send_input(input.as_bytes());
                }
            }

            // Set tab title: custom_title > last command name > pane:N.
            // The "(WSL)" suffix is preserved because it's still a
            // meaningful distinguisher on Windows.
            let suffix = if matches!(shell, PaneShell::Wsl) { " (WSL)" } else { "" };
            let active_id = self.terminal_manager().active_pane_id().cloned();
            if let Some(ref pid) = active_id {
                if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                    if let Some(tab) = pane.tabs.get_mut(pane.active_tab) {
                        let title = if let Some(ref t) = pane_cfg.title {
                            format!("{}{}", t, suffix)
                        } else if let Some(last_cmd) = pane_cfg.commands.last() {
                            format!("{}{}", last_cmd.split_whitespace().next()
                                .unwrap_or("Terminal"), suffix)
                        } else {
                            format!("pane:{}{}", pane_cfg.pane_num, suffix)
                        };
                        tab.title = title;
                        tab.custom_title = pane_cfg.title.is_some();
                    }
                }
            }
        }

        // Equalize splits after creating all panes
        self.terminal_manager_mut().equalize_splits();
    }

    /// Rewrite paths inside a command line to match the shell the
    /// pane is about to run. **Windows-only.**
    ///
    /// On macOS/Linux the user types paths in the one and only
    /// format that works there — no rewriting, no guessing. The
    /// old version ran the conversion unconditionally and could
    /// misfire in rare edge cases (e.g. a literal `/mnt/d/foo`
    /// written in a macOS startup file got silently turned into
    /// `D:\foo`).
    ///
    /// On Windows the conversion matters because users might mix
    /// WSL-style (`/mnt/d/foo`) and Windows-style (`D:\foo`) paths
    /// in the same file and expect them to "just work" regardless
    /// of the target shell. Dispatch by shell kind:
    ///
    /// * **`Default`** — native Windows shell, so convert any
    ///   `/mnt/x/...` to `X:\...`.
    /// * **`Wsl`** — rewrite `X:\...` to `/mnt/x/...`.
    /// * **`Explicit`** — unknown target, passthrough untouched.
    fn convert_command_paths(cmd: &str, shell: &PaneShell) -> String {
        if !cfg!(target_os = "windows") {
            return cmd.to_string();
        }
        let target_wsl = match shell {
            PaneShell::Wsl => true,
            PaneShell::Default => false,
            PaneShell::Explicit(_) => return cmd.to_string(),
        };

        let parts: Vec<&str> = cmd.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            return cmd.to_string();
        }
        let command = parts[0];
        let arg = parts[1].trim();

        match command {
            "cd" | "pushd" | "ls" | "dir" | "cat" | "type" | "code" | "vim" | "nano" => {
                let converted = Self::normalize_path_for_env(arg, target_wsl);
                format!("{} {}", command, converted)
            }
            _ => cmd.to_string(),
        }
    }

    /// Platform-appropriate starter content for a brand-new
    /// `.startup` file. The examples match what actually works on
    /// the current OS — no Windows-centric `D:\` paths on macOS,
    /// no `~/project` on Windows — so a user opening the file for
    /// the first time can copy the snippet and get a real result.
    fn startup_template(ws_name: &str) -> String {
        if cfg!(target_os = "windows") {
            format!(
                "# Startup commands for workspace: {ws}\n\
                 # Each [pane:N] section creates a new terminal pane.\n\
                 #\n\
                 # Attributes (all optional):\n\
                 #   title=<Name>   custom tab title (no spaces)\n\
                 #   shell=default  platform default (this is the default)\n\
                 #   shell=wsl      run inside WSL (wsl.exe --cd <cwd>)\n\
                 #   shell=<cmd>    explicit shell by name (pwsh, bash, ...)\n\
                 #\n\
                 # Example:\n\
                 # [pane:1 title=Build]\n\
                 # cd D:\\projects\\myapp\n\
                 # cargo watch -x check\n\
                 #\n\
                 # [pane:2 title=AI shell=wsl]\n\
                 # cd /mnt/d/projects/myapp\n\
                 # claude\n",
                ws = ws_name,
            )
        } else {
            format!(
                "# Startup commands for workspace: {ws}\n\
                 # Each [pane:N] section creates a new terminal pane.\n\
                 #\n\
                 # Attributes (all optional):\n\
                 #   title=<Name>   custom tab title (no spaces)\n\
                 #   shell=default  your $SHELL (this is the default)\n\
                 #   shell=<cmd>    explicit shell by name (bash, fish, zsh, ...)\n\
                 #\n\
                 # Example:\n\
                 # [pane:1 title=Build]\n\
                 # cd ~/project\n\
                 # cargo watch -x check\n\
                 #\n\
                 # [pane:2 title=AI]\n\
                 # cd ~/project\n\
                 # claude\n",
                ws = ws_name,
            )
        }
    }

    /// Open the startup file for editing in a new split pane.
    pub(crate) fn edit_startup_file(&mut self) {
        let ws_name = self.model.active_workspace_name
            .clone()
            .unwrap_or_else(|| self.active_workspace_id.clone());
        let path = Self::startup_file_path(&ws_name);

        // Create directory and template file if it doesn't exist
        if !path.exists() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let template = Self::startup_template(&ws_name);
            let _ = std::fs::write(&path, template);
        }

        if cfg!(target_os = "windows") {
            // Windows: open with GUI editor directly (no terminal pane needed)
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "notepad".to_string());
            let _ = std::process::Command::new(&editor)
                .arg(&path)
                .spawn();
            return;
        }

        // Linux/Mac: open in a split pane with terminal editor
        self.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
        let cmd = format!("{} {}", editor, path.to_string_lossy());
        let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let _ = self.terminal_manager_mut().spawn_in_active(&sh, &["-ilc".to_string(), cmd], None);

        // Rename tab
        let active_id = self.terminal_manager().active_pane_id().cloned();
        if let Some(ref pid) = active_id {
            if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                if let Some(tab) = pane.tabs.last_mut() {
                    tab.title = "Startup Config".to_string();
                    tab.custom_title = true;
                }
            }
        }
    }

    // === Layout Template storage ===

    /// Templates directory path
    fn templates_dir() -> std::path::PathBuf {
        Self::amux_dir().join("templates")
    }

    /// Save a layout template to ~/.amux/templates/. Only called
    /// from `save_current_as_template`, which is itself only
    /// reachable once the command palette is wired — see that
    /// function's comment.
    #[allow(dead_code)]
    pub(crate) fn save_template(template: &amux_platform::terminal::manager::LayoutTemplate) {
        let dir = Self::templates_dir();
        let safe_name = template.name.replace(['/', '\\', ':', ' '], "_");
        let path = dir.join(format!("{}.json", safe_name));
        let envelope = TemplateEnvelope {
            schema_version: PERSISTENCE_SCHEMA_VERSION,
            template: template.clone(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&envelope) {
            let _ = atomic_write(&path, json.as_bytes());
        }
    }

    /// Load all custom templates from ~/.amux/templates/
    pub(crate) fn load_custom_templates() -> Vec<amux_platform::terminal::manager::LayoutTemplate> {
        let dir = Self::templates_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        let mut templates = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(data) = std::fs::read_to_string(&path) {
                    // Prefer the envelope shape; fall back to the
                    // legacy bare template for files written by older
                    // versions.
                    if let Ok(env) = serde_json::from_str::<TemplateEnvelope>(&data) {
                        templates.push(env.template);
                    } else if let Ok(t) = serde_json::from_str(&data) {
                        templates.push(t);
                    }
                }
            }
        }
        templates
    }

    /// Delete a custom template by name
    pub(crate) fn delete_template(name: &str) {
        let safe_name = name.replace(['/', '\\', ':', ' '], "_");
        let path = Self::templates_dir().join(format!("{}.json", safe_name));
        let _ = std::fs::remove_file(path);
    }

    /// All templates: built-in + custom
    pub(crate) fn all_templates() -> Vec<amux_platform::terminal::manager::LayoutTemplate> {
        let mut all = amux_platform::terminal::manager::LayoutTemplate::builtins();
        all.extend(Self::load_custom_templates());
        all
    }

    /// Save all workspace layouts to disk (atomic, versioned).
    pub(crate) fn save_all_layouts(&self) {
        let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (ws_id, tm) in &self.workspace_terminals {
            map.insert(ws_id.clone(), tm.save_layout());
        }
        let envelope = LayoutsEnvelope {
            schema_version: PERSISTENCE_SCHEMA_VERSION,
            layouts: map,
        };
        let Ok(json) = serde_json::to_string(&envelope) else { return; };
        let path = Self::layout_file_path();
        if atomic_write(&path, json.as_bytes()).is_ok() {
            // Publish the last known-good snapshot to the crash
            // logger so a subsequent panic can attach it to the
            // crash report.
            crate::crash::update_layout_snapshot(json);
        }
    }

    /// Load all workspace layouts from disk. Accepts both the current
    /// envelope format and legacy bare-map files.
    pub(crate) fn load_all_layouts() -> std::collections::HashMap<String, String> {
        let path = Self::layout_file_path();
        let Ok(data) = std::fs::read_to_string(&path) else {
            return std::collections::HashMap::new();
        };
        if let Ok(env) = serde_json::from_str::<LayoutsEnvelope>(&data) {
            return env.layouts;
        }
        match serde_json::from_str(&data) {
            Ok(map) => map,
            Err(e) => {
                eprintln!(
                    "amux: failed to parse {} as layouts envelope or legacy map ({e}); starting with empty layouts",
                    path.display()
                );
                std::collections::HashMap::new()
            }
        }
    }
}

#[cfg(all(test, feature = "gpui"))]
mod tests {
    use super::*;

    // ─── parse_startup_content ─────────────────────────────────

    #[test]
    fn parse_empty_file() {
        assert!(parse_startup_content("").is_empty());
    }

    #[test]
    fn parse_comments_and_blanks_only() {
        let input = "# top comment\n\n   \n# another\n";
        assert!(parse_startup_content(input).is_empty());
    }

    #[test]
    fn parse_implicit_first_pane_default_shell() {
        // No [pane:N] header → commands go to pane 1, shell=Default.
        let input = "cd ~/project\nls\n";
        let panes = parse_startup_content(input);
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].pane_num, 1);
        assert_eq!(panes[0].shell, PaneShell::Default);
        assert_eq!(panes[0].commands, vec!["cd ~/project", "ls"]);
        assert!(panes[0].title.is_none());
    }

    #[test]
    fn parse_shell_default_explicit() {
        let input = "[pane:1 shell=default]\nls\n";
        let panes = parse_startup_content(input);
        assert_eq!(panes[0].shell, PaneShell::Default);
    }

    #[test]
    fn parse_shell_wsl() {
        let input = "[pane:1 shell=wsl]\nls\n";
        let panes = parse_startup_content(input);
        assert_eq!(panes[0].shell, PaneShell::Wsl);
    }

    #[test]
    fn parse_shell_explicit_names() {
        for name in ["bash", "fish", "zsh", "pwsh", "nu"] {
            let input = format!("[pane:1 shell={name}]\nls\n");
            let panes = parse_startup_content(&input);
            assert_eq!(
                panes[0].shell,
                PaneShell::Explicit(name.to_string()),
                "shell={name}"
            );
        }
    }

    #[test]
    fn parse_title_and_shell_together() {
        let input = "[pane:1 title=Build shell=bash]\ncargo build\n";
        let panes = parse_startup_content(input);
        assert_eq!(panes[0].title.as_deref(), Some("Build"));
        assert_eq!(panes[0].shell, PaneShell::Explicit("bash".to_string()));
    }

    #[test]
    fn parse_attribute_order_independent() {
        // shell before title should work just like title before shell.
        let a = parse_startup_content("[pane:1 title=X shell=bash]\nls\n");
        let b = parse_startup_content("[pane:1 shell=bash title=X]\nls\n");
        assert_eq!(a[0].title, b[0].title);
        assert_eq!(a[0].shell, b[0].shell);
    }

    #[test]
    fn parse_multiple_panes() {
        let input = "[pane:1 title=A]\ncmd1\n[pane:2 title=B shell=fish]\ncmd2\ncmd3\n";
        let panes = parse_startup_content(input);
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0].pane_num, 1);
        assert_eq!(panes[0].title.as_deref(), Some("A"));
        assert_eq!(panes[0].shell, PaneShell::Default);
        assert_eq!(panes[1].pane_num, 2);
        assert_eq!(panes[1].title.as_deref(), Some("B"));
        assert_eq!(panes[1].shell, PaneShell::Explicit("fish".to_string()));
        assert_eq!(panes[1].commands, vec!["cmd2", "cmd3"]);
    }

    #[test]
    fn parse_legacy_env_is_silently_ignored() {
        // The old `env=win` / `env=wsl` syntax has been dropped.
        // Unknown attributes are ignored with no error — the pane
        // falls back to the defaults (PaneShell::Default, no title).
        let input = "[pane:1 env=wsl]\nls\n";
        let panes = parse_startup_content(input);
        assert_eq!(panes[0].shell, PaneShell::Default);
        assert!(panes[0].title.is_none());
    }

    #[test]
    fn parse_shell_empty_value_is_default() {
        let input = "[pane:1 shell=]\nls\n";
        let panes = parse_startup_content(input);
        assert_eq!(panes[0].shell, PaneShell::Default);
    }

    #[test]
    fn parse_shell_case_insensitive_keywords() {
        // `shell=WSL` and `shell=Default` should still hit the
        // built-in variants, not fall through to Explicit.
        let a = parse_startup_content("[pane:1 shell=WSL]\nls\n");
        let b = parse_startup_content("[pane:1 shell=Default]\nls\n");
        assert_eq!(a[0].shell, PaneShell::Wsl);
        assert_eq!(b[0].shell, PaneShell::Default);
    }

    #[test]
    fn parse_shell_explicit_preserves_case() {
        // For Explicit(name), we want to pass the literal name to
        // spawn, not a lowercased version — some macOS/Linux shell
        // wrappers are case-sensitive.
        let panes = parse_startup_content("[pane:1 shell=Pwsh]\nls\n");
        assert_eq!(panes[0].shell, PaneShell::Explicit("Pwsh".to_string()));
    }

    // ─── resolve_pane_shell (cross-platform fallback) ──────────

    #[test]
    fn resolve_default_passthrough() {
        assert_eq!(resolve_pane_shell(&PaneShell::Default, 1), PaneShell::Default);
    }

    #[test]
    fn resolve_explicit_passthrough() {
        let bash = PaneShell::Explicit("bash".to_string());
        assert_eq!(resolve_pane_shell(&bash, 1), bash);
    }

    #[test]
    fn resolve_wsl_platform_specific() {
        let got = resolve_pane_shell(&PaneShell::Wsl, 1);
        if cfg!(target_os = "windows") {
            assert_eq!(got, PaneShell::Wsl);
        } else {
            // On macOS/Linux: downgrade to Default with a warning.
            assert_eq!(got, PaneShell::Default);
        }
    }

    // ─── convert_command_paths ─────────────────────────────────
    //
    // These tests run on the host platform. The function's behavior
    // is gated on `cfg!(target_os = "windows")`, so we assert what
    // should happen on THIS machine — which is almost always non-
    // Windows in practice, and passthrough is the only correct
    // behavior on non-Windows.

    #[test]
    fn convert_passthrough_non_windows() {
        if cfg!(target_os = "windows") { return; }
        // macOS/Linux: never rewrite paths.
        assert_eq!(
            GpuiShellView::convert_command_paths("cd /Users/foo", &PaneShell::Default),
            "cd /Users/foo"
        );
        assert_eq!(
            GpuiShellView::convert_command_paths("cd /mnt/d/foo", &PaneShell::Default),
            "cd /mnt/d/foo"
        );
        assert_eq!(
            GpuiShellView::convert_command_paths("cd /mnt/d/foo", &PaneShell::Wsl),
            "cd /mnt/d/foo"
        );
    }

    #[test]
    fn convert_explicit_shell_is_passthrough_everywhere() {
        let bash = PaneShell::Explicit("bash".to_string());
        // Unknown target shell → never rewrite.
        assert_eq!(
            GpuiShellView::convert_command_paths("cd D:\\foo", &bash),
            "cd D:\\foo"
        );
        assert_eq!(
            GpuiShellView::convert_command_paths("cd /mnt/d/foo", &bash),
            "cd /mnt/d/foo"
        );
    }

    #[test]
    fn convert_non_path_command_unchanged() {
        if cfg!(target_os = "windows") { return; }
        // Even the old command did this — commands not in the
        // path-taking list are never rewritten.
        assert_eq!(
            GpuiShellView::convert_command_paths("npm run dev", &PaneShell::Default),
            "npm run dev"
        );
        assert_eq!(
            GpuiShellView::convert_command_paths("echo hi", &PaneShell::Wsl),
            "echo hi"
        );
    }
}
