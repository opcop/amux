use std::path::PathBuf;

use amux_platform::PlatformCapabilities;
use amux_core::{PaneId, SplitAxis, SurfaceState, TabId, WorkspaceTarget};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiAction {
    ToggleCommandPalette,
    SetCommandPaletteQuery(String),
    AppendCommandPaletteQuery(String),
    BackspaceCommandPaletteQuery,
    ClearCommandPaletteQuery,
    SetCommandPaletteSelectedIndex(usize),
    SelectNextCommandPaletteItem,
    SelectPreviousCommandPaletteItem,
    OpenWorkspacePicker,
    OpenLocalWorkspace(PathBuf),
    OpenWslWorkspace {
        distro: String,
        path: String,
    },
    FocusPane(PaneId),
    FocusNextPane,
    FocusPreviousPane,
    FocusNextTab,
    FocusPreviousTab,
    OpenSurface {
        pane_id: PaneId,
        surface: SurfaceState,
    },
    CloseTab {
        pane_id: PaneId,
        tab_id: TabId,
    },
    SplitPane {
        pane_id: PaneId,
        axis: SplitAxis,
    },
    PinTab,
    UnpinTab,
    RenameTab(String),
    CloseOtherTabs,
}

impl UiAction {
    pub fn to_core_command(self) -> Option<amux_core::Command> {
        match self {
            UiAction::ToggleCommandPalette
            | UiAction::SetCommandPaletteQuery(_)
            | UiAction::AppendCommandPaletteQuery(_)
            | UiAction::BackspaceCommandPaletteQuery
            | UiAction::ClearCommandPaletteQuery
            | UiAction::SetCommandPaletteSelectedIndex(_)
            | UiAction::SelectNextCommandPaletteItem
            | UiAction::SelectPreviousCommandPaletteItem
            | UiAction::OpenWorkspacePicker
            | UiAction::FocusNextPane
            | UiAction::FocusPreviousPane
            | UiAction::FocusNextTab
            | UiAction::FocusPreviousTab => None,
            UiAction::OpenLocalWorkspace(path) => Some(amux_core::Command::OpenWorkspace(
                WorkspaceTarget::LocalPath { path },
            )),
            UiAction::OpenWslWorkspace { distro, path } => Some(amux_core::Command::OpenWorkspace(
                WorkspaceTarget::WslPath { distro, path },
            )),
            UiAction::FocusPane(pane_id) => Some(amux_core::Command::FocusPane(pane_id)),
            UiAction::OpenSurface { pane_id, surface } => {
                Some(amux_core::Command::OpenSurface { pane_id, surface })
            }
            UiAction::CloseTab { pane_id, tab_id } => {
                Some(amux_core::Command::CloseTab { pane_id, tab_id })
            }
            UiAction::SplitPane { pane_id, axis } => {
                Some(amux_core::Command::SplitPane { pane_id, axis })
            }
            UiAction::PinTab
            | UiAction::UnpinTab
            | UiAction::RenameTab(_)
            | UiAction::CloseOtherTabs => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppCommand {
    Ui(UiAction),
    LaunchAgent { provider_id: String },
    OpenFile { relative_path: String },
    ResizeSplit(f32),
    ResetSplitRatios,
    ShowHelp,
    SaveSession,
    ListWslDistros,
    // Auto-save commands
    EnableAutoSave,
    DisableAutoSave,
    SetAutoSaveInterval(u64), // seconds
    ShowAutoSaveStatus,
    // WSL file browser commands
    BrowseWslRoot,
    BrowseWslPath(String), // path to browse
    // Quick switcher commands
    SwitchWorkspace(usize),  // Switch to workspace by index (1-9)
    SwitchNextWorkspace,     // Switch to next workspace
    SwitchPreviousWorkspace, // Switch to previous workspace
    FocusNextPane,           // Switch to next pane
    FocusPreviousPane,       // Switch to previous pane
    FocusNextTab,            // Switch to next tab in current pane
    FocusPreviousTab,        // Switch to previous tab in current pane
    OpenSettings,            // Open settings panel
    IncreaseFontSize,        // Increase terminal font size
    DecreaseFontSize,        // Decrease terminal font size
    ResetFontSize,           // Reset terminal font size to default
    // File operations
    CreateFile { path: String },
    CreateDirectory { path: String },
    DeleteFile { path: String },
    RenameFile { old_path: String, new_path: String },
    // Workspace operations
    CloseWorkspace { id: Option<String> },
    RenameWorkspace { id: String, new_name: String },
    ReorderWorkspace { from_index: usize, to_index: usize },
    // Browser operations
    OpenBrowser { url: Option<String> },
}

pub fn parse_command(input: &str, active_pane_id: Option<PaneId>) -> Result<AppCommand, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty command".into());
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["help"] => Ok(AppCommand::ShowHelp),
        ["save"] => Ok(AppCommand::SaveSession),
        ["palette"] => Ok(AppCommand::Ui(UiAction::ToggleCommandPalette)),
        ["workspace", "open", path] => Ok(AppCommand::Ui(UiAction::OpenLocalWorkspace(
            PathBuf::from(path),
        ))),
        ["workspace", "open-wsl", distro, path] => Ok(AppCommand::Ui(UiAction::OpenWslWorkspace {
            distro: (*distro).into(),
            path: (*path).into(),
        })),
        ["workspace", "close"] => Ok(AppCommand::CloseWorkspace { id: None }),
        ["workspace", "close", id] => Ok(AppCommand::CloseWorkspace { id: Some((*id).into()) }),
        ["pane", "split-right"] => {
            let pane_id = active_pane_id.ok_or_else(|| "no active pane".to_string())?;
            Ok(AppCommand::Ui(UiAction::SplitPane {
                pane_id,
                axis: SplitAxis::Horizontal,
            }))
        }
        ["pane", "split-down"] => {
            let pane_id = active_pane_id.ok_or_else(|| "no active pane".to_string())?;
            Ok(AppCommand::Ui(UiAction::SplitPane {
                pane_id,
                axis: SplitAxis::Vertical,
            }))
        }
        ["agent", provider_id] => Ok(AppCommand::LaunchAgent {
            provider_id: (*provider_id).into(),
        }),
        ["file", "open", relative_path] => Ok(AppCommand::OpenFile {
            relative_path: (*relative_path).into(),
        }),
        ["pane", "resize-left"] => Ok(AppCommand::ResizeSplit(-0.1)),
        ["pane", "resize-right"] => Ok(AppCommand::ResizeSplit(0.1)),
        ["pane", "resize-reset"] => Ok(AppCommand::ResetSplitRatios),
        ["wsl", "list"] => Ok(AppCommand::ListWslDistros),
        // Auto-save commands
        ["autosave", "enable"] => Ok(AppCommand::EnableAutoSave),
        ["autosave", "disable"] => Ok(AppCommand::DisableAutoSave),
        ["autosave", "interval", secs] => {
            let interval: u64 = secs
                .parse()
                .map_err(|_| "invalid interval, expected number of seconds".to_string())?;
            Ok(AppCommand::SetAutoSaveInterval(interval))
        }
        ["autosave", "status"] => Ok(AppCommand::ShowAutoSaveStatus),
        // WSL file browser commands
        ["wsl", "browse", path] => Ok(AppCommand::BrowseWslPath((*path).into())),
        ["wsl", "ls"] => Ok(AppCommand::BrowseWslRoot),
        // Quick switcher commands - specific patterns first
        ["switch", "workspace", "next"] => Ok(AppCommand::SwitchNextWorkspace),
        ["switch", "workspace", "prev"] => Ok(AppCommand::SwitchPreviousWorkspace),
        ["switch", "pane", "next"] => Ok(AppCommand::FocusNextPane),
        ["switch", "pane", "prev"] => Ok(AppCommand::FocusPreviousPane),
        ["switch", "tab", "next"] => Ok(AppCommand::FocusNextTab),
        ["switch", "tab", "prev"] => Ok(AppCommand::FocusPreviousTab),
        ["switch", "workspace", n] => {
            let idx: usize = n
                .parse()
                .map_err(|_| "invalid workspace number".to_string())?;
            Ok(AppCommand::SwitchWorkspace(idx))
        }
        ["settings"] | ["preferences"] => Ok(AppCommand::OpenSettings),
        // Font size commands
        ["font", "increase"] | ["font", "zoom-in"] => Ok(AppCommand::IncreaseFontSize),
        ["font", "decrease"] | ["font", "zoom-out"] => Ok(AppCommand::DecreaseFontSize),
        ["font", "reset"] => Ok(AppCommand::ResetFontSize),
        // Browser commands
        ["browser"] | ["open", "url"] | ["open", "browser"] => {
            let url = parts.get(1).map(|s| s.to_string());
            Ok(AppCommand::OpenBrowser { url })
        }
        _ => Err(format!("unknown command: {trimmed}")),
    }
}

pub fn command_help() -> &'static [&'static str] {
    const HELP: &[&str] = &[
        "help",
        "save",
        "palette",
        "settings",
        "workspace open <path>",
        "workspace open-wsl <distro> <path>",
        "wsl list",
        "wsl ls",
        "wsl browse <path>",
        "pane split-right",
        "pane split-down",
        "pane resize-left",
        "pane resize-right",
        "pane resize-reset",
        "switch workspace <n>",
        "switch workspace next|prev",
        "switch pane next|prev",
        "switch tab next|prev",
        "autosave enable",
        "autosave disable",
        "autosave interval <seconds>",
        "autosave status",
        "agent <provider_id>",
        "file open <relative_path>",
        "browser",
    ];
    HELP
}

pub fn command_help_for(capabilities: &PlatformCapabilities) -> Vec<&'static str> {
    command_help()
        .iter()
        .copied()
        .filter(|entry| {
            if entry.starts_with("workspace open-wsl")
                || entry.starts_with("wsl list")
                || entry.starts_with("wsl ls")
                || entry.starts_with("wsl browse")
            {
                return capabilities.wsl_workspace;
            }
            if entry == &"browser" {
                return capabilities.browser_tabs;
            }
            true
        })
        .collect()
}

/// Category for palette commands
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PaletteCategory {
    General,
    Workspace,
    Pane,
    Layout,
    Agent,
    File,
    Navigation,
    Session,
}

impl PaletteCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Workspace => "Workspace",
            Self::Pane => "Pane",
            Self::Layout => "Layout",
            Self::Agent => "Agent",
            Self::File => "File",
            Self::Navigation => "Navigation",
            Self::Session => "Session",
        }
    }

    pub fn all() -> &'static [PaletteCategory] {
        &[
            Self::General,
            Self::Workspace,
            Self::Pane,
            Self::Layout,
            Self::Agent,
            Self::File,
            Self::Navigation,
            Self::Session,
        ]
    }
}

/// Structured palette command entry
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteCommand {
    pub command: String,
    pub label: String,
    pub description: String,
    pub category: PaletteCategory,
    pub keybinding: Option<String>,
}

impl PaletteCommand {
    fn new(
        command: &str,
        label: &str,
        description: &str,
        category: PaletteCategory,
        keybinding: Option<&str>,
    ) -> Self {
        Self {
            command: command.to_string(),
            label: label.to_string(),
            description: description.to_string(),
            category,
            keybinding: keybinding.map(|k| k.to_string()),
        }
    }
}

fn command_supported(command: &PaletteCommand, capabilities: &PlatformCapabilities) -> bool {
    if command.command.starts_with("workspace open-wsl")
        || command.command.starts_with("wsl list")
        || command.command.starts_with("wsl browse")
    {
        return capabilities.wsl_workspace;
    }
    if command.command == "browser" || command.command.starts_with("open browser") {
        return capabilities.browser_tabs;
    }
    true
}

pub fn palette_filter_help() -> &'static [&'static str] {
    &[
        "all",
        "workspace",
        "pane",
        "layout",
        "agent",
        "file",
        "navigation",
        "session",
    ]
}

pub fn palette_command_catalog() -> Vec<PaletteCommand> {
    // Per-platform shortcut string helper. macOS shows ⌘ and uses
    // it as the primary modifier; Windows/Linux use Ctrl. Kept
    // as string literals rather than pulling in a key-formatting
    // crate — the palette is a text label, not a key binding.
    #[cfg(target_os = "macos")]
    macro_rules! primary { ($k:literal) => { concat!("⌘", $k) }; }
    #[cfg(not(target_os = "macos"))]
    macro_rules! primary { ($k:literal) => { concat!("Ctrl+", $k) }; }
    #[cfg(target_os = "macos")]
    macro_rules! primary_shift { ($k:literal) => { concat!("⌘⇧", $k) }; }
    #[cfg(not(target_os = "macos"))]
    macro_rules! primary_shift { ($k:literal) => { concat!("Ctrl+Shift+", $k) }; }

    vec![
        // General
        PaletteCommand::new(
            "help",
            "Show Help",
            "Display available commands",
            PaletteCategory::General,
            Some("?"),
        ),
        PaletteCommand::new(
            "save",
            "Save Session",
            "Persist current session to disk",
            PaletteCategory::General,
            None,
        ),
        PaletteCommand::new(
            "palette",
            "Toggle Palette",
            "Open or close command palette",
            PaletteCategory::General,
            Some(primary_shift!("P")),
        ),
        PaletteCommand::new(
            "browser",
            "Open Browser",
            "Open an embedded browser tab",
            PaletteCategory::General,
            Some(primary_shift!("B")),
        ),
        PaletteCommand::new(
            "find",
            "Find in Terminal",
            "Search scrollback with literal, regex, or fuzzy mode",
            PaletteCategory::General,
            Some(primary_shift!("S")),
        ),
        PaletteCommand::new(
            "quit",
            "Quit Amux",
            "Close the application",
            PaletteCategory::General,
            Some(primary!("Q")),
        ),
        // Workspace
        PaletteCommand::new(
            "workspace new",
            "New Workspace",
            "Open a folder as a new workspace (native folder picker)",
            PaletteCategory::Workspace,
            Some(primary_shift!("N")),
        ),
        PaletteCommand::new(
            "workspace open D:/repo/amux",
            "Open Workspace by Path",
            "Open a directory as workspace by typing its path",
            PaletteCategory::Workspace,
            None,
        ),
        PaletteCommand::new(
            "workspace open-wsl Ubuntu /home/user/project",
            "Open WSL Workspace",
            "Open a WSL directory as workspace",
            PaletteCategory::Workspace,
            None,
        ),
        PaletteCommand::new(
            "wsl list",
            "List WSL Distros",
            "Show available WSL distributions",
            PaletteCategory::Workspace,
            None,
        ),
        PaletteCommand::new(
            "wsl browse /home/user",
            "Browse WSL Path",
            "Browse files in WSL directory",
            PaletteCategory::Workspace,
            None,
        ),
        // Pane
        PaletteCommand::new(
            "pane split-right",
            "Split Right",
            "Split current pane horizontally",
            PaletteCategory::Pane,
            Some(primary_shift!("\\")),
        ),
        PaletteCommand::new(
            "pane split-down",
            "Split Down",
            "Split current pane vertically",
            PaletteCategory::Pane,
            Some(primary_shift!("D")),
        ),
        PaletteCommand::new(
            "pane new-tab",
            "New Tab",
            "Add a new terminal tab to the active pane",
            PaletteCategory::Pane,
            Some(primary_shift!("T")),
        ),
        PaletteCommand::new(
            "pane close",
            "Close Pane",
            "Close the active pane (or its last tab)",
            PaletteCategory::Pane,
            Some(primary_shift!("W")),
        ),
        PaletteCommand::new(
            "pane zoom",
            "Toggle Zoom",
            "Maximize the active pane to full content area",
            PaletteCategory::Pane,
            Some(primary_shift!("F")),
        ),
        PaletteCommand::new(
            "pane equalize",
            "Equalize Splits",
            "Reset all split ratios to equal size",
            PaletteCategory::Pane,
            Some(primary_shift!("E")),
        ),
        PaletteCommand::new(
            "pane send",
            "Send Selection to Pane",
            "Open the pane picker to route selected text elsewhere",
            PaletteCategory::Pane,
            Some(primary_shift!("Enter")),
        ),
        PaletteCommand::new(
            "pane resize-left",
            "Resize Left",
            "Shrink split ratio",
            PaletteCategory::Pane,
            Some(primary_shift!("Left")),
        ),
        PaletteCommand::new(
            "pane resize-right",
            "Resize Right",
            "Grow split ratio",
            PaletteCategory::Pane,
            Some(primary_shift!("Right")),
        ),
        PaletteCommand::new(
            "pane resize-reset",
            "Reset Split Ratio",
            "Reset all splits to equal size",
            PaletteCategory::Pane,
            None,
        ),
        // Navigation
        PaletteCommand::new(
            "switch workspace next",
            "Next Workspace",
            "Switch to next workspace",
            PaletteCategory::Navigation,
            None,
        ),
        PaletteCommand::new(
            "switch workspace prev",
            "Previous Workspace",
            "Switch to previous workspace",
            PaletteCategory::Navigation,
            None,
        ),
        PaletteCommand::new(
            "switch pane next",
            "Next Pane",
            "Focus next pane",
            PaletteCategory::Navigation,
            Some(primary!("Right")),
        ),
        PaletteCommand::new(
            "switch pane prev",
            "Previous Pane",
            "Focus previous pane",
            PaletteCategory::Navigation,
            Some(primary!("Left")),
        ),
        PaletteCommand::new(
            "switch tab next",
            "Next Tab",
            "Switch to next tab in pane",
            PaletteCategory::Navigation,
            Some("Ctrl+PageDown"),
        ),
        PaletteCommand::new(
            "switch tab prev",
            "Previous Tab",
            "Switch to previous tab in pane",
            PaletteCategory::Navigation,
            Some(primary!("PageUp")),
        ),
        PaletteCommand::new(
            "switch tab next",
            "Next Tab",
            "Switch to next tab in pane",
            PaletteCategory::Navigation,
            Some(primary!("PageDown")),
        ),
        PaletteCommand::new(
            "switch workspace 1",
            "Workspace 1",
            "Switch to workspace 1",
            PaletteCategory::Navigation,
            Some(primary!("1")),
        ),
        PaletteCommand::new(
            "switch workspace 2",
            "Workspace 2",
            "Switch to workspace 2",
            PaletteCategory::Navigation,
            Some(primary!("2")),
        ),
        // View
        PaletteCommand::new(
            "sidebar toggle",
            "Toggle Sidebar",
            "Collapse or expand the workspace sidebar",
            PaletteCategory::General,
            Some(primary_shift!("M")),
        ),
        PaletteCommand::new(
            "sidebar mode",
            "Switch Sidebar Mode",
            "Flip between Workspaces list and Agents list",
            PaletteCategory::General,
            Some(primary_shift!("A")),
        ),
        PaletteCommand::new(
            "scrollback clear",
            "Clear Scrollback",
            "Erase the current terminal's scrollback history",
            PaletteCategory::General,
            Some(primary!("K")),
        ),
        PaletteCommand::new(
            "font increase",
            "Font: Increase",
            "Make terminal text one size larger",
            PaletteCategory::General,
            Some(primary!("+")),
        ),
        PaletteCommand::new(
            "font decrease",
            "Font: Decrease",
            "Make terminal text one size smaller",
            PaletteCategory::General,
            Some(primary!("-")),
        ),
        PaletteCommand::new(
            "font reset",
            "Font: Reset",
            "Restore the default terminal font size",
            PaletteCategory::General,
            Some(primary!("0")),
        ),
        // Layout Templates
        PaletteCommand::new(
            "layout template AI + Shell",
            "Layout: AI + Shell",
            "Left 70% AI agent, right 30% shell",
            PaletteCategory::Layout,
            None,
        ),
        PaletteCommand::new(
            "layout template AI + Test + Git",
            "Layout: AI + Test + Git",
            "Left AI, right-top test runner, right-bottom git",
            PaletteCategory::Layout,
            None,
        ),
        PaletteCommand::new(
            "layout template Multi-Agent",
            "Layout: Multi-Agent",
            "Two AI agents top, shell bottom",
            PaletteCategory::Layout,
            None,
        ),
        PaletteCommand::new(
            "layout template Full Stack",
            "Layout: Full Stack",
            "4-grid: frontend, backend, test, shell",
            PaletteCategory::Layout,
            None,
        ),
        PaletteCommand::new(
            "layout save-as-template",
            "Save Layout as Template",
            "Save current pane layout as a reusable template",
            PaletteCategory::Layout,
            None,
        ),
        // Agent
        PaletteCommand::new(
            "agent codex",
            "Launch Codex",
            "Start Codex AI agent in terminal",
            PaletteCategory::Agent,
            None,
        ),
        PaletteCommand::new(
            "agent claude",
            "Launch Claude",
            "Start Claude AI agent in terminal",
            PaletteCategory::Agent,
            None,
        ),
        // File
        PaletteCommand::new(
            "file open README.md",
            "Open README",
            "Open README.md in editor",
            PaletteCategory::File,
            None,
        ),
        PaletteCommand::new(
            "file open notes.md",
            "Open Notes",
            "Open notes.md in editor",
            PaletteCategory::File,
            None,
        ),
        // Session
        PaletteCommand::new(
            "autosave enable",
            "Enable Auto-Save",
            "Turn on automatic session saving",
            PaletteCategory::Session,
            None,
        ),
        PaletteCommand::new(
            "autosave disable",
            "Disable Auto-Save",
            "Turn off automatic session saving",
            PaletteCategory::Session,
            None,
        ),
        PaletteCommand::new(
            "autosave interval 60",
            "Set Auto-Save Interval",
            "Set auto-save interval in seconds",
            PaletteCategory::Session,
            None,
        ),
        PaletteCommand::new(
            "autosave status",
            "Auto-Save Status",
            "Show auto-save configuration",
            PaletteCategory::Session,
            None,
        ),
    ]
}

pub fn palette_query_suggestions() -> &'static [&'static str] {
    &[
        "workspace",
        "pane",
        "split",
        "agent",
        "file",
        "switch",
        "settings",
    ]
}

pub fn palette_command_catalog_for(capabilities: &PlatformCapabilities) -> Vec<PaletteCommand> {
    palette_command_catalog()
        .into_iter()
        .filter(|command| command_supported(command, capabilities))
        .collect()
}

/// Filter palette commands by query string and optional category
pub fn filtered_palette_commands(query: &str) -> Vec<PaletteCommand> {
    filtered_palette_commands_for(query, &PlatformCapabilities::default())
}

pub fn filtered_palette_commands_for(
    query: &str,
    capabilities: &PlatformCapabilities,
) -> Vec<PaletteCommand> {
    let normalized = query.trim().to_ascii_lowercase();
    let catalog = palette_command_catalog_for(capabilities);

    if normalized.is_empty() {
        return catalog;
    }

    // Check if query starts with a category filter like "pane:" or "agent:"
    let (category_filter, search_term) = if let Some(pos) = normalized.find(':') {
        let cat = &normalized[..pos];
        let term = normalized[pos + 1..].trim();
        (Some(cat.to_string()), term.to_string())
    } else {
        (None, normalized)
    };

    catalog
        .into_iter()
        .filter(|cmd| {
            // Category filter
            if let Some(ref cat) = category_filter {
                if !cmd.category.label().to_ascii_lowercase().starts_with(cat) {
                    return false;
                }
            }
            // Text search across command, label, and description
            if search_term.is_empty() {
                return true;
            }
            let haystack = format!(
                "{} {} {}",
                cmd.command.to_ascii_lowercase(),
                cmd.label.to_ascii_lowercase(),
                cmd.description.to_ascii_lowercase(),
            );
            // Support multi-word queries: all words must match
            search_term
                .split_whitespace()
                .all(|word| haystack.contains(word))
        })
        .collect()
}

/// Legacy: get filtered commands as strings (for backward compatibility)
pub fn filtered_palette_command_strings(query: &str) -> Vec<String> {
    filtered_palette_commands(query)
        .iter()
        .map(|cmd| cmd.command.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use amux_core::{PaneId, SplitAxis};

    use super::{filtered_palette_commands, parse_command, AppCommand, PaletteCategory, UiAction};

    #[test]
    fn parses_split_command_against_active_pane() {
        let command = parse_command("pane split-down", Some(PaneId::new("pane-1")))
            .expect("command should parse");

        assert_eq!(
            command,
            AppCommand::Ui(UiAction::SplitPane {
                pane_id: PaneId::new("pane-1"),
                axis: SplitAxis::Vertical,
            })
        );
    }

    #[test]
    fn filters_palette_by_text_query() {
        let results = filtered_palette_commands("split");
        assert!(!results.is_empty());
        assert!(results.iter().all(|cmd| {
            let haystack =
                format!("{} {} {}", cmd.command, cmd.label, cmd.description).to_ascii_lowercase();
            haystack.contains("split")
        }));
    }

    #[test]
    fn filters_palette_by_category_prefix() {
        let results = filtered_palette_commands("agent:");
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .all(|cmd| cmd.category == PaletteCategory::Agent));
    }

    #[test]
    fn filters_palette_by_category_and_text() {
        let results = filtered_palette_commands("pane:split");
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .all(|cmd| cmd.category == PaletteCategory::Pane));
    }

    #[test]
    fn empty_query_returns_all_commands() {
        let all = filtered_palette_commands("");
        assert!(all.len() > 20);
    }

    #[test]
    fn multi_word_query_matches_all_words() {
        let results = filtered_palette_commands("workspace next");
        assert!(!results.is_empty());
        for cmd in &results {
            let haystack =
                format!("{} {} {}", cmd.command, cmd.label, cmd.description).to_ascii_lowercase();
            assert!(haystack.contains("workspace") && haystack.contains("next"));
        }
    }
}
