//! Plain-data UI state structs extracted from `gpui_entry.rs`.
//!
//! `gpui_entry.rs` grew past 3700 lines and the signal-to-noise
//! ratio of opening it suffered because its prelude carried a dozen
//! unrelated state structs. This module collects the pure-data ones
//! — picker states, drag state, toast, context menu, search — so
//! that the entry file only has to host `GpuiShellView` + its
//! `Render` impl + the root `run()`.
//!
//! ## Scope policy
//!
//! What lives here:
//!   * plain POD-ish `Clone + Debug` state structs,
//!   * their trivial constructors / small `impl` helpers,
//!   * a handful of UI constants tied to that state.
//!
//! What **does not** live here:
//!   * `GpuiShellView` itself,
//!   * anything with an `impl Render<Self>` or other heavy GPUI
//!     coupling (drag ghosts, context-menu renderer),
//!   * business logic — `impl` blocks that mutate the world belong
//!     with their owner type in `gpui_entry.rs` until they're
//!     refactored in a later pass.
//!
//! The `pub(crate)` visibility here is deliberate: these structs
//! are still internal to the desktop binary. Exposing them via the
//! crate namespace just means call sites spell them as
//! `crate::state::SearchState` rather than `crate::gpui_entry::…`.

#![cfg(feature = "gpui")]

/// Scrollback search mode. Cycled with Tab inside the search bar.
///
///  * `Literal` — exact substring, smart case. Empty-result friendly.
///  * `Regex`   — full regex (alacritty's regex_automata engine).
///                Smart case unless the pattern already contains `(?i)`
///                or any uppercase. Invalid regex shows `err` in the UI.
///  * `Fuzzy`   — fzf-style subsequence match against each scrollback
///                line, case-insensitive. Scoring is not exposed
///                yet — results are returned in scrollback order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchMode {
    Literal,
    Regex,
    Fuzzy,
}

impl SearchMode {
    pub(crate) fn short_label(self) -> &'static str {
        match self {
            SearchMode::Literal => "L",
            SearchMode::Regex => "R",
            SearchMode::Fuzzy => "F",
        }
    }

    pub(crate) fn cycle(self) -> Self {
        match self {
            SearchMode::Literal => SearchMode::Regex,
            SearchMode::Regex => SearchMode::Fuzzy,
            SearchMode::Fuzzy => SearchMode::Literal,
        }
    }
}

/// Maximum matches collected per query — keeps UI responsive on huge
/// scrollbacks. UI indicates truncation with a trailing `+`.
pub(crate) const SEARCH_MATCH_CAP: usize = 1000;

/// State for the terminal scrollback search bar (Ctrl+Shift+S).
///
/// On every query or mode change the matches list is rebuilt from
/// scratch against the active terminal's scrollback. Navigation
/// (`Enter` / `Shift+Enter`) just advances `current` — the expensive
/// work happens once per edit, not per jump. `truncated` is set when
/// the scan hit `SEARCH_MATCH_CAP`; the UI shows it as a trailing
/// `+`. `error` is `true` when the last rebuild failed to compile
/// (regex mode with an invalid pattern).
#[derive(Clone, Debug)]
pub(crate) struct SearchState {
    pub(crate) query: String,
    pub(crate) mode: SearchMode,
    pub(crate) matches: Vec<alacritty_terminal::term::search::Match>,
    pub(crate) current: usize,
    pub(crate) truncated: bool,
    pub(crate) error: bool,
}

impl SearchState {
    pub(crate) fn new() -> Self {
        Self {
            query: String::new(),
            mode: SearchMode::Literal,
            matches: Vec::new(),
            current: 0,
            truncated: false,
            error: false,
        }
    }
}

/// Right-click context menu state.
///
/// Fields are `pub(crate)` so the render path in `gpui_entry.rs`
/// can read `menu.position` and the action dispatcher can read
/// `menu.source_pane` without going through accessors.
#[derive(Clone, Debug)]
pub(crate) struct ContextMenuState {
    pub(crate) position: gpui::Point<gpui::Pixels>,
    /// The pane that was active when the menu was opened.
    /// Actions should target this pane, not whatever pane is
    /// active at click time.
    pub(crate) source_pane: Option<amux_platform::terminal::manager::PaneId>,
}

/// Drag state for resizing split panes.
#[derive(Clone, Debug)]
pub(crate) struct ResizeDragState {
    pub(crate) split_first_pane: String,
    pub(crate) is_horizontal: bool,
    pub(crate) start_mouse_pos: f32,
    pub(crate) start_ratio: f32,
    pub(crate) container_length: f32,
}

/// Toast notification for agent status changes.
#[derive(Clone, Debug)]
pub(crate) struct ToastNotification {
    pub(crate) message: String,
    pub(crate) color: u32,
    pub(crate) frame_created: u32,
    pub(crate) pane_id: amux_platform::terminal::manager::PaneId,
    pub(crate) tab_index: usize,
}

/// Pane picker state for "Send to Pane" feature.
#[derive(Clone, Debug)]
pub(crate) struct PanePickerState {
    pub(crate) text: String,
    pub(crate) targets: Vec<(amux_platform::terminal::manager::PaneId, String)>,
    pub(crate) selected_index: usize,
}

/// Template picker state for "Apply Layout" feature.
#[derive(Clone, Debug)]
pub(crate) struct TemplatePickerState {
    pub(crate) templates: Vec<amux_platform::terminal::manager::LayoutTemplate>,
    pub(crate) selected_index: usize,
}

/// Agent launcher picker state.
#[derive(Clone, Debug)]
pub(crate) struct AgentPickerState {
    /// (tool_id, display_label, is_wsl)
    pub(crate) agents: Vec<(String, String, bool)>,
    pub(crate) selected_index: usize,
}

/// New-tab picker state (dropdown from the `+▾` button).
#[derive(Clone, Debug)]
pub(crate) struct NewTabPickerState {
    /// Which pane this picker was opened from.
    pub(crate) pane_id: amux_platform::terminal::manager::PaneId,
    pub(crate) items: Vec<NewTabPickerItem>,
    pub(crate) selected_index: usize,
    /// Anchor position (top-right of the `+▾` button) for dropdown placement.
    pub(crate) anchor: gpui::Point<gpui::Pixels>,
}

#[derive(Clone, Debug)]
pub(crate) struct NewTabPickerItem {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) icon: &'static str,
    pub(crate) separator_after: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_state_default_is_empty_literal() {
        let s = SearchState::new();
        assert!(s.query.is_empty());
        assert_eq!(s.mode, SearchMode::Literal);
        assert!(s.matches.is_empty());
        assert_eq!(s.current, 0);
        assert!(!s.truncated);
        assert!(!s.error);
    }

    #[test]
    fn mode_cycle_is_three_cycle() {
        let mut m = SearchMode::Literal;
        m = m.cycle();
        assert_eq!(m, SearchMode::Regex);
        m = m.cycle();
        assert_eq!(m, SearchMode::Fuzzy);
        m = m.cycle();
        assert_eq!(m, SearchMode::Literal);
    }
}
