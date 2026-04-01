//! Keyboard Shortcuts Module
//!
//! Defines all keyboard shortcuts for AMUX based on limux conventions.
//! Shortcuts use Ctrl as the primary modifier.

use std::collections::HashMap;

/// Represents a keyboard shortcut with its action
#[derive(Clone, Debug)]
pub struct Shortcut {
    /// The keystroke string (e.g., "ctrl+d", "escape")
    pub keystroke: String,
    /// Display label (e.g., "Ctrl+D")
    pub label: String,
    /// Description of the action
    pub description: String,
    /// Category for grouping
    pub category: ShortcutCategory,
}

#[derive(Clone, Debug, PartialEq, Eq, std::hash::Hash)]
pub enum ShortcutCategory {
    App,
    Workspace,
    Terminal,
    Pane,
    Browser,
    Find,
}

impl Shortcut {
    pub fn new(keystroke: &str, label: &str, description: &str, category: ShortcutCategory) -> Self {
        Self {
            keystroke: keystroke.to_lowercase(),
            label: label.to_string(),
            description: description.to_string(),
            category,
        }
    }
}

/// Returns all defined shortcuts
pub fn all_shortcuts() -> Vec<Shortcut> {
    vec![
        // App shortcuts
        Shortcut::new(
            "ctrl+q",
            "Ctrl+Q",
            "Quit AMUX",
            ShortcutCategory::App,
        ),
        Shortcut::new(
            "f11",
            "F11",
            "Toggle fullscreen",
            ShortcutCategory::App,
        ),
        Shortcut::new(
            "ctrl+shift+n",
            "Ctrl+Shift+N",
            "New workspace (folder picker)",
            ShortcutCategory::Workspace,
        ),
        Shortcut::new(
            "ctrl+shift+w",
            "Ctrl+Shift+W",
            "Close workspace",
            ShortcutCategory::Workspace,
        ),
        // Pane shortcuts
        Shortcut::new(
            "ctrl+d",
            "Ctrl+D",
            "Split right",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+shift+d",
            "Ctrl+Shift+D",
            "Split down",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+w",
            "Ctrl+W",
            "Close focused pane",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+t",
            "Ctrl+T",
            "New terminal tab",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+shift+t",
            "Ctrl+Shift+T",
            "New terminal tab in focused pane",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+pageup",
            "Ctrl+PageUp",
            "Previous workspace",
            ShortcutCategory::Workspace,
        ),
        Shortcut::new(
            "ctrl+pagedown",
            "Ctrl+PageDown",
            "Next workspace",
            ShortcutCategory::Workspace,
        ),
        Shortcut::new(
            "ctrl+m",
            "Ctrl+M",
            "Toggle sidebar",
            ShortcutCategory::Pane,
        ),
        // Terminal shortcuts
        Shortcut::new(
            "ctrl+k",
            "Ctrl+K",
            "Clear scrollback",
            ShortcutCategory::Terminal,
        ),
        Shortcut::new(
            "ctrl+c",
            "Ctrl+C",
            "Copy selection",
            ShortcutCategory::Terminal,
        ),
        Shortcut::new(
            "ctrl+v",
            "Ctrl+V",
            "Paste",
            ShortcutCategory::Terminal,
        ),
        Shortcut::new(
            "ctrl+=",
            "Ctrl++",
            "Increase font size",
            ShortcutCategory::Terminal,
        ),
        Shortcut::new(
            "ctrl+-",
            "Ctrl+-",
            "Decrease font size",
            ShortcutCategory::Terminal,
        ),
        Shortcut::new(
            "ctrl+0",
            "Ctrl+0",
            "Reset font size",
            ShortcutCategory::Terminal,
        ),
        // Resize shortcuts
        Shortcut::new(
            "ctrl+alt+left",
            "Ctrl+Alt+←",
            "Resize pane left",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+alt+right",
            "Ctrl+Alt+→",
            "Resize pane right",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+alt+up",
            "Ctrl+Alt+↑",
            "Resize pane up",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+alt+down",
            "Ctrl+Alt+↓",
            "Resize pane down",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+alt+=",
            "Ctrl+Alt++",
            "Equalize pane sizes",
            ShortcutCategory::Pane,
        ),
        // Find shortcuts
        Shortcut::new(
            "ctrl+f",
            "Ctrl+F",
            "Open find on focused terminal/browser",
            ShortcutCategory::Find,
        ),
        Shortcut::new(
            "ctrl+g",
            "Ctrl+G",
            "Find next",
            ShortcutCategory::Find,
        ),
        Shortcut::new(
            "ctrl+shift+g",
            "Ctrl+Shift+G",
            "Find previous",
            ShortcutCategory::Find,
        ),
        Shortcut::new(
            "ctrl+e",
            "Ctrl+E",
            "Use selection for find",
            ShortcutCategory::Find,
        ),
        // Command palette
        Shortcut::new(
            "ctrl+p",
            "Ctrl+P",
            "Open command palette",
            ShortcutCategory::App,
        ),
        Shortcut::new(
            "escape",
            "Escape",
            "Close palette / Cancel",
            ShortcutCategory::App,
        ),
        // Arrow navigation
        Shortcut::new(
            "ctrl+left",
            "Ctrl+←",
            "Focus pane left",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+right",
            "Ctrl+→",
            "Focus pane right",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+up",
            "Ctrl+↑",
            "Focus pane up",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+down",
            "Ctrl+↓",
            "Focus pane down",
            ShortcutCategory::Pane,
        ),
        // Tab shortcuts
        Shortcut::new(
            "ctrl+tab",
            "Ctrl+Tab",
            "Next tab",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+shift+tab",
            "Ctrl+Shift+Tab",
            "Previous tab",
            ShortcutCategory::Pane,
        ),
        Shortcut::new(
            "ctrl+shift+enter",
            "Ctrl+Shift+Enter",
            "Send selection to pane",
            ShortcutCategory::Pane,
        ),
        // Settings
        Shortcut::new(
            "ctrl+,",
            "Ctrl+,",
            "Open settings",
            ShortcutCategory::App,
        ),
        // WSL browser
        Shortcut::new(
            "ctrl+shift+b",
            "Ctrl+Shift+B",
            "Toggle WSL file browser",
            ShortcutCategory::Browser,
        ),
        // Quick switcher
        Shortcut::new(
            "ctrl+1",
            "Ctrl+1",
            "Switch to workspace 1",
            ShortcutCategory::Workspace,
        ),
        Shortcut::new(
            "ctrl+2",
            "Ctrl+2",
            "Switch to workspace 2",
            ShortcutCategory::Workspace,
        ),
        Shortcut::new(
            "ctrl+3",
            "Ctrl+3",
            "Switch to workspace 3",
            ShortcutCategory::Workspace,
        ),
        Shortcut::new(
            "ctrl+4",
            "Ctrl+4",
            "Switch to workspace 4",
            ShortcutCategory::Workspace,
        ),
        Shortcut::new(
            "ctrl+5",
            "Ctrl+5",
            "Switch to workspace 5",
            ShortcutCategory::Workspace,
        ),
    ]
}

/// Returns shortcuts grouped by category
pub fn shortcuts_by_category() -> HashMap<ShortcutCategory, Vec<Shortcut>> {
    let mut map: HashMap<ShortcutCategory, Vec<Shortcut>> = HashMap::new();
    for shortcut in all_shortcuts() {
        map.entry(shortcut.category.clone())
            .or_default()
            .push(shortcut);
    }
    map
}

/// Category display names
pub fn category_display_name(category: &ShortcutCategory) -> &'static str {
    match category {
        ShortcutCategory::App => "Application",
        ShortcutCategory::Workspace => "Workspace",
        ShortcutCategory::Terminal => "Terminal",
        ShortcutCategory::Pane => "Panes & Tabs",
        ShortcutCategory::Browser => "Browser",
        ShortcutCategory::Find => "Find",
    }
}

/// Check if a keystroke matches a shortcut pattern
pub fn keystroke_matches(keystroke: &str, pattern: &str) -> bool {
    let keystroke = keystroke.to_lowercase();
    let pattern = pattern.to_lowercase();

    // Direct match
    if keystroke == pattern {
        return true;
    }

    // Normalize key names
    let normalize = |s: &str| -> String {
        s.replace("control", "ctrl")
            .replace("escape", "esc")
            .replace("arrowup", "up")
            .replace("arrowdown", "down")
            .replace("arrowleft", "left")
            .replace("arrowright", "right")
    };

    normalize(&keystroke) == normalize(&pattern)
}

/// Parse a keystroke string into its components
pub fn parse_keystroke(keystroke: &str) -> (Vec<String>, String) {
    let parts: Vec<&str> = keystroke.split('+').collect();
    if parts.len() > 1 {
        (parts[..parts.len() - 1].iter().map(|s| s.to_lowercase()).collect(), parts.last().unwrap().to_lowercase())
    } else {
        (Vec::new(), keystroke.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keystroke_matches() {
        assert!(keystroke_matches("ctrl+d", "ctrl+d"));
        assert!(keystroke_matches("control+d", "ctrl+d"));
        assert!(keystroke_matches("escape", "escape"));
        assert!(keystroke_matches("ESCAPE", "escape"));
    }

    #[test]
    fn test_parse_keystroke() {
        let (mods, key) = parse_keystroke("ctrl+shift+d");
        assert_eq!(mods, vec!["ctrl", "shift"]);
        assert_eq!(key, "d");

        let (mods, key) = parse_keystroke("escape");
        assert!(mods.is_empty());
        assert_eq!(key, "escape");
    }

    #[test]
    fn test_shortcuts_by_category() {
        let map = shortcuts_by_category();
        assert!(map.contains_key(&ShortcutCategory::App));
        assert!(map.contains_key(&ShortcutCategory::Pane));
        assert!(map.contains_key(&ShortcutCategory::Terminal));
    }
}
