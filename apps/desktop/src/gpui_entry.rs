#[cfg(feature = "gpui")]
use amux_ui::{DesktopApp, GpuiWindowModel};
#[cfg(feature = "gpui")]
use gpui::{AppContext, Context, Window};
#[cfg(feature = "gpui")]
use amux_platform::terminal::manager::TerminalManager;
#[cfg(feature = "gpui")]
use crate::gpui_workspace_sidebar::WorkspaceSidebarState;


#[cfg(feature = "gpui")]
pub(crate) const SIDEBAR_WIDTH_COLLAPSED: f32 = 28.0;
pub(crate) const SIDEBAR_WIDTH_MIN: f32 = 120.0;
pub(crate) const SIDEBAR_WIDTH_MAX: f32 = 480.0;

#[cfg(feature = "gpui")]
pub(crate) struct GpuiShellView {
    pub(crate) app: DesktopApp,
    pub(crate) model: GpuiWindowModel,
    pub(crate) sidebar_state: WorkspaceSidebarState,
    pub(crate) workspace_terminals: std::collections::HashMap<String, TerminalManager>,
    pub(crate) active_workspace_id: String,
    pub(crate) focus_handle: gpui::FocusHandle,
    pub(crate) cell_metrics: Option<crate::gpui_terminal::CellMetrics>,
    pub(crate) selecting: bool,
    pub(crate) context_menu: Option<ContextMenuState>,
    pub(crate) resize_drag: Option<ResizeDragState>,
    pub(crate) scrollbar_drag: Option<ScrollbarDragState>,
    pub(crate) selection_autoscroll: Option<SelectionAutoScrollState>,
    /// Pane whose scrollbar the cursor is currently hovering over.
    /// Drives the hover-to-expand visual on the scrollbar.
    pub(crate) scrollbar_hover_pane: Option<amux_platform::terminal::manager::PaneId>,
    /// Hovered file-path link for Cmd/Ctrl+Click preview. Present
    /// when the preview modifier is held AND the cursor is over a
    /// valid path. Drives the underline highlight in the terminal
    /// renderer. Cleared on mouse move when the modifier is not
    /// held or no path is under the cursor.
    pub(crate) hover_link: Option<HoverLinkState>,
    pub(crate) cursor_blink_frame: u32,
    /// Frame number of the last PTY output (dirty flag set).
    /// Used for adaptive polling: 16ms when active, 100ms when idle.
    pub(crate) last_dirty_frame: u32,
    /// Frame when the last terminal bell (BEL) occurred. None = no bell.
    /// Used to render a brief visual flash when the bell rings.
    pub(crate) bell_flash_frame: Option<u32>,
    pub(crate) renaming_workspace:
        Option<(String, gpui::Entity<gpui_component::input::InputState>)>,
    pub(crate) renaming_tab: Option<(
        String,
        usize,
        gpui::Entity<gpui_component::input::InputState>,
    )>,
    pub(crate) search_state: Option<SearchState>,
    pub(crate) detected_vibe_tools: Vec<(&'static str, &'static str, &'static str)>,
    pub(crate) wsl_detected: bool,
    pub(crate) terminals_spawned: bool,
    pub(crate) tools_detected: bool,
    pub(crate) zoomed_pane: Option<amux_platform::terminal::manager::PaneId>,
    pub(crate) workspace_order: Vec<String>,
    pub(crate) pane_bounds: std::collections::HashMap<String, (f32, f32, f32, f32)>,
    pub(crate) config: crate::gpui_config::AmuxConfig,
    pub(crate) terminal_theme: crate::gpui_terminal::TerminalTheme,
    /// Toast notifications for agent status changes.
    pub(crate) toasts: Vec<ToastNotification>,
    /// Pane picker for "Send to Pane" (Ctrl+Shift+Enter)
    pub(crate) pane_picker: Option<PanePickerState>,
    /// Template picker for "Apply Layout..."
    pub(crate) template_picker: Option<TemplatePickerState>,
    /// Agent launcher picker
    pub(crate) agent_picker: Option<AgentPickerState>,
    /// New-tab dropdown picker (from `+▾` button on tab bar)
    pub(crate) new_tab_picker: Option<NewTabPickerState>,
    /// IME preedit text (composition in progress)
    pub(crate) ime_preedit: Option<String>,
    /// Accumulated fractional scroll for smooth trackpad scrolling.
    /// Trackpads send many small pixel-delta events; we accumulate
    /// them and only scroll by integer lines when a full cell_h has
    /// been reached. Positive = scrolling up (seeing earlier content).
    pub(crate) scroll_accumulator: f32,
    /// Sidebar resize drag: (start_mouse_x, start_width)
    pub(crate) sidebar_drag_start: Option<(f32, f32)>,
    /// Preview tab states keyed by file path
    pub(crate) preview_tabs: std::collections::HashMap<String, crate::gpui_preview::PreviewState>,
    /// File picker (Ctrl+P)
    pub(crate) file_picker: Option<crate::gpui_preview::FilePickerState>,
    /// Browser tab states keyed by browser_id (each browser tab has its own WebView2)
    pub(crate) browser_tabs: std::collections::HashMap<u64, crate::gpui_browser::BrowserTabEntry>,
    /// Next browser_id to assign. Bumped past the max id in saved
    /// layouts during startup restore so new browsers never collide
    /// with restored ones.
    pub(crate) next_browser_id: u64,
    /// One-shot latch: restored browser tab entries from the saved
    /// layout on the first render frame where `cached_window_handle`
    /// is available. Without this, persisted browser panes render
    /// as the "Browser loading..." fallback forever because
    /// `browser_tabs` starts empty but the pane tree already has
    /// `TabKind::Browser` entries.
    pub(crate) browsers_restored: bool,
    /// Flag: restore terminal focus on next render (set after URL input Enter)
    pub(crate) restore_terminal_focus: bool,
    /// Pending URL to sync to the address bar Input (set by timer, consumed by render)
    pub(crate) pending_url_bar_update: Option<String>,
    /// Cached raw window handle for WebView2 creation (avoids RefCell re-borrow)
    #[cfg(feature = "gpui")]
    pub(crate) cached_window_handle: Option<raw_window_handle::RawWindowHandle>,
    /// Count of crash logs found in `~/.amux/logs/crash` at startup.
    /// Surfaces as a passive red badge in the status bar until the
    /// user clears the directory manually. `None` when nothing was
    /// found (nothing to display).
    pub(crate) crash_notice: Option<usize>,
    /// Filesystem watcher for preview auto-reload. Created lazily on
    /// the first preview tab open and then reused for the lifetime of
    /// the app. Paths are added via `watch()` on preview-tab open and
    /// removed via `unwatch()` on preview-tab close. Events flow into
    /// `preview_reload_rx` via the callback; `poll_preview_reloads`
    /// drains the channel each render tick and schedules a background
    /// reload for any path that still has a live `preview_tabs` entry.
    pub(crate) preview_watcher: Option<notify::RecommendedWatcher>,
    pub(crate) preview_reload_tx:
        std::sync::mpsc::Sender<Result<notify::Event, notify::Error>>,
    pub(crate) preview_reload_rx:
        std::sync::mpsc::Receiver<Result<notify::Event, notify::Error>>,
    /// In-document search state for the active preview (`/`, `n`, `N`).
    /// `None` when no search is active. Cleared on Escape or when the
    /// active preview tab changes path.
    pub(crate) preview_search: Option<crate::preview_search::PreviewSearchState>,
    /// Persistent scroll handle bound to the preview's code
    /// `uniform_list`. Held on the view (not recreated per render)
    /// so `scroll_to_item` calls survive across frames until gpui's
    /// layout pass consumes the deferred scroll request.
    pub(crate) preview_scroll_handle: gpui::UniformListScrollHandle,
    /// Per-preview-path `ListState` for the markdown render path.
    /// Each markdown preview keeps its own scroll position so the
    /// user doesn't lose context when switching tabs. Entries are
    /// pruned when previews close (see `prune_preview_list_states`).
    /// Fresh entries are created lazily on the first render tick
    /// that sees a new markdown preview.
    pub(crate) preview_list_states: std::collections::HashMap<String, gpui::ListState>,
    /// Table-of-contents overlay state (`o` / `:`). None when the
    /// overlay is closed. Scoped to a single preview path; switching
    /// panes or pressing Escape drops it.
    pub(crate) preview_toc: Option<crate::preview_toc::TocPickerState>,
    /// One-shot latch: re-hydrate `preview_tabs` for any Preview tab
    /// that survived in the saved layout on startup. Without this
    /// pass, restored panes show the fallback "Preview: <path>"
    /// placeholder forever because `preview_tabs` starts empty but
    /// the pane tree already has `TabKind::Preview` entries. Mirrors
    /// the `browsers_restored` pattern.
    pub(crate) previews_restored: bool,
    /// Ephemeral text-selection state for the markdown preview
    /// (see `plans/preview-text-selection-spec.md`). `None` when no
    /// selection is active. Cleared on tab switch, auto-reload
    /// (via generation mismatch), or click outside the preview body.
    /// Not persisted across app restart — selection is a per-session
    /// UI affordance, not saved state.
    pub(crate) preview_selection:
        Option<crate::preview_selection::PreviewSelectionState>,
    /// Window-space bounds of the markdown preview body container,
    /// refreshed every render tick via an `on_prepaint` canvas inside
    /// `render_markdown_body`. Mouse handlers read this to convert
    /// window-space event positions into content-space selection
    /// coords. Stays `None` while no markdown preview is visible.
    pub(crate) preview_body_bounds: Option<gpui::Bounds<gpui::Pixels>>,
    /// Side-channel from `SelectableText::paint` → `Cmd/Ctrl+C`
    /// handler. Each paint tick populates one entry per text run
    /// that intersects the selection rect; the copy handler walks
    /// these to rebuild the clipboard string. Cleared at the top of
    /// every `Render::render` so stale ranges from a previous frame
    /// can't leak into the next copy.
    pub(crate) preview_selection_ranges: crate::preview_selection::SelectionRangeSink,
}

/// Per-row visual segment of a hovered file link: `(row, start_col,
/// end_col)` inclusive. A path that wraps across the terminal's
/// right edge produces multiple segments.
pub(crate) type HoverSegment = (usize, usize, usize);

/// Per-pane hover state for the Cmd/Ctrl+Click file-link preview.
/// `segments` covers the visual cells to underline — a single-row
/// match has one segment; a wrapped path (via WRAPLINE flag or OSC
/// 8 hyperlink id continuity) has one segment per row it spans.
#[cfg(feature = "gpui")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HoverLinkState {
    pub pane_id: amux_platform::terminal::manager::PaneId,
    pub segments: Vec<HoverSegment>,
}

// UI state structs (SearchState, pickers, toast, context menu,
// resize drag) live in `crate::state`. See that module's header
// comment for scope policy. Imported here for internal use only.
#[cfg(feature = "gpui")]
pub(crate) use crate::state::{
    AgentPickerState, ContextMenuState, NewTabPickerState, PanePickerState,
    ResizeDragState, ScrollbarDragState, ScrollbarHit, SearchState,
    SelectionAutoScrollState, TemplatePickerState, ToastNotification,
};

// Drag ghost views (`DragTab`, `DragWorkspace`) live in
// `crate::drag`. Re-exported here so
// `use crate::gpui_entry::DragTab` in gpui_layout_renderer.rs
// keeps compiling unchanged.
#[cfg(feature = "gpui")]
pub(crate) use crate::drag::DragTab;

// `ContextMenuItem` lives in `crate::menu` along with the menu
// builder and dispatch. Re-exported here so existing imports like
// `use crate::gpui_entry::ContextMenuItem` in
// `gpui_layout_renderer.rs` keep compiling unchanged.
#[cfg(feature = "gpui")]
pub(crate) use crate::menu::ContextMenuItem;

/// Captured terminal environment for spawning a new pane/tab.
#[cfg(feature = "gpui")]
pub(crate) struct CapturedEnv {
    pub shell: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    /// Optional command to send as input after the shell starts (e.g. "wsl --cd /path")
    pub initial_input: Option<String>,
}

#[cfg(feature = "gpui")]
impl GpuiShellView {
    pub(crate) fn browser_supported(&self) -> bool {
        self.model.browser_supported
    }

    pub(crate) fn wsl_supported(&self) -> bool {
        self.model.wsl_supported
    }

    fn activate_new_active_workspace(&mut self) {
        if let Some(new_ws) = self.model.workspace_items.iter().find(|w| w.is_active) {
            self.switch_workspace_terminal(&new_ws.id.clone());
        }
    }

    /// Acknowledge the startup crash-notice banner: open the crash log
    /// directory in the system file manager and clear the in-memory
    /// count so the status-bar badge disappears. The log files on disk
    /// are left alone — the user may still want to inspect them.
    pub(crate) fn reveal_crash_logs(&mut self) {
        let dir = crate::crash::crash_log_dir();
        crate::preview_open::open_url_external(&dir.to_string_lossy());
        self.crash_notice = None;
    }

    /// Open a folder picker so the user can pick a workspace folder.
    ///
    /// **Important**: this MUST be called from a context where `cx.spawn`
    /// is available, because the native folder dialog has to be deferred
    /// out of the current GPUI listener stack frame. Calling
    /// `rfd::FileDialog::pick_folder()` synchronously from inside a render
    /// listener pumps a nested NSApp run loop on macOS, which re-enters
    /// GPUI's RefCell and panics with `"RefCell already borrowed"`. This
    /// follows the same deferral pattern documented at the WebView2 init
    /// site (`open_browser` ~line 1483).
    ///
    /// On platforms where `folder_picker_supported` is false (e.g. Linux
    /// without xdg-desktop-portal), or if the user cancels the dialog,
    /// the function falls back to opening the command palette pre-filled
    /// with `workspace open `.
    pub(crate) fn prompt_open_local_workspace(&mut self, cx: &mut Context<Self>) {
        if !self.model.local_workspace_supported {
            return;
        }
        if self.model.folder_picker_supported {
            // Build the dialog future on the main thread but await it
            // from a spawned task. AsyncFileDialog routes the actual
            // NSOpenPanel onto the main run loop internally; we just
            // need to be off the listener's borrow stack when it fires.
            let dialog = rfd::AsyncFileDialog::new()
                .set_title("Select AMUX workspace folder")
                .pick_folder();
            cx.spawn(async move |this, cx| {
                let folder = dialog.await;
                let _ = this.update(cx, |this, cx| {
                    if let Some(handle) = folder {
                        let path = handle.path().to_path_buf();
                        this.app.open_local_workspace(path);
                        this.refresh_model();
                        this.activate_new_active_workspace();
                    }
                    // Dialog cancelled → no fallback, the user simply closed it.
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        // Picker capability not advertised — historically this
        // dropped into the command palette with a "workspace open "
        // query pre-filled, but the palette UI was never mounted in
        // the render tree so that fallback only turned the terminal
        // into an invisible keystroke trap. Log the capability gap
        // and return without side effects; the caller (right-click
        // menu or sidebar) already has its own no-op on miss.
        eprintln!(
            "[amux] prompt_open_local_workspace: folder picker not available on this platform"
        );
    }

    pub(crate) fn start_workspace_rename(
        &mut self,
        ws_id: String,
        current_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use gpui_component::input::{InputEvent, InputState};
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(current_name)
                .placeholder("workspace name")
        });

        // Input isn't mounted until the next paint, so focusing it
        // now targets a handle no element claims yet and GPUI drops
        // the keystrokes. `InputState::focus` also starts the caret
        // blink; without it the field looks uneditable.
        let input_for_focus = input.clone();
        window.on_next_frame(move |window, cx| {
            input_for_focus.update(cx, |state, cx| state.focus(window, cx));
        });

        let ws_id_for_sub = ws_id.clone();
        cx.subscribe(
            &input,
            move |this: &mut GpuiShellView, input_entity, event: &InputEvent, cx| {
                let commit = |this: &mut GpuiShellView,
                              new_name: String,
                              cx: &mut Context<GpuiShellView>| {
                    let trimmed = new_name.trim();
                    if !trimmed.is_empty() {
                        let _ = this.app.rename_workspace(&ws_id_for_sub, trimmed);
                        this.refresh_model();
                    }
                    this.renaming_workspace = None;
                    cx.notify();
                };
                match event {
                    InputEvent::PressEnter { .. } => {
                        let v = input_entity.read(cx).value().to_string();
                        commit(this, v, cx);
                    }
                    InputEvent::Blur => {
                        if let Some((ref id, ref state)) = this.renaming_workspace {
                            if id == &ws_id_for_sub {
                                let v = state.read(cx).value().to_string();
                                commit(this, v, cx);
                            }
                        }
                    }
                    _ => {}
                }
            },
        )
        .detach();

        self.renaming_workspace = Some((ws_id, input));
        cx.notify();
    }

    pub(crate) fn start_tab_rename(
        &mut self,
        pane_id: String,
        tab_idx: usize,
        current_title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use gpui_component::input::{InputEvent, InputState};
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(current_title)
                .placeholder("tab title")
        });
        // See `start_workspace_rename` for why focus is deferred.
        let input_for_focus = input.clone();
        window.on_next_frame(move |window, cx| {
            input_for_focus.update(cx, |state, cx| state.focus(window, cx));
        });

        let pane_id_for_sub = pane_id.clone();
        cx.subscribe(
            &input,
            move |this: &mut GpuiShellView, input_entity, event: &InputEvent, cx| {
                let commit = |this: &mut GpuiShellView,
                              new_name: String,
                              cx: &mut Context<GpuiShellView>| {
                    let trimmed = new_name.trim();
                    if !trimmed.is_empty() {
                        let pid =
                            amux_platform::terminal::manager::PaneId(pane_id_for_sub.clone());
                        if let Some(pane) = this.terminal_manager_mut().get_pane_mut(&pid) {
                            if let Some(tab) = pane.tabs.get_mut(tab_idx) {
                                tab.title = trimmed.to_string();
                                tab.custom_title = true;
                            }
                        }
                    }
                    this.renaming_tab = None;
                    cx.notify();
                };
                match event {
                    InputEvent::PressEnter { .. } => {
                        let v = input_entity.read(cx).value().to_string();
                        commit(this, v, cx);
                    }
                    InputEvent::Blur => {
                        if let Some((ref pid, _, ref state)) = this.renaming_tab {
                            if pid == &pane_id_for_sub {
                                let v = state.read(cx).value().to_string();
                                commit(this, v, cx);
                            }
                        }
                    }
                    _ => {}
                }
            },
        )
        .detach();

        self.renaming_tab = Some((pane_id, tab_idx, input));
        cx.notify();
    }

    /// Create a fresh workspace rooted at the user's home
    /// directory. Each click produces a new, distinct workspace —
    /// unlike `prompt_open_local_workspace`, this path skips the
    /// target-equality dedup so clicking "+ New" ten times gives
    /// the user ten separate organizational buckets they can
    /// rename into meaningful labels (e.g. "client work", "side
    /// project", "scratch"). The auto-assigned names are
    /// disambiguated ("arden", "arden 2", "arden 3", …) so the
    /// sidebar rows stay visually distinct until the user takes
    /// over naming.
    ///
    /// If `HOME` / `USERPROFILE` don't resolve to a real directory,
    /// silently no-ops — the picker path ("+ Open") is still
    /// available as a manual escape hatch.
    pub(crate) fn new_home_workspace(&mut self, cx: &mut Context<Self>) {
        let home = match std::env::var("HOME")
            .ok()
            .or_else(|| std::env::var("USERPROFILE").ok())
        {
            Some(raw) => std::path::PathBuf::from(raw),
            None => return,
        };
        if !home.is_dir() {
            return;
        }
        self.app.create_local_workspace(home);
        self.refresh_model();
        self.activate_new_active_workspace();
        cx.notify();
    }

    /// Returns the display name for the active workspace, falling back to the workspace ID.
    pub(crate) fn workspace_name(&self) -> String {
        self.model.active_workspace_name.clone()
            .unwrap_or_else(|| self.active_workspace_id.clone())
    }

    /// Look up the agent kind for a given pane, defaulting to the provided fallback.
    fn agent_kind_for_pane(&self, pane_id: &amux_platform::terminal::manager::PaneId, default: &str) -> String {
        self.terminal_manager().pane_list().iter()
            .find(|p| p.pane_id == *pane_id)
            .and_then(|p| p.agent_kind.clone())
            .unwrap_or_else(|| default.to_string())
    }

    /// Create a new shell view with terminal manager
    pub fn new(app: DesktopApp, model: GpuiWindowModel, config: crate::gpui_config::AmuxConfig, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // Get the active workspace ID
        let active_ws_id = model.workspace_items.iter()
            .find(|w| w.is_active)
            .map(|w| w.id.clone())
            .unwrap_or_else(|| "default".to_string());

        // Restore layout structures (fast — no PTY spawning yet)
        let mut workspace_terminals = std::collections::HashMap::new();
        let layouts = Self::load_all_layouts();
        for ws in &model.workspace_items {
            let mut tm = if let Some(json) = layouts.get(&ws.id) {
                TerminalManager::restore_layout(json)
                    .unwrap_or_else(|| TerminalManager::with_scrollback(config.scrollback))
            } else {
                TerminalManager::with_scrollback(config.scrollback)
            };
            tm.set_workspace_name(&ws.name);
            tm.heal_layout();
            workspace_terminals.insert(ws.id.clone(), tm);
        }
        if !workspace_terminals.contains_key(&active_ws_id) {
            let mut tm = TerminalManager::with_scrollback(config.scrollback);
            tm.set_workspace_name(&active_ws_id);
            workspace_terminals.insert(active_ws_id.clone(), tm);
        }
        // PTY processes will be spawned on the first render frame (deferred for fast startup)

        let ws_order: Vec<String> = model.workspace_items.iter().map(|w| w.id.clone()).collect();
        let (preview_reload_tx, preview_reload_rx) = std::sync::mpsc::channel();
        Self {
            app,
            model,
            sidebar_state: WorkspaceSidebarState::default(),
            workspace_terminals,
            active_workspace_id: active_ws_id,
            focus_handle,
            cell_metrics: None,
            selecting: false,
            context_menu: None,
            resize_drag: None,
            scrollbar_drag: None,
            selection_autoscroll: None,
            scrollbar_hover_pane: None,
            hover_link: None,
            cursor_blink_frame: 0,
            last_dirty_frame: 0,
            bell_flash_frame: None,
            renaming_workspace: None,
            renaming_tab: None,
            search_state: None,
            terminals_spawned: false,
            detected_vibe_tools: Vec::new(),
            tools_detected: false,
            zoomed_pane: None,
            workspace_order: ws_order,
            pane_bounds: std::collections::HashMap::new(),
            wsl_detected: false, // detected lazily in background
            terminal_theme: crate::gpui_terminal::TerminalTheme::by_name(&config.theme),
            config,
            toasts: Vec::new(),
            pane_picker: None,
            template_picker: None,
            agent_picker: None,
            new_tab_picker: None,
            ime_preedit: None,
            scroll_accumulator: 0.0,
            sidebar_drag_start: None,
            preview_tabs: std::collections::HashMap::new(),
            file_picker: None,
            browser_tabs: std::collections::HashMap::new(),
            browsers_restored: false,
            next_browser_id: 1,
            restore_terminal_focus: false,
            pending_url_bar_update: None,
            cached_window_handle: None,
            crash_notice: {
                let dir = crate::crash::crash_log_dir();
                let n = crate::crash::list_crashes(&dir).len();
                if n > 0 { Some(n) } else { None }
            },
            preview_watcher: None,
            preview_reload_tx,
            preview_reload_rx,
            preview_search: None,
            preview_scroll_handle: gpui::UniformListScrollHandle::new(),
            preview_list_states: std::collections::HashMap::new(),
            preview_toc: None,
            previews_restored: false,
            preview_selection: None,
            preview_body_bounds: None,
            preview_selection_ranges: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        }
    }


    /// Get cell dimensions (width, height). Falls back to defaults if not yet measured.
    pub(crate) fn cell_dims(&self) -> (f32, f32) {
        match &self.cell_metrics {
            Some(m) => (m.width, m.height),
            None => (8.0, 20.0), // safe fallback before first render
        }
    }

    /// Current sidebar width in pixels.
    pub(crate) fn sidebar_width(&self) -> f32 {
        if self.sidebar_state.collapsed {
            SIDEBAR_WIDTH_COLLAPSED
        } else {
            self.sidebar_state.width
        }
    }

    /// Check if the active terminal has mouse reporting enabled.
    /// Returns (mouse_mode, sgr_mode).
    /// Same as [`Self::active_term_mouse_mode`] but for an
    /// arbitrary pane id. Used by the scroll handler so wheel
    /// events can target the pane **under the cursor**, not
    /// just the keyboard-active one.
    pub(crate) fn term_mouse_mode_for_pane(
        &self,
        pid: &amux_platform::terminal::manager::PaneId,
    ) -> (bool, bool) {
        let mgr = self.terminal_manager();
        let pane = match mgr.get_pane(pid) {
            Some(p) => p,
            None => return (false, false),
        };
        let term = match pane.active_terminal_ref() {
            Some(t) => t,
            None => return (false, false),
        };
        term.with_term(|t| {
            let mode = t.mode();
            (
                mode.intersects(alacritty_terminal::term::TermMode::MOUSE_MODE),
                mode.contains(alacritty_terminal::term::TermMode::SGR_MOUSE),
            )
        })
    }

    /// Per-pane check for alternate-screen + alternate-scroll mode.
    /// When true, scroll wheel should send arrow keys to the
    /// application in that pane instead of scrolling the (empty)
    /// scrollback buffer.
    pub(crate) fn term_alt_screen_scroll_for_pane(
        &self,
        pid: &amux_platform::terminal::manager::PaneId,
    ) -> bool {
        let mgr = self.terminal_manager();
        let pane = match mgr.get_pane(pid) {
            Some(p) => p,
            None => return false,
        };
        let term = match pane.active_terminal_ref() {
            Some(t) => t,
            None => return false,
        };
        term.with_term(|t| {
            let mode = t.mode();
            mode.contains(alacritty_terminal::term::TermMode::ALT_SCREEN)
                && mode.contains(alacritty_terminal::term::TermMode::ALTERNATE_SCROLL)
        })
    }

    pub(crate) fn active_term_mouse_mode(&self) -> (bool, bool) {
        let mgr = self.terminal_manager();
        let pid = match mgr.active_pane_id() {
            Some(id) => id,
            None => return (false, false),
        };
        let pane = match mgr.get_pane(pid) {
            Some(p) => p,
            None => return (false, false),
        };
        let term = match pane.active_terminal_ref() {
            Some(t) => t,
            None => return (false, false),
        };
        term.with_term(|t| {
            let mode = t.mode();
            (
                mode.intersects(alacritty_terminal::term::TermMode::MOUSE_MODE),
                mode.contains(alacritty_terminal::term::TermMode::SGR_MOUSE),
            )
        })
    }

    /// Convert a window-space pixel position to terminal cell coordinates.
    /// Finds the pane under the cursor — does NOT assume the active pane.
    /// Returns (pane_id, col, row) so the caller knows which pane was clicked.
    pub(crate) fn pixel_to_term_cell_at(&self, pos: gpui::Point<gpui::Pixels>) -> Option<(amux_platform::terminal::manager::PaneId, usize, usize)> {
        let (cw, ch) = self.cell_dims();
        let cw = cw.max(1.0);
        let ch = ch.max(1.0);
        let pad = crate::gpui_terminal::TERMINAL_LEFT_PADDING;

        let x = pos.x.as_f32();
        let y = pos.y.as_f32();

        // Find which pane contains this pixel position.
        for (pid, &(px_x, px_y, pw, ph)) in &self.pane_bounds {
            if x >= px_x && x < px_x + pw && y >= px_y && y < px_y + ph {
                let col = ((x - px_x - pad).max(0.0) / cw) as usize;
                let row = ((y - px_y) / ch).max(0.0) as usize;
                return Some((amux_platform::terminal::manager::PaneId(pid.clone()), col, row));
            }
        }
        None
    }

    /// Convert a window-space pixel position to terminal cell coordinates
    /// for the currently active pane. (Fallback for single-pane layouts.)
    pub(crate) fn pixel_to_term_cell(&self, pos: gpui::Point<gpui::Pixels>) -> (usize, usize) {
        let (cw, ch) = self.cell_dims();
        let cw = cw.max(1.0);
        let ch = ch.max(1.0);
        let pad = crate::gpui_terminal::TERMINAL_LEFT_PADDING;

        // Look up active pane's screen bounds
        if let Some(pid) = self.terminal_manager().active_pane_id() {
            if let Some(&(px_x, px_y, _pw, _ph)) = self.pane_bounds.get(&pid.0) {
                let x = (pos.x.as_f32() - px_x - pad).max(0.0);
                let y = (pos.y.as_f32() - px_y).max(0.0);
                return ((x / cw) as usize, (y / ch) as usize);
            }
        }

        // Fallback: assume single pane after sidebar + tab strip
        let sidebar_w = self.sidebar_width();
        let tab_strip_h = 28.0_f32;
        let x = (pos.x.as_f32() - sidebar_w - pad).max(0.0);
        let y = (pos.y.as_f32() - tab_strip_h).max(0.0);
        ((x / cw) as usize, (y / ch) as usize)
    }

    /// Hit-test the scrollbar of every visible pane against a window-space
    /// pixel position. Returns `(pane_id, hit, snapshot)` where `hit` says
    /// whether the click landed on the thumb itself or on the empty track
    /// above/below it, and `snapshot` carries the geometry needed to drive
    /// a subsequent drag.
    ///
    /// Geometry mirrors `gpui_terminal.rs` Phase 4 so visual ↔ hit area
    /// stay in lockstep. The clickable x-range is widened to ~12px (real
    /// thumb is 4px) so the bar is actually grabbable with a mouse.
    pub(crate) fn scrollbar_hit_test(
        &self,
        pos: gpui::Point<gpui::Pixels>,
    ) -> Option<(amux_platform::terminal::manager::PaneId, ScrollbarHit, ScrollbarDragState)> {
        use amux_platform::terminal::manager::PaneId;
        let (cw, ch) = self.cell_dims();
        let cw = cw.max(1.0);
        let ch = ch.max(1.0);
        let pad = crate::gpui_terminal::TERMINAL_LEFT_PADDING;
        // Hit area uses the *hover* width plus a couple px of slop so
        // the cursor catches the bar before the bar visually expands.
        // The renderer right-aligns the bar to `cols*cw`, so we test
        // against that right edge regardless of current visual width.
        let bar_w = crate::gpui_terminal::SCROLLBAR_WIDTH_HOVER;
        let hit_pad = 2.0_f32;
        let mx = pos.x.as_f32();
        let my = pos.y.as_f32();

        for (pid_str, &(px_x, px_y, pw, ph)) in &self.pane_bounds {
            // Only fire for the pane currently under the cursor.
            if !(mx >= px_x && mx < px_x + pw && my >= px_y && my < px_y + ph) {
                continue;
            }
            let pid = PaneId(pid_str.clone());
            let pane = self.terminal_manager().get_pane(&pid)?;
            let term = pane.active_terminal_ref()?;
            let (offset, history, visible) = term.with_term(|t| {
                use alacritty_terminal::grid::Dimensions;
                (
                    t.grid().display_offset(),
                    t.grid().history_size(),
                    t.grid().screen_lines(),
                )
            });
            if history == 0 || offset == 0 {
                return None; // bar not drawn
            }
            let cols = ((pw - pad).max(0.0) / cw) as usize;
            let track_h = visible as f32 * ch;
            let track_x = px_x + pad + cols as f32 * cw - bar_w;
            let track_y = px_y;
            let total = history + visible;
            let thumb_ratio = (visible as f32 / total as f32).clamp(0.05, 1.0);
            let thumb_h = (track_h * thumb_ratio).max(8.0);
            let scroll_frac = (offset as f32 / history as f32).clamp(0.0, 1.0);
            let thumb_y = track_y + (track_h - thumb_h) * (1.0 - scroll_frac);

            // x hit area widened symmetrically around the bar.
            let x_min = track_x - hit_pad;
            let x_max = track_x + bar_w + hit_pad;
            if !(mx >= x_min && mx <= x_max && my >= track_y && my <= track_y + track_h) {
                return None;
            }
            let hit = if my >= thumb_y && my <= thumb_y + thumb_h {
                ScrollbarHit::Thumb
            } else if my < thumb_y {
                ScrollbarHit::TrackAbove
            } else {
                ScrollbarHit::TrackBelow
            };
            let snapshot = ScrollbarDragState {
                pane_id: pid.clone(),
                start_mouse_y: my,
                start_offset: offset,
                history,
                track_h,
                thumb_h,
            };
            return Some((pid, hit, snapshot));
        }
        None
    }

    /// Send a mouse event to the active terminal PTY.
    /// `button`: 0=left, 1=middle, 2=right, 64=scroll_up, 65=scroll_down
    /// `pressed`: true for press (M), false for release (m)
    pub(crate) fn send_mouse_event(&mut self, button: u8, col: usize, row: usize, pressed: bool) {
        let col = col.min(223);
        let row = row.min(223);
        let cx_1 = col + 1;
        let cy_1 = row + 1;
        let (_, sgr_mode) = self.active_term_mouse_mode();
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            if sgr_mode {
                let suffix = if pressed { 'M' } else { 'm' };
                let seq = format!("\x1b[<{};{};{}{}", button, cx_1, cy_1, suffix);
                term.send_input(seq.as_bytes());
            } else {
                // Legacy encoding — only supports press, release uses button 3
                let b = if pressed { button + 32 } else { 35 }; // 35 = release in legacy
                let x = (col.min(222) as u8) + 33;
                let y = (row.min(222) as u8) + 33;
                let seq = [b'\x1b', b'[', b'M', b, x, y];
                term.send_input(&seq);
            }
        }
    }

    /// Get the terminal manager for the active workspace (immutable)
    /// Remove browser_tabs and preview_tabs entries for the active pane's tabs.
    /// Must be called BEFORE close_active_pane() so the pane data is still available.
    /// Also clears the tab-interception pointer so the NSEvent monitor doesn't
    /// reference freed memory after the terminal is dropped.
    pub(crate) fn cleanup_pane_tab_entries(&mut self) {
        // Clear the NSEvent monitor's terminal pointer before any terminal
        // is dropped. The next frame tick will restore it if a new terminal
        // is spawned in the same pane or if the user switches panes.
        crate::tab_intercept::set_active_terminal(0);
        use amux_platform::terminal::manager::TabKind;
        let manager = self.workspace_terminals.get(&self.active_workspace_id);
        if let Some(mgr) = manager {
            if let Some(pane_id) = mgr.active_pane_id().cloned() {
                if let Some(pane) = mgr.get_pane(&pane_id) {
                    let mut browser_ids = Vec::new();
                    let mut preview_paths = Vec::new();
                    for tab in &pane.tabs {
                        match &tab.kind {
                            TabKind::Browser { browser_id, .. } => {
                                browser_ids.push(*browser_id);
                            }
                            TabKind::Preview { path } => {
                                preview_paths.push(path.clone());
                            }
                            _ => {}
                        }
                    }
                    for bid in browser_ids {
                        self.browser_tabs.remove(&bid);
                    }
                    for path in preview_paths {
                        self.preview_tabs.remove(&path);
                        self.preview_unwatch_path(&path);
                    }
                }
            }
        }
    }

    pub(crate) fn terminal_manager(&self) -> &TerminalManager {
        self.workspace_terminals.get(&self.active_workspace_id)
            .expect("active workspace must have a terminal manager")
    }

    /// Get the terminal manager for the active workspace (mutable).
    /// Auto-creates if missing (defensive against stale workspace IDs).
    pub(crate) fn terminal_manager_mut(&mut self) -> &mut TerminalManager {
        if !self.workspace_terminals.contains_key(&self.active_workspace_id) {
            self.ensure_workspace_terminal(&self.active_workspace_id.clone());
        }
        self.workspace_terminals.get_mut(&self.active_workspace_id)
            .expect("just ensured workspace exists")
    }

    /// Resolve a workspace's spawn cwd. Prefers the workspace's
    /// own `target_path` (what the user actually opened), falling
    /// back to `default_cwd` if the path is missing or no longer
    /// exists on disk. Without this lookup, all workspace terminals
    /// inherit amux's own launch directory — which is `/` when amux
    /// is started from a macOS .app bundle, and many shell prompts
    /// (p10k, spaceship, starship) flag `PWD=/` with a lock icon.
    pub(crate) fn workspace_spawn_cwd(&self, workspace_id: &str) -> Option<String> {
        let target = self
            .model
            .workspace_items
            .iter()
            .find(|w| w.id == workspace_id)
            .and_then(|w| w.target_path.clone())?;
        if std::path::Path::new(&target).is_dir() {
            Some(target)
        } else {
            Self::default_cwd()
        }
    }

    /// "Where should a new terminal spawn right now?" — the
    /// canonical fallback for every spawn/split/restart path. The
    /// rule: prefer the ACTIVE workspace's target path, fall back
    /// to the GUI process's launch directory only if the workspace
    /// has no `target_path` at all. Never just use `default_cwd`
    /// alone — that leaks amux's own launch directory (e.g.
    /// `/Users/.../amux` from `cargo run`, or `/` from a macOS
    /// .app bundle) into user workspaces.
    ///
    /// Callers that already have a pane-specific live cwd (e.g.
    /// split/new-tab inheriting the parent pane's cwd) should
    /// apply this as `live_cwd.or_else(|| self.spawn_cwd())`.
    pub(crate) fn spawn_cwd(&self) -> Option<String> {
        self.workspace_spawn_cwd(&self.active_workspace_id)
            .or_else(Self::default_cwd)
    }

    /// Ensure a workspace has a terminal manager, creating one if needed.
    /// Always heals layout/pane inconsistencies and fills every pane's
    /// active tab with a live PTY — the single invariant callers can rely
    /// on is "after this returns, every pane in the layout has a spawned
    /// terminal, or has an explicit `Spawn failed` error tab".
    fn ensure_workspace_terminal(&mut self, workspace_id: &str) {
        // Resolve spawn cwd BEFORE the `&mut self.workspace_terminals`
        // borrow below — otherwise we'd hold a mutable borrow while
        // still trying to read `self.model` for the target path.
        let spawn_cwd = self
            .workspace_spawn_cwd(workspace_id)
            .or_else(Self::default_cwd);

        if !self.workspace_terminals.contains_key(workspace_id) {
            let mut tm = TerminalManager::with_scrollback(self.config.scrollback);
            let ws_name = self.model.workspace_items.iter()
                .find(|w| w.id == workspace_id)
                .map(|w| w.name.clone())
                .unwrap_or_else(|| workspace_id.to_string());
            tm.set_workspace_name(&ws_name);
            self.workspace_terminals.insert(workspace_id.to_string(), tm);
        }

        // Single heal + spawn path for both freshly-created and restored
        // managers. Previously the "new workspace" branch only called
        // `spawn_in_active`, which left the pane map and layout in sync
        // but skipped the heal step — so any drift in the pane map
        // (observed in practice, root cause unconfirmed) made the
        // subsequent render fall through to the "Empty pane" placeholder.
        // Running heal_layout unconditionally closes that window.
        if let Some(tm) = self.workspace_terminals.get_mut(workspace_id) {
            tm.heal_layout();
            let (shell, args) = Self::default_shell();
            let pane_ids: Vec<_> = tm.active_layout()
                .map(|l| l.pane_ids()).unwrap_or_default();
            for pid in pane_ids {
                tm.spawn_all_tabs_in_pane(&pid, &shell, &args, spawn_cwd.as_deref());
            }
        }
    }

    /// Switch the active workspace terminal.
    /// Auto-runs startup commands if the workspace is empty and has a startup file.
    pub(crate) fn switch_workspace_terminal(&mut self, workspace_id: &str) {
        self.ensure_workspace_terminal(workspace_id);
        self.active_workspace_id = workspace_id.to_string();

        // Auto-run startup if workspace is empty and has a startup file
        if self.is_workspace_empty() {
            let ws_name = self.model.workspace_items.iter()
                .find(|w| w.id == workspace_id)
                .map(|w| w.name.clone())
                .unwrap_or_else(|| workspace_id.to_string());
            let path = Self::startup_file_path(&ws_name);
            if path.exists() {
                self.run_startup_commands();
            }
        }
    }

    /// Get the default shell program and args for the current platform
    pub(crate) fn default_shell() -> (String, Vec<String>) {
        if cfg!(target_os = "windows") {
            let shell = if Self::silent_command("pwsh.exe").arg("--version").output().is_ok() {
                "pwsh.exe"
            } else {
                "powershell.exe"
            };
            // -NoExit keeps shell open after running the init command
            // PSStyle fix removes background colors from directory listings
            (shell.to_string(), vec![
                "-NoLogo".to_string(),
                "-NoExit".to_string(),
                "-Command".to_string(),
                "$PSStyle.FileInfo.Directory = \"`e[34;1m\"; $PSStyle.FileInfo.Executable = \"`e[32;1m\"".to_string(),
            ])
        } else {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            (shell, vec!["-l".to_string()])
        }
    }

    pub(crate) fn default_cwd() -> Option<String> {
        std::env::current_dir().ok().map(|p| p.to_string_lossy().to_string())
    }



    /// Capture the active tab's shell + cwd before any operation that changes active pane.
    ///
    /// On Windows: detects if the user is inside a WSL session by checking the terminal
    /// title (bash sets it to "user@host:path"). If detected, spawns the same default shell
    /// but sends "wsl --cd <path>" as input, so `exit` from WSL returns to the shell normally.
    pub(crate) fn capture_active_env(&self) -> CapturedEnv {
        let title = self.terminal_manager().active_terminal_title();

        // Check terminal title for WSL session (user@host:path pattern)
        if cfg!(target_os = "windows") {
            if let Some(ref t) = title {
                if let Some(wsl_cmd) = Self::detect_wsl_from_title_str(t) {
                    let (shell, args) = Self::default_shell();
                    let wsl_cwd = Self::extract_wsl_path_from_title(t);
                    return CapturedEnv { shell, args, cwd: wsl_cwd, initial_input: Some(wsl_cmd) };
                }
            }
            // Fallback: detect WSL from prompt line (user@host:/path$)
            if let Some(prompt_line) = self.terminal_manager().active_terminal_ref()
                .map(|t| t.cursor_line_text())
            {
                if let Some(linux_path) = crate::preview_open::extract_cwd_from_prompt_line(&prompt_line) {
                    if linux_path.starts_with('/') {
                        let (shell, args) = Self::default_shell();
                        let wsl_cmd = format!("wsl --cd {}", linux_path);
                        return CapturedEnv { shell, args, cwd: Some(linux_path), initial_input: Some(wsl_cmd) };
                    }
                }
            }
        }

        let inherited = self.terminal_manager().active_shell_cmd()
            .map(|(s, a)| (s.to_string(), a.to_vec()));

        // Best-effort CWD: use the same resolve chain as file picker
        // Prompt extraction is the most reliable source on Windows —
        // PowerShell prompt always shows the real current directory.
        // sysinfo often returns the spawn-time CWD, not the live one after `cd`.
        let prompt_cwd = self.terminal_manager().active_terminal_ref()
            .map(|t| t.cursor_line_text())
            .and_then(|line| crate::preview_open::extract_cwd_from_prompt_line(&line))
            .map(|p| crate::preview_open::maybe_convert_wsl_path(self, &p));
        let process_cwd = self.terminal_manager().active_process_cwd();
        let saved_cwd = self.terminal_manager().active_saved_cwd();

        let live_cwd = prompt_cwd.filter(|p| std::path::Path::new(p).is_dir())
            .or_else(|| process_cwd.filter(|p| std::path::Path::new(p).is_dir()))
            .or_else(|| saved_cwd.filter(|p| std::path::Path::new(p).is_dir()));

        let (shell, args) = inherited.unwrap_or_else(Self::default_shell);
        // If the pane has no detectable live cwd (PTY not up yet,
        // /proc unavailable, prompt not parseable), fall back through
        // the active workspace's own path before hitting the GUI
        // launch dir. See `spawn_cwd` doc for why.
        let cwd = live_cwd.or_else(|| self.spawn_cwd());
        CapturedEnv { shell, args, cwd, initial_input: None }
    }

    /// Parse a "user@host:path" or "user@host/path" terminal title.
    /// Returns the path portion if present, or empty string if the format matches but has no path.
    /// Returns None if the title doesn't match the WSL pattern at all.
    fn parse_wsl_title_path(title: &str) -> Option<&str> {
        let title = title.trim();
        let at_pos = title.find('@')?;
        if at_pos == 0 { return None; }
        let after_at = &title[at_pos + 1..];
        let path = if let Some(colon_pos) = after_at.find(':') {
            after_at[colon_pos + 1..].trim_start()
        } else if let Some(slash_pos) = after_at.find('/') {
            &after_at[slash_pos..]
        } else {
            return None;
        };
        Some(path.trim())
    }

    /// Detect WSL session from terminal title and return the wsl command to send.
    /// Returns Some("wsl --cd /path") or Some("wsl") if WSL detected.
    fn detect_wsl_from_title_str(title: &str) -> Option<String> {
        let path = Self::parse_wsl_title_path(title)?;
        if path.is_empty() {
            Some("wsl".to_string())
        } else if path.starts_with('/') {
            Some(format!("wsl --cd {}", path))
        } else {
            Some("wsl".to_string())
        }
    }

    /// Extract the WSL path from a "user@host:path" terminal title.
    fn extract_wsl_path_from_title(title: &str) -> Option<String> {
        let path = Self::parse_wsl_title_path(title)?;
        if path.starts_with('/') { Some(path.to_string()) } else { None }
    }

    /// Spawn a terminal in the active pane's active tab, inheriting env from the current tab.
    pub(crate) fn spawn_terminal_in_active(&mut self) {
        let env = self.capture_active_env();
        self.spawn_with_captured_env(&env);
    }

    /// Spawn a terminal with pre-captured environment (use after split/new-tab).
    pub(crate) fn spawn_with_captured_env(&mut self, env: &CapturedEnv) {
        // When initial_input is set (WSL scenario), the CWD is a Linux path that
        // Windows ConPTY cannot use as working_directory. Pass None to PTY and let
        // the initial_input command (e.g. "wsl --cd /path") handle directory setup.
        let pty_cwd = if env.initial_input.is_some() {
            None
        } else {
            env.cwd.as_deref()
        };
        if let Err(e) = self.terminal_manager_mut().spawn_in_active(&env.shell, &env.args, pty_cwd) {
            eprintln!("[amux] spawn_in_active failed: {} | shell={:?} args={:?} cwd={:?}", e, env.shell, env.args, pty_cwd);
        }
        // Send initial command if present (e.g. "wsl --cd /path")
        if let Some(ref cmd) = env.initial_input {
            if let Some(term) = self.terminal_manager_mut().active_terminal() {
                let input = format!("{}\r", cmd);
                term.send_input(input.as_bytes());
            }
        }
        // Record the logical CWD on the tab for future inheritance and persistence,
        // even if it wasn't passed to the PTY (e.g. WSL Linux paths on Windows).
        // Always overwrite so that live CWD from the parent pane is captured.
        if let Some(ref cwd) = env.cwd {
            if let Some(pane) = self.terminal_manager_mut().active_pane_mut() {
                if let Some(tab) = pane.tabs.get_mut(pane.active_tab) {
                    tab.cwd = Some(cwd.clone());
                }
            }
        }
    }

    /// Restart the terminal in a specific pane (used when process exits)
    pub(crate) fn restart_terminal_in_pane(&mut self, pane_id: &amux_platform::terminal::manager::PaneId) {
        self.terminal_manager_mut().set_active_pane(pane_id);
        // Restart with the same shell + cwd the tab was using
        let inherited = self.terminal_manager().active_shell_cmd()
            .map(|(s, a)| (s.to_string(), a.to_vec()));
        let saved_cwd = self.terminal_manager().active_cwd();

        let (shell, args) = inherited.unwrap_or_else(Self::default_shell);
        // Fall back through workspace cwd if the tab has no saved
        // cwd (shouldn't normally happen — tab records cwd at spawn
        // — but be robust).
        let cwd = saved_cwd.or_else(|| self.spawn_cwd());
        let _ = self.terminal_manager_mut().restart_active_terminal(&shell, &args, cwd.as_deref());
    }


    /// Reorder a workspace by moving it from one index to another.
    pub(crate) fn reorder_workspace(&mut self, from_index: usize, to_id: &str) {
        let items = &mut self.model.workspace_items;
        let to_index = items.iter().position(|w| w.id == to_id);
        if let Some(to_index) = to_index {
            if from_index != to_index && from_index < items.len() {
                let item = items.remove(from_index);
                // After remove, adjust target: if we removed before target, target shifted left
                let adjusted = if from_index < to_index { to_index.saturating_sub(1) } else { to_index };
                let insert_at = adjusted.min(items.len());
                items.insert(insert_at, item);
                self.workspace_order = items.iter().map(|w| w.id.clone()).collect();
            }
        }
    }

    /// Refresh model from backend, then re-apply custom workspace order.
    pub(crate) fn refresh_model(&mut self) {
        self.model = self.app.render_with(&amux_ui::GpuiRenderer);
        self.apply_workspace_order();
    }

    /// Sort workspace_items to match the stored workspace_order.
    /// New workspaces (not in order list) go to the end.
    fn apply_workspace_order(&mut self) {
        let order = &self.workspace_order;
        self.model.workspace_items.sort_by(|a, b| {
            let ia = order.iter().position(|id| id == &a.id).unwrap_or(usize::MAX);
            let ib = order.iter().position(|id| id == &b.id).unwrap_or(usize::MAX);
            ia.cmp(&ib)
        });
        // Add any new workspaces to the order list
        for w in &self.model.workspace_items {
            if !self.workspace_order.contains(&w.id) {
                self.workspace_order.push(w.id.clone());
            }
        }
    }

    /// Rebuild the match list from the current query + mode.
    /// Thin wrapper around `crate::search::rebuild` — the glue
    /// here owns the `take()`/`with_term_mut` borrow dance so the
    /// logic in `search.rs` can be a plain `fn(&mut SearchState,
    /// &mut Term)`.
    pub(crate) fn search_rebuild(&mut self) {
        let Some(mut state) = self.search_state.take() else { return };
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            term.with_term_mut(|t| {
                crate::search::rebuild(&mut state, t);
                crate::search::apply_current(&state, t);
            });
        }
        self.search_state = Some(state);
    }

    /// Jump to the next/previous search match, wrapping at either
    /// end. Thin wrapper — the cycling is trivial and the scroll/
    /// highlight work lives in `crate::search::apply_current`.
    pub(crate) fn search_navigate(&mut self, forward: bool) {
        let Some(mut state) = self.search_state.take() else { return };
        if state.matches.is_empty() {
            self.search_state = Some(state);
            return;
        }
        let len = state.matches.len();
        state.current = if forward {
            (state.current + 1) % len
        } else {
            (state.current + len - 1) % len
        };
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            term.with_term_mut(|t| crate::search::apply_current(&state, t));
        }
        self.search_state = Some(state);
    }

    /// Toggle zoom on the active pane — fills the entire content area.
    /// Press again to restore the original split layout.
    pub(crate) fn toggle_zoom(&mut self) {
        if self.zoomed_pane.is_some() {
            self.zoomed_pane = None;
        } else if let Some(pid) = self.terminal_manager().active_pane_id().cloned() {
            self.zoomed_pane = Some(pid);
        }
    }


    // Terminal context menu lives in `crate::menu` now — the
    // builder and action dispatch are free functions taking
    // `&GpuiShellView` / `&mut GpuiShellView`. Call sites go
    // through `crate::menu::build_items(self)` and
    // `crate::menu::dispatch(self, label, window, cx)`.


    // ─── Browser Pane ────────────────────────────────────────────

    /// Open a browser tab in the active pane (limux-style).
    /// WebView2 creation is deferred via cx.spawn to avoid RefCell re-borrow.
    /// Scan every loaded workspace's pane tree for `TabKind::Browser`
    /// entries and install a `BrowserTabEntry` for each, re-creating
    /// the URL bar input and deferring WebView2 init the same way
    /// `open_browser` does. Also bumps `next_browser_id` past any
    /// saved id so new browsers opened post-restore never collide
    /// with a restored one.
    ///
    /// Called once on startup as soon as `cached_window_handle` is
    /// available, gated by the `browsers_restored` latch. Idempotent
    /// by the latch — never runs twice.
    ///
    /// LIMITATION: `TabKind::Browser.url` holds the URL the tab was
    /// originally opened with, not the last-navigated URL. On
    /// restore we open that original URL. WebView2's own on-disk
    /// cache means history and cookies are preserved, but the
    /// visible page resets. Updating `TabKind` on every navigation
    /// would let us restore the exact last page — tracked as a
    /// follow-up.
    pub(crate) fn restore_browser_tabs_from_layouts(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use amux_platform::terminal::manager::TabKind;

        // Collect (browser_id, url) pairs without holding a borrow
        // on self — install_browser_tab_entry takes `&mut self`.
        let mut pairs: Vec<(u64, String)> = Vec::new();
        for tm in self.workspace_terminals.values() {
            for pane in tm.all_panes() {
                for tab in &pane.tabs {
                    if let TabKind::Browser { url, browser_id } = &tab.kind {
                        pairs.push((*browser_id, url.clone()));
                    }
                }
            }
        }

        if pairs.is_empty() { return; }

        // Push next_browser_id past the highest saved id so new
        // browsers opened later don't collide with a restored entry.
        let max_id = pairs.iter().map(|(id, _)| *id).max().unwrap_or(0);
        if self.next_browser_id <= max_id {
            self.next_browser_id = max_id + 1;
        }

        for (bid, url) in pairs {
            // Skip if somehow already installed (defensive — the
            // latch should prevent this, but be safe).
            if self.browser_tabs.contains_key(&bid) { continue; }
            self.install_browser_tab_entry(bid, &url, window, cx);
        }
    }

    pub(crate) fn open_browser(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        use crate::gpui_browser::default_welcome_url;

        // Empty URL → load the embedded welcome page instead of
        // http://localhost:3000. The old default sat on a 30-second TCP
        // connect timeout when no dev server was running, which made the
        // browser feel "slow" on first open. The welcome page renders
        // instantly and tells the user how to drive the URL bar / F12.
        let url_owned = if url.is_empty() {
            default_welcome_url()
        } else {
            url.to_string()
        };
        let url = url_owned.as_str();

        // Bail early if WebView2 can't be created yet — matches
        // previous behavior and avoids orphaning a pane tab that
        // can't be realized.
        if self.cached_window_handle.is_none() {
            eprintln!("[amux-browser] no cached window handle yet");
            return;
        }

        // Assign a unique browser_id.
        let browser_id = self.next_browser_id;
        self.next_browser_id += 1;

        // Add browser tab to the active pane.
        let active_pid = self.terminal_manager().active_pane_id().cloned();
        if let Some(ref pid) = active_pid {
            if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                pane.add_browser_tab(url, browser_id);
            }
        }

        self.install_browser_tab_entry(browser_id, url, window, cx);
        cx.notify();
    }

    /// Install the per-browser UI state (URL bar `InputState`,
    /// `BrowserTabEntry`, deferred WebView2 init) for a `browser_id`
    /// that is **already present** in some pane's tab list. Used by
    /// both `open_browser` (after it creates the pane tab) and the
    /// startup restore path (where the pane tab was loaded from
    /// disk but `browser_tabs` is still empty).
    ///
    /// Requires `cached_window_handle` to be set — callers must
    /// check and bail otherwise.
    pub(crate) fn install_browser_tab_entry(
        &mut self,
        browser_id: u64,
        url: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use gpui_component::input::{InputState, InputEvent};
        use crate::gpui_browser::{BrowserPaneState, BrowserTabEntry};

        let raw_handle = match self.cached_window_handle {
            Some(h) => h,
            None => {
                eprintln!("[amux-browser] install_browser_tab_entry: no window handle");
                return;
            }
        };

        // Create URL bar Input entity seeded with the current URL.
        let url_owned = url.to_string();
        let url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(url_owned)
                .placeholder("Enter URL and press Enter...")
        });

        // Subscribe: Enter navigates.
        let bid = browser_id;
        cx.subscribe(&url_input, move |this: &mut GpuiShellView, input_entity, event: &InputEvent, cx| {
            match event {
                InputEvent::PressEnter { .. } => {
                    let url = input_entity.read(cx).value().to_string();
                    if url.is_empty() { return; }
                    let url = if !url.contains("://") {
                        if url.starts_with("localhost") || url.contains(':') {
                            format!("http://{}", url)
                        } else {
                            format!("https://{}", url)
                        }
                    } else { url };
                    if let Some(entry) = this.browser_tabs.get_mut(&bid) {
                        entry.browser.navigate(&url);
                    }
                    this.restore_terminal_focus = true;
                    cx.notify();
                }
                InputEvent::Blur => { cx.notify(); }
                _ => {}
            }
        }).detach();

        let bounds_cell = std::rc::Rc::new(std::cell::Cell::new(None));

        self.browser_tabs.insert(browser_id, BrowserTabEntry {
            browser: BrowserPaneState::new(url),
            url_input,
            bounds_cell: bounds_cell.clone(),
        });

        // Defer WebView2 creation — matches the original open path.
        cx.spawn(async move |this, cx| {
            smol::Timer::after(std::time::Duration::from_millis(50)).await;
            let _ = this.update(cx, |this: &mut GpuiShellView, cx| {
                if let Some(entry) = this.browser_tabs.get_mut(&bid) {
                    if !entry.browser.is_initialized() {
                        entry.browser.init_webview(raw_handle);
                        if let Some(bounds) = entry.bounds_cell.get() {
                            entry.browser.sync_bounds(bounds);
                        }
                        entry.browser.focus_parent();
                    }
                }
                this.restore_terminal_focus = true;
                cx.notify();
            });
        }).detach();
    }

    /// Close the browser tab that is active in the current pane.
    pub(crate) fn close_browser(&mut self) {
        // Find the active pane's active tab — if it's a browser, close it
        let active_pid = self.terminal_manager().active_pane_id().cloned();
        if let Some(ref pid) = active_pid {
            let browser_id = self.terminal_manager().get_pane(pid)
                .and_then(|p| p.active_tab_kind())
                .and_then(|k| match k {
                    amux_platform::terminal::manager::TabKind::Browser { browser_id, .. } => Some(*browser_id),
                    _ => None,
                });
            if let Some(bid) = browser_id {
                self.browser_tabs.remove(&bid);
                // Close the tab in the pane
                if let Some(pane) = self.terminal_manager_mut().get_pane_mut(pid) {
                    let idx = pane.active_tab;
                    pane.close_tab(idx);
                }
            }
        }
        self.restore_terminal_focus = true;
    }

    /// Get the active browser tab entry (if the active pane's active tab is a browser).
    pub(crate) fn active_browser_entry(&self) -> Option<(u64, &crate::gpui_browser::BrowserTabEntry)> {
        let pid = self.terminal_manager().active_pane_id()?;
        let pane = self.terminal_manager().get_pane(pid)?;
        match pane.active_tab_kind()? {
            amux_platform::terminal::manager::TabKind::Browser { browser_id, .. } => {
                self.browser_tabs.get(browser_id).map(|e| (*browser_id, e))
            }
            _ => None,
        }
    }

    /// Check if any browser tab exists and is visible (active in its pane).
    pub(crate) fn has_visible_browser(&self) -> bool {
        self.active_browser_entry().is_some()
    }


    /// Check if the current terminal input line is an `amux` command.
    /// Returns Some(true) if intercepted, Some(false) if not an amux command, None if can't read.
    fn try_intercept_amux_command(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<bool> {
        // Read the cursor line — this is always the line the user just typed on,
        // regardless of screen position or leftover content below.
        let last_line = self.terminal_manager().active_terminal_ref()
            .map(|t| t.cursor_line_text())?;

        // Extract the command after the prompt. Look for common prompt patterns:
        // "PS C:\path> amux preview file.md"
        // "user@host:~$ amux preview file.md"
        // "> amux preview file.md"
        let cmd = extract_command_after_prompt(&last_line);
        let cmd = cmd.trim();

        if !cmd.starts_with("amux ") && cmd != "amux" {
            return Some(false);
        }

        // Extract CWD from the prompt portion of the SAME line we just read.
        // This is the most reliable source — the prompt always shows the path,
        // and we read it before any state changes (Ctrl+C, etc.).
        let prompt_cwd = crate::preview_open::extract_cwd_from_prompt_line(&last_line);

        let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
        match parts.get(1).map(|s| *s) {
            Some("preview") | Some("view") | Some("open") => {
                if let Some(path) = parts.get(2) {
                    let path = path.trim();
                    if !path.is_empty() {
                        crate::preview_open::open_preview_file_with_cwd(self, cx, path, prompt_cwd.as_deref());
                        return Some(true);
                    }
                }
                // No file specified — open file picker
                crate::preview_open::open_file_picker_with_cwd(self, cx, prompt_cwd);
                Some(true)
            }
            Some("browser") | Some("web") => {
                // Empty url falls through to the welcome page (avoids the
                // localhost:3000 30-second TCP timeout when no dev server
                // is running).
                let url = parts.get(2).map(|s| s.trim()).unwrap_or("");
                self.open_browser(url, window, cx);
                Some(true)
            }
            Some("pane") => {
                let pane_rest = parts.get(2).map(|s| s.trim()).unwrap_or("");
                self.handle_pane_command(pane_rest);
                Some(true)
            }
            _ => Some(false),
        }
    }

    /// Handle `amux pane <subcommand>` commands.
    fn handle_pane_command(&mut self, rest: &str) {
        let sub_parts: Vec<&str> = rest.splitn(2, ' ').collect();
        let sub = sub_parts.first().map(|s| s.trim()).unwrap_or("");
        let sub_args = sub_parts.get(1).map(|s| s.trim()).unwrap_or("");

        match sub {
            "list" => {
                let pane_list = self.terminal_manager().pane_list();
                let json = serde_json::to_string_pretty(&pane_list).unwrap_or_default();
                self.echo_to_terminal(&json);
            }
            "read" => {
                // Parse: <pane-id> [--lines N]
                let args: Vec<&str> = sub_args.split_whitespace().collect();
                if let Some(pane_id_str) = args.first() {
                    let lines = args.iter().position(|a| *a == "--lines")
                        .and_then(|i| args.get(i + 1))
                        .and_then(|n| n.parse::<usize>().ok())
                        .unwrap_or(50);
                    let pane_id = amux_platform::terminal::manager::PaneId(pane_id_str.to_string());
                    match self.terminal_manager().pane_read(&pane_id, lines) {
                        Some(content) => {
                            let output = content.join("\n");
                            self.echo_to_terminal(&output);
                        }
                        None => {
                            self.echo_to_terminal(&format!("error: pane '{}' not found or has no terminal", pane_id_str));
                        }
                    }
                } else {
                    self.echo_to_terminal("usage: amux pane read <pane-id> [--lines N]");
                }
            }
            "message" => {
                // Parse: <pane-id> "<text>"
                let args: Vec<&str> = sub_args.splitn(2, ' ').collect();
                if args.len() >= 2 {
                    let target_id_str = args[0];
                    let text = args[1].trim_matches('"');
                    let target = amux_platform::terminal::manager::PaneId(target_id_str.to_string());

                    // Build bridge message from current pane identity
                    let ws_name = self.workspace_name();
                    let source_pane_id = self.terminal_manager().active_pane_id()
                        .cloned().unwrap_or_else(|| amux_platform::terminal::manager::PaneId("unknown".to_string()));
                    let agent_kind = self.agent_kind_for_pane(&source_pane_id, "user");

                    let msg = amux_core::bridge::BridgeMessage {
                        workspace: ws_name,
                        pane_id: source_pane_id.0,
                        agent: agent_kind,
                        text: text.to_string(),
                    };
                    let formatted = msg.format();
                    match self.terminal_manager_mut().pane_send_text(&target, &formatted) {
                        Ok(()) => self.echo_to_terminal(&format!("sent to {}", target_id_str)),
                        Err(e) => self.echo_to_terminal(&format!("error: {}", e)),
                    }
                } else {
                    self.echo_to_terminal("usage: amux pane message <pane-id> \"<text>\"");
                }
            }
            "id" => {
                let ws_name = self.workspace_name();
                let pane_id = self.terminal_manager().active_pane_id()
                    .cloned().unwrap_or_else(|| amux_platform::terminal::manager::PaneId("unknown".to_string()));
                let agent_kind = self.agent_kind_for_pane(&pane_id, "none");
                let output = format!("pane_id: {}\nworkspace: {}\nagent: {}", pane_id.0, ws_name, agent_kind);
                self.echo_to_terminal(&output);
            }
            "teach" => {
                let template = Self::agent_teach_template();
                self.echo_to_terminal(&template);
            }
            _ => {
                self.echo_to_terminal(&format!("unknown pane command: '{}'\navailable: list, read, message, id, teach", sub));
            }
        }
    }

    /// Write text to the active terminal as visible output.
    /// Sends Ctrl+U to clear the typed command, then uses `echo` to display the text.
    fn echo_to_terminal(&mut self, text: &str) {
        if let Some(terminal) = self.terminal_manager_mut().active_terminal() {
            // Clear the "amux pane ..." command the user typed
            terminal.send_input(b"\x15"); // Ctrl+U: kill line
            // Write output via a temp file to avoid shell escaping issues
            let tmp = std::env::temp_dir().join("amux_bridge_output.txt");
            if std::fs::write(&tmp, text).is_ok() {
                let cmd = format!("cat {}\n", tmp.display());
                terminal.send_input(cmd.as_bytes());
            }
        }
    }

    /// Return the agent teach template text.
    fn agent_teach_template() -> String {
        r#"## Amux Inter-Agent Communication

You are running inside Amux terminal multiplexer. You can collaborate with other agents using `amux` commands:

- **Discover**: `amux pane list` — JSON list of all panes with agent state
- **Observe**: `amux pane read <pane-id> --lines 20` — read another pane's output
- **Communicate**: `amux pane message <pane-id> "message text"` — send message to another agent
- **Identity**: `amux pane id` — show your pane identity

Messages you receive will be in this format:
`[amux-bridge workspace:<w> pane:<id> agent:<kind>] <text>`

When you see input in this format, it is a message from another agent. Read the sender info and message text, then respond appropriately.

Environment variables available: $AMUX_PANE_ID, $AMUX_WORKSPACE, $AMUX_VERSION"#.to_string()
    }

    /// Generate ~/.amux/agent-prompt.md if it does not already exist.
    pub(crate) fn ensure_agent_prompt_file() {
        let prompt_path = Self::amux_dir().join("agent-prompt.md");
        if !prompt_path.exists() {
            let _ = std::fs::create_dir_all(Self::amux_dir());
            let _ = std::fs::write(&prompt_path, Self::agent_teach_template());
        }
    }

    /// Handle key input for the terminal
    pub fn handle_terminal_input(&mut self, key: &str, ctrl: bool, shift: bool, alt: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Reset cursor blink on any terminal input
        self.cursor_blink_frame = 0;
        use amux_platform::terminal::keys;
        
        // GPUI sends lowercase keys but to_pty expects title case
        let normalized_key = match key {
            "enter" => "Enter",
            "tab" => "Tab",
            "escape" => "Escape",
            "backspace" => "Backspace",
            "up" | "arrowup" => "ArrowUp",
            "down" | "arrowdown" => "ArrowDown",
            "left" | "arrowleft" => "ArrowLeft",
            "right" | "arrowright" => "ArrowRight",
            "home" => "Home",
            "end" => "End",
            "pageup" => "PageUp",
            "pagedown" => "PageDown",
            "insert" => "Insert",
            "delete" => "Delete",
            "f1" => "F1",
            "f2" => "F2",
            "f3" => "F3",
            "f4" => "F4",
            "f5" => "F5",
            "f6" => "F6",
            "f7" => "F7",
            "f8" => "F8",
            "f9" => "F9",
            "f10" => "F10",
            "f11" => "F11",
            "f12" => "F12",
            "space" => "Space",
            _ => key,
        };
        
        // Intercept `amux` commands on Enter before sending to PTY
        if normalized_key == "Enter" && !ctrl && !alt {
            if let Some(handled) = self.try_intercept_amux_command(window, cx) {
                if handled {
                    // Send Enter to PTY so the shell gets a blank line (command was "eaten")
                    // Then send Ctrl+C to cancel the partially typed command
                    if let Some(terminal) = self.terminal_manager_mut().active_terminal() {
                        terminal.send_input(b"\x03"); // Ctrl+C to clear the line
                    }
                    return;
                }
            }
        }

        // Check app cursor key mode from active terminal
        let app_cursor = self.terminal_manager().active_terminal_ref()
            .map(|t| t.with_term(|term| term.mode().contains(alacritty_terminal::term::TermMode::APP_CURSOR)))
            .unwrap_or(false);
        let input = keys::to_pty_with_mode(normalized_key, ctrl, shift, alt, app_cursor);

        // Scroll to bottom on input so user always sees what they type
        if let Some(terminal) = self.terminal_manager_mut().active_terminal() {
            terminal.scroll_to_bottom();
            terminal.send_input(&input);
        }
        
        // Don't request re-render here - the 60fps polling loop will trigger re-render when PTY output arrives
    }
}

/// Extract the command portion from a terminal line, stripping the shell prompt.
/// Handles: "PS C:\path> cmd", "user@host:~$ cmd", "% cmd", "> cmd"
///
/// Last-resort fallback searches anywhere in the line for `amux`. That
/// covers custom prompts whose delimiter glyph we can't enumerate —
/// Powerline, Starship, p10k, the Brc20BatchMint-style prompt that
/// ends with a Nerd Font icon + space. Without this fallback,
/// `amux preview <file>` on a Powerline zsh silently falls through to
/// the shell, which reports `command not found: amux`.
#[cfg(feature = "gpui")]
fn extract_command_after_prompt(line: &str) -> &str {
    // PowerShell: "PS C:\foo> command"
    if let Some(pos) = line.find("> ") {
        let after = &line[pos + 2..];
        // Make sure it's actually a prompt (has PS prefix or short prefix)
        if line[..pos].contains("PS ") || line[..pos].contains("❯") || pos < 80 {
            return after;
        }
    }
    // Bash/Zsh: "user@host:~/dir$ command"
    if let Some(pos) = line.find("$ ") {
        return &line[pos + 2..];
    }
    // Zsh: "% command"
    if let Some(pos) = line.find("% ") {
        if pos < 5 {
            return &line[pos + 2..];
        }
    }
    // Fallback: find `amux` anywhere in the line. Using `rfind` so a
    // prompt that happens to contain the literal word "amux" (very
    // unlikely, but e.g. a cwd path like `/Users/me/amux-dev/`)
    // doesn't shadow the user-typed command at the end of the line.
    // Match `amux ` (with trailing space, typical when followed by
    // subcommand/args) first; fall back to bare "amux" at end of
    // the trimmed line so `amux` with no args is still intercepted.
    if let Some(pos) = line.rfind("amux ") {
        return &line[pos..];
    }
    let trimmed = line.trim_end();
    if trimmed.ends_with("amux") {
        let pos = trimmed.len() - 4;
        // Guard: must be a word boundary — "samux", "foamux", etc.
        // should not be matched. Prev char (if any) should be a
        // non-alphanumeric (space, slash, separator).
        let prev_ok = pos == 0
            || !trimmed[..pos]
                .chars()
                .last()
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);
        if prev_ok {
            return &line[pos..];
        }
    }
    line
}


/// Spawn the selection edge auto-scroll loop. The loop ticks every
/// 40ms while `selection_autoscroll` is `Some`, scrolls the
/// scrollback by a progressive line count (1 line near the edge, up
/// to ~8 lines further out), and extends the active selection to
/// the row that just rolled into view at the top/bottom edge. The
/// loop exits as soon as the cursor returns to the viewport, the
/// mouse is released, or the selection is canceled — all of which
/// clear `selection_autoscroll` from the regular event handlers.
#[cfg(feature = "gpui")]
pub(crate) fn spawn_selection_autoscroll_loop(cx: &mut gpui::Context<GpuiShellView>) {
    cx.spawn(async move |this, cx| {
        loop {
            smol::Timer::after(std::time::Duration::from_millis(40)).await;
            let cont = this
                .update(cx, |this, cx| {
                    if !this.selecting {
                        this.selection_autoscroll = None;
                        return false;
                    }
                    let Some(state) = this.selection_autoscroll.clone() else {
                        return false;
                    };
                    tick_selection_autoscroll(this, state);
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !cont {
                break;
            }
        }
    })
    .detach();
}

/// One step of the selection auto-scroll: scroll the active pane's
/// scrollback by a progressive number of lines and slide the
/// selection endpoint to the freshly revealed top/bottom row.
#[cfg(feature = "gpui")]
fn tick_selection_autoscroll(this: &mut GpuiShellView, state: SelectionAutoScrollState) {
    use alacritty_terminal::grid::{Dimensions, Scroll};
    use alacritty_terminal::index::{Column, Line, Point as AlacPoint, Side as AlacSide};

    let dist = state.edge_pixels.abs();
    // Progressive: ~1 line right at the edge, ramping up to ~8 lines
    // when the cursor is far outside. Matches macOS Terminal feel.
    let lines = ((1.0 + dist / 25.0).min(8.0)) as usize;
    let lines = lines.max(1);
    let scrolling_up = state.edge_pixels > 0.0;

    let (cw, _ch) = this.cell_dims();
    let cw = cw.max(1.0);
    let pad = crate::gpui_terminal::TERMINAL_LEFT_PADDING;
    let Some(&(px_x, _, _, _)) = this.pane_bounds.get(&state.pane_id.0) else {
        return;
    };
    let col_hint = ((state.last_mouse_x - px_x - pad).max(0.0) / cw) as usize;

    let pane_id = state.pane_id.clone();
    let Some(pane) = this.terminal_manager_mut().get_pane_mut(&pane_id) else {
        return;
    };
    let Some(term) = pane.active_terminal() else {
        return;
    };
    term.with_term_mut(|t| {
        let delta = if scrolling_up {
            lines as i32
        } else {
            -(lines as i32)
        };
        t.scroll_display(Scroll::Delta(delta));

        let display_offset = t.grid().display_offset() as i32;
        let screen_lines = t.grid().screen_lines();
        let viewport_row = if scrolling_up {
            0i32
        } else {
            screen_lines as i32 - 1
        };
        let grid_line = viewport_row - display_offset;
        let cols_total = t.grid().columns();
        let col_clamped = col_hint.min(cols_total.saturating_sub(1));
        let point = AlacPoint::new(Line(grid_line), Column(col_clamped));
        if let Some(ref mut sel) = t.selection {
            sel.update(point, AlacSide::Right);
        }
    });
}



// NOTE: render_context_menu, first_pane_in_layout, render_layout
// have been moved to gpui_layout_renderer.rs
// NOTE: pub fn run, the macOS dock icon pipeline, and the 60 fps
// PTY/cursor/browser polling timer have been moved to
// `crate::app_bootstrap`.
// NOTE: `impl Render for GpuiShellView` — the ~1675-line per-frame
// tree — has been moved to `gpui_entry_render.rs`. This file now
// owns `GpuiShellView`, its fields, its constructor, and its
// helper-method impl block; the render tree lives next door.

#[cfg(all(test, feature = "gpui"))]
mod extract_command_tests {
    use super::extract_command_after_prompt;

    #[test]
    fn plain_bash_prompt() {
        assert_eq!(
            extract_command_after_prompt("user@host:~/dir$ amux preview README.md"),
            "amux preview README.md"
        );
    }

    #[test]
    fn zsh_percent_prompt() {
        assert_eq!(
            extract_command_after_prompt("% amux preview README.md"),
            "amux preview README.md"
        );
    }

    #[test]
    fn powershell_prompt() {
        assert_eq!(
            extract_command_after_prompt("PS C:\\Users\\me> amux preview README.md"),
            "amux preview README.md"
        );
    }

    #[test]
    fn powerline_prompt_no_standard_delimiter() {
        // Regression: Brc20BatchMint-style zsh prompt ending in a
        // Nerd Font glyph (U+E0A0) + space fell through every
        // prompt-based detector and returned the full line, which
        // then failed the `cmd.starts_with("amux ")` gate. Result:
        // `amux preview file.md` reached the shell and zsh reported
        // `command not found: amux`.
        let line = "Brc20BatchMint \u{e0a0} main amux preview README.md";
        assert_eq!(
            extract_command_after_prompt(line),
            "amux preview README.md"
        );
    }

    #[test]
    fn starship_style_arrow_prompt() {
        // Starship/p10k default: path on one line, `❯ ` on next.
        // When user types on the ❯ line, the cursor_line_text may be
        // `❯ amux preview file.md`. `> ` detector catches it.
        let line = "❯ amux preview README.md";
        assert_eq!(
            extract_command_after_prompt(line),
            "amux preview README.md"
        );
    }

    #[test]
    fn bare_amux_no_args() {
        // User typed just `amux` and hit Enter — should still be
        // detected as amux command (opens file picker).
        let line = "Brc20BatchMint \u{e0a0} main amux";
        assert_eq!(extract_command_after_prompt(line), "amux");
    }

    #[test]
    fn word_boundary_guards_false_match() {
        // If the prompt itself contains a token ending in "amux"
        // (e.g. a cwd like `~/foamux`), the bare-amux fallback must
        // not treat the last 4 chars as a command. Guarded by the
        // word-boundary check.
        assert_eq!(
            extract_command_after_prompt("~/foamux"),
            "~/foamux"
        );
    }

    #[test]
    fn cwd_containing_amux_does_not_shadow_user_command() {
        // Prompt cwd has `amux` in it, user types `amux preview`.
        // `rfind` gives us the last occurrence — the user command —
        // not the cwd substring.
        let line = "~/data/repository/ai/arden/amux \u{e0a0} main amux preview README.md";
        assert_eq!(
            extract_command_after_prompt(line),
            "amux preview README.md"
        );
    }

    #[test]
    fn non_amux_line_falls_through_unchanged() {
        // Lines that don't contain amux at all return the original
        // line — the caller's `cmd.starts_with("amux ")` gate then
        // rejects them.
        let line = "Brc20BatchMint \u{e0a0} main ls -la";
        assert_eq!(extract_command_after_prompt(line), line);
    }
}

