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

/// Pane execution environment for .startup files
#[cfg(feature = "gpui")]
#[derive(Clone, Debug, PartialEq)]
enum PaneEnv {
    Win,
    Wsl,
}

/// Parsed pane config from .startup file
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
struct PaneStartup {
    pane_num: usize,
    title: Option<String>,
    env: PaneEnv,
    commands: Vec<String>,
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
    /// Format:
    ///   [pane:1 title=My Title env=wsl]
    ///   cd D:\projects\myapp
    ///   claude
    ///   [pane:2 env=win]
    ///   cd /mnt/d/projects/backend
    ///   npm run dev
    ///
    /// Each pane can specify its own `env=win` or `env=wsl` (default: win).
    /// Paths in commands are auto-converted to match the target environment.
    fn parse_startup_file(path: &std::path::Path) -> Vec<PaneStartup> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut result: Vec<PaneStartup> = Vec::new();
        let mut current_pane: usize = 1;
        let mut current_title: Option<String> = None;
        let mut current_env = PaneEnv::Win;
        let mut current_cmds: Vec<String> = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // Check for [pane:N ...] header
            if trimmed.starts_with("[pane:") && trimmed.ends_with(']') {
                // Save previous pane
                if !current_cmds.is_empty() {
                    result.push(PaneStartup {
                        pane_num: current_pane,
                        title: current_title.take(),
                        env: current_env.clone(),
                        commands: current_cmds.clone(),
                    });
                    current_cmds.clear();
                }
                // Reset defaults
                current_env = PaneEnv::Win;
                current_title = None;

                let inner = &trimmed[6..trimmed.len() - 1]; // e.g. "1 title=xxx env=wsl"
                // Split into pane number and attributes
                let (num_str, attrs_str) = match inner.find(' ') {
                    Some(pos) => (&inner[..pos], &inner[pos + 1..]),
                    None => (inner, ""),
                };
                current_pane = num_str.parse().unwrap_or(1);

                // Parse space-separated key=value attributes
                for attr in attrs_str.split_whitespace() {
                    if let Some(val) = attr.strip_prefix("title=") {
                        if !val.is_empty() {
                            current_title = Some(val.to_string());
                        }
                    } else if let Some(val) = attr.strip_prefix("env=") {
                        current_env = match val.to_lowercase().as_str() {
                            "wsl" | "linux" => PaneEnv::Wsl,
                            _ => PaneEnv::Win,
                        };
                    }
                }
            } else {
                current_cmds.push(trimmed.to_string());
            }
        }
        // Save last pane
        if !current_cmds.is_empty() {
            result.push(PaneStartup {
                pane_num: current_pane,
                title: current_title,
                env: current_env,
                commands: current_cmds,
            });
        }
        result
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

        let cwd = Self::default_cwd();

        for (i, pane_cfg) in pane_cmds.iter().enumerate() {
            let is_wsl = pane_cfg.env == PaneEnv::Wsl;

            // First pane already exists, subsequent panes need split
            if i > 0 {
                let direction = if i % 2 == 1 {
                    SplitDirection::Horizontal
                } else {
                    SplitDirection::Vertical
                };
                self.terminal_manager_mut().split_active_pane(direction);
            }

            if is_wsl && cfg!(target_os = "windows") {
                // Spawn a WSL shell in this pane
                let mut wsl_args = vec![];
                if let Some(ref cwd_str) = cwd {
                    let wsl_path = Self::windows_path_to_wsl(cwd_str);
                    wsl_args.extend(["--cd".to_string(), wsl_path]);
                }
                let _ = self.terminal_manager_mut().spawn_in_active("wsl.exe", &wsl_args, None);
            } else if i > 0 {
                // Spawn a native shell for non-first panes
                self.spawn_terminal_in_active();
            }

            // Send commands with path auto-conversion
            if let Some(term) = self.terminal_manager_mut().active_terminal() {
                for cmd in &pane_cfg.commands {
                    let converted = Self::convert_command_paths(cmd, is_wsl);
                    let input = format!("{}\r", converted);
                    term.send_input(input.as_bytes());
                }
            }

            // Set tab title: custom_title > last command name > pane:N
            let env_suffix = if is_wsl { " (WSL)" } else { "" };
            let active_id = self.terminal_manager().active_pane_id().cloned();
            if let Some(ref pid) = active_id {
                if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                    if let Some(tab) = pane.tabs.get_mut(pane.active_tab) {
                        let title = if let Some(ref t) = pane_cfg.title {
                            format!("{}{}", t, env_suffix)
                        } else if let Some(last_cmd) = pane_cfg.commands.last() {
                            format!("{}{}", last_cmd.split_whitespace().next()
                                .unwrap_or("Terminal"), env_suffix)
                        } else {
                            format!("pane:{}{}", pane_cfg.pane_num, env_suffix)
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

    /// Convert paths in a command to match the target environment.
    /// Handles `cd` commands and other commands that contain paths.
    fn convert_command_paths(cmd: &str, target_wsl: bool) -> String {
        let parts: Vec<&str> = cmd.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            return cmd.to_string();
        }
        let command = parts[0];
        let arg = parts[1].trim();

        // Commands that take a path as first argument
        match command {
            "cd" | "pushd" | "ls" | "dir" | "cat" | "type" | "code" | "vim" | "nano" => {
                let converted = Self::normalize_path_for_env(arg, target_wsl);
                format!("{} {}", command, converted)
            }
            _ => cmd.to_string(),
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
            let template = format!(
                "# Startup commands for workspace: {}\n\
                 # Each [pane:N] section creates a new terminal pane.\n\
                 # Use [pane:N title=Name] to set a custom tab title.\n\
                 # Lines are sent as commands to the shell.\n\
                 #\n\
                 # Example:\n\
                 # [pane:1 title=Build]\n\
                 # cd /my/project\n\
                 # cargo watch -x check\n\
                 #\n\
                 # [pane:2 title=AI]\n\
                 # cd /my/project\n\
                 # claude\n",
                ws_name
            );
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

    /// Save a layout template to ~/.amux/templates/
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
        serde_json::from_str(&data).unwrap_or_default()
    }
}
