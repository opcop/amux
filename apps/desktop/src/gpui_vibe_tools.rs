//! Vibe Coding tool detection and launching
//!
//! Handles detection of AI CLI tools (Claude, Codex, OpenCode, Aider, Gemini, Copilot)
//! on both native and WSL environments, and launching them in terminal panes.

#[cfg(feature = "gpui")]
use amux_platform::terminal::manager::SplitDirection;

#[cfg(feature = "gpui")]
use crate::gpui_entry::GpuiShellView;

#[cfg(feature = "gpui")]
impl GpuiShellView {
    /// Detect all Vibe Coding tools once at startup.
    /// On Windows: checks both native PATH and WSL, may add two entries per tool.
    pub(crate) fn detect_all_vibe_tools() -> Vec<(&'static str, &'static str, &'static str)> {
        let tool_ids: &[&str] = &["claude", "opencode", "aider", "codex", "gemini", "copilot"];
        let has_wsl = if cfg!(target_os = "windows") { Self::wsl_available() } else { false };
        let mut results = Vec::new();

        for &tool_id in tool_ids {
            let Some((linux_bin, win_bin, _, _)) = Self::vibe_tool_info(tool_id) else {
                continue;
            };

            // Check native (Windows: where xxx.cmd, Linux: bash -ilc "command -v xxx")
            let native_bin = if cfg!(target_os = "windows") { win_bin } else { linux_bin };
            let found_native = Self::native_has_tool(native_bin);

            // Check WSL (Windows only)
            let found_wsl = if cfg!(target_os = "windows") && has_wsl {
                Self::wsl_has_tool(linux_bin)
            } else {
                false
            };

            // Add native entry
            if found_native {
                let label: &'static str = match tool_id {
                    "claude"   => "Launch Claude",
                    "opencode" => "Launch OpenCode",
                    "aider"    => "Launch Aider",
                    "codex"    => "Launch Codex",
                    "gemini"   => "Launch Gemini",
                    "copilot"  => "Launch Copilot",
                    _ => continue,
                };
                results.push((tool_id, label, "native"));
            }

            // Add WSL entry (even if native also exists — user may prefer WSL)
            if found_wsl {
                let label: &'static str = match tool_id {
                    "claude"   => "Launch Claude (WSL)",
                    "opencode" => "Launch OpenCode (WSL)",
                    "aider"    => "Launch Aider (WSL)",
                    "codex"    => "Launch Codex (WSL)",
                    "gemini"   => "Launch Gemini (WSL)",
                    "copilot"  => "Launch Copilot (WSL)",
                    _ => continue,
                };
                results.push((tool_id, label, "wsl"));
            }
        }
        results
    }

    // ─── WSL-aware tool detection ───────────────────────────────

    /// Create a Command that won't flash a console window on Windows.
    pub(crate) fn silent_command(program: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new(program);
        cmd.stdout(std::process::Stdio::null())
           .stderr(std::process::Stdio::null());
        // On Windows, prevent the subprocess from creating a visible console window
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        cmd
    }

    /// Check if WSL is available (Windows only, always false on other platforms).
    pub(crate) fn wsl_available() -> bool {
        if !cfg!(target_os = "windows") { return false; }
        Self::silent_command("wsl.exe")
            .arg("--status")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check if a binary exists in WSL (Windows only, always false on other platforms).
    fn wsl_has_tool(bin: &str) -> bool {
        if !cfg!(target_os = "windows") { return false; }
        Self::silent_command("wsl.exe")
            .args(["--", "which", bin])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check if a binary exists on the native platform.
    fn native_has_tool(bin: &str) -> bool {
        if cfg!(target_os = "windows") {
            Self::silent_command("where")
                .arg(bin)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        } else {
            let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            Self::silent_command(&sh)
                .args(["-ilc", &format!("command -v {} >/dev/null 2>&1", bin)])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
    }

    /// Detect where a Vibe Coding tool is available.
    /// Returns: "native", "wsl", or "" (not found).
    #[allow(dead_code)]
    fn detect_tool_env(bin: &str) -> &'static str {
        // 1. Check native PATH first
        if Self::native_has_tool(bin) {
            return "native";
        }
        // 2. On Windows, also check WSL
        #[cfg(target_os = "windows")]
        if Self::wsl_available() && Self::wsl_has_tool(bin) {
            return "wsl";
        }
        ""
    }

    /// Convert a Windows path to WSL mount path.
    /// e.g. "D:\projects\myapp" → "/mnt/d/projects/myapp"
    pub(crate) fn windows_path_to_wsl(path: &str) -> String {
        // Handle "D:\foo\bar" or "D:/foo/bar"
        let path = path.replace('\\', "/");
        if path.len() >= 2 && path.as_bytes()[1] == b':' {
            let drive = (path.as_bytes()[0] as char).to_ascii_lowercase();
            format!("/mnt/{}{}", drive, &path[2..])
        } else {
            path
        }
    }

    /// Vibe Coding tool definitions: (linux_bin, win_bin, extra_args, tab_title)
    pub(crate) fn vibe_tool_info(tool: &str) -> Option<(&'static str, &'static str, Vec<String>, &'static str)> {
        Some(match tool {
            "claude"   => ("claude",   "claude.cmd",   vec![], "Claude Code"),
            "opencode" => ("opencode", "opencode.cmd", vec![], "OpenCode"),
            "aider"    => ("aider",    "aider",        vec![], "Aider"),
            "codex"    => ("codex",    "codex.cmd",    vec![], "Codex CLI"),
            "gemini"   => ("gemini",   "gemini.cmd",   vec![], "Gemini CLI"),
            "copilot"  => ("gh",       "gh",           vec!["copilot".into()], "Copilot CLI"),
            _ => return None,
        })
    }

    /// Launch a Vibe Coding CLI tool in a new split pane.
    /// `use_wsl`: true to force WSL launch, false for native.
    pub(crate) fn launch_vibe_tool_env(&mut self, tool: &str, use_wsl: bool) {
        // Split right, then spawn in the new pane
        self.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
        self.spawn_vibe_tool_in_active(tool, use_wsl);
    }

    /// Spawn a vibe tool in the current active pane (no split).
    /// Used by compare task which manages its own layout.
    pub(crate) fn spawn_vibe_tool_in_active(&mut self, tool: &str, use_wsl: bool) {
        let Some((linux_bin, win_bin, extra_args, tab_title)) = Self::vibe_tool_info(tool) else {
            return;
        };
        let env = if use_wsl { "wsl" } else { "native" };
        let cwd = Self::default_cwd();

        let tool_cmd = if extra_args.is_empty() {
            linux_bin.to_string()
        } else {
            format!("{} {}", linux_bin, extra_args.join(" "))
        };

        let (shell, shell_args, spawn_cwd) = if use_wsl && cfg!(target_os = "windows") {
            let mut wsl_args = vec![];
            if let Some(ref cwd_str) = cwd {
                let wsl_path = Self::windows_path_to_wsl(cwd_str);
                wsl_args.extend(["--cd".to_string(), wsl_path]);
            }
            wsl_args.extend(["--".to_string(), "bash".to_string(), "-ilc".to_string(), tool_cmd]);
            ("wsl.exe".to_string(), wsl_args, None)
        } else if use_wsl {
            let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            (sh, vec!["-ilc".to_string(), tool_cmd], cwd.as_deref().map(|s| s.to_string()))
        } else if cfg!(target_os = "windows") {
            let bin = win_bin;
            let win_cmd = if extra_args.is_empty() {
                bin.to_string()
            } else {
                format!("{} {}", bin, extra_args.join(" "))
            };
            let (ps, _) = Self::default_shell();
            (ps, vec!["-NoLogo".to_string(), "-Command".to_string(), win_cmd], cwd.as_deref().map(|s| s.to_string()))
        } else {
            let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            (sh, vec!["-ilc".to_string(), tool_cmd], cwd.as_deref().map(|s| s.to_string()))
        };

        let spawn_cwd_ref = spawn_cwd.as_deref();
        let _ = self.terminal_manager_mut().spawn_in_active(&shell, &shell_args, spawn_cwd_ref);

        // Rename the tab
        let suffix = if env == "wsl" { " (WSL)" } else { "" };
        let title = format!("{}{}", tab_title, suffix);
        let active_id = self.terminal_manager().active_pane_id().cloned();
        if let Some(ref pid) = active_id {
            if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                if let Some(tab) = pane.tabs.last_mut() {
                    tab.title = title;
                }
            }
        }
    }

    /// Open a WSL bash shell in a new tab in the current pane.
    pub(crate) fn launch_wsl_shell(&mut self) {
        self.terminal_manager_mut().add_tab_to_active_pane("WSL".into());
        let cwd = Self::default_cwd();
        let mut wsl_args = vec![];
        if let Some(ref cwd_str) = cwd {
            let wsl_path = Self::windows_path_to_wsl(cwd_str);
            wsl_args.extend(["--cd".to_string(), wsl_path]);
        }
        let _ = self.terminal_manager_mut().spawn_in_active("wsl.exe", &wsl_args, None);
        // Rename the tab
        let active_id = self.terminal_manager().active_pane_id().cloned();
        if let Some(ref pid) = active_id {
            if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                if let Some(tab) = pane.tabs.last_mut() {
                    tab.title = "WSL".to_string();
                }
            }
        }
    }
}
