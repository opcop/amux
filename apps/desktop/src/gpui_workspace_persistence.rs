//! Workspace persistence — startup files, layout save/restore
//!
//! Manages ~/.amux directory structure, .startup file parsing,
//! workspace layout serialization, and startup command execution.

#[cfg(feature = "gpui")]
use amux_platform::terminal::manager::SplitDirection;

/// Get the ~/.amux base directory (available without gpui feature for config loading)
pub(crate) fn amux_base_dir() -> std::path::PathBuf {
    let home = if cfg!(target_os = "windows") {
        std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into())
    } else {
        std::env::var("HOME").unwrap_or_else(|_| ".".into())
    };
    std::path::PathBuf::from(home).join(".amux")
}

/// Get the config.toml path
pub(crate) fn amux_config_path() -> std::path::PathBuf {
    amux_base_dir().join("config.toml")
}

#[cfg(feature = "gpui")]
use crate::gpui_entry::GpuiShellView;

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
    ///   [pane:1 title=My Title]
    ///   cd /some/dir
    ///   command arg1 arg2
    ///   [pane:2]
    ///   another-command
    /// Returns vec of (pane_number, custom_title, vec_of_commands)
    fn parse_startup_file(path: &std::path::Path) -> Vec<(usize, Option<String>, Vec<String>)> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut result: Vec<(usize, Option<String>, Vec<String>)> = Vec::new();
        let mut current_pane: usize = 1;
        let mut current_title: Option<String> = None;
        let mut current_cmds: Vec<String> = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // Check for [pane:N] or [pane:N title=xxx] header
            if trimmed.starts_with("[pane:") && trimmed.ends_with(']') {
                // Save previous pane
                if !current_cmds.is_empty() {
                    result.push((current_pane, current_title.take(), current_cmds.clone()));
                    current_cmds.clear();
                }
                let inner = &trimmed[6..trimmed.len() - 1]; // "1 title=xxx" or "1"
                if let Some(space_pos) = inner.find(' ') {
                    current_pane = inner[..space_pos].parse().unwrap_or(1);
                    // Parse key=value attributes
                    let attrs = &inner[space_pos + 1..];
                    if let Some(t) = attrs.strip_prefix("title=") {
                        let t = t.trim();
                        if !t.is_empty() {
                            current_title = Some(t.to_string());
                        }
                    }
                } else {
                    current_pane = inner.parse().unwrap_or(1);
                    current_title = None;
                }
            } else {
                current_cmds.push(trimmed.to_string());
            }
        }
        // Save last pane
        if !current_cmds.is_empty() {
            result.push((current_pane, current_title, current_cmds));
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
    pub(crate) fn run_startup_commands(&mut self) {
        let ws_name = self.model.active_workspace_name
            .clone()
            .unwrap_or_else(|| self.active_workspace_id.clone());
        let path = Self::startup_file_path(&ws_name);
        let pane_cmds = Self::parse_startup_file(&path);
        if pane_cmds.is_empty() {
            return;
        }

        let (shell, shell_args) = Self::default_shell();
        let cwd = Self::default_cwd();

        for (i, (pane_num, custom_title, cmds)) in pane_cmds.iter().enumerate() {
            // First pane already exists, subsequent panes need split
            if i > 0 {
                let direction = if i % 2 == 1 {
                    SplitDirection::Horizontal
                } else {
                    SplitDirection::Vertical
                };
                self.terminal_manager_mut().split_active_pane(direction);
                self.spawn_terminal_in_active();
            }

            // Send commands to the active terminal
            if let Some(term) = self.terminal_manager_mut().active_terminal() {
                for cmd in cmds {
                    let input = format!("{}\r", cmd);
                    term.send_input(input.as_bytes());
                }
            }

            // Set tab title: custom_title > last command name > pane:N
            let active_id = self.terminal_manager().active_pane_id().cloned();
            if let Some(ref pid) = active_id {
                if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                    if let Some(tab) = pane.tabs.get_mut(pane.active_tab) {
                        let title = if let Some(t) = custom_title {
                            t.clone()
                        } else if let Some(last_cmd) = cmds.last() {
                            last_cmd.split_whitespace().next()
                                .unwrap_or("Terminal").to_string()
                        } else {
                            format!("pane:{}", pane_num)
                        };
                        tab.title = title;
                        tab.custom_title = custom_title.is_some();
                    }
                }
            }
        }

        // Equalize splits after creating all panes
        self.terminal_manager_mut().equalize_splits();
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
        let _ = std::fs::create_dir_all(&dir);
        let safe_name = template.name.replace(['/', '\\', ':', ' '], "_");
        let path = dir.join(format!("{}.json", safe_name));
        if let Ok(json) = serde_json::to_string_pretty(template) {
            let _ = std::fs::write(path, json);
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
                    if let Ok(t) = serde_json::from_str(&data) {
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

    /// Save all workspace layouts to disk
    pub(crate) fn save_all_layouts(&self) {
        let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (ws_id, tm) in &self.workspace_terminals {
            map.insert(ws_id.clone(), tm.save_layout());
        }
        let path = Self::layout_file_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string(&map) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Load all workspace layouts from disk
    pub(crate) fn load_all_layouts() -> std::collections::HashMap<String, String> {
        let path = Self::layout_file_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        }
    }
}
