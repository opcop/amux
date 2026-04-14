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
    /// Absolute path resolved from the active selection at the
    /// moment the menu opened. `Some` iff the selection text
    /// resolves to an existing file via the Tier 1 pipeline.
    /// Drives the "Open Selection as File" menu item — resolved
    /// once at right-click time instead of on every render frame
    /// so the FS stats don't run in the render hot path.
    pub(crate) selection_path: Option<String>,
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

/// Auto-scroll state while the user is dragging a selection past the
/// edge of a terminal pane. The mouse_move handler updates this every
/// time the cursor leaves the pane vertically; a single spawned tick
/// loop reads it on a timer and scrolls the scrollback while
/// extending the selection to the freshly revealed rows. The loop
/// exits on its own as soon as this becomes `None` (cursor returned
/// to the viewport, mouse released, or selection canceled).
#[derive(Clone, Debug)]
pub(crate) struct SelectionAutoScrollState {
    pub(crate) pane_id: amux_platform::terminal::manager::PaneId,
    /// Pixels the cursor is past the pane edge. Positive = past the
    /// top edge (need older history), negative = past the bottom edge
    /// (need to rewind toward the live screen).
    pub(crate) edge_pixels: f32,
    /// Last known mouse x (window space) — used to compute which
    /// column the auto-extended selection endpoint should land on.
    pub(crate) last_mouse_x: f32,
}

/// Where on the scrollbar a click landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollbarHit {
    Thumb,
    TrackAbove,
    TrackBelow,
}

/// Drag state for the scrollback scrollbar thumb.
///
/// Snapshotted at mousedown so the drag stays consistent even if
/// new PTY output grows the history mid-drag — the thumb tracks the
/// mouse against the geometry the user clicked on, not a moving target.
#[derive(Clone, Debug)]
pub(crate) struct ScrollbarDragState {
    pub(crate) pane_id: amux_platform::terminal::manager::PaneId,
    pub(crate) start_mouse_y: f32,
    pub(crate) start_offset: usize,
    pub(crate) history: usize,
    pub(crate) track_h: f32,
    pub(crate) thumb_h: f32,
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
