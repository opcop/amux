#[cfg(feature = "gpui")]
use amux_ui::{DesktopApp, GpuiWindowModel};
#[cfg(feature = "gpui")]
use gpui::{
    rgb, AppContext, Context, FontWeight, IntoElement, Render, Window,
    px, div, prelude::*,
};
#[cfg(feature = "gpui")]
use amux_platform::terminal::manager::TerminalManager;
#[cfg(feature = "gpui")]
use crate::gpui_status_bar::{render_status_bar, StatusBarData, AgentSummary};
#[cfg(feature = "gpui")]
use crate::gpui_workspace_sidebar::{WorkspaceSidebarState, SidebarMode, AgentSidebarItem};
#[cfg(feature = "gpui")]
use crate::gpui_layout_renderer::{render_context_menu, render_layout, render_pane_picker, render_template_picker, render_agent_picker, render_new_tab_picker};


#[cfg(feature = "gpui")]
const SIDEBAR_WIDTH_COLLAPSED: f32 = 28.0;
const SIDEBAR_WIDTH_MIN: f32 = 120.0;
const SIDEBAR_WIDTH_MAX: f32 = 480.0;

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
    scroll_accumulator: f32,
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
    AgentPickerState, ContextMenuState, NewTabPickerItem, NewTabPickerState, PanePickerState,
    ResizeDragState, ScrollbarDragState, ScrollbarHit, SearchMode, SearchState,
    SelectionAutoScrollState, TemplatePickerState, ToastNotification,
};

// Drag ghost views (`DragTab`, `DragWorkspace`) live in
// `crate::drag`. Re-exported here so
// `use crate::gpui_entry::DragTab` in gpui_layout_renderer.rs
// keeps compiling unchanged.
#[cfg(feature = "gpui")]
pub(crate) use crate::drag::{DragTab, DragWorkspace};

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
    fn browser_supported(&self) -> bool {
        self.model.browser_supported
    }

    fn wsl_supported(&self) -> bool {
        self.model.wsl_supported
    }

    fn activate_new_active_workspace(&mut self) {
        if let Some(new_ws) = self.model.workspace_items.iter().find(|w| w.is_active) {
            self.switch_workspace_terminal(&new_ws.id.clone());
        }
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
    fn workspace_name(&self) -> String {
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
        }
    }


    /// Get cell dimensions (width, height). Falls back to defaults if not yet measured.
    fn cell_dims(&self) -> (f32, f32) {
        match &self.cell_metrics {
            Some(m) => (m.width, m.height),
            None => (8.0, 20.0), // safe fallback before first render
        }
    }

    /// Current sidebar width in pixels.
    fn sidebar_width(&self) -> f32 {
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
    fn term_mouse_mode_for_pane(
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
    fn term_alt_screen_scroll_for_pane(
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

    fn active_term_mouse_mode(&self) -> (bool, bool) {
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
    fn pixel_to_term_cell_at(&self, pos: gpui::Point<gpui::Pixels>) -> Option<(amux_platform::terminal::manager::PaneId, usize, usize)> {
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
    fn pixel_to_term_cell(&self, pos: gpui::Point<gpui::Pixels>) -> (usize, usize) {
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
    fn scrollbar_hit_test(
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
    fn send_mouse_event(&mut self, button: u8, col: usize, row: usize, pressed: bool) {
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
    pub(crate) fn cleanup_pane_tab_entries(&mut self) {
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

    /// Ensure a workspace has a terminal manager, creating one if needed.
    /// Also heals layout/pane inconsistencies for existing managers.
    fn ensure_workspace_terminal(&mut self, workspace_id: &str) {
        // Resolve spawn cwd BEFORE the `&mut self.workspace_terminals`
        // borrow below — otherwise we'd hold a mutable borrow while
        // still trying to read `self.model` for the target path.
        let spawn_cwd = self
            .workspace_spawn_cwd(workspace_id)
            .or_else(Self::default_cwd);

        if !self.workspace_terminals.contains_key(workspace_id) {
            let mut tm = TerminalManager::with_scrollback(self.config.scrollback);
            let ws_name = self.model.active_workspace_name
                .clone().unwrap_or_else(|| workspace_id.to_string());
            tm.set_workspace_name(&ws_name);
            let (shell, args) = Self::default_shell();
            let _ = tm.spawn_in_active(&shell, &args, spawn_cwd.as_deref());
            self.workspace_terminals.insert(workspace_id.to_string(), tm);
        } else if let Some(tm) = self.workspace_terminals.get_mut(workspace_id) {
            // Heal layout, then spawn all tabs (not just active) for restored workspaces
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
    fn switch_workspace_terminal(&mut self, workspace_id: &str) {
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
        let cwd = live_cwd.or_else(Self::default_cwd);
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

    /// Apply a layout template to the current workspace.
    /// Replaces all panes and spawns terminals in each.
    pub(crate) fn apply_template(&mut self, template: &amux_platform::terminal::manager::LayoutTemplate) {
        let mut tm = TerminalManager::from_template(template);
        tm.set_scrollback(self.config.scrollback);
        let ws_name = self.workspace_name();
        tm.set_workspace_name(&ws_name);
        self.workspace_terminals.insert(self.active_workspace_id.clone(), tm);
        // Spawn terminals in all panes
        let (shell, args) = Self::default_shell();
        let cwd = Self::default_cwd();
        let pane_ids: Vec<_> = self.terminal_manager().active_layout()
            .map(|l| l.pane_ids()).unwrap_or_default();
        for pid in pane_ids {
            self.terminal_manager_mut().spawn_all_tabs_in_pane(&pid, &shell, &args, cwd.as_deref());
        }
        self.save_all_layouts();
    }

    /// Save current layout as a custom template with auto-generated name.
    /// Currently unreachable: was only called from the command palette
    /// dispatcher, which has never been wired (see palette_dispatch).
    /// Kept so the future palette revival has a working target.
    #[allow(dead_code)]
    pub(crate) fn save_current_as_template(&mut self, name: &str) {
        let desc = format!("{} panes", self.terminal_manager().total_panes());
        let template = self.terminal_manager().to_template(name, &desc);
        Self::save_template(&template);
    }

    /// Open the template picker overlay.
    pub(crate) fn open_template_picker(&mut self) {
        let templates = Self::all_templates();
        if templates.is_empty() { return; }
        self.template_picker = Some(TemplatePickerState {
            templates,
            selected_index: 0,
        });
    }

    /// Execute the template picker selection.
    pub(crate) fn execute_template_picker(&mut self) {
        if let Some(picker) = self.template_picker.take() {
            if let Some(t) = picker.templates.get(picker.selected_index) {
                self.apply_template(t);
            }
        }
    }

    /// Delete the selected custom template from the picker.
    pub(crate) fn delete_selected_template(&mut self) {
        if let Some(ref mut picker) = self.template_picker {
            if let Some(t) = picker.templates.get(picker.selected_index) {
                if t.builtin { return; } // can't delete built-in
                let name = t.name.clone();
                Self::delete_template(&name);
                picker.templates.remove(picker.selected_index);
                if picker.templates.is_empty() {
                    self.template_picker = None;
                } else if picker.selected_index >= picker.templates.len() {
                    picker.selected_index = picker.templates.len() - 1;
                }
            }
        }
    }

    /// Open the agent launcher picker. Unreachable today: the
    /// `+▾` new-tab dropdown (see `NewTabPickerState`) covers the
    /// same "pick an agent to launch" flow, and the command
    /// palette that used to reach this function has never been
    /// mounted. Retained so either entry point can revive it.
    #[allow(dead_code)]
    pub(crate) fn open_agent_picker(&mut self) {
        let mut agents: Vec<(String, String, bool)> = Vec::new();
        if self.wsl_supported() && self.wsl_detected {
            agents.push(("wsl".into(), "WSL Terminal".into(), true));
        }
        for &(tool_id, label, _env) in &self.detected_vibe_tools {
            agents.push((tool_id.into(), label.into(), false));
        }
        if agents.is_empty() { return; }
        // If only one option, launch directly
        if agents.len() == 1 {
            let (tool_id, _, is_wsl) = &agents[0];
            if *is_wsl {
                self.launch_wsl_shell();
            } else {
                self.launch_vibe_tool_env(tool_id, false);
            }
            return;
        }
        self.agent_picker = Some(AgentPickerState {
            agents,
            selected_index: 0,
        });
    }

    /// Execute the agent picker selection.
    pub(crate) fn execute_agent_picker(&mut self) {
        if let Some(picker) = self.agent_picker.take() {
            if let Some((tool_id, _, is_wsl)) = picker.agents.get(picker.selected_index) {
                if *is_wsl {
                    self.launch_wsl_shell();
                } else {
                    self.launch_vibe_tool_env(tool_id, false);
                }
            }
        }
    }

    /// Open the new-tab dropdown picker for a specific pane.
    pub(crate) fn open_new_tab_picker(
        &mut self,
        pane_id: amux_platform::terminal::manager::PaneId,
        anchor: gpui::Point<gpui::Pixels>,
    ) {
        let mut items = vec![
            NewTabPickerItem { id: "terminal".into(), label: "Terminal".into(), icon: ">_", separator_after: false },
        ];

        // WSL terminal
        if self.wsl_supported() && self.wsl_detected {
            items.push(NewTabPickerItem {
                id: "wsl".into(), label: "WSL Terminal".into(), icon: "🐧", separator_after: false,
            });
        }

        // Add separator after terminal group if there are agents
        if !self.detected_vibe_tools.is_empty() {
            items.last_mut().unwrap().separator_after = true;
        }

        // Detected vibe coding tools
        for &(tool_id, label, _env) in &self.detected_vibe_tools {
            items.push(NewTabPickerItem {
                id: tool_id.into(), label: label.into(), icon: "●", separator_after: false,
            });
        }

        // Separator before utility items
        items.last_mut().unwrap().separator_after = true;

        // Preview & Browser
        items.push(NewTabPickerItem {
            id: "preview".into(), label: "Preview File...".into(), icon: "◈", separator_after: false,
        });
        if self.browser_supported() {
            items.push(NewTabPickerItem {
                id: "browser".into(), label: "Browser".into(), icon: "◉", separator_after: false,
            });
        }

        self.new_tab_picker = Some(NewTabPickerState {
            pane_id,
            items,
            selected_index: 0,
            anchor,
        });
    }

    /// Execute the selected item from the new-tab picker.
    pub(crate) fn execute_new_tab_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.new_tab_picker.take() else { return };
        let Some(item) = picker.items.get(picker.selected_index) else { return };
        self.terminal_manager_mut().set_active_pane(&picker.pane_id);

        match item.id.as_str() {
            "terminal" => {
                let env = self.capture_active_env();
                self.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                self.spawn_with_captured_env(&env);
            }
            "wsl" => {
                self.launch_wsl_shell();
            }
            "preview" => {
                crate::preview_open::open_file_picker(self);
            }
            "browser" => {
                self.open_browser("", window, cx);
            }
            tool_id => {
                // Vibe coding tool
                self.launch_vibe_tool_env(tool_id, false);
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
        let cwd = saved_cwd.or_else(Self::default_cwd);
        let _ = self.terminal_manager_mut().restart_active_terminal(&shell, &args, cwd.as_deref());
    }


    /// Reorder a workspace by moving it from one index to another.
    fn reorder_workspace(&mut self, from_index: usize, to_id: &str) {
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
                        crate::preview_open::open_preview_file_with_cwd(self, path, prompt_cwd.as_deref());
                        return Some(true);
                    }
                }
                // No file specified — open file picker
                crate::preview_open::open_file_picker_with_cwd(self, prompt_cwd);
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
    // Fallback: if line starts with "amux ", treat entire line as command
    if line.trim_start().starts_with("amux ") {
        return line.trim_start();
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
fn spawn_selection_autoscroll_loop(cx: &mut gpui::Context<GpuiShellView>) {
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

#[cfg(feature = "gpui")]
impl Render for GpuiShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Record frame time into the debug-stats HUD ring buffer.
        // Cheap when the HUD is disabled (one Instant + one atomic
        // push); the formatted snapshot is only materialized below if
        // AMUX_DEBUG_STATS=1.
        let _frame_guard = crate::metrics::FrameGuard::start();

        // Input latency: if a keystroke arrived since the last
        // frame, compute its latency so the HUD can display it.
        crate::metrics::consume_input_latency();

        // First-frame startup instrumentation: record the phase
        // and (if AMUX_BENCH_STARTUP=1) dump the full phase
        // report to stderr. Both are behind a `Once` so calling
        // them every frame is a no-op after the first.
        {
            use std::sync::Once;
            static FIRST_RENDER: Once = Once::new();
            FIRST_RENDER.call_once(|| {
                crate::metrics::startup_phase("first_render");
                crate::metrics::dump_startup_report();
            });
        }

        // Focus management.
        // When the browser is open, trust GPUI's own focus system:
        // - Input's track_focus + prevent_default handles URL bar focus correctly
        // - Root's track_focus handles terminal focus on clicks outside Input
        // Only use explicit flags for specific transitions (Enter navigate, close browser).
        if self.restore_terminal_focus {
            self.restore_terminal_focus = false;
            self.focus_handle.focus(window, cx);
            // Reclaim OS focus from any active browser WebView2
            if let Some((_, entry)) = self.active_browser_entry() {
                entry.browser.focus_parent();
            }
        } else if self.has_visible_browser() {
            // Browser is open AND visible — do NOT aggressively grab focus.
            // WebView2 is a child HWND that takes OS focus on click, which may
            // cause GPUI to clear its internal focus state. If we force-focus root
            // here every frame, we'd fight WebView2 and break the URL Input.
            // Focus is managed entirely by click events:
            //   - Click terminal  → root's track_focus + focus_parent()
            //   - Click URL Input → Input's track_focus (with prevent_default)
            //   - Click WebView2  → WebView2 gets OS focus, GPUI does nothing
        } else if self.renaming_workspace.is_some() || self.renaming_tab.is_some() {
            // Rename active: leave focus on the Input. Re-grabbing
            // the root handle here races the focus `on_next_frame`
            // the rename helper just scheduled.
        } else {
            // No browser, no rename — safe to ensure terminal
            // always has focus.
            if !self.focus_handle.is_focused(window) {
                self.focus_handle.focus(window, cx);
            }
        }

        // Sync URL bar when navigation changed the page address.
        // Only update when the Input is NOT focused (don't overwrite user's editing).
        if let Some(url) = self.pending_url_bar_update.take() {
            let child_input_focused = self.active_browser_entry()
                .map(|(_, e)| {
                    use gpui::Focusable;
                    e.url_input.read(cx).focus_handle(cx).is_focused(window)
                })
                .unwrap_or(false);
            if child_input_focused {
                self.pending_url_bar_update = Some(url);
            } else if let Some((_, entry)) = self.active_browser_entry() {
                let input = entry.url_input.clone();
                input.update(cx, |state, cx| {
                    state.set_value(url, window, cx);
                });
            }
        }

        // Cache native window handle on first render (needed for WebView2 creation later)
        if self.cached_window_handle.is_none() {
            use raw_window_handle::HasWindowHandle;
            if let Ok(handle) = window.window_handle() {
                self.cached_window_handle = Some(handle.as_raw());
            }
        }

        // Restore browser tabs persisted in the workspace layout.
        // Gated on the window handle being cached (required for
        // WebView2 init) AND a one-shot latch so we never run twice.
        // Without this, panes whose active tab is a `TabKind::Browser`
        // render as the "Browser loading..." fallback because
        // `browser_tabs` starts empty post-restart.
        if !self.browsers_restored && self.cached_window_handle.is_some() {
            self.browsers_restored = true;
            self.restore_browser_tabs_from_layouts(window, cx);
        }

        // Browser bounds sync is done in the 60fps timer, not here in render,
        // to avoid timing issues with canvas prepaint.

        let sidebar_visible = !self.sidebar_state.collapsed;
        let workspaces = self.model.workspace_items.clone();
        let workspace_groups = self.model.workspace_groups.clone();

        // Measure font metrics on first render
        let metrics = self.cell_metrics.get_or_insert_with(|| {
            crate::gpui_terminal::measure_cell_metrics(window, &self.config.font_family, self.config.font_size, self.config.line_height)
        }).clone();
        let cell_w = metrics.width.max(1.0);  // guard against zero
        let cell_h = metrics.height.max(1.0);

        // Resize terminals — skip during drag to avoid content loss
        if self.resize_drag.is_none() && self.sidebar_drag_start.is_none() {
            let sidebar_w = self.sidebar_width();
            let vp = window.viewport_size();
            let content_w = vp.width.as_f32() - sidebar_w;
            let status_bar_h = 34.0_f32;
            // macOS transparent titlebar uses pt(28px) on the root div,
            // which eats into the viewport but isn't accounted for by
            // status_bar_h alone. Without subtracting it, the terminal
            // computes 1-2 extra rows that get clipped at the bottom.
            let titlebar_h = if cfg!(target_os = "macos") { 28.0_f32 } else { 0.0 };
            let content_h = vp.height.as_f32() - status_bar_h - titlebar_h;            if let Some(zpid) = self.zoomed_pane.clone() {
                // Zoom mode: give the zoomed pane the full content area
                self.terminal_manager_mut().resize_pane_terminals(
                    &zpid, content_w, content_h, cell_w, cell_h,
                );
            } else {
                self.terminal_manager_mut().resize_all_panes(
                    content_w, content_h, cell_w, cell_h,
                );
            }
        }


        
        // IME input handler canvas. GPUI positions its built-in IME
        // composition box (the "方框" with preedit text) at the canvas
        // bounds and uses those bounds as the anchor for the macOS
        // candidate/suggestion window. We track the terminal cursor
        // position each frame so the composition box appears inline
        // at the cursor — previously the canvas was a hidden 1×1px
        // element at (-10, -10), which put the IME UI offscreen.
        let view_entity = cx.entity().clone();
        let focus_for_ime = self.focus_handle.clone();

        // The IME canvas that registers handle_input is kept offscreen
        // (0×0 at -100,-100) so GPUI's built-in composition box (the
        // "方框") is invisible. We render our own preedit overlay
        // further down in the tree, positioned at the terminal cursor.
        // The macOS candidate window is positioned via bounds_for_range,
        // which returns the cursor's screen position independently of
        // the canvas bounds.
        let (ime_x, ime_y, ime_w, ime_h) = (-100.0_f32, -100.0_f32, 0.0_f32, 0.0_f32);

        // Main layout - limux/mori style dark theme
        div()
            .track_focus(&self.focus_handle)
            .child(gpui::canvas(
                move |bounds, _window, _cx| bounds,
                move |bounds, _, window, cx| {
                    window.handle_input(
                        &focus_for_ime,
                        gpui::ElementInputHandler::new(bounds, view_entity),
                        cx,
                    );
                },
            ).w(px(ime_w)).h(px(ime_h)).absolute().left(px(ime_x)).top(px(ime_y)))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(crate::theme::SURFACE))
            .text_color(rgb(crate::theme::TEXT))
            // macOS: with `appears_transparent: true` the content area
            // extends behind the titlebar, so the top ~28px overlap the
            // traffic light buttons. Pad the root flex column down on
            // macOS only so the sidebar / tab strip start *below* the
            // overlay. Windows / Linux keep the standard layout (the
            // window manager handles the titlebar above the content).
            .when(cfg!(target_os = "macos"), |d| d.pt(px(28.0)))
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.on_global_key_down(event, window, cx);
            }))
            // Mouse: left button down — forward to PTY or start selection
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                if this.resize_drag.is_some() {
                    return;
                }
                // Context menu open: the menu row's own on_click and the
                // dismiss-overlay's on_click both run independently, so
                // we just need this root handler to stay out of their
                // way. Without this guard, the root creates a new
                // zero-width Simple selection at the click cell —
                // wiping the selection `start_send_to_pane` (and
                // anything else that reads `selection_to_string`) is
                // about to consume. Matches the user's observation
                // that the keyboard shortcut works but the menu item
                // doesn't: the shortcut bypasses this handler entirely.
                if this.context_menu.is_some() {
                    return;
                }
                // Ignore clicks in the sidebar region — those are handled by
                // workspace/tab click handlers, not terminal selection.
                // MUST check before clearing rename state, otherwise double-click
                // rename on a workspace gets set by the workspace handler then
                // immediately cleared here via event bubbling.
                let sidebar_w = this.sidebar_width();
                if event.position.x.as_f32() < sidebar_w {
                    return;
                }
                // If any browser tab exists, reclaim OS focus from WebView2 on every
                // click in the GPUI area (terminal, URL bar, etc.). WebView2 is a
                // child HWND that steals OS keyboard focus; this ensures GPUI gets
                // keyboard events after clicking anywhere in our window.
                for entry in this.browser_tabs.values() {
                    if entry.browser.is_initialized() {
                        entry.browser.focus_parent();
                        break; // one call is enough
                    }
                }
                // Rename dismissal runs via the Input's Blur
                // handler — clearing the state here would race it.

                // Scrollbar hit-test runs BEFORE selection so a click on the
                // thumb/track doesn't also start a text selection underneath.
                if let Some((sb_pane, hit, snapshot)) = this.scrollbar_hit_test(event.position) {
                    this.terminal_manager_mut().set_active_pane(&sb_pane);
                    match hit {
                        ScrollbarHit::Thumb => {
                            this.scrollbar_drag = Some(snapshot);
                        }
                        ScrollbarHit::TrackAbove => {
                            // Page up by `visible` lines.
                            let page = (snapshot.track_h
                                / this.cell_dims().1.max(1.0))
                                as usize;
                            if let Some(term) = this.terminal_manager_mut().active_terminal() {
                                term.scroll_up(page.max(1));
                            }
                        }
                        ScrollbarHit::TrackBelow => {
                            let page = (snapshot.track_h
                                / this.cell_dims().1.max(1.0))
                                as usize;
                            if let Some(term) = this.terminal_manager_mut().active_terminal() {
                                term.scroll_down(page.max(1));
                            }
                        }
                    }
                    cx.notify();
                    return;
                }

                // Find which pane was clicked — use its bounds for cell coords.
                // This fixes selection when clicking a non-active pane in a split layout.
                let (clicked_pane_id, col, row) = match this.pixel_to_term_cell_at(event.position) {
                    Some(result) => result,
                    None => return, // Click outside any terminal — ignore.
                };

                // Activate the clicked pane so subsequent operations target it.
                this.terminal_manager_mut().set_active_pane(&clicked_pane_id);

                let (mouse_mode, _) = this.active_term_mouse_mode();

                // Ctrl/Cmd+Click: try to preview file path under cursor.
                // Always takes priority, even when mouse mode is on (e.g. Claude Code).
                // macOS convention uses Cmd; other platforms use Ctrl.
                let preview_modifier = if cfg!(target_os = "macos") {
                    event.modifiers.platform
                } else {
                    event.modifiers.control
                };
                if preview_modifier {
                    if crate::preview_open::try_preview_path_at(this, col, row) {
                        cx.notify();
                        return;
                    }
                }

                if mouse_mode {
                    this.send_mouse_event(0, col, row, true);
                } else {
                    use alacritty_terminal::index::{Column, Line, Point as AlacPoint, Direction};
                    use alacritty_terminal::selection::{Selection, SelectionType};
                    let clicks = event.click_count;
                    let sel_type = if clicks >= 3 {
                        SelectionType::Lines
                    } else if clicks == 2 {
                        SelectionType::Semantic
                    } else {
                        SelectionType::Simple
                    };
                    let side = Direction::Left;
                    if let Some(term) = this.terminal_manager_mut().active_terminal() {
                        term.with_term_mut(|t| {
                            // Translate viewport row → grid line, accounting for scrollback.
                            // alacritty uses negative Line values for scrollback history;
                            // a click at viewport row 5 with display_offset=10 corresponds
                            // to grid Line(5 - 10) = Line(-5). This is the inverse of the
                            // grid_line → viewport_line conversion done in gpui_terminal.rs.
                            let display_offset = t.grid().display_offset() as i32;
                            let grid_line = row as i32 - display_offset;
                            let point = AlacPoint::new(Line(grid_line), Column(col));
                            t.selection = Some(Selection::new(sel_type, point, side));
                        });
                    }
                    this.selecting = true;
                }
                cx.notify();
            }))
            // Modifier key release clears stale hover-link underline.
            // Without this, releasing Cmd/Ctrl without moving the mouse
            // leaves the underline visible until the next mouse move.
            .on_modifiers_changed(cx.listener(|this, event: &gpui::ModifiersChangedEvent, _window, cx| {
                let held = if cfg!(target_os = "macos") {
                    event.modifiers.platform
                } else {
                    event.modifiers.control
                };
                if !held && this.hover_link.is_some() {
                    this.hover_link = None;
                    cx.notify();
                }
            }))
            // Mouse: move — forward to PTY or extend selection
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _window, cx| {
                // Handle scrollbar thumb drag — recompute display_offset from
                // mouse delta against the snapshot taken at mousedown.
                if let Some(drag) = this.scrollbar_drag.clone() {
                    let dy = event.position.y.as_f32() - drag.start_mouse_y;
                    let usable = (drag.track_h - drag.thumb_h).max(1.0);
                    let frac_delta = dy / usable; // +down = scroll forward = lower offset
                    let new_offset_f =
                        drag.start_offset as f32 - frac_delta * drag.history as f32;
                    let new_offset = new_offset_f.round().clamp(0.0, drag.history as f32) as usize;
                    if new_offset != drag.start_offset
                        || dy != 0.0
                    {
                        let pane_id = drag.pane_id.clone();
                        if let Some(pane) = this.terminal_manager_mut().get_pane_mut(&pane_id) {
                            if let Some(term) = pane.active_terminal() {
                                term.with_term_mut(|t| {
                                    let cur = t.grid().display_offset() as i32;
                                    let delta = new_offset as i32 - cur;
                                    if delta != 0 {
                                        t.scroll_display(
                                            alacritty_terminal::grid::Scroll::Delta(delta),
                                        );
                                    }
                                });
                            }
                        }
                    }
                    cx.notify();
                    return;
                }
                // Track scrollbar hover for the expand-on-hover visual.
                // Cheap: only re-runs the hit test against the pane the
                // cursor is currently inside; everything else is hashmap
                // lookups + a small math block.
                {
                    let new_hover = this
                        .scrollbar_hit_test(event.position)
                        .map(|(pid, _, _)| pid);
                    if new_hover != this.scrollbar_hover_pane {
                        this.scrollbar_hover_pane = new_hover;
                        cx.notify();
                    }
                }
                // File-link hover feedback: underline the path under
                // the cursor when the preview modifier (Cmd on macOS,
                // Ctrl elsewhere) is held. Driven by mouse move only,
                // so releasing the modifier without moving the mouse
                // leaves the underline visible until the next move.
                {
                    let modifier_held = if cfg!(target_os = "macos") {
                        event.modifiers.platform
                    } else {
                        event.modifiers.control
                    };
                    // Resolve by enumeration: collect every plausible
                    // candidate (hyperlink / markdown / quoted /
                    // bareword with wrap extension), classify as
                    // file or URL, and validate accordingly.
                    // Underline = "clickable" — the modifier-click
                    // will open the hit (preview for files, system
                    // browser for URLs).
                    let new_hover: Option<HoverLinkState> = if modifier_held {
                        this.pixel_to_term_cell_at(event.position).and_then(|(pid, col, row)| {
                            let term = this.terminal_manager().get_pane(&pid)?.active_terminal_ref()?;
                            let hit = crate::preview_open::resolve_click_at_term(term, &*this, col, row)?;
                            Some(HoverLinkState { pane_id: pid, segments: hit.segments })
                        })
                    } else {
                        None
                    };
                    if new_hover != this.hover_link {
                        this.hover_link = new_hover;
                        cx.notify();
                    }
                }
                // Handle sidebar resize drag
                if let Some((start_x, start_w)) = this.sidebar_drag_start {
                    let delta = event.position.x.as_f32() - start_x;
                    this.sidebar_state.width = (start_w + delta).clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX);
                    cx.notify();
                    return;
                }
                // (Preview/browser panel resize drag removed — both are now pane tabs)
                // Handle split resize drag
                if let Some(ref drag) = this.resize_drag.clone() {
                    let current_pos = if drag.is_horizontal {
                        event.position.x.as_f32()
                    } else {
                        event.position.y.as_f32()
                    };
                    let delta = current_pos - drag.start_mouse_pos;
                    let new_ratio = (drag.start_ratio + delta / drag.container_length).clamp(0.1, 0.9);
                    let pane_id = amux_platform::terminal::manager::PaneId(drag.split_first_pane.clone());
                    this.terminal_manager_mut().update_split_ratio(&pane_id, new_ratio);
                    return;
                }
                let (mouse_mode, _) = this.active_term_mouse_mode();
                if mouse_mode && event.pressed_button == Some(gpui::MouseButton::Left) {
                    let (col, row) = this.pixel_to_term_cell(event.position);
                    this.send_mouse_event(32, col, row, true);
                } else if this.selecting {
                    // Edge auto-scroll: when the cursor leaves the active pane
                    // vertically, kick off (or refresh) a tick loop that
                    // scrolls the scrollback and extends the selection while
                    // the cursor stays out of bounds. This matches macOS
                    // Terminal / iTerm2 behavior.
                    let active_pid_opt = this
                        .terminal_manager()
                        .active_pane_id()
                        .cloned();
                    if let Some(ref active_pid) = active_pid_opt {
                        if let Some(&(px_x, px_y, pw, ph)) =
                            this.pane_bounds.get(&active_pid.0)
                        {
                            let mx = event.position.x.as_f32();
                            let my = event.position.y.as_f32();
                            let in_x = mx >= px_x && mx < px_x + pw;
                            let edge = if in_x && my < px_y {
                                Some(px_y - my) // above top → positive
                            } else if in_x && my >= px_y + ph {
                                Some(-(my - (px_y + ph))) // below bottom → negative
                            } else {
                                None
                            };
                            if let Some(edge_px) = edge {
                                let was_none = this.selection_autoscroll.is_none();
                                this.selection_autoscroll = Some(SelectionAutoScrollState {
                                    pane_id: active_pid.clone(),
                                    edge_pixels: edge_px,
                                    last_mouse_x: mx,
                                });
                                if was_none {
                                    spawn_selection_autoscroll_loop(cx);
                                }
                                cx.notify();
                                return;
                            } else {
                                this.selection_autoscroll = None;
                            }
                        }
                    }

                    // Extend selection — use cell side based on direction relative
                    // to the mouse position within the cell. This ensures the leftmost
                    // character can be selected when dragging right-to-left.
                    // Use pane-aware cell lookup so selection extends correctly
                    // regardless of which pane the mouse is currently over.
                    use alacritty_terminal::index::{Column, Line, Point as AlacPoint, Direction};
                    let (clicked_pid, col, row) = match this.pixel_to_term_cell_at(event.position) {
                        Some(r) => r,
                        None => { cx.notify(); return; },
                    };
                    let cw = this.cell_dims().0.max(1.0);
                    // Compute sub-cell position to determine which side of the cell the cursor is on
                    let pad = crate::gpui_terminal::TERMINAL_LEFT_PADDING;
                    let raw_x = this.pane_bounds.get(&clicked_pid.0)
                        .map(|&(px_x, _, _, _)| event.position.x.as_f32() - px_x - pad)
                        .unwrap_or(0.0);
                    let cell_offset = raw_x - col as f32 * cw;
                    let side = if cell_offset < cw * 0.5 { Direction::Left } else { Direction::Right };
                    if let Some(term) = this.terminal_manager_mut().active_terminal() {
                        term.with_term_mut(|t| {
                            // Same viewport→grid translation as the mouse-down handler:
                            // when the user has scrolled into history, drag-extending the
                            // selection must update against negative grid Lines, not the
                            // visible viewport row.
                            let display_offset = t.grid().display_offset() as i32;
                            let grid_line = row as i32 - display_offset;
                            let point = AlacPoint::new(Line(grid_line), Column(col));
                            if let Some(ref mut sel) = t.selection {
                                sel.update(point, side);
                            }
                        });
                    }
                }
                cx.notify();
            }))
            // Mouse: left button up — forward to PTY or finalize selection + auto-copy
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseUpEvent, _window, cx| {
                let (mouse_mode, _) = this.active_term_mouse_mode();
                if mouse_mode {
                    let (col, row) = this.pixel_to_term_cell(event.position);
                    this.send_mouse_event(0, col, row, false);
                } else if this.selecting {
                    // Finalize the selection endpoint at the release
                    // position. Without this step the selection is
                    // frozen at whatever the last processed mouse_move
                    // set it to — if the cursor moved 1-2 cells between
                    // that event and the button release (OS event
                    // coalescing, fast drags), those trailing cells
                    // never enter the selection and the copied text is
                    // short by the delta. Mirrors the extend block in
                    // on_mouse_move so the math matches exactly.
                    use alacritty_terminal::index::{Column, Line, Point as AlacPoint, Direction};
                    if let Some((clicked_pid, col, row)) = this.pixel_to_term_cell_at(event.position) {
                        let cw = this.cell_dims().0.max(1.0);
                        let pad = crate::gpui_terminal::TERMINAL_LEFT_PADDING;
                        let raw_x = this.pane_bounds.get(&clicked_pid.0)
                            .map(|&(px_x, _, _, _)| event.position.x.as_f32() - px_x - pad)
                            .unwrap_or(0.0);
                        let cell_offset = raw_x - col as f32 * cw;
                        let side = if cell_offset < cw * 0.5 { Direction::Left } else { Direction::Right };
                        if let Some(term) = this.terminal_manager_mut().active_terminal() {
                            term.with_term_mut(|t| {
                                let display_offset = t.grid().display_offset() as i32;
                                let grid_line = row as i32 - display_offset;
                                let point = AlacPoint::new(Line(grid_line), Column(col));
                                if let Some(ref mut sel) = t.selection {
                                    sel.update(point, side);
                                }
                            });
                        }
                    }
                    // Copy selected text to clipboard
                    if let Some(term) = this.terminal_manager_mut().active_terminal() {
                        let text = term.with_term(|t| t.selection_to_string());
                        if let Some(text) = text {
                            if !text.is_empty() {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                            }
                        }
                    }
                }
                this.selecting = false;
                this.resize_drag = None;
                this.sidebar_drag_start = None;
                this.scrollbar_drag = None;
                this.selection_autoscroll = None;
                cx.notify();
            }))
            // Mouse: right button up — forward release to PTY
            .on_mouse_up(gpui::MouseButton::Right, cx.listener(|this, event: &gpui::MouseUpEvent, _window, _cx| {
                let (mouse_mode, _) = this.active_term_mouse_mode();
                if mouse_mode {
                    let (col, row) = this.pixel_to_term_cell(event.position);
                    this.send_mouse_event(2, col, row, false);
                }
            }))
            // Mouse: middle click — paste clipboard
            .on_mouse_down(gpui::MouseButton::Middle, cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.paste_clipboard(cx);
            }))
            // Mouse wheel: scroll terminal or forward to PTY
            //
            // When an app enables mouse mode (Claude Code, vim, fzf), it expects
            // to receive scroll events so it can handle scrolling internally.
            // This matches Alacritty/kitty/WezTerm behavior: mouse mode → app
            // gets the events. Shift+scroll bypasses mouse mode to scroll our
            // scrollback buffer (for apps in primary screen with history).
            //
            // For alt screen apps without mouse mode (less with ALTERNATE_SCROLL),
            // convert scroll to arrow keys.
            .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _window, cx| {
                // Hover-follows-scroll: the scroll event targets the
                // pane the mouse is currently over, NOT the keyboard-
                // active pane. This matches Chrome / VS Code / every
                // modern multi-pane tool and lets the user compare
                // two panes side-by-side without click-focusing the
                // one they want to scroll. Keyboard focus is *not*
                // affected — that still requires an explicit click,
                // because focus-follows-hover misroutes keystrokes
                // into the wrong terminal and is a well-known
                // footgun.
                //
                // Mouse mode / alt-scroll detection and the col/row
                // used for forwarded mouse events all come from the
                // hover pane so the downstream PTY sees a coherent
                // picture — we never mix "mouse mode from active,
                // cursor position from hover".
                let (hover_pid, col, row) =
                    match this.pixel_to_term_cell_at(event.position) {
                        Some(r) => r,
                        None => {
                            // Mouse isn't over any terminal pane
                            // (sidebar / tab bar / non-terminal tab).
                            // Fall back to the active pane so wheel
                            // still does *something* reasonable —
                            // same as pre-hover-follows behavior for
                            // that edge case.
                            match this.terminal_manager().active_pane_id().cloned() {
                                Some(pid) => {
                                    let (c, r) = this.pixel_to_term_cell(event.position);
                                    (pid, c, r)
                                }
                                None => return,
                            }
                        }
                    };

                // Smooth scrolling: trackpads send many small pixel-
                // delta events (including momentum). We accumulate
                // fractional pixels and only scroll by integer lines
                // when a full cell_h has been reached. Mouse wheels
                // send Lines deltas which are used directly (1 notch
                // = 3 lines typically).
                let cell_h = this.cell_dims().1;
                let raw_delta = match event.delta {
                    gpui::ScrollDelta::Lines(pt) => pt.y * cell_h,  // convert to pixels
                    gpui::ScrollDelta::Pixels(pt) => pt.y.as_f32(),
                };
                if raw_delta == 0.0 { return; }

                // If the hover pane's active tab isn't a terminal
                // (preview / browser tab), don't scroll the
                // terminal scrollback — those tabs handle their
                // own wheel events.
                {
                    let kind = this
                        .terminal_manager()
                        .get_pane(&hover_pid)
                        .and_then(|p| p.active_tab_kind().cloned());
                    if let Some(ref k) = kind {
                        if !k.is_terminal() {
                            return;
                        }
                    }
                }

                // Reset accumulator on direction change to prevent lag
                // when the user reverses scroll direction quickly.
                if (raw_delta > 0.0) != (this.scroll_accumulator > 0.0) {
                    this.scroll_accumulator = 0.0;
                }

                this.scroll_accumulator += raw_delta;

                // Convert accumulated pixels to integer line count.
                let line_count = (this.scroll_accumulator / cell_h).trunc() as i32;
                if line_count == 0 {
                    // Not enough accumulated for a full line yet — wait
                    // for more events. Don't notify (no visual change).
                    return;
                }
                // Keep the fractional remainder for the next event.
                this.scroll_accumulator -= line_count as f32 * cell_h;

                let lines_abs = line_count.unsigned_abs() as usize;
                let scrolling_up = line_count > 0;

                let (mouse_mode, _sgr) = this.term_mouse_mode_for_pane(&hover_pid);
                let alt_scroll = this.term_alt_screen_scroll_for_pane(&hover_pid);
                let shift = event.modifiers.shift;

                // Resolve the hover pane's active terminal for the
                // scrollback branches. We don't use `active_terminal*`
                // here because that would target the click-focused
                // pane, not the one under the cursor.
                if mouse_mode && !shift {
                    // Mouse mode ON: forward scroll events to the app
                    // running in the hover pane.
                    let button: u8 = if scrolling_up { 64 } else { 65 };
                    // Build the wire bytes directly and push them
                    // into the hover pane's terminal. We can't reuse
                    // `send_mouse_event` because that one targets
                    // active.
                    let col_clamped = col.min(223);
                    let row_clamped = row.min(223);
                    let cx_1 = col_clamped + 1;
                    let cy_1 = row_clamped + 1;
                    if let Some(pane) =
                        this.terminal_manager_mut().get_pane_mut(&hover_pid)
                    {
                        if let Some(term) = pane.active_terminal() {
                            for _ in 0..lines_abs {
                                if _sgr {
                                    let seq = format!(
                                        "\x1b[<{};{};{}M",
                                        button, cx_1, cy_1
                                    );
                                    term.send_input(seq.as_bytes());
                                } else {
                                    let b = button + 32;
                                    let x = (col_clamped.min(222) as u8) + 33;
                                    let y = (row_clamped.min(222) as u8) + 33;
                                    let seq = [b'\x1b', b'[', b'M', b, x, y];
                                    term.send_input(&seq);
                                }
                            }
                        }
                    }
                } else if alt_scroll && !mouse_mode && !shift {
                    // Alt screen + ALTERNATE_SCROLL: send arrow keys
                    // to the hover pane.
                    let arrow: &[u8] = if scrolling_up { b"\x1b[A" } else { b"\x1b[B" };
                    if let Some(pane) =
                        this.terminal_manager().get_pane(&hover_pid)
                    {
                        if let Some(term) = pane.active_terminal_ref() {
                            for _ in 0..lines_abs {
                                term.send_input(arrow);
                            }
                        }
                    }
                } else if let Some(pane) =
                    this.terminal_manager_mut().get_pane_mut(&hover_pid)
                {
                    // Scroll the hover pane's scrollback buffer.
                    if let Some(term) = pane.active_terminal() {
                        if scrolling_up {
                            term.scroll_up(lines_abs);
                        } else {
                            term.scroll_down(lines_abs);
                        }
                    }
                }
                cx.notify();
            }))
            // Right-click: forward to PTY if mouse mode, else show context menu
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                let (mouse_mode, _) = this.active_term_mouse_mode();
                if mouse_mode {
                    let (col, row) = this.pixel_to_term_cell(event.position);
                    this.send_mouse_event(2, col, row, true); // button 2 = right press
                } else {
                    // Find which pane was right-clicked so context menu targets it.
                    let source_pane = this.pixel_to_term_cell_at(event.position)
                        .map(|(pid, _, _)| pid)
                        .or_else(|| this.terminal_manager().active_pane_id().cloned());

                    // Resolve the active selection to a real file once,
                    // at menu-open time. Stored on ContextMenuState so
                    // `menu::build_items` can decide enable/disable for
                    // the "Open Selection as File" row without running
                    // FS stats every render frame.
                    let selection_path: Option<String> = this
                        .terminal_manager()
                        .active_terminal_ref()
                        .and_then(|t| t.with_term(|term| term.selection_to_string()))
                        .filter(|s| !s.is_empty())
                        .and_then(|s| {
                            crate::preview_open::try_resolve_selection_as_path(&*this, &s)
                                .map(|hit| hit.absolute)
                        });

                    this.context_menu = Some(ContextMenuState {
                        position: event.position,
                        source_pane,
                        selection_path,
                    });
                }
                cx.notify();
            }))
            // Main content
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    // Sidebar
                    .child({
                        if sidebar_visible {
                            let sw = self.sidebar_state.width;
                            div()
                                .id("sidebar-expanded")
                                .w(px(sw))
                                .bg(rgb(crate::theme::SURFACE_DIM))
                                .flex()
                                .flex_row()
                                .overflow_hidden()
                                // Sidebar content column
                                .child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .overflow_hidden()
                                // Header: mode tabs + collapse button
                                .child({
                                    let is_ws_mode = self.sidebar_state.mode == SidebarMode::Workspaces;
                                    let ws_text_color = if is_ws_mode { rgb(crate::theme::TEXT) } else { rgb(crate::theme::TEXT_DIM) };
                                    let ag_text_color = if !is_ws_mode { rgb(crate::theme::TEXT) } else { rgb(crate::theme::TEXT_DIM) };
                                    div()
                                        .flex()
                                        .justify_between()
                                        .items_center()
                                        .px_3()
                                        .py_2()
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(2.0))
                                                // Workspaces tab
                                                .child(
                                                    div()
                                                        .id("sidebar-tab-ws")
                                                        .px(px(6.0))
                                                        .py(px(3.0))
                                                        .rounded(px(3.0))
                                                        .text_xs()
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(ws_text_color)
                                                        .when(is_ws_mode, |d| d.border_b_2().border_color(rgb(crate::theme::ACCENT)))
                                                        .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)).text_color(rgb(crate::theme::TEXT)))
                                                        .cursor_pointer()
                                                        .child("WS")
                                                        .on_click(cx.listener(|this, _e, _w, cx| {
                                                            this.sidebar_state.mode = SidebarMode::Workspaces;
                                                            cx.notify();
                                                        })),
                                                )
                                                // Agents tab
                                                .child(
                                                    div()
                                                        .id("sidebar-tab-agents")
                                                        .px(px(6.0))
                                                        .py(px(3.0))
                                                        .rounded(px(3.0))
                                                        .text_xs()
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(ag_text_color)
                                                        .when(!is_ws_mode, |d| d.border_b_2().border_color(rgb(crate::theme::ACCENT)))
                                                        .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)).text_color(rgb(crate::theme::TEXT)))
                                                        .cursor_pointer()
                                                        .child("Agents")
                                                        .on_click(cx.listener(|this, _e, _w, cx| {
                                                            this.sidebar_state.mode = SidebarMode::Agents;
                                                            cx.notify();
                                                        })),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("sidebar-collapse-btn")
                                                .px(px(5.0))
                                                .py(px(2.0))
                                                .rounded(px(3.0))
                                                .text_xs()
                                                .text_color(rgb(crate::theme::TEXT_DIM))
                                                .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)).text_color(rgb(crate::theme::TEXT)))
                                                .child("◀")
                                                .on_click(cx.listener(|this, _e, _w, cx| {
                                                    this.sidebar_state.collapsed = true;
                                                    cx.notify();
                                                })),
                                        )
                                })
                                // Sidebar body: workspace list or agents view
                                .child(if self.sidebar_state.mode == SidebarMode::Agents {
                                    // Agents view: collect agent items from terminal manager
                                    let agent_items: Vec<AgentSidebarItem> = self.terminal_manager()
                                        .pane_list()
                                        .into_iter()
                                        .map(|info| {
                                            let (icon, color) = match info.agent_status.as_deref() {
                                                Some("thinking...") => ("*".to_string(), 0x81a2beu32),
                                                Some("waiting")     => ("!".to_string(), 0xf9e2af),
                                                Some("done")        => ("+".to_string(), 0xb5bd68),
                                                Some("error")       => ("!".to_string(), 0xf38ba8),
                                                _                   => ("-".to_string(), 0x969896),
                                            };
                                            AgentSidebarItem {
                                                pane_id: info.pane_id.0.clone(),
                                                tab_title: info.tab_title,
                                                agent_kind: info.agent_kind,
                                                agent_status: info.agent_status,
                                                status_icon: icon,
                                                status_color: color,
                                            }
                                        })
                                        .collect();
                                    // Group agents by status and render with click handlers
                                    let mut grouped: std::collections::BTreeMap<u8, Vec<&AgentSidebarItem>> =
                                        std::collections::BTreeMap::new();
                                    for agent in &agent_items {
                                        let key = match agent.agent_status.as_deref() {
                                            Some("waiting") | Some("error") => 0u8,
                                            Some("thinking...") => 1,
                                            Some("done") => 2,
                                            _ => 3,
                                        };
                                        grouped.entry(key).or_default().push(agent);
                                    }
                                    let group_meta: [(u8, &str, &str, u32); 4] = [
                                        (0, "!", "ATTENTION", 0xf9e2af),
                                        (1, "*", "RUNNING",   0x81a2be),
                                        (2, "+", "COMPLETED", 0xb5bd68),
                                        (3, "-", "IDLE",      0x969896),
                                    ];
                                    let mut col = div()
                                        .flex_col()
                                        .flex_1()
                                        .overflow_y_hidden();
                                    if agent_items.is_empty() {
                                        col = col.child(
                                            div()
                                                .px_3().py_2()
                                                .text_xs()
                                                .text_color(rgb(crate::theme::TEXT_DIM))
                                                .child("No panes in workspace"),
                                        );
                                    }
                                    for (key, icon, label, color) in &group_meta {
                                        if let Some(items) = grouped.get(key) {
                                            // Group header
                                            col = col.child(
                                                div()
                                                    .flex().items_center().gap(px(6.0))
                                                    .px_3().pt(px(8.0)).pb(px(4.0))
                                                    .child(div().text_xs().text_color(rgb(*color)).font_weight(FontWeight::BOLD).child(*icon))
                                                    .child(div().text_xs().text_color(rgb(*color)).font_weight(FontWeight::SEMIBOLD).child(*label))
                                                    .child(div().text_xs().text_color(rgb(crate::theme::TEXT_DIM)).child(format!("({})", items.len()))),
                                            );
                                            for agent in items {
                                                let pane_id_click = agent.pane_id.clone();
                                                let icon_c = agent.status_icon.clone();
                                                let icon_color = agent.status_color;
                                                let title_c = agent.tab_title.clone();
                                                let kind_c = agent.agent_kind.clone().unwrap_or_default();
                                                let pane_short = if agent.pane_id.len() > 8 {
                                                    agent.pane_id[agent.pane_id.len() - 6..].to_string()
                                                } else {
                                                    agent.pane_id.clone()
                                                };
                                                col = col.child(
                                                    div()
                                                        .id(gpui::ElementId::Name(format!("agent-{}", agent.pane_id).into()))
                                                        .flex().items_center().gap(px(6.0))
                                                        .px_3().py(px(5.0)).mx_1()
                                                        .rounded(px(4.0))
                                                        .cursor_pointer()
                                                        .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
                                                        .child(div().text_xs().text_color(rgb(icon_color)).child(icon_c))
                                                        .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_sm().text_color(rgb(crate::theme::TEXT)).child(title_c))
                                                        .when(!kind_c.is_empty(), move |d| {
                                                            d.child(div().text_xs().text_color(rgb(crate::theme::TEXT_DIM)).child(kind_c))
                                                        })
                                                        .child(div().text_xs().text_color(rgb(crate::theme::SURFACE_RAISED)).child(pane_short))
                                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                                            let pid = amux_platform::terminal::manager::PaneId(pane_id_click.clone());
                                                            this.terminal_manager_mut().set_active_pane(&pid);
                                                            cx.notify();
                                                        })),
                                                );
                                            }
                                        }
                                    }
                                    col.into_any_element()
                                } else {
                                    // Workspaces mode — group-aware render.
                                    //
                                    // Iterate groups in their declared
                                    // order, and inside each group render
                                    // the workspaces that belong to it.
                                    // Rules:
                                    //   * A group whose `name` is empty
                                    //     (the default / migration group)
                                    //     renders its members flat with
                                    //     no header, so legacy users see
                                    //     the pre-group layout unchanged.
                                    //   * A group whose `name` is
                                    //     non-empty gets a header row.
                                    //   * Workspaces whose `group_id`
                                    //     doesn't match any known group
                                    //     (shouldn't happen after
                                    //     migration, but defensive) fall
                                    //     into a trailing "orphans"
                                    //     bucket rendered flat after all
                                    //     groups.
                                    let mut ws_col = div()
                                        .flex_col()
                                        .flex_1()
                                        .overflow_y_hidden();

                                    // Build an iteration plan: for each
                                    // group, collect its members (with
                                    // original `ws_idx` preserved — the
                                    // existing per-item render captures
                                    // that index for drag-reorder).
                                    let mut grouped: Vec<(
                                        String, // group id (unused below but kept for debuggability)
                                        String, // group name (empty => flat)
                                        Vec<(usize, amux_ui::GpuiWorkspaceItem)>,
                                    )> = workspace_groups
                                        .iter()
                                        .map(|g| (g.id.clone(), g.name.clone(), Vec::new()))
                                        .collect();
                                    let mut orphans: Vec<(usize, amux_ui::GpuiWorkspaceItem)> =
                                        Vec::new();
                                    for (ws_idx, item) in workspaces.iter().enumerate() {
                                        if let Some((_, _, bucket)) =
                                            grouped.iter_mut().find(|(id, _, _)| id == &item.group_id)
                                        {
                                            bucket.push((ws_idx, item.clone()));
                                        } else {
                                            orphans.push((ws_idx, item.clone()));
                                        }
                                    }

                                    // Flatten plan into a single vec of
                                    // (optional header, members) so the
                                    // rendering loop stays linear.
                                    let mut plan: Vec<(
                                        Option<String>,
                                        Vec<(usize, amux_ui::GpuiWorkspaceItem)>,
                                    )> = Vec::new();
                                    for (_, name, members) in grouped {
                                        if members.is_empty() {
                                            continue;
                                        }
                                        let header = if name.is_empty() {
                                            None
                                        } else {
                                            Some(name)
                                        };
                                        plan.push((header, members));
                                    }
                                    if !orphans.is_empty() {
                                        plan.push((None, orphans));
                                    }

                                    for (header, members) in plan {
                                        if let Some(header_name) = header {
                                            // Render group header: small
                                            // all-caps label + top/bottom
                                            // spacing, no click affordance
                                            // yet (Phase 3 will add
                                            // collapse + rename).
                                            ws_col = ws_col.child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .px_3()
                                                    .pt(px(8.0))
                                                    .pb(px(4.0))
                                                    .text_xs()
                                                    .text_color(rgb(crate::theme::TEXT_DIM))
                                                    .child(header_name),
                                            );
                                        }
                                        for (ws_idx, item) in members.iter() {
                                            let ws_idx = *ws_idx;
                                            let item = item;
                                            let is_active = item.is_active;
                                            let has_ws_activity = !is_active && self.workspace_terminals
                                                .get(&item.id)
                                                .map(|tm| tm.has_any_activity())
                                                .unwrap_or(false);
                                            let bg_color = if is_active { rgb(crate::theme::SURFACE_RAISED) } else { rgb(crate::theme::SURFACE_DIM) };
                                            let text_color = if is_active { rgb(crate::theme::TEXT) } else { rgb(crate::theme::TEXT_DIM) };
                                            let ws_id = item.id.clone();
                                            let ws_id_dbl = item.id.clone();
                                            let ws_id_drop = item.id.clone();
                                            let ws_name = item.name.clone();
                                            let drag_name = item.name.clone();
                                            let ws_id_del = item.id.clone();
                                            let can_delete = workspaces.len() > 1;
                                            let is_renaming = self.renaming_workspace.as_ref()
                                                .map(|(id, _)| id == &item.id)
                                                .unwrap_or(false);

                                            ws_col = ws_col.child(
                                            div()
                                                .id(gpui::ElementId::Name(format!("ws-{}", item.id).into()))
                                                .group(format!("ws-group-{}", item.id))
                                                .flex()
                                                .items_center()
                                                .px_3()
                                                .py(px(6.0))
                                                .mx_1()
                                                .my_px()
                                                .rounded(px(4.0))
                                                .bg(bg_color)
                                                .cursor_grab()
                                                .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
                                                .when(is_active, |d| d.border_l_2().border_color(rgb(crate::theme::ACCENT)))
                                                // Drag to reorder
                                                .on_drag(
                                                    DragWorkspace { name: drag_name, index: ws_idx },
                                                    |drag, _, _, cx| {
                                                        cx.stop_propagation();
                                                        cx.new(|_| drag.clone())
                                                    },
                                                )
                                                .drag_over::<DragWorkspace>(|style, _, _, _| {
                                                    style.bg(rgb(crate::theme::SURFACE_RAISED)).border_t_2().border_color(rgb(crate::theme::ACCENT))
                                                })
                                                .on_drop(cx.listener(move |this, drag: &DragWorkspace, _window, cx| {
                                                    this.reorder_workspace(drag.index, &ws_id_drop);
                                                    cx.notify();
                                                }))
                                                .on_mouse_down(gpui::MouseButton::Left, cx.listener(
                                                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                                                        if event.click_count >= 2 {
                                                            this.start_workspace_rename(
                                                                ws_id_dbl.clone(),
                                                                ws_name.clone(),
                                                                window,
                                                                cx,
                                                            );
                                                        } else if this.renaming_workspace.is_none() {
                                                            let _ = this.app.activate_workspace(&ws_id);
                                                            this.switch_workspace_terminal(&ws_id);
                                                            this.refresh_model();
                                                            cx.notify();
                                                        }
                                                    }
                                                ))
                                                .child(if is_renaming {
                                                    let input_state = self.renaming_workspace
                                                        .as_ref()
                                                        .map(|(_, s)| s.clone());
                                                    if let Some(state) = input_state {
                                                        // `stop_propagation` blocks clicks
                                                        // in the field from reaching the
                                                        // parent row's click-to-activate
                                                        // handler.
                                                        div()
                                                            .flex_1()
                                                            .px_1()
                                                            .text_sm()
                                                            .text_color(rgb(crate::theme::TEXT))
                                                            .bg(rgb(crate::theme::SURFACE_RAISED))
                                                            .rounded(px(2.0))
                                                            .border_1()
                                                            .border_color(rgb(crate::theme::ACCENT))
                                                            .on_mouse_down(
                                                                gpui::MouseButton::Left,
                                                                |_, _, cx| {
                                                                    cx.stop_propagation();
                                                                },
                                                            )
                                                            .child(
                                                                gpui_component::input::Input::new(&state)
                                                                    .cleanable(false)
                                                                    .appearance(false),
                                                            )
                                                            .into_any_element()
                                                    } else {
                                                        div().into_any_element()
                                                    }
                                                } else {
                                                    let group_name = format!("ws-group-{}", item.id);
                                                    div()
                                                        .flex()
                                                        .flex_row()
                                                        .items_center()
                                                        .gap(px(6.0))
                                                        .flex_1()
                                                        .when(has_ws_activity, |d| {
                                                            d.child(
                                                                div().w(px(6.0)).h(px(6.0)).rounded(px(3.0))
                                                                    .bg(rgb(crate::theme::SUCCESS)).flex_shrink_0()
                                                            )
                                                        })
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .overflow_hidden()
                                                                .whitespace_nowrap()
                                                                .text_sm()
                                                                .text_color(text_color)
                                                                .when(is_active, |d| d.font_weight(FontWeight::MEDIUM))
                                                                .child(item.name.clone())
                                                        )
                                                        .when(can_delete, |d| {
                                                            d.child(
                                                                div()
                                                                    .id(gpui::ElementId::Name(format!("ws-del-{}", ws_id_del).into()))
                                                                    .px(px(3.0))
                                                                    .rounded(px(3.0))
                                                                    .text_xs()
                                                                    .text_color(rgb(crate::theme::SURFACE_DIM))
                                                                    .group_hover(&group_name, |d| {
                                                                        d.text_color(rgb(crate::theme::TEXT_DIM))
                                                                    })
                                                                    .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)).text_color(rgb(crate::theme::DANGER)))
                                                                    .child("✕")
                                                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                                                        let _ = this.app.run_command(&format!("workspace close {}", ws_id_del));
                                                                        this.workspace_terminals.remove(&ws_id_del);
                                                                        this.workspace_order.retain(|id| id != &ws_id_del);
                                                                        this.refresh_model();
                                                                        if this.active_workspace_id == ws_id_del {
                                                                            if let Some(first) = this.model.workspace_items.first() {
                                                                                let new_id = first.id.clone();
                                                                                this.switch_workspace_terminal(&new_id);
                                                                            }
                                                                        }
                                                                        cx.notify();
                                                                    }))
                                                            )
                                                        })
                                                        .into_any_element()
                                                })
                                            );
                                        }
                                    }
                                    // "+ New" / "+ Open" bottom bar.
                                    //
                                    // Lives in its own flex child so it's
                                    // **pinned** to the sidebar's bottom
                                    // edge. The workspace list above gets
                                    // `flex_1` and can grow/shrink; this
                                    // row is `flex_shrink_0` so it always
                                    // occupies its natural height and
                                    // never scrolls out of view no matter
                                    // how many workspaces the user has
                                    // (up to the point where the list
                                    // region stops showing all items —
                                    // at that scale we'd add proper
                                    // scrolling, but the pinned buttons
                                    // still stay visible).
                                    //
                                    // Two entry points sit side-by-side
                                    // because they solve different
                                    // problems:
                                    //   * "+ New" creates a fresh
                                    //     workspace at `$HOME` via
                                    //     `Command::CreateWorkspace` —
                                    //     skips dedup so each click
                                    //     gives a distinct bucket the
                                    //     user can rename. Shares `$HOME`
                                    //     resolution with
                                    //     `StartupMode::DefaultHome`.
                                    //   * "+ Open" preserves the
                                    //     file-picker flow for users
                                    //     who want a specific project
                                    //     directory (dedup still
                                    //     applies there — opening the
                                    //     same folder twice is always
                                    //     a re-activate).
                                    let bottom_bar = div()
                                        .flex_shrink_0()
                                        .flex()
                                        .flex_row()
                                        .gap_1()
                                        .px_1()
                                        .mb_1()
                                        .child(
                                            div()
                                                .id("sidebar-new-empty-ws")
                                                .flex_1()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .py_2()
                                                .rounded(px(4.0))
                                                .text_xs()
                                                .text_color(rgb(crate::theme::TEXT_DIM))
                                                .cursor_pointer()
                                                .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)).text_color(rgb(crate::theme::TEXT)))
                                                .child("+  New")
                                                .on_click(cx.listener(|this, _event, _window, cx| {
                                                    this.new_home_workspace(cx);
                                                })),
                                        )
                                        .child(
                                            div()
                                                .id("sidebar-open-ws")
                                                .flex_1()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .py_2()
                                                .rounded(px(4.0))
                                                .text_xs()
                                                .text_color(rgb(crate::theme::TEXT_DIM))
                                                .cursor_pointer()
                                                .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)).text_color(rgb(crate::theme::TEXT)))
                                                .child("+  Open")
                                                .on_click(cx.listener(|this, _event, _window, cx| {
                                                    this.prompt_open_local_workspace(cx);
                                                    cx.notify();
                                                })),
                                        );

                                    // Assemble: scrollable workspace
                                    // list on top, pinned button bar on
                                    // the bottom. The outer wrapper is
                                    // `flex_col().flex_1()` so it fills
                                    // the sidebar content area that the
                                    // mode-switcher above this block
                                    // already sized.
                                    div()
                                        .flex_col()
                                        .flex_1()
                                        .child(ws_col)
                                        .child(bottom_bar)
                                        .into_any_element()
                                })
                                ) // end sidebar content column
                                // Resize handle (right edge)
                                .child(
                                    div()
                                        .id("sidebar-resize-handle")
                                        .group("sidebar-handle")
                                        .w(px(4.0))
                                        .h_full()
                                        .flex_shrink_0()
                                        .cursor_col_resize()
                                        .child(
                                            div()
                                                .w(px(1.0))
                                                .h_full()
                                                .bg(rgb(crate::theme::SURFACE_RAISED))
                                                .group_hover("sidebar-handle", |d| d.w(px(2.0)).bg(rgb(crate::theme::ACCENT)))
                                        )
                                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, _w, _cx| {
                                            this.sidebar_drag_start = Some(
                                                (event.position.x.as_f32(), this.sidebar_state.width)
                                            );
                                        }))
                                )
                        } else {
                            // Collapsed sidebar: narrow strip with expand button
                            div()
                                .id("sidebar-expand")
                                .w(px(SIDEBAR_WIDTH_COLLAPSED))
                                .bg(rgb(crate::theme::SURFACE_DIM))
                                .flex()
                                .flex_col()
                                .items_center()
                                .border_r_1()
                                .border_color(rgb(crate::theme::SURFACE_RAISED))
                                .child(
                                    div()
                                        .id("sidebar-expand-btn")
                                        .mt_2()
                                        .px(px(5.0))
                                        .py(px(4.0))
                                        .rounded(px(3.0))
                                        .text_xs()
                                        .text_color(rgb(crate::theme::TEXT_DIM))
                                        .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)).text_color(rgb(crate::theme::TEXT)))
                                        .child("▶")
                                        .on_click(cx.listener(|this, _e, _w, cx| {
                                            this.sidebar_state.collapsed = false;
                                            cx.notify();
                                        })),
                                )
                        }
                    })
                    // Main content area (terminal + optional preview panel)
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_row()
                            .overflow_hidden()
                            // Terminal column
                            .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            // Terminal pane(s) — renders split layout recursively
                            .child({
                                let active_pane_id = self.terminal_manager_mut().active_pane_id().cloned();
                                let sidebar_w = self.sidebar_width();
                                let vp = window.viewport_size();
                                let content_w = vp.width.as_f32() - sidebar_w;
                                let status_bar_h = 34.0_f32;
                                // Must match the resize calculation exactly.
                                let titlebar_h = if cfg!(target_os = "macos") { 28.0_f32 } else { 0.0 };
                                let content_h = vp.height.as_f32() - status_bar_h - titlebar_h;
                                // Cursor blinks: visible for 30 frames, hidden for 30 frames (~500ms each at 60fps)
                                let cursor_blink_on = (self.cursor_blink_frame % 60) < 30;
                                // Compute pane bounds for mouse hit-testing.
                                // Take ownership of pane_bounds to avoid the need for
                                // unsafe pointer tricks — render_layout fills it, we put it back.
                                let mut pane_bounds = std::mem::take(&mut self.pane_bounds);
                                pane_bounds.clear();
                                let origin_x = sidebar_w;
                                // Include macOS titlebar offset so pane_bounds Y
                                // matches GPUI mouse event coordinates (which are
                                // in window coordinates, not content coordinates).
                                let origin_y = titlebar_h;
                                let zoomed = self.zoomed_pane.clone();
                                let layout_cloned = self.terminal_manager_mut().active_layout().cloned();
                                let renaming_tab = self.renaming_tab.clone();
                                // Grab the current search match list so
                                // the terminal paint layer can highlight
                                // every hit (not just the current one
                                // that lives in `Term::selection`).
                                // Empty slice when no search is active.
                                let search_matches: Vec<alacritty_terminal::term::search::Match> =
                                    self.search_state.as_ref()
                                        .map(|s| s.matches.clone())
                                        .unwrap_or_default();
                                // Pane whose scrollbar should render in the
                                // expanded (hover/drag) style. Drag wins over
                                // hover so the bar stays big while the user
                                // is actively dragging the thumb.
                                let sb_expanded_pane = self.scrollbar_drag.as_ref()
                                    .map(|d| d.pane_id.clone())
                                    .or_else(|| self.scrollbar_hover_pane.clone());
                                let sb_expanded_pane_ref = sb_expanded_pane.as_ref();
                                let hover_link_ref = self.hover_link.as_ref();
                                let result = if let Some(zpid) = zoomed {
                                    let single = amux_platform::terminal::manager::PaneLayout::Single(zpid.clone());
                                    render_layout(&single, self.terminal_manager(), Some(&zpid), content_w, content_h, cursor_blink_on, &metrics, true, &renaming_tab, origin_x, origin_y, &mut pane_bounds, &self.config.font_family, self.config.font_size, &self.terminal_theme, &self.browser_tabs, &self.preview_tabs, &search_matches, sb_expanded_pane_ref, hover_link_ref, cx)
                                } else if let Some(layout) = layout_cloned {
                                    render_layout(&layout, self.terminal_manager(), active_pane_id.as_ref(), content_w, content_h, cursor_blink_on, &metrics, false, &renaming_tab, origin_x, origin_y, &mut pane_bounds, &self.config.font_family, self.config.font_size, &self.terminal_theme, &self.browser_tabs, &self.preview_tabs, &search_matches, sb_expanded_pane_ref, hover_link_ref, cx)
                                } else {
                                    div().flex_1().bg(rgb(crate::theme::SURFACE)).child("No terminal").into_any_element()
                                };
                                self.pane_bounds = pane_bounds;
                                result
                            })
                            ) // end terminal column
                            // (Preview is now rendered inside pane tabs, not as a separate column)
                            // (Browser is now rendered inside pane tabs, not as a separate column)
                    ),
            )
            .child(render_status_bar(&StatusBarData {
                workspace_name: self.model.active_workspace_name
                    .clone()
                    .unwrap_or_else(|| "No workspace".into()),
                pane_count: self.terminal_manager().total_panes(),
                tab_count: self.terminal_manager().total_tabs(),
                shell_name: if cfg!(target_os = "windows") { "pwsh".into() } else {
                    std::env::var("SHELL").unwrap_or_else(|_| "bash".into())
                        .rsplit('/').next().unwrap_or("bash").to_string()
                },
                agents: self.terminal_manager().agent_summaries()
                    .into_iter()
                    .map(|(name, icon, color)| AgentSummary {
                        name,
                        status_icon: icon,
                        color,
                    })
                    .collect(),
                crash_notice: self.crash_notice,
                debug_stats: crate::metrics::snapshot(),
            }))
            // Context menu: dismiss overlay + menu
            .when_some(self.context_menu.clone(), |this, menu| {
                let items = crate::menu::build_items(self);
                let vp = window.viewport_size();
                this
                    // Full-screen transparent overlay to catch clicks outside menu
                    .child(
                        div()
                            .id("context-menu-dismiss")
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.context_menu = None;
                                cx.notify();
                            }))
                    )
                    // The actual menu (rendered on top of the overlay)
                    .child(render_context_menu(menu.position, items, vp.width, vp.height, cx))
            })
            // Search bar overlay (top-right)
            .when_some(self.search_state.clone(), |this, state| {
                use crate::theme;
                // Counter string: "3/17", "1/1000+", "0/0", or "err"
                // when the regex didn't compile.
                let counter = if state.error {
                    "err".to_string()
                } else if state.matches.is_empty() {
                    if state.query.is_empty() { String::new() } else { "0/0".to_string() }
                } else {
                    let total = state.matches.len();
                    let suffix = if state.truncated { "+" } else { "" };
                    format!("{}/{}{}", state.current + 1, total, suffix)
                };
                // Red for bad regex or a non-empty query with zero
                // matches; dim otherwise. Semantic tokens so palette
                // edits propagate without a per-call-site diff.
                let counter_color = if state.error
                    || (!state.query.is_empty() && state.matches.is_empty())
                {
                    theme::DANGER
                } else {
                    theme::TEXT_DIM
                };
                let mode_label = state.mode.short_label();
                let mode_bg = match state.mode {
                    SearchMode::Literal => theme::MODE_LITERAL_BG,
                    SearchMode::Regex => theme::MODE_REGEX_BG,
                    SearchMode::Fuzzy => theme::MODE_FUZZY_BG,
                };
                this.child(
                    div()
                        .absolute()
                        .top(px(4.0))
                        .right(px(16.0))
                        .w(px(380.0))
                        .px_3()
                        .py(px(6.0))
                        .rounded(px(theme::RADIUS_LG))
                        .bg(rgb(theme::SURFACE))
                        .border_1()
                        .border_color(rgb(theme::BORDER))
                        .shadow_lg()
                        .flex()
                        .items_center()
                        .gap_2()
                        // Mode badge (Tab to cycle)
                        .child(
                            div()
                                .px(px(6.0))
                                .py(px(1.0))
                                .rounded(px(theme::RADIUS_SM))
                                .bg(rgb(mode_bg))
                                .text_xs()
                                .text_color(rgb(theme::TEXT))
                                .child(mode_label)
                        )
                        // Query field
                        .child(
                            div()
                                .flex_1()
                                .px_2()
                                .py(px(2.0))
                                .rounded(px(theme::RADIUS_SM))
                                .bg(rgb(theme::SURFACE_DIM))
                                .border_1()
                                .border_color(rgb(theme::BORDER_DIM))
                                .text_sm()
                                .text_color(rgb(theme::TEXT))
                                .min_h(px(20.0))
                                .child(if state.query.is_empty() {
                                    div().text_color(rgb(theme::TEXT_DIM))
                                        .child("Type to search…  Tab: cycle mode")
                                        .into_any_element()
                                } else {
                                    div().child(format!("{}▎", state.query)).into_any_element()
                                })
                        )
                        // Match counter
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(counter_color))
                                .min_w(px(52.0))
                                .child(counter)
                        )
                        .child(
                            div().text_xs().text_color(rgb(theme::TEXT_DIM)).child("Esc")
                        )
                )
            })
            // IME preedit overlay — renders the composition text (e.g.
            // pinyin letters) inline at the terminal cursor position,
            // matching how macOS Terminal.app displays it: just the
            // characters with a subtle underline, no floating box or
            // bordered dialog. The candidate selection window is
            // positioned by macOS via `first_rect_for_character_range`
            // / our `bounds_for_range`.
            .when_some(self.ime_preedit.clone(), |this, preedit| {
                let pos = self.cell_metrics.as_ref().and_then(|m| {
                    let pid = self.terminal_manager().active_pane_id()?;
                    let &(ox, oy, _, _) = self.pane_bounds.get(&pid.0)?;
                    let (col, row) = self.terminal_manager().active_terminal_ref()
                        .map(|t| t.with_term(|term| {
                            let c = term.renderable_content().cursor;
                            let display_offset = term.grid().display_offset() as i32;
                            let viewport_row = (c.point.line.0 + display_offset).max(0) as usize;
                            (c.point.column.0, viewport_row)
                        }))?;
                    let pad = crate::gpui_terminal::TERMINAL_LEFT_PADDING;
                    // pane_bounds coordinates are in the root div's CONTENT
                    // coordinate system (Y=0 = after macOS titlebar padding).
                    // But .absolute().top() on the root div positions from the
                    // PADDING BOX edge (Y=0 = window top, before padding).
                    // On macOS we have pt(28px), so absolute Y needs +28 to
                    // match content coordinates. On Windows/Linux there's no
                    // titlebar padding, so no offset is needed.
                    #[cfg(target_os = "macos")]
                    let titlebar_inset = 28.0_f32;
                    #[cfg(not(target_os = "macos"))]
                    let titlebar_inset = 0.0_f32;
                    Some((ox + pad + col as f32 * m.width, oy + row as f32 * m.height + titlebar_inset))
                });
                if let Some((x, y)) = pos {
                    let font_size = self.config.font_size;
                    this.child(
                        div()
                            .absolute()
                            .left(px(x))
                            .top(px(y))
                            .text_size(px(font_size))
                            .font_family(self.config.font_family.clone())
                            .text_color(rgb(crate::theme::TEXT))
                            .text_decoration_1()
                            .text_decoration_color(rgb(crate::theme::ACCENT))
                            .child(format!("{preedit}▏"))
                    )
                } else {
                    this
                }
            })
            // File picker overlay (Ctrl+P)
            .when_some(self.file_picker.clone(), |this, picker| {
                this.child(crate::gpui_preview::render_file_picker(&picker, cx))
            })
            // Agent picker overlay (Launch Agent)
            .when_some(self.agent_picker.clone(), |this, picker| {
                this.child(render_agent_picker(&picker, cx))
            })
            // New-tab dropdown picker (from +▾ button)
            .when_some(self.new_tab_picker.clone(), |this, picker| {
                this.child(render_new_tab_picker(&picker, cx))
            })
            // Template picker overlay (Apply Layout)
            .when_some(self.template_picker.clone(), |this, picker| {
                this.child(render_template_picker(&picker, cx))
            })
            // Pane picker overlay (Send to Pane)
            .when_some(self.pane_picker.clone(), |this, picker| {
                this.child(render_pane_picker(&picker, cx))
            })
            // Agent toast notifications (bottom-right)
            .when(!self.toasts.is_empty(), |this| {
                let toast_els: Vec<_> = self.toasts.iter().enumerate().map(|(i, t)| {
                    let pane_id = t.pane_id.clone();
                    let tab_idx = t.tab_index;
                    div()
                        .id(gpui::ElementId::Name(format!("toast-{}", i).into()))
                        .px_3()
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .bg(rgb(crate::theme::SURFACE))
                        .border_1()
                        .border_color(rgb(t.color))
                        .shadow_lg()
                        .text_xs()
                        .text_color(rgb(t.color))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
                        .child(t.message.clone())
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.terminal_manager_mut().set_active_pane(&pane_id);
                            this.terminal_manager_mut().set_active_tab_in_pane(tab_idx);
                            // Dismiss all toasts on click
                            this.toasts.clear();
                            cx.notify();
                        }))
                        .into_any_element()
                }).collect();
                this.child(
                    div()
                        .absolute()
                        .bottom(px(36.0))
                        .right(px(16.0))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(toast_els)
                )
            })
    }
}


// NOTE: render_context_menu, first_pane_in_layout, render_layout
// have been moved to gpui_layout_renderer.rs
// NOTE: pub fn run, the macOS dock icon pipeline, and the 60 fps
// PTY/cursor/browser polling timer have been moved to
// `crate::app_bootstrap`. This file now owns GpuiShellView, its
// fields, its constructor, and its Render impl — nothing else.

