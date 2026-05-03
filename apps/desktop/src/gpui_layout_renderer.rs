//! Layout rendering — split panes, tab strips, context menu
//!
//! Extracted from gpui_entry.rs to keep rendering logic separate from
//! application state management.

#[cfg(feature = "gpui")]
use gpui::{rgb, px, div, prelude::*, Context, IntoElement, Styled};

#[cfg(feature = "gpui")]
use amux_platform::terminal::manager::{TerminalManager, SplitDirection};

#[cfg(feature = "gpui")]
use crate::gpui_entry::{GpuiShellView, ContextMenuItem, ResizeDragState, DragTab, NewTabPickerState};

/// Render the right-click context menu
#[cfg(feature = "gpui")]
pub(crate) fn render_context_menu(
    pos: gpui::Point<gpui::Pixels>,
    items: Vec<ContextMenuItem>,
    viewport_w: gpui::Pixels,
    viewport_h: gpui::Pixels,
    cx: &mut Context<GpuiShellView>,
) -> impl IntoElement {
    let menu_w = 240.0_f32;
    // Each item ~ 30px (6px padding + ~18px text), separator ~ 10px
    let separator_count = items.iter().filter(|i| i.separator_after).count();
    let menu_h = (items.len() as f32) * 30.0 + (separator_count as f32) * 10.0 + 12.0;
    let max_menu_h = viewport_h.as_f32() * 0.8; // never exceed 80% of viewport

    // Adjust position to keep menu within viewport
    let mut x = pos.x.as_f32();
    let mut y = pos.y.as_f32();
    if x + menu_w > viewport_w.as_f32() {
        x = (viewport_w.as_f32() - menu_w).max(0.0);
    }
    if y + menu_h.min(max_menu_h) > viewport_h.as_f32() {
        y = (viewport_h.as_f32() - menu_h.min(max_menu_h)).max(0.0);
    }

    let mut menu = div()
        .id("context-menu-container")
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(menu_w))
        .max_h(px(max_menu_h))
        .overflow_y_scroll()
        .rounded(px(8.0))
        .bg(rgb(crate::theme::SURFACE_RAISED))
        .border_1()
        .border_color(rgb(crate::theme::BORDER))
        .shadow_lg()
        .py_1()
        .flex()
        .flex_col();

    for item in items {
        let label = item.label;
        let enabled = item.enabled;

        let text_color = if enabled { rgb(crate::theme::TEXT) } else { rgb(crate::theme::TEXT_DISABLED) };

        let left = div().flex().flex_row().items_center().gap(px(6.0))
            .child(div().text_sm().text_color(text_color).child(label));

        let row = div()
            .id(gpui::ElementId::Name(label.into()))
            .px_3()
            .py(px(6.0))
            .mx_1()
            .rounded(px(4.0))
            .flex()
            .justify_between()
            .items_center()
            .when(enabled, |d| d.hover(|d| d.bg(rgb(crate::theme::BORDER))))
            .when(enabled, |d| {
                d.on_click(cx.listener(move |this, _event, window, cx| {
                    crate::menu::dispatch(this, label, window, cx);
                }))
            })
            .child(left)
            .children(item.shortcut.map(|kb| {
                div()
                    .text_xs()
                    .text_color(rgb(crate::theme::TEXT_DIM))
                    .child(kb)
            }));

        menu = menu.child(row);

        if item.separator_after {
            menu = menu.child(
                div()
                    .mx_2()
                    .my_1()
                    .h(px(1.0))
                    .bg(rgb(crate::theme::BORDER)),
            );
        }
    }

    menu
}

/// Get the first pane ID from a layout subtree (for identifying splits)
#[cfg(feature = "gpui")]
pub(crate) fn first_pane_in_layout(layout: &amux_platform::terminal::manager::TabLayout) -> Option<amux_platform::terminal::manager::PaneId> {
    use amux_platform::terminal::manager::TabLayout;
    match layout {
        TabLayout::Single(id) => Some(id.clone()),
        TabLayout::Horizontal { left, .. } => first_pane_in_layout(left),
        TabLayout::Vertical { top, .. } => first_pane_in_layout(top),
    }
}

/// Recursively render the tab layout tree (split panes)
#[cfg(feature = "gpui")]
pub(crate) fn render_layout(
    layout: &amux_platform::terminal::manager::TabLayout,
    manager: &TerminalManager,
    active_pane_id: Option<&amux_platform::terminal::manager::PaneId>,
    avail_w: f32,
    avail_h: f32,
    cursor_blink_on: bool,
    bell_flash_on: bool,
    metrics: &crate::gpui_terminal::CellMetrics,
    is_zoomed: bool,
    renaming_tab: &Option<(
        String,
        usize,
        gpui::Entity<gpui_component::input::InputState>,
    )>,
    origin_x: f32,
    origin_y: f32,
    pane_bounds: &mut std::collections::HashMap<String, (f32, f32, f32, f32)>,
    font_family: &str,
    font_size: f32,
    theme: &crate::gpui_terminal::TerminalTheme,
    browser_tabs: &std::collections::HashMap<u64, crate::gpui_browser::BrowserTabEntry>,
    preview_tabs: &std::collections::HashMap<String, crate::gpui_preview::PreviewState>,
    preview_search: Option<&crate::preview_search::PreviewSearchState>,
    preview_scroll_handle: &gpui::UniformListScrollHandle,
    preview_list_states: &std::collections::HashMap<String, gpui::ListState>,
    preview_toc: Option<&crate::preview_toc::TocPickerState>,
    preview_selection: Option<&crate::preview_selection::PreviewSelectionState>,
    preview_body_bounds: Option<gpui::Bounds<gpui::Pixels>>,
    preview_selection_bg: gpui::Hsla,
    preview_selection_sink: &crate::preview_selection::SelectionRangeSink,
    search_matches: &[alacritty_terminal::term::search::Match],
    scrollbar_expanded_pane: Option<&amux_platform::terminal::manager::PaneId>,
    hover_link: Option<&crate::gpui_entry::HoverLinkState>,
    cx: &mut Context<GpuiShellView>,
) -> gpui::AnyElement {
    use amux_platform::terminal::manager::TabLayout;

    match layout {
        TabLayout::Single(pane_id) => {
            // Record this pane's screen bounds for mouse hit-testing.
            // Tab strip (28px) is at the top; terminal content starts below it.
            let tab_strip_h = crate::theme::TAB_STRIP_H;
            pane_bounds.insert(pane_id.0.clone(), (origin_x, origin_y + tab_strip_h, avail_w, (avail_h - tab_strip_h).max(0.0)));
            let is_active = active_pane_id == Some(pane_id);

            // Build per-pane tab strip + terminal content.
            // `get_pane` should always return Some after `heal_layout`, which
            // `ensure_workspace_terminal` runs on every workspace switch. If
            // we still fall through (e.g. a save captured a transient
            // inconsistent state and we're one frame ahead of the next
            // heal), show the same "Starting terminal..." placeholder the
            // missing-PTY branch uses. The bootstrap tick will re-run
            // ensure_workspace_terminal on the next interaction.
            let (tab_strip, content) = if let Some(pane) = manager.get_pane(pane_id) {
                let tabs = pane.tab_titles();
                let pid_for_tabs = pane_id.clone();
                let has_multiple_panes = manager.total_panes() > 1;

                // Left side: tab buttons
                let tab_count = tabs.len();
                let tabs_row = div()
                    .flex()
                    .flex_row()
                    .gap_px()
                    .flex_1()
                    .overflow_hidden()
                    .children(tabs.into_iter().map(|(idx, title, is_tab_active, has_activity, tab_exited, agent_status, tab_kind)| {
                        let pid_click = pid_for_tabs.clone();
                        let pid_close_tab = pid_for_tabs.clone();
                        let pid_drag = pid_for_tabs.clone();
                        let pid_tab_drop = pid_for_tabs.clone();
                        let drag_title = title.clone();
                        // Skip tab width clamps during rename so the row
                        // grows with typed content instead of locking at 180px.
                        let tab_in_rename = renaming_tab
                            .as_ref()
                            .map(|(p, i, _)| p == &pid_for_tabs.0 && *i == idx)
                            .unwrap_or(false);
                        div()
                            .id(gpui::ElementId::Name(
                                format!("{}-tab-{}", pid_for_tabs.0, idx).into(),
                            ))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(4.0))
                            .min_w(px(60.0))
                            .when(!tab_in_rename, |d| {
                                d.max_w(px(180.0)).flex_shrink().overflow_hidden()
                            })
                            .px_3()
                            .py(px(4.0))
                            .text_xs()
                            .cursor_grab()
                            .text_color(if is_tab_active { rgb(crate::theme::TEXT) } else { rgb(crate::theme::TEXT_DIM) })
                            .bg(if is_tab_active { rgb(crate::theme::SURFACE) } else { rgb(crate::theme::SURFACE_DIM) })
                            .border_b_2()
                            .border_color(if is_tab_active { rgb(crate::theme::ACCENT) } else { rgb(crate::theme::SURFACE_DIM) })
                            .when(is_tab_active, |d| d.font_weight(gpui::FontWeight::MEDIUM))
                            .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
                            .on_drag(
                                DragTab {
                                    source_pane: pid_drag,
                                    tab_index: idx,
                                    title: drag_title,
                                },
                                |drag, _, _, cx| {
                                    cx.stop_propagation();
                                    cx.new(|_| drag.clone())
                                },
                            )
                            // Drop on tab: reorder within same pane, or move across panes
                            .drag_over::<DragTab>(|style, _, _, _| {
                                style.border_l_2().border_color(rgb(crate::theme::ACCENT))
                            })
                            .on_drop(cx.listener(move |this, drag: &DragTab, _window, cx| {
                                if drag.source_pane == pid_tab_drop {
                                    // Same pane: reorder
                                    this.terminal_manager_mut().reorder_tab(
                                        &drag.source_pane,
                                        drag.tab_index,
                                        idx,
                                    );
                                } else {
                                    // Different pane: append to target pane (positional
                                    // cross-pane insert not yet supported).
                                    this.terminal_manager_mut().move_tab_to_pane(
                                        &drag.source_pane,
                                        drag.tab_index,
                                        &pid_tab_drop,
                                    );
                                }
                                cx.notify();
                            }))
                            .on_click({
                                let pid_rename = pid_click.clone();
                                let current_title = title.clone();
                                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                    if event.click_count() >= 2 {
                                        this.start_tab_rename(
                                            pid_rename.0.clone(),
                                            idx,
                                            current_title.clone(),
                                            window,
                                            cx,
                                        );
                                    } else {
                                        // Single click: switch tab
                                        this.terminal_manager_mut().set_active_pane(&pid_click);
                                        this.terminal_manager_mut().set_active_tab_in_pane(idx);
                                        // When switching to a non-browser tab, reclaim OS focus
                                        // from any WebView2 that may hold it.
                                        let switched_to_browser = this.has_visible_browser();
                                        if !switched_to_browser {
                                            for entry in this.browser_tabs.values() {
                                                if entry.browser.is_initialized() {
                                                    entry.browser.focus_parent();
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    cx.notify();
                                })
                            })
                            .child({
                                let is_tab_renaming = renaming_tab.as_ref()
                                    .map(|(p, i, _)| p == &pid_for_tabs.0 && *i == idx)
                                    .unwrap_or(false);
                                if is_tab_renaming {
                                    let input_state = renaming_tab.as_ref().map(|(_, _, s)| s.clone());
                                    if let Some(state) = input_state {
                                        // Fixed width: Input's `size_full`
                                        // collapses to 0 against an
                                        // intrinsic-sized parent.
                                        use gpui_component::Sizable;
                                        div()
                                            .w(px(160.0))
                                            .text_color(rgb(crate::theme::TEXT))
                                            .bg(rgb(crate::theme::SURFACE_RAISED))
                                            .rounded(px(2.0))
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                |_, _, cx| {
                                                    cx.stop_propagation();
                                                },
                                            )
                                            .child(
                                                gpui_component::input::Input::new(&state)
                                                    .small()
                                                    .cleanable(false)
                                                    .appearance(false),
                                            )
                                            .into_any_element()
                                    } else {
                                        div().into_any_element()
                                    }
                                } else {
                                    let mut tab_content = div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(4.0))
                                        .overflow_hidden()
                                        .flex_1();
                                    // Status indicator: red dot for exited, green dot for activity
                                    if tab_exited {
                                        tab_content = tab_content.child(
                                            div().w(px(6.0)).h(px(6.0)).rounded(px(3.0))
                                                .bg(rgb(crate::theme::DANGER)).flex_shrink_0() // red
                                        );
                                    } else if has_activity && !is_tab_active {
                                        tab_content = tab_content.child(
                                            div().w(px(6.0)).h(px(6.0)).rounded(px(3.0))
                                                .bg(rgb(crate::theme::SUCCESS)).flex_shrink_0() // green
                                        );
                                    }
                                    // Show tab kind icon for non-terminal tabs
                                    let icon = tab_kind.icon();
                                    if !icon.is_empty() {
                                        tab_content = tab_content.child(
                                            div().whitespace_nowrap().flex_shrink_0().child(icon)
                                        );
                                    }
                                    tab_content = tab_content.child(
                                        div().whitespace_nowrap().child(title)
                                    );
                                    // Show agent status badge with color coding
                                    if let Some((ref label, color)) = agent_status {
                                        tab_content = tab_content.child(
                                            div()
                                                .text_xs()
                                                .whitespace_nowrap()
                                                .text_color(rgb(color))
                                                .child(format!("[{}]", label))
                                        );
                                    }
                                    tab_content.into_any_element()
                                }
                            })
                            .child(
                                div()
                                    .id(gpui::ElementId::Name(
                                        format!("{}-tab-{}-close", pid_close_tab.0, idx).into(),
                                    ))
                                    .px(px(2.0))
                                    .rounded(px(3.0))
                                    .text_color(rgb(crate::theme::TEXT_DIM))
                                    .hover(|d| d.bg(rgb(crate::theme::BORDER)).text_color(rgb(crate::theme::DANGER)))
                                    .child("×")
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.terminal_manager_mut().set_active_pane(&pid_close_tab);
                                        // If closing a browser tab, clean up its WebView2 state
                                        // Clean up tab-specific state before closing
                                        let tab_kind = this.terminal_manager().get_pane(&pid_close_tab)
                                            .and_then(|p| p.tabs.get(idx))
                                            .map(|t| t.kind.clone());
                                        match tab_kind.as_ref() {
                                            Some(amux_platform::terminal::manager::TabKind::Browser { browser_id, .. }) => {
                                                this.browser_tabs.remove(browser_id);
                                            }
                                            Some(amux_platform::terminal::manager::TabKind::Preview { path }) => {
                                                this.preview_tabs.remove(path);
                                                this.preview_unwatch_path(path);
                                            }
                                            _ => {}
                                        }
                                        let is_last_tab = this.terminal_manager()
                                            .get_pane(&pid_close_tab)
                                            .map_or(false, |p| p.tab_count() <= 1);
                                        if is_last_tab {
                                            this.terminal_manager_mut().close_active_pane();
                                        } else if let Some(pane) = this.terminal_manager_mut().get_pane_mut(&pid_close_tab) {
                                            pane.close_tab(idx);
                                        }
                                        cx.notify();
                                    }))
                            )
                    }));

                // Right side: action buttons
                let pid_new = pane_id.clone();
                let pid_sr = pane_id.clone();
                let pid_sd = pane_id.clone();
                let pid_close = pane_id.clone();

                // Pane action buttons — styled to be visible but
                // unobtrusive. Slightly larger than before so they
                // feel "clickable" rather than decorative.
                let btn_text = rgb(crate::theme::TEXT_DIM);     // softer than bg but visible
                let btn_hover_bg = rgb(crate::theme::BORDER);
                let btn_hover_text = rgb(crate::theme::TEXT);

                let pid_dropdown = pane_id.clone();
                let actions_row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
                    .px_2()
                    // [+][▾] composite: + = new terminal, ▾ = dropdown picker
                    .child(
                        div()
                            .flex().flex_row().items_center()
                            .rounded(px(4.0))
                            .overflow_hidden()
                            // "+" half
                            .child(
                                div()
                                    .id(gpui::ElementId::Name(format!("{}-btn-add", pane_id.0).into()))
                                    .px(px(8.0))
                                    .py(px(3.0))
                                    .text_sm()
                                    .text_color(btn_text)
                                    .cursor_pointer()
                                    .hover(|d| d.bg(btn_hover_bg).text_color(btn_hover_text))
                                    .child("+")
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.terminal_manager_mut().set_active_pane(&pid_new);
                                        let env = this.capture_active_env();
                                        this.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                                        this.spawn_with_captured_env(&env);
                                        cx.notify();
                                    })),
                            )
                            // thin divider
                            .child(div().w(px(1.0)).h(px(14.0)).bg(rgb(crate::theme::BORDER)))
                            // "▾" half
                            .child(
                                div()
                                    .id(gpui::ElementId::Name(format!("{}-btn-dropdown", pane_id.0).into()))
                                    .px(px(6.0))
                                    .py(px(3.0))
                                    .text_sm()
                                    .text_color(btn_text)
                                    .cursor_pointer()
                                    .hover(|d| d.bg(btn_hover_bg).text_color(btn_hover_text))
                                    .child("▾")
                                    .on_click(cx.listener(move |this, _event: &gpui::ClickEvent, _window, cx| {
                                        let bounds = this.pane_bounds.get(&pid_dropdown.0)
                                            .copied()
                                            .unwrap_or((0.0, 0.0, 400.0, 30.0));
                                        let anchor = gpui::point(px(bounds.0 + bounds.2 - 230.0), px(bounds.1 + 30.0));
                                        this.open_new_tab_picker(pid_dropdown.clone(), anchor);
                                        cx.notify();
                                    })),
                            )
                    )
                    // Split Right
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("{}-btn-sr", pane_id.0).into()))
                            .px(px(7.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .text_sm()
                            .text_color(btn_text)
                            .cursor_pointer()
                            .hover(|d| d.bg(btn_hover_bg).text_color(btn_hover_text))
                            .child("⬕")
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.terminal_manager_mut().set_active_pane(&pid_sr);
                                let env = this.capture_active_env();
                                this.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
                                this.spawn_with_captured_env(&env);
                                cx.notify();
                            })),
                    )
                    // Split Down
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("{}-btn-sd", pane_id.0).into()))
                            .px(px(7.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .text_sm()
                            .text_color(btn_text)
                            .cursor_pointer()
                            .hover(|d| d.bg(btn_hover_bg).text_color(btn_hover_text))
                            .child("⬓")
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.terminal_manager_mut().set_active_pane(&pid_sd);
                                let env = this.capture_active_env();
                                this.terminal_manager_mut().split_active_pane(SplitDirection::Vertical);
                                this.spawn_with_captured_env(&env);
                                cx.notify();
                            })),
                    )
                    // Zoom / Restore
                    .when(has_multiple_panes || is_zoomed, |d| {
                        let pid_zoom = pane_id.clone();
                        let zoom_icon = if is_zoomed { "⤡" } else { "⤢" };
                        d.child(
                            div()
                                .id(gpui::ElementId::Name(format!("{}-btn-zoom", pane_id.0).into()))
                                .px(px(7.0))
                                .py(px(3.0))
                                .rounded(px(4.0))
                                .text_sm()
                                .text_color(btn_text)
                                .cursor_pointer()
                                .hover(|d| d.bg(btn_hover_bg).text_color(btn_hover_text))
                                .child(zoom_icon)
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.terminal_manager_mut().set_active_pane(&pid_zoom);
                                    this.toggle_zoom();
                                    cx.notify();
                                })),
                        )
                    })
                    // Close
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("{}-btn-close", pane_id.0).into()))
                            .px(px(7.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .text_sm()
                            .text_color(if has_multiple_panes { btn_text } else { rgb(crate::theme::BORDER) })
                            .cursor_pointer()
                            .when(has_multiple_panes, |d| {
                                d.hover(|d| d.bg(rgb(crate::theme::DANGER_BG)).text_color(rgb(crate::theme::DANGER)))
                            })
                            .child("✕")
                            .when(has_multiple_panes, |d| {
                                d.on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.terminal_manager_mut().set_active_pane(&pid_close);
                                    this.cleanup_pane_tab_entries();
                                    this.terminal_manager_mut().close_active_pane();
                                    cx.notify();
                                }))
                            }),
                    );

                // Combine into tab strip
                let tab_strip = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .bg(rgb(crate::theme::SURFACE_DIM))
                    .border_b_1()
                    .border_color(rgb(crate::theme::SURFACE_RAISED))
                    .child(tabs_row)
                    // Zoom indicator: inline between tabs and actions
                    .when(is_zoomed, |d| {
                        d.child(
                            div()
                                .px_2()
                                .py(px(2.0))
                                .rounded(px(8.0))
                                .bg(rgb(crate::theme::SURFACE))
                                .border_1()
                                .border_color(rgb(crate::theme::BORDER))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(5.0))
                                .child(
                                    div()
                                        .w(px(6.0))
                                        .h(px(6.0))
                                        .rounded(px(3.0))
                                        .bg(rgb(crate::theme::SUCCESS))
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(crate::theme::TEXT_DIM))
                                        .child("ZOOMED")
                                )
                        )
                    })
                    .child(actions_row)
                    .into_any_element();

                let active_tab_exited = pane.active_tab_exited();
                use amux_platform::terminal::manager::TabKind;
                let active_kind = pane.active_tab_kind().cloned();
                let content = match active_kind.as_ref() {
                    Some(TabKind::Browser { browser_id, .. }) => {
                        // Render browser tab content (URL bar + WebView2).
                        // Pass the pane's content size so the canvas gets exact pixel dimensions.
                        let bid = *browser_id;
                        let browser_content_w = avail_w;
                        let browser_content_h = (avail_h - tab_strip_h).max(0.0);
                        if let Some(entry) = browser_tabs.get(&bid) {
                            let input = entry.url_input.clone();
                            let bcell = entry.bounds_cell.clone();
                            crate::gpui_browser::render_browser_tab_content(input, bcell, bid, browser_content_w, browser_content_h, cx).into_any_element()
                        } else {
                            div().flex_1().bg(rgb(crate::theme::SURFACE)).child("Browser loading...").into_any_element()
                        }
                    }
                    Some(TabKind::Preview { path }) => {
                        let preview_w = avail_w;
                        let preview_h = (avail_h - tab_strip_h).max(0.0);
                        if let Some(preview) = preview_tabs.get(path) {
                            let list_state = preview_list_states.get(path).cloned();
                            // Selection ctx is only live when the
                            // stored selection is for THIS preview's
                            // path AND has non-coincident endpoints
                            // AND we've captured bounds on a prior
                            // frame. Anything else and the ctx is
                            // None — no highlights painted.
                            let selection_ctx = preview_selection
                                .filter(|s| s.path == *path)
                                .and_then(|s| {
                                    let anchor = s.anchor?;
                                    let head = s.head?;
                                    if anchor == head { return None; }
                                    let bounds = preview_body_bounds?;
                                    let scroll = preview_list_states
                                        .get(path)
                                        .map(|ls| ls.scroll_px_offset_for_scrollbar())
                                        .unwrap_or_default();
                                    Some(crate::preview_selection::SelectionRenderCtx {
                                        start_window: crate::preview_selection::content_to_window(
                                            anchor, bounds, scroll,
                                        ),
                                        end_window: crate::preview_selection::content_to_window(
                                            head, bounds, scroll,
                                        ),
                                        background: preview_selection_bg,
                                        sink: preview_selection_sink.clone(),
                                    })
                                });
                            crate::gpui_preview::render_preview_panel(
                                preview,
                                preview_w,
                                preview_h,
                                preview_search,
                                preview_scroll_handle.clone(),
                                list_state,
                                preview_toc,
                                selection_ctx,
                                cx,
                            ).into_any_element()
                        } else {
                            div().flex_1().bg(rgb(crate::theme::SURFACE))
                                .child(format!("Preview: {}", path))
                                .into_any_element()
                        }
                    }
                    _ => {
                        // Terminal tab (default). Search matches are
                        // only meaningful for the active pane — the
                        // search state is scoped to the active
                        // terminal, so other panes always get an
                        // empty slice.
                        let term_matches: &[alacritty_terminal::term::search::Match] =
                            if is_active { search_matches } else { &[] };
                        if let Some(term) = pane.active_terminal_ref() {
                            let sb_expanded = scrollbar_expanded_pane == Some(pane_id);
                            // Per-pane hover segments: only this pane's, empty otherwise.
                            let hover_segments: Vec<(usize, usize, usize)> = hover_link
                                .filter(|h| &h.pane_id == pane_id)
                                .map(|h| h.segments.clone())
                                .unwrap_or_default();
                            if active_tab_exited {
                                render_exited_overlay(term, cursor_blink_on, bell_flash_on, &metrics, is_active, font_family, font_size, theme, pane_id, term_matches, sb_expanded, hover_segments, cx)
                            } else {
                                crate::gpui_terminal::render_alacritty_terminal(term, cursor_blink_on, &metrics, is_active, font_family, font_size, theme, term_matches, sb_expanded, hover_segments, bell_flash_on).into_any_element()
                            }
                        } else {
                            div().flex_1().flex().items_center().justify_center()
                                .bg(rgb(crate::theme::SURFACE))
                                .child(
                                    div().flex().flex_col().items_center().gap_2()
                                        .child(div().text_sm().text_color(rgb(crate::theme::TEXT_DIM)).child("Starting terminal..."))
                                )
                                .into_any_element()
                        }
                    }
                };
                (tab_strip, content)
            } else {
                (
                    div().into_any_element(),
                    div().flex_1().flex().items_center().justify_center()
                        .bg(rgb(crate::theme::SURFACE))
                        .child(
                            div().flex().flex_col().items_center().gap_2()
                                .child(div().text_sm().text_color(rgb(crate::theme::TEXT_DIM)).child("Starting terminal..."))
                        )
                        .into_any_element(),
                )
            };

            let pid = pane_id.clone();
            let pid_drop = pane_id.clone();
            let pane_has_hover_link = hover_link.map(|h| &h.pane_id == pane_id).unwrap_or(false);
            let pane_div = div()
                .id(gpui::ElementId::Name(pane_id.0.clone().into()))
                .flex_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                .bg(rgb(crate::theme::SURFACE));
            let pane_div = if pane_has_hover_link {
                pane_div.cursor_pointer()
            } else {
                pane_div
            };
            pane_div
                // Active pane indicator: only show when multiple panes exist
                // No extra border — active pane is indicated by tab strip's blue underline
                // Tab strip at top (limux style)
                .child(tab_strip)
                // Terminal content
                .child(content)
                // Drag-and-drop: visual feedback when dragging a tab over this pane
                .drag_over::<DragTab>(|style, _, _, _| {
                    style.border_t_2().border_color(rgb(crate::theme::TEXT_DIM))
                })
                // Drag-and-drop: accept a dropped tab
                .on_drop(cx.listener(move |this, drag: &DragTab, _window, cx| {
                    this.terminal_manager_mut().move_tab_to_pane(
                        &drag.source_pane,
                        drag.tab_index,
                        &pid_drop,
                    );
                    cx.notify();
                }))
                .on_mouse_down(gpui::MouseButton::Right, {
                    let pid_right = pid.clone();
                    cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                        this.terminal_manager_mut().set_active_pane(&pid_right);
                        cx.notify();
                    })
                })
                // Switch active pane on mouse_down (not click) so it happens
                // BEFORE the root div's mouse_down handler reads active_terminal().
                // This ensures text selection targets the correct pane.
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                    this.terminal_manager_mut().set_active_pane(&pid);
                    cx.notify();
                }))
                .into_any_element()
        }
        TabLayout::Horizontal { left, right, ratio } => {
            let r = *ratio;
            let handle_px = crate::theme::SPLIT_HANDLE_W;
            let usable = (avail_w - handle_px).max(0.0);
            let left_w = usable * r;
            let right_w = usable * (1.0 - r);

            let split_id = first_pane_in_layout(right)
                .map(|p| p.0.clone())
                .unwrap_or_default();
            let split_id_clone = split_id.clone();

            let left_div = div()
                .id(gpui::ElementId::Name(format!("split-l-{}", split_id).into()))
                .w(px(left_w))
                .h_full()
                .overflow_hidden()
                .child(render_layout(left, manager, active_pane_id, left_w, avail_h, cursor_blink_on, bell_flash_on, metrics, is_zoomed, renaming_tab, origin_x, origin_y, pane_bounds, font_family, font_size, theme, browser_tabs, preview_tabs, preview_search, preview_scroll_handle, preview_list_states, preview_toc, preview_selection, preview_body_bounds, preview_selection_bg, preview_selection_sink, search_matches, scrollbar_expanded_pane, hover_link, cx));

            let handle = div()
                .id(gpui::ElementId::Name(format!("resize-h-{}", split_id).into()))
                .group("resize-h")
                .w(px(handle_px))
                .flex_shrink_0()
                .cursor_col_resize()
                .child(
                    div()
                        .w(px(1.0))
                        .h_full()
                        .mx_auto()
                        .bg(rgb(crate::theme::SURFACE_RAISED))
                        .group_hover("resize-h", |d| d.w(px(2.0)).bg(rgb(crate::theme::TEXT_DIM)))
                )
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, event: &gpui::MouseDownEvent, _window, _cx| {
                    this.resize_drag = Some(ResizeDragState {
                        split_first_pane: split_id_clone.clone(),
                        is_horizontal: true,
                        start_mouse_pos: event.position.x.as_f32(),
                        start_ratio: r,
                        container_length: usable,
                    });
                }));

            let right_div = div()
                .id(gpui::ElementId::Name(format!("split-r-{}", split_id).into()))
                .w(px(right_w))
                .h_full()
                .overflow_hidden()
                .child(render_layout(right, manager, active_pane_id, right_w, avail_h, cursor_blink_on, bell_flash_on, metrics, is_zoomed, renaming_tab, origin_x + left_w + handle_px, origin_y, pane_bounds, font_family, font_size, theme, browser_tabs, preview_tabs, preview_search, preview_scroll_handle, preview_list_states, preview_toc, preview_selection, preview_body_bounds, preview_selection_bg, preview_selection_sink, search_matches, scrollbar_expanded_pane, hover_link, cx));

            div()
                .w(px(avail_w))
                .h(px(avail_h))
                .flex()
                .flex_row()
                .overflow_hidden()
                .child(left_div)
                .child(handle)
                .child(right_div)
                .into_any_element()
        }
        TabLayout::Vertical { top, bottom, ratio } => {
            let r = *ratio;
            let handle_px = crate::theme::SPLIT_HANDLE_W;
            let usable = (avail_h - handle_px).max(0.0);
            let top_h = usable * r;
            let bottom_h = usable * (1.0 - r);

            let split_id = first_pane_in_layout(bottom)
                .map(|p| p.0.clone())
                .unwrap_or_default();
            let split_id_clone = split_id.clone();

            let top_div = div()
                .id(gpui::ElementId::Name(format!("split-t-{}", split_id).into()))
                .w_full()
                .h(px(top_h))
                .overflow_hidden()
                .child(render_layout(top, manager, active_pane_id, avail_w, top_h, cursor_blink_on, bell_flash_on, metrics, is_zoomed, renaming_tab, origin_x, origin_y, pane_bounds, font_family, font_size, theme, browser_tabs, preview_tabs, preview_search, preview_scroll_handle, preview_list_states, preview_toc, preview_selection, preview_body_bounds, preview_selection_bg, preview_selection_sink, search_matches, scrollbar_expanded_pane, hover_link, cx));

            let handle = div()
                .id(gpui::ElementId::Name(format!("resize-v-{}", split_id).into()))
                .group("resize-v")
                .h(px(handle_px))
                .flex_shrink_0()
                .cursor_ns_resize()
                .child(
                    div()
                        .h(px(1.0))
                        .w_full()
                        .my_auto()
                        .bg(rgb(crate::theme::SURFACE_RAISED))
                        .group_hover("resize-v", |d| d.h(px(2.0)).bg(rgb(crate::theme::TEXT_DIM)))
                )
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, event: &gpui::MouseDownEvent, _window, _cx| {
                    this.resize_drag = Some(ResizeDragState {
                        split_first_pane: split_id_clone.clone(),
                        is_horizontal: false,
                        start_mouse_pos: event.position.y.as_f32(),
                        start_ratio: r,
                        container_length: usable,
                    });
                }));

            let bottom_div = div()
                .id(gpui::ElementId::Name(format!("split-b-{}", split_id).into()))
                .w_full()
                .h(px(bottom_h))
                .overflow_hidden()
                .child(render_layout(bottom, manager, active_pane_id, avail_w, bottom_h, cursor_blink_on, bell_flash_on, metrics, is_zoomed, renaming_tab, origin_x, origin_y + top_h + handle_px, pane_bounds, font_family, font_size, theme, browser_tabs, preview_tabs, preview_search, preview_scroll_handle, preview_list_states, preview_toc, preview_selection, preview_body_bounds, preview_selection_bg, preview_selection_sink, search_matches, scrollbar_expanded_pane, hover_link, cx));

            div()
                .w(px(avail_w))
                .h(px(avail_h))
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(top_div)
                .child(handle)
                .child(bottom_div)
                .into_any_element()
        }
    }
}

/// Render the agent launcher picker overlay
#[cfg(feature = "gpui")]
pub(crate) fn render_agent_picker(
    picker: &crate::gpui_entry::AgentPickerState,
    cx: &mut Context<GpuiShellView>,
) -> impl IntoElement {
    let mut list = div().flex().flex_col().gap_px();

    for (i, (_tool_id, label, is_wsl)) in picker.agents.iter().enumerate() {
        let is_selected = i == picker.selected_index;
        let idx = i;
        list = list.child(
            div()
                .id(gpui::ElementId::Name(format!("agent-{}", i).into()))
                .px_3()
                .py(px(6.0))
                .rounded(px(4.0))
                .flex()
                .items_center()
                .gap_2()
                .bg(if is_selected { rgb(crate::theme::SURFACE_RAISED) } else { rgb(crate::theme::SURFACE) })
                .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
                .cursor_pointer()
                .child(
                    div().text_xs().text_color(rgb(crate::theme::ACCENT)).min_w(px(16.0))
                        .child(format!("{}", i + 1))
                )
                .child(
                    div().text_sm()
                        .text_color(if is_selected { rgb(crate::theme::TEXT) } else { rgb(crate::theme::TEXT_DIM) })
                        .child(label.clone())
                )
                .when(*is_wsl, |d| {
                    d.child(
                        div().text_xs().px(px(4.0)).py(px(1.0))
                            .rounded(px(3.0)).bg(rgb(crate::theme::SURFACE_RAISED))
                            .text_color(rgb(crate::theme::TEXT_DIM)).child("WSL")
                    )
                })
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    if let Some(ref mut p) = this.agent_picker {
                        p.selected_index = idx;
                    }
                    this.execute_agent_picker();
                    cx.notify();
                }))
        );
    }

    div()
        .absolute()
        .top_0().left_0().right_0().bottom_0()
        .flex().items_center().justify_center()
        .child(
            div()
                .id("agent-picker-backdrop")
                .absolute()
                .top_0().left_0().right_0().bottom_0()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.agent_picker = None;
                    cx.notify();
                }))
        )
        .child(
            div()
                .w(px(320.0))
                .rounded(px(8.0))
                .bg(rgb(crate::theme::SURFACE))
                .border_1()
                .border_color(rgb(crate::theme::BORDER))
                .shadow_lg()
                .flex().flex_col().overflow_hidden()
                .child(
                    div().px_3().py(px(8.0))
                        .border_b_1().border_color(rgb(crate::theme::SURFACE_RAISED))
                        .child(
                            div().text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(crate::theme::TEXT))
                                .child("Launch Agent")
                        )
                )
                .child(div().p_1().child(list))
                .child(
                    div().px_3().py(px(6.0))
                        .border_t_1().border_color(rgb(crate::theme::SURFACE_RAISED))
                        .text_xs().text_color(rgb(crate::theme::TEXT_DIM))
                        .child("↑↓ navigate  1-9 quick select  Enter launch  Esc cancel")
                )
        )
}

pub(crate) fn render_ai_profile_picker(
    picker: &crate::gpui_entry::AiProfilePickerState,
    active_profile: &Option<String>,
    cx: &mut Context<GpuiShellView>,
) -> impl IntoElement {
    let mut list = div().flex().flex_col().gap_px();

    for (i, item) in picker.items.iter().enumerate() {
        let is_selected = i == picker.selected_index;
        let is_active = match item.kind {
            crate::state::AiProfileKind::None => active_profile.is_none(),
            _ => active_profile.as_deref() == Some(&item.label),
        };
        let idx = i;
        let label = item.label.clone();
        let kind = item.kind.clone();

        let mut row = div()
            .id(gpui::ElementId::Name(format!("ai-profile-{}", i).into()))
            .px_3()
            .py(px(6.0))
            .rounded(px(4.0))
            .flex()
            .items_center()
            .gap_2()
            .bg(if is_selected { rgb(crate::theme::SURFACE_RAISED) } else { rgb(crate::theme::SURFACE) })
            .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
            .cursor_pointer()
            .child(
                div().text_xs().text_color(rgb(crate::theme::ACCENT)).min_w(px(16.0))
                    .child(format!("{}", i + 1))
            )
            .child(
                div().text_sm()
                    .text_color(if is_selected { rgb(crate::theme::TEXT) } else { rgb(crate::theme::TEXT_DIM) })
                    .child(label)
            );

        if is_active {
            row = row.child(
                div().text_xs().px(px(4.0)).py(px(1.0))
                    .rounded(px(3.0)).bg(rgb(crate::theme::ACCENT))
                    .text_color(rgb(crate::theme::SURFACE)).child("active")
            );
        } else if kind == crate::state::AiProfileKind::PresetNeedsKey {
            row = row.child(
                div().text_xs().px(px(4.0)).py(px(1.0))
                    .rounded(px(3.0)).bg(rgb(crate::theme::WARNING))
                    .text_color(rgb(crate::theme::SURFACE)).child("key needed")
            );
        }

        list = list.child(
            row.on_click(cx.listener(move |this, _event, window, cx| {
                if let Some(ref mut p) = this.ai_profile_picker {
                    p.selected_index = idx;
                }
                this.execute_ai_profile_picker(window, cx);
                cx.notify();
            }))
        );
    }

    div()
        .absolute()
        .top_0().left_0().right_0().bottom_0()
        .flex().items_center().justify_center()
        .child(
            div()
                .id("ai-profile-picker-backdrop")
                .absolute()
                .top_0().left_0().right_0().bottom_0()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.ai_profile_picker = None;
                    cx.notify();
                }))
        )
        .child(
            div()
                .w(px(320.0))
                .rounded(px(8.0))
                .bg(rgb(crate::theme::SURFACE))
                .border_1()
                .border_color(rgb(crate::theme::BORDER))
                .shadow_lg()
                .flex().flex_col().overflow_hidden()
                .child(
                    div().px_3().py(px(8.0))
                        .border_b_1().border_color(rgb(crate::theme::SURFACE_RAISED))
                        .child(
                            div().text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(crate::theme::TEXT))
                                .child("AI Profile")
                        )
                )
                .child(div().p_1().child(list))
                .child(
                    div().px_3().py(px(6.0))
                        .border_t_1().border_color(rgb(crate::theme::SURFACE_RAISED))
                        .text_xs().text_color(rgb(crate::theme::TEXT_DIM))
                        .child("↑↓ navigate  1-9 quick select  Enter select  Esc cancel")
                )
        )
}

/// Render the API key input overlay for preset activation.
#[cfg(feature = "gpui")]
pub(crate) fn render_api_key_input(
    input: &crate::state::ApiKeyInputState,
    cx: &mut Context<GpuiShellView>,
) -> impl IntoElement {
    // Use the container as the backdrop (click to close)
    div()
        .id("api-key-input-backdrop")
        .absolute()
        .top_0().left_0().right_0().bottom_0()
        .flex().items_center().justify_center()
        .on_click(cx.listener(|this, _event, _window, cx| {
            this.api_key_input = None;
            cx.notify();
        }))
        .child(
            // Dialog container - stops click propagation
            div()
                .id("api-key-dialog")
                .w(px(400.0))
                .rounded(px(8.0))
                .bg(rgb(crate::theme::SURFACE))
                .border_1()
                .border_color(rgb(crate::theme::BORDER))
                .shadow_lg()
                .flex().flex_col().overflow_hidden()
                .on_click(|_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div().px_3().py(px(8.0))
                        .border_b_1().border_color(rgb(crate::theme::SURFACE_RAISED))
                        .child(
                            div().text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(crate::theme::TEXT))
                                .child(format!("API Key for {}", input.preset_name))
                        )
                )
                .child(
                    div().p_3().flex().flex_col().gap_2()
                        .child(
                            div().text_xs().text_color(rgb(crate::theme::TEXT_DIM))
                                .child(format!("Enter your API key (e.g. {})", input.key_hint))
                        )
                        .child({
                            use gpui_component::Sizable;
                            gpui_component::input::Input::new(&input.input)
                                .small()
                                .cleanable(false)
                                .appearance(true)
                        })
                )
                .child(
                    div().px_3().py(px(6.0))
                        .border_t_1().border_color(rgb(crate::theme::SURFACE_RAISED))
                        .flex().justify_end().gap_2()
                        .child(
                            div()
                                .id("api-key-cancel")
                                .text_xs().px(px(8.0)).py(px(4.0))
                                .rounded(px(4.0))
                                .bg(rgb(crate::theme::SURFACE_RAISED))
                                .text_color(rgb(crate::theme::TEXT_DIM))
                                .cursor_pointer()
                                .child("Cancel")
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.api_key_input = None;
                                    cx.notify();
                                }))
                        )
                        .child(
                            div()
                                .id("api-key-save")
                                .text_xs().px(px(8.0)).py(px(4.0))
                                .rounded(px(4.0))
                                .bg(rgb(crate::theme::ACCENT))
                                .text_color(rgb(crate::theme::SURFACE))
                                .cursor_pointer()
                                .child("Save & Activate")
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.execute_api_key_input(cx);
                                    cx.notify();
                                }))
                        )
                )
        )
}

/// Render the template picker overlay for "Apply Layout"
#[cfg(feature = "gpui")]
pub(crate) fn render_template_picker(
    picker: &crate::gpui_entry::TemplatePickerState,
    cx: &mut Context<GpuiShellView>,
) -> impl IntoElement {
    let mut list = div().flex().flex_col().gap_px();

    for (i, template) in picker.templates.iter().enumerate() {
        let is_selected = i == picker.selected_index;
        let idx = i;
        let pane_count = template.layout.pane_count();
        let is_custom = !template.builtin;
        let del_idx = i;
        let mut row = div()
            .id(gpui::ElementId::Name(format!("tpl-{}", i).into()))
            .group("tpl-row")
            .px_3()
            .py(px(6.0))
            .rounded(px(4.0))
            .flex()
            .items_center()
            .gap_2()
            .bg(if is_selected { rgb(crate::theme::SURFACE_RAISED) } else { rgb(crate::theme::SURFACE) })
            .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
            .cursor_pointer()
            .child(
                div().text_xs().text_color(rgb(crate::theme::ACCENT)).min_w(px(16.0))
                    .child(format!("{}", i + 1))
            )
            .child(
                div().flex().flex_col().flex_1().overflow_hidden()
                    .child(
                        div().text_sm().flex().gap_1().items_center()
                            .text_color(if is_selected { rgb(crate::theme::TEXT) } else { rgb(crate::theme::TEXT_DIM) })
                            .child(template.name.clone())
                            .when(is_custom, |d| {
                                d.child(
                                    div().text_xs().text_color(rgb(crate::theme::TEXT_DIM)).child("(custom)")
                                )
                            })
                    )
                    .child(
                        div().text_xs().text_color(rgb(crate::theme::TEXT_DIM))
                            .child(format!("{} — {} panes", template.description, pane_count))
                    )
            )
            .on_click(cx.listener(move |this, _event, _window, cx| {
                if let Some(ref mut p) = this.template_picker {
                    p.selected_index = idx;
                }
                this.execute_template_picker();
                cx.notify();
            }));

        // Delete button for custom templates — hidden by default, visible on hover
        if is_custom {
            row = row.child(
                div()
                    .id(gpui::ElementId::Name(format!("tpl-del-{}", i).into()))
                    .px(px(4.0))
                    .py(px(2.0))
                    .rounded(px(3.0))
                    .text_xs()
                    .text_color(rgb(crate::theme::SURFACE)) // invisible by default (matches bg)
                    .group_hover("tpl-row", |d| d.text_color(rgb(crate::theme::TEXT_DIM))) // visible on row hover
                    .hover(|d| d.bg(rgb(crate::theme::BORDER)).text_color(rgb(crate::theme::DANGER))) // red on button hover
                    .child("✕")
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if let Some(ref mut p) = this.template_picker {
                            p.selected_index = del_idx;
                        }
                        this.delete_selected_template();
                        cx.notify();
                    }))
            );
        }

        list = list.child(row);
    }

    div()
        .absolute()
        .top_0().left_0().right_0().bottom_0()
        .flex().items_center().justify_center()
        // Dismiss backdrop — clicking outside the picker closes it
        .child(
            div()
                .id("template-picker-backdrop")
                .absolute()
                .top_0().left_0().right_0().bottom_0()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.template_picker = None;
                    cx.notify();
                }))
        )
        .child(
            div()
                .w(px(360.0))
                .rounded(px(8.0))
                .bg(rgb(crate::theme::SURFACE))
                .border_1()
                .border_color(rgb(crate::theme::BORDER))
                .shadow_lg()
                .flex().flex_col().overflow_hidden()
                // Header
                .child(
                    div().px_3().py(px(8.0))
                        .border_b_1().border_color(rgb(crate::theme::SURFACE_RAISED))
                        .child(
                            div().text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(crate::theme::TEXT))
                                .child("Apply Layout Template")
                        )
                )
                // Template list
                .child(div().p_1().child(list))
                // Footer
                .child(
                    div().px_3().py(px(6.0))
                        .border_t_1().border_color(rgb(crate::theme::SURFACE_RAISED))
                        .text_xs().text_color(rgb(crate::theme::TEXT_DIM))
                        .child("↑↓ navigate  1-9 select  Enter apply  Del remove  Esc cancel")
                )
        )
}

/// Render the pane picker overlay for "Send to Pane"
#[cfg(feature = "gpui")]
pub(crate) fn render_pane_picker(
    picker: &crate::gpui_entry::PanePickerState,
    cx: &mut Context<GpuiShellView>,
) -> impl IntoElement {
    let text_preview = if picker.text.len() > 40 {
        format!("{}...", &picker.text[..40])
    } else {
        picker.text.clone()
    };

    let mut list = div()
        .flex()
        .flex_col()
        .gap_px();

    for (i, (_pid, title)) in picker.targets.iter().enumerate() {
        let is_selected = i == picker.selected_index;
        let idx = i;
        list = list.child(
            div()
                .id(gpui::ElementId::Name(format!("picker-{}", i).into()))
                .px_3()
                .py(px(5.0))
                .rounded(px(4.0))
                .flex()
                .items_center()
                .gap_2()
                .bg(if is_selected { rgb(crate::theme::SURFACE_RAISED) } else { rgb(crate::theme::SURFACE) })
                .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
                .cursor_pointer()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(crate::theme::ACCENT))
                        .min_w(px(16.0))
                        .child(format!("{}", i + 1))
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(if is_selected { rgb(crate::theme::TEXT) } else { rgb(crate::theme::TEXT_DIM) })
                        .child(title.clone())
                )
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    if let Some(ref mut p) = this.pane_picker {
                        p.selected_index = idx;
                    }
                    this.execute_pane_picker();
                    cx.notify();
                }))
        );
    }

    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .flex()
        .items_center()
        .justify_center()
        // Dismiss backdrop
        .child(
            div()
                .id("pane-picker-backdrop")
                .absolute()
                .top_0().left_0().right_0().bottom_0()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.pane_picker = None;
                    cx.notify();
                }))
        )
        .child(
            div()
                .w(px(320.0))
                .rounded(px(8.0))
                .bg(rgb(crate::theme::SURFACE))
                .border_1()
                .border_color(rgb(crate::theme::BORDER))
                .shadow_lg()
                .flex()
                .flex_col()
                .overflow_hidden()
                // Header
                .child(
                    div()
                        .px_3()
                        .py(px(8.0))
                        .border_b_1()
                        .border_color(rgb(crate::theme::SURFACE_RAISED))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(crate::theme::TEXT))
                                .child("Send to Pane")
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(crate::theme::TEXT_DIM))
                                .child(text_preview)
                        )
                )
                // Pane list
                .child(
                    div().p_1().child(list)
                )
                // Footer
                .child(
                    div()
                        .px_3()
                        .py(px(6.0))
                        .border_t_1()
                        .border_color(rgb(crate::theme::SURFACE_RAISED))
                        .text_xs()
                        .text_color(rgb(crate::theme::TEXT_DIM))
                        .child("↑↓ navigate  1-9 quick select  Enter send  Esc cancel")
                )
        )
}

/// Render the "Process exited" overlay with Restart/Close buttons.
/// Extracted as a separate function to reduce render_layout's stack frame size
/// (prevents stack overflow on Windows where default stack is 1MB).
#[cfg(feature = "gpui")]
fn render_exited_overlay(
    term: &amux_platform::terminal::alacritty_view::AlacrittyTerminal,
    cursor_blink_on: bool,
    bell_flash_on: bool,
    metrics: &crate::gpui_terminal::CellMetrics,
    is_active: bool,
    font_family: &str,
    font_size: f32,
    theme: &crate::gpui_terminal::TerminalTheme,
    pane_id: &amux_platform::terminal::manager::PaneId,
    search_matches: &[alacritty_terminal::term::search::Match],
    scrollbar_expanded: bool,
    hover_link_segments: Vec<(usize, usize, usize)>,
    cx: &mut Context<GpuiShellView>,
) -> gpui::AnyElement {
    let terminal_content = crate::gpui_terminal::render_alacritty_terminal(
        term, cursor_blink_on, metrics, is_active, font_family, font_size, theme, search_matches,
        scrollbar_expanded, hover_link_segments, bell_flash_on,
    );
    let pid_restart = pane_id.clone();
    let pid_close = pane_id.clone();

    div()
        .relative()
        .flex_1()
        .child(terminal_content)
        .child(
            div()
                .absolute()
                .top_0().left_0().right_0().bottom_0()
                .bg(gpui::Hsla { h: 0.0, s: 0.0, l: 0.0, a: 0.6 })
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_3()
                        .child(
                            div().text_sm().text_color(rgb(crate::theme::TEXT_DIM)).child("Process exited")
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    div()
                                        .id(gpui::ElementId::Name(format!("{}-restart", pane_id.0).into()))
                                        .px_3().py_1().rounded(px(4.0))
                                        .bg(rgb(crate::theme::SURFACE_RAISED))
                                        .hover(|d| d.bg(rgb(crate::theme::BORDER)))
                                        .cursor_pointer()
                                        .text_sm().text_color(rgb(crate::theme::SUCCESS))
                                        .child("↻ Restart")
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.restart_terminal_in_pane(&pid_restart);
                                            cx.notify();
                                        }))
                                )
                                .child(
                                    div()
                                        .id(gpui::ElementId::Name(format!("{}-close-exited", pane_id.0).into()))
                                        .px_3().py_1().rounded(px(4.0))
                                        .bg(rgb(crate::theme::SURFACE_RAISED))
                                        .hover(|d| d.bg(rgb(crate::theme::BORDER)))
                                        .cursor_pointer()
                                        .text_sm().text_color(rgb(crate::theme::DANGER))
                                        .child("✕ Close")
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.terminal_manager_mut().set_active_pane(&pid_close);
                                            this.cleanup_pane_tab_entries();
                                            this.terminal_manager_mut().close_active_pane();
                                            cx.notify();
                                        }))
                                )
                        )
                )
        )
        .into_any_element()
}

/// Render the new-tab dropdown picker (from the `+▾` button)
#[cfg(feature = "gpui")]
pub(crate) fn render_new_tab_picker(
    picker: &NewTabPickerState,
    cx: &mut Context<GpuiShellView>,
) -> impl IntoElement {
    let mut list = div().flex().flex_col();

    for (i, item) in picker.items.iter().enumerate() {
        let is_selected = i == picker.selected_index;
        let idx = i;
        list = list
            .child(
                div()
                    .id(gpui::ElementId::Name(format!("newtab-{}", i).into()))
                    .px_2()
                    .py(px(5.0))
                    .mx_1()
                    .rounded(px(4.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .bg(if is_selected { rgb(crate::theme::SURFACE_RAISED) } else { rgb(crate::theme::SURFACE) })
                    .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
                    .cursor_pointer()
                    .child(
                        div().text_xs().text_color(rgb(crate::theme::ACCENT)).min_w(px(18.0))
                            .child(item.icon)
                    )
                    .child(
                        div().text_sm()
                            .text_color(if is_selected { rgb(crate::theme::TEXT) } else { rgb(crate::theme::TEXT_DIM) })
                            .child(item.label.clone())
                    )
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        if let Some(ref mut p) = this.new_tab_picker {
                            p.selected_index = idx;
                        }
                        this.execute_new_tab_picker(window, cx);
                        cx.notify();
                    }))
            )
            .when(item.separator_after, |d| {
                d.child(
                    div().mx_2().my(px(3.0)).h(px(1.0)).bg(rgb(crate::theme::SURFACE_RAISED))
                )
            });
    }

    let anchor = picker.anchor;
    div()
        .absolute()
        .top_0().left_0().right_0().bottom_0()
        // Backdrop: click to dismiss
        .child(
            div()
                .id("newtab-picker-backdrop")
                .absolute()
                .top_0().left_0().right_0().bottom_0()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.new_tab_picker = None;
                    cx.notify();
                }))
        )
        // Dropdown panel anchored below the button
        .child(
            div()
                .absolute()
                .top(anchor.y)
                .left(anchor.x)
                .w(px(220.0))
                .rounded(px(6.0))
                .bg(rgb(crate::theme::SURFACE))
                .border_1()
                .border_color(rgb(crate::theme::BORDER))
                .shadow_lg()
                .flex().flex_col().overflow_hidden()
                .py_1()
                .child(list)
        )
}

/// Render the help overlay (F1) showing keyboard shortcuts and commands.
#[cfg(feature = "gpui")]
pub(crate) fn render_help_overlay(cx: &mut Context<GpuiShellView>) -> impl IntoElement {
    use crate::gpui_keyboard_shortcuts::{all_shortcuts, ShortcutCategory};

    let shortcuts = all_shortcuts();

    // Build shortcut sections by category
    let categories = [
        (ShortcutCategory::App, "General"),
        (ShortcutCategory::Workspace, "Workspace"),
        (ShortcutCategory::Pane, "Pane & Tabs"),
        (ShortcutCategory::Terminal, "Terminal"),
        (ShortcutCategory::Find, "Search"),
        (ShortcutCategory::Browser, "Browser"),
    ];

    let mut sections = Vec::new();
    for (cat, title) in &categories {
        let items: Vec<_> = shortcuts.iter().filter(|s| s.category == *cat).collect();
        if items.is_empty() { continue; }
        let rows: Vec<_> = items.iter().map(|s| {
            let label = s.display_label();
            div().flex().justify_between().gap_4()
                .child(
                    div().text_sm().text_color(rgb(crate::theme::TEXT))
                        .child(s.description.clone())
                )
                .child(
                    div().text_sm().text_color(rgb(crate::theme::ACCENT))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(label)
                )
        }).collect();
        sections.push(
            div().flex().flex_col().gap_1()
                .child(
                    div().text_xs().font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(crate::theme::WARNING))
                        .child(title.to_string())
                )
                .children(rows)
        );
    }

    // Command palette commands (grouped)
    let palette_cmds = amux_ui::commands::palette_command_catalog();
    let palette_categories = [
        (amux_ui::commands::PaletteCategory::General, "Commands: General"),
        (amux_ui::commands::PaletteCategory::Workspace, "Commands: Workspace"),
        (amux_ui::commands::PaletteCategory::Pane, "Commands: Pane"),
        (amux_ui::commands::PaletteCategory::Agent, "Commands: Agent"),
        (amux_ui::commands::PaletteCategory::File, "Commands: File"),
    ];
    for (cat, title) in &palette_categories {
        let items: Vec<_> = palette_cmds.iter().filter(|c| c.category == *cat).collect();
        if items.is_empty() { continue; }
        let rows: Vec<_> = items.iter().map(|c| {
            div().flex().justify_between().gap_4()
                .child(
                    div().text_sm().text_color(rgb(crate::theme::TEXT))
                        .child(format!("{} — {}", c.label, c.description))
                )
                .child(
                    div().text_sm().text_color(rgb(crate::theme::TEXT_DIM))
                        .child(c.command.clone())
                )
        }).collect();
        sections.push(
            div().flex().flex_col().gap_1()
                .child(
                    div().text_xs().font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(crate::theme::INFO))
                        .child(title.to_string())
                )
                .children(rows)
        );
    }

    // Backdrop
    div()
        .id("help-overlay-backdrop")
        .absolute().top_0().left_0().right_0().bottom_0()
        .bg(gpui::rgba(0x000000aa))
        .flex().items_center().justify_center()
        .on_click(cx.listener(|this, _event, _window, cx| {
            this.show_help = false;
            cx.notify();
        }))
        .child(
            // Modal
            div()
                .id("help-overlay-modal")
                .w(px(560.0))
                .max_h(px(600.0))
                .rounded(px(10.0))
                .bg(rgb(crate::theme::SURFACE_DIM))
                .border_1()
                .border_color(rgb(crate::theme::BORDER))
                .shadow_lg()
                .flex().flex_col().overflow_hidden()
                .on_click(|_event, _window, cx| { cx.stop_propagation(); })
                // Header
                .child(
                    div().px_4().py_3()
                        .border_b_1().border_color(rgb(crate::theme::BORDER))
                        .flex().justify_between().items_center()
                        .child(
                            div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(crate::theme::TEXT))
                                .child("Keyboard Shortcuts & Commands")
                        )
                        .child(
                            div().text_xs().text_color(rgb(crate::theme::TEXT_DIM))
                                .child(if cfg!(target_os = "macos") {
                                    "Cmd+Shift+H / About Amux / Esc to close"
                                } else {
                                    "Ctrl+Shift+H / About Amux / Esc to close"
                                })
                        )
                )
                // Scrollable content
                .child(
                    div()
                        .id("help-overlay-content")
                        .p_4().flex().flex_col().gap_3()
                        .overflow_y_scroll()
                        .flex_1()
                        .children(sections)
                )
        )
}
