//! `Render` impl for `GpuiShellView`, extracted from `gpui_entry.rs` so
//! the entry file can own struct definition + helper methods and this
//! file can own the per-frame render tree. Pure code move — no
//! behavior change. See `gpui_entry.rs` for field documentation and
//! the main `impl GpuiShellView` block.

#![cfg(feature = "gpui")]

use gpui::{
    rgb, AppContext, Context, FontWeight, IntoElement, Render, Window,
    px, div, prelude::*,
};

use crate::drag::DragWorkspace;
use crate::gpui_entry::{
    spawn_selection_autoscroll_loop, GpuiShellView, HoverLinkState, SIDEBAR_WIDTH_COLLAPSED,
    SIDEBAR_WIDTH_MAX, SIDEBAR_WIDTH_MIN,
};
use crate::gpui_layout_renderer::{
    render_agent_picker, render_ai_profile_picker, render_api_key_input, render_context_menu,
    render_help_overlay, render_layout, render_new_tab_picker, render_pane_picker, render_template_picker,
};
use crate::gpui_status_bar::{render_status_bar, AgentSummary, StatusBarData};
use crate::gpui_workspace_sidebar::{AgentSidebarItem, SidebarMode};
use crate::state::{
    ContextMenuState, ScrollbarHit, SearchMode, SelectionAutoScrollState,
};

/// Format a u64 token count into a human-readable string.
fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M tk", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}K tk", n / 1_000)
    } else if n > 0 {
        format!("{} tk", n)
    } else {
        String::new()
    }
}

#[cfg(feature = "gpui")]
impl Render for GpuiShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Record frame time into the debug-stats HUD ring buffer.
        // Cheap when the HUD is disabled (one Instant + one atomic
        // push); the formatted snapshot is only materialized below if
        // AMUX_DEBUG_STATS=1.
        let _frame_guard = crate::metrics::FrameGuard::start();

        // Check if "About Amux" menu item was clicked
        if crate::app_bootstrap::ABOUT_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
            self.show_help = !self.show_help;
            cx.notify();
        }

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
        } else if self.renaming_workspace.is_some() || self.renaming_tab.is_some() || self.api_key_input.is_some() {
            // Rename / API key input active: leave focus on the Input.
            // Re-grabbing the root handle here races the focus
            // `on_next_frame` the rename/input helper just scheduled.
        } else {
            // No browser, no rename — safe to ensure terminal
            // always has focus.
            if !self.focus_handle.is_focused(window) {
                self.focus_handle.focus(window, cx);
            }
        }

        // Sync URL bar and tab title when navigation changed the page address.
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
            } else {
                // Update the tab title in TerminalManager to show the domain
                self.terminal_manager_mut().update_active_browser_url(&url);
                if let Some((_, entry)) = self.active_browser_entry() {
                    let input = entry.url_input.clone();
                    input.update(cx, |state, cx| {
                        state.set_value(url, window, cx);
                    });
                }
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
            let status_bar_h = crate::theme::STATUS_BAR_H;
            // macOS transparent titlebar uses pt(28px) on the root div,
            // which eats into the viewport but isn't accounted for by
            // status_bar_h alone. Without subtracting it, the terminal
            // computes 1-2 extra rows that get clipped at the bottom.
            let titlebar_h = if cfg!(target_os = "macos") { crate::theme::TITLEBAR_H } else { 0.0 };
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
                // Preview text-selection dismissal: if a selection is
                // live but this click landed outside the markdown
                // body, clear it. Mirrors gpui-component's pattern
                // (see `TextView::paint`'s "down outside to clear
                // selection" global handler). Click INSIDE body:
                // skip — the body's own `on_mouse_down` will start a
                // fresh selection via `start_selection`, which
                // overwrites the old one. `preview_body_bounds` is
                // `None` on the first frame or when no markdown
                // preview is visible; treat that as "outside" so a
                // click in any other region clears the orphaned
                // state.
                if this.preview_selection.is_some() {
                    let in_body = this
                        .preview_body_bounds
                        .map(|b| b.contains(&event.position))
                        .unwrap_or(false);
                    if !in_body {
                        this.preview_selection = None;
                        this.preview_selection_ranges.borrow_mut().clear();
                        cx.notify();
                    }
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
                    if crate::preview_open::try_preview_path_at(this, cx, col, row) {
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
                                    // Agents view: only show panes where a known
                                    // VibeCoding tool (Claude Code, OpenCode, Codex,
                                    // Aider, Gemini, Copilot) was detected via
                                    // terminal-title matching. Regular terminals,
                                    // browser tabs and preview tabs are excluded.
                                    let agent_items: Vec<AgentSidebarItem> = self.terminal_manager()
                                        .pane_list()
                                        .into_iter()
                                        .filter(|info| info.agent_kind.is_some())
                                        .map(|info| {
                                            let (icon, color) = match info.agent_status.as_deref() {
                                                Some("thinking...") => ("*".to_string(), 0x81a2beu32),
                                                Some("waiting")     => ("!".to_string(), 0xf9e2af),
                                                Some("done")        => ("+".to_string(), 0xb5bd68),
                                                Some("error")       => ("!".to_string(), 0xf38ba8),
                                                _                   => ("-".to_string(), 0x969896),
                                            };
                                            let session = info.agent_session.as_ref();
                                            AgentSidebarItem {
                                                pane_id: info.pane_id.0.clone(),
                                                tab_title: info.tab_title,
                                                agent_kind: info.agent_kind,
                                                agent_status: info.agent_status,
                                                status_icon: icon,
                                                status_color: color,
                                                session_tool: session
                                                    .and_then(|s| s.tool_label()),
                                                session_tokens: session
                                                    .map(|s| s.total_tokens())
                                                    .filter(|t| *t > 0),
                                                session_subagents: session
                                                    .map_or(0, |s| s.subagent_count),
                                                session_todo_done: session
                                                    .map_or(0, |s| s.todo_progress().0),
                                                session_todo_total: session
                                                    .map_or(0, |s| s.todo_progress().1),
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
                                                .child("No AI agents detected"),
                                        )
                                        .child(
                                            div()
                                                .px_3().pb_2()
                                                .text_xs()
                                                .text_color(rgb(crate::theme::TEXT_DIM))
                                                .child("Start Claude Code, OpenCode, or Codex in a terminal pane"),
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
                                                let tool_c = agent.session_tool.clone().unwrap_or_default();
                                                let tokens_c = agent.session_tokens.map(|t| format_tokens(t));
                                                let sub_c = if agent.session_subagents > 0 {
                                                    Some(agent.session_subagents)
                                                } else { None };
                                                let todo_c = if agent.session_todo_total > 0 {
                                                    Some(format!("{}/{}", agent.session_todo_done, agent.session_todo_total))
                                                } else { None };
                                                let pane_short = if agent.pane_id.len() > 8 {
                                                    agent.pane_id[agent.pane_id.len() - 6..].to_string()
                                                } else {
                                                    agent.pane_id.clone()
                                                };
                                                col = col.child(
                                                    div()
                                                        .id(gpui::ElementId::Name(format!("agent-{}", agent.pane_id).into()))
                                                        .flex_col()
                                                        .px_3().py(px(5.0)).mx_1()
                                                        .rounded(px(4.0))
                                                        .cursor_pointer()
                                                        .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
                                                        .child(
                                                            div().flex().items_center().gap(px(6.0))
                                                                .child(div().text_xs().text_color(rgb(icon_color)).child(icon_c))
                                                                .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_sm().text_color(rgb(crate::theme::TEXT)).child(title_c))
                                                                .when(!kind_c.is_empty(), move |d| {
                                                                    d.child(div().text_xs().text_color(rgb(crate::theme::TEXT_DIM)).child(kind_c))
                                                                })
                                                                .child(div().text_xs().text_color(rgb(crate::theme::SURFACE_RAISED)).child(pane_short))
                                                        )
                                                        // Session detail line: tool | tokens | sub-agents | todos
                                                        .when(!tool_c.is_empty() || tokens_c.is_some() || sub_c.is_some() || todo_c.is_some(), move |d| {
                                                            d.child(
                                                                div().flex().items_center().gap(px(8.0)).pt(px(2.0))
                                                                    .when(!tool_c.is_empty(), move |d2| {
                                                                        d2.child(div().text_xs().text_color(rgb(0x81a2be)).child(tool_c))
                                                                    })
                                                                    .when_some(tokens_c, |d2, t| {
                                                                        d2.child(div().text_xs().text_color(rgb(crate::theme::TEXT_DIM)).child(t))
                                                                    })
                                                                    .when_some(sub_c, |d2, n| {
                                                                        d2.child(div().text_xs().text_color(rgb(0xcba6f7)).child(format!("{} sub", n)))
                                                                    })
                                                                    .when_some(todo_c, |d2, p| {
                                                                        d2.child(div().text_xs().text_color(rgb(0xa6e3a1)).child(p))
                                                                    })
                                                            )
                                                        })
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
                                            let ws_id_confirm = ws_id_del.clone();
                                            let can_delete = workspaces.len() > 1;
                                            let is_confirming = self.confirming_delete_ws.as_ref() == Some(&ws_id_del);
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
                                                            this.confirming_delete_ws = None;
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
                                                                    .when(is_confirming, |d| {
                                                                        d.text_color(rgb(crate::theme::DANGER))
                                                                            .bg(rgb(crate::theme::DANGER_BG))
                                                                    })
                                                                    .when(!is_confirming, |d| {
                                                                        d.text_color(rgb(crate::theme::SURFACE_DIM))
                                                                            .group_hover(&group_name, |d| {
                                                                                d.text_color(rgb(crate::theme::TEXT_DIM))
                                                                            })
                                                                            .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)).text_color(rgb(crate::theme::DANGER)))
                                                                    })
                                                                    .child(if is_confirming { "Delete?" } else { "✕" })
                                                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                                                        if this.confirming_delete_ws.as_ref() == Some(&ws_id_confirm) {
                                                                            // Second click: actually delete
                                                                            this.confirming_delete_ws = None;
                                                                            let _ = this.app.run_command(&format!("workspace close {}", ws_id_confirm));
                                                                            this.workspace_terminals.remove(&ws_id_confirm);
                                                                            this.workspace_order.retain(|id| id != &ws_id_confirm);
                                                                            this.refresh_model();
                                                                            if this.active_workspace_id == ws_id_confirm {
                                                                                if let Some(first) = this.model.workspace_items.first() {
                                                                                    let new_id = first.id.clone();
                                                                                    this.switch_workspace_terminal(&new_id);
                                                                                }
                                                                            }
                                                                        } else {
                                                                            // First click: enter confirmation state
                                                                            this.confirming_delete_ws = Some(ws_id_confirm.clone());
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
                                let status_bar_h = crate::theme::STATUS_BAR_H;
                                // Must match the resize calculation exactly.
                                let titlebar_h = if cfg!(target_os = "macos") { crate::theme::TITLEBAR_H } else { 0.0 };
                                let content_h = vp.height.as_f32() - status_bar_h - titlebar_h;
                                // Cursor blinks: visible for 30 frames, hidden for 30 frames (~500ms each at 60fps)
                                let cursor_blink_on = (self.cursor_blink_frame % 60) < 30;
                                // Visual bell flash: tint the terminal background gold
                                // for ~4 frames after the bell rings.
                                let bell_flash_on = self.bell_flash_frame
                                    .is_some_and(|f| self.cursor_blink_frame.wrapping_sub(f) < 4);
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
                                // Ensure every open markdown preview has a
                                // ListState with the correct item count
                                // *before* we hand the map to render_layout.
                                // Must run before the immutable borrows below
                                // (hover_link_ref, preview_search, etc.) — it
                                // takes &mut self and would otherwise collide.
                                self.sync_preview_list_states();
                                // Auto-reload invalidation: drop stale
                                // selection before any downstream borrow
                                // captures a potentially-stale reference.
                                // Mutates self.preview_selection, so must
                                // fire before hover_link_ref etc.
                                self.invalidate_preview_selection_if_stale();
                                let hover_link_ref = self.hover_link.as_ref();
                                // Preview text-selection background: theme's
                                // `selection` color with alpha so the
                                // underlying text stays readable. Computed
                                // once per frame and passed to render_layout
                                // (and through it, to any visible preview).
                                let preview_selection_bg = {
                                    let mut h = gpui::Hsla::from(rgb(self.terminal_theme.selection));
                                    h.a = 0.55;
                                    h
                                };
                                // Drop byte ranges collected by last frame's
                                // SelectableText::paint before this frame's
                                // paint runs. Without the clear, the range
                                // sink would grow unbounded and Cmd+C would
                                // copy bytes from both frames.
                                self.clear_preview_selection_ranges();
                                let preview_selection_sink = self.preview_selection_ranges.clone();
                                let result = if let Some(zpid) = zoomed {
                                    let single = amux_platform::terminal::manager::PaneLayout::Single(zpid.clone());
                                    render_layout(&single, self.terminal_manager(), Some(&zpid), content_w, content_h, cursor_blink_on, bell_flash_on, &metrics, true, &renaming_tab, origin_x, origin_y, &mut pane_bounds, &self.config.font_family, self.config.font_size, &self.terminal_theme, &self.browser_tabs, &self.preview_tabs, self.preview_search.as_ref(), &self.preview_scroll_handle, &self.preview_list_states, self.preview_toc.as_ref(), self.preview_selection.as_ref(), self.preview_body_bounds, preview_selection_bg, &preview_selection_sink, &search_matches, sb_expanded_pane_ref, hover_link_ref, cx)
                                } else if let Some(layout) = layout_cloned {
                                    render_layout(&layout, self.terminal_manager(), active_pane_id.as_ref(), content_w, content_h, cursor_blink_on, bell_flash_on, &metrics, false, &renaming_tab, origin_x, origin_y, &mut pane_bounds, &self.config.font_family, self.config.font_size, &self.terminal_theme, &self.browser_tabs, &self.preview_tabs, self.preview_search.as_ref(), &self.preview_scroll_handle, &self.preview_list_states, self.preview_toc.as_ref(), self.preview_selection.as_ref(), self.preview_body_bounds, preview_selection_bg, &preview_selection_sink, &search_matches, sb_expanded_pane_ref, hover_link_ref, cx)
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
            .child(render_status_bar(
                &StatusBarData {
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
                        .map(|(name, icon, color, pane_id, tab_index)| AgentSummary {
                            name,
                            status_icon: icon,
                            color,
                            pane_id,
                            tab_index,
                        })
                        .collect(),
                    crash_notice: self.crash_notice,
                    debug_stats: crate::metrics::snapshot(),
                    // Show per-pane profile if set, otherwise workspace-level
                    active_ai_profile: self.terminal_manager().active_pane_id()
                        .and_then(|pid| self.terminal_manager().pane_profile_id(pid).map(|s| s.to_string()))
                        .or_else(|| self.active_ai_profile.clone()),
                },
                cx,
            ))
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
                    let titlebar_inset = crate::theme::TITLEBAR_H;
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
            // AI profile picker overlay
            .when_some(self.ai_profile_picker.clone(), |this, picker| {
                this.child(render_ai_profile_picker(&picker, &self.active_ai_profile, cx))
            })
            // API key input overlay (for preset activation)
            .when_some(self.api_key_input.clone(), |this, input| {
                this.child(render_api_key_input(&input, cx))
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
            // Help overlay (F1)
            .when(self.show_help, |this| {
                this.child(render_help_overlay(cx))
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
                            // Dismiss only the clicked toast
                            if i < this.toasts.len() {
                                this.toasts.remove(i);
                            }
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
