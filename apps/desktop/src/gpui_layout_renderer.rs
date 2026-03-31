//! Layout rendering — split panes, tab strips, context menu
//!
//! Extracted from gpui_entry.rs to keep rendering logic separate from
//! application state management.

#[cfg(feature = "gpui")]
use gpui::{rgb, px, div, prelude::*, Context, IntoElement, Styled};

#[cfg(feature = "gpui")]
use amux_platform::terminal::manager::{TerminalManager, SplitDirection};

#[cfg(feature = "gpui")]
use crate::gpui_entry::{GpuiShellView, ContextMenuItem, ResizeDragState, DragTab};

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
        .bg(rgb(0x282a2e))
        .border_1()
        .border_color(rgb(0x373b41))
        .shadow_lg()
        .py_1()
        .flex()
        .flex_col();

    for item in items {
        let label = item.label;
        let enabled = item.enabled;

        let text_color = if enabled { rgb(0xc5c8c6) } else { rgb(0x4a4d4e) };

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
            .when(enabled, |d| d.hover(|d| d.bg(rgb(0x373b41))))
            .when(enabled, |d| {
                d.on_click(cx.listener(move |this, _event, _window, cx| {
                    this.execute_context_menu_action(label, cx);
                }))
            })
            .child(left)
            .children(item.shortcut.map(|kb| {
                div()
                    .text_xs()
                    .text_color(rgb(0x696d70))
                    .child(kb)
            }));

        menu = menu.child(row);

        if item.separator_after {
            menu = menu.child(
                div()
                    .mx_2()
                    .my_1()
                    .h(px(1.0))
                    .bg(rgb(0x373b41)),
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
    metrics: &crate::gpui_terminal::CellMetrics,
    is_zoomed: bool,
    renaming_tab: &Option<(String, usize, String)>,
    origin_x: f32,
    origin_y: f32,
    pane_bounds: &mut std::collections::HashMap<String, (f32, f32, f32, f32)>,
    font_family: &str,
    font_size: f32,
    cx: &mut Context<GpuiShellView>,
) -> gpui::AnyElement {
    use amux_platform::terminal::manager::{PaneId, TabLayout};

    match layout {
        TabLayout::Single(pane_id) => {
            // Record this pane's screen bounds for mouse hit-testing.
            // Tab strip (28px) is at the top; terminal content starts below it.
            let tab_strip_h = 28.0_f32;
            pane_bounds.insert(pane_id.0.clone(), (origin_x, origin_y + tab_strip_h, avail_w, (avail_h - tab_strip_h).max(0.0)));
            let is_active = active_pane_id == Some(pane_id);
            let has_multiple_panes = manager.total_panes() > 1;

            // Build per-pane tab strip + terminal content
            // get_pane may return None if layout references a pane that doesn't
            // exist in the panes map (e.g., corrupted saved layout). In that case,
            // we skip the pane and render a placeholder.
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
                    .children(tabs.into_iter().map(|(idx, title, is_tab_active, has_activity, tab_exited)| {
                        let pid_click = pid_for_tabs.clone();
                        let pid_close_tab = pid_for_tabs.clone();
                        let pid_drag = pid_for_tabs.clone();
                        let can_close_tab = tab_count > 1;
                        let drag_title = title.clone();
                        div()
                            .id(gpui::ElementId::Name(
                                format!("{}-tab-{}", pid_for_tabs.0, idx).into(),
                            ))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(4.0))
                            .min_w(px(60.0))
                            .max_w(px(180.0))
                            .flex_shrink()
                            .overflow_hidden()
                            .px_3()
                            .py(px(4.0))
                            .text_xs()
                            .cursor_grab()
                            .text_color(if is_tab_active { rgb(0xcdd6f4) } else { rgb(0x7f849c) })
                            .bg(if is_tab_active { rgb(0x1e1e2e) } else { rgb(0x11111b) })
                            .border_b_2()
                            .border_color(if is_tab_active { rgb(0x89b4fa) } else { rgb(0x11111b) })
                            .when(is_tab_active, |d| d.font_weight(gpui::FontWeight::MEDIUM))
                            .hover(|d| d.bg(rgb(0x252530)))
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
                            .on_click({
                                let pid_rename = pid_click.clone();
                                let rename_title = title.clone();
                                cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                                    if event.click_count() >= 2 {
                                        // Double-click: start inline rename
                                        this.renaming_tab = Some((pid_rename.0.clone(), idx, String::new()));
                                    } else {
                                        // Single click: switch tab
                                        this.terminal_manager_mut().set_active_pane(&pid_click);
                                        this.terminal_manager_mut().set_active_tab_in_pane(idx);
                                    }
                                    cx.notify();
                                })
                            })
                            .child({
                                let is_tab_renaming = renaming_tab.as_ref()
                                    .map(|(p, i, _): &(String, usize, String)| p == &pid_for_tabs.0 && *i == idx)
                                    .unwrap_or(false);
                                if is_tab_renaming {
                                    let rename_text = renaming_tab.as_ref()
                                        .map(|(_, _, t): &(String, usize, String)| t.clone())
                                        .unwrap_or_default();
                                    div()
                                        .flex_1()
                                        .overflow_hidden()
                                        .text_sm()
                                        .text_color(rgb(0xcdd6f4))
                                        .bg(rgb(0x313244))
                                        .rounded(px(2.0))
                                        .border_1()
                                        .border_color(rgb(0x89b4fa))
                                        .px_1()
                                        .child(if rename_text.is_empty() { "▎".to_string() } else { format!("{}▎", rename_text) })
                                        .into_any_element()
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
                                                .bg(rgb(0xf38ba8)).flex_shrink_0() // red
                                        );
                                    } else if has_activity && !is_tab_active {
                                        tab_content = tab_content.child(
                                            div().w(px(6.0)).h(px(6.0)).rounded(px(3.0))
                                                .bg(rgb(0xa6e3a1)).flex_shrink_0() // green
                                        );
                                    }
                                    tab_content = tab_content.child(
                                        div().whitespace_nowrap().child(title)
                                    );
                                    tab_content.into_any_element()
                                }
                            })
                            .when(can_close_tab, |d| {
                                d.child(
                                    div()
                                        .id(gpui::ElementId::Name(
                                            format!("{}-tab-{}-close", pid_close_tab.0, idx).into(),
                                        ))
                                        .px(px(2.0))
                                        .rounded(px(3.0))
                                        .text_color(rgb(0x585b70))
                                        .hover(|d| d.bg(rgb(0x45475a)).text_color(rgb(0xf38ba8)))
                                        .child("×")
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.terminal_manager_mut().set_active_pane(&pid_close_tab);
                                            if let Some(pane) = this.terminal_manager_mut().get_pane_mut(&pid_close_tab) {
                                                pane.close_tab(idx);
                                            }
                                            cx.notify();
                                        }))
                                )
                            })
                    }));

                // Right side: action buttons
                let pid_new = pane_id.clone();
                let pid_sr = pane_id.clone();
                let pid_sd = pane_id.clone();
                let pid_close = pane_id.clone();

                let actions_row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(2.0))
                    .px_2()
                    // + New Tab
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("{}-btn-add", pane_id.0).into()))
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(3.0))
                            .text_xs()
                            .text_color(rgb(0x6c7086))
                            .hover(|d| d.bg(rgb(0x313244)).text_color(rgb(0xcdd6f4)))
                            .child("+")
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.terminal_manager_mut().set_active_pane(&pid_new);
                                let env = this.capture_active_env();
                                this.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                                this.spawn_with_captured_env(&env);
                                cx.notify();
                            })),
                    )
                    // Split Right
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("{}-btn-sr", pane_id.0).into()))
                            .px(px(5.0))
                            .py(px(2.0))
                            .rounded(px(3.0))
                            .text_xs()
                            .text_color(rgb(0x6c7086))
                            .hover(|d| d.bg(rgb(0x313244)).text_color(rgb(0xcdd6f4)))
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
                            .px(px(5.0))
                            .py(px(2.0))
                            .rounded(px(3.0))
                            .text_xs()
                            .text_color(rgb(0x6c7086))
                            .hover(|d| d.bg(rgb(0x313244)).text_color(rgb(0xcdd6f4)))
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
                                .px(px(5.0))
                                .py(px(2.0))
                                .rounded(px(3.0))
                                .text_xs()
                                .text_color(rgb(0x6c7086))
                                .hover(|d| d.bg(rgb(0x313244)).text_color(rgb(0xcdd6f4)))
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
                            .px(px(5.0))
                            .py(px(2.0))
                            .rounded(px(3.0))
                            .text_xs()
                            .text_color(if has_multiple_panes { rgb(0x6c7086) } else { rgb(0x313244) })
                            .when(has_multiple_panes, |d| {
                                d.hover(|d| d.bg(rgb(0x45475a)).text_color(rgb(0xf38ba8)))
                            })
                            .child("✕")
                            .when(has_multiple_panes, |d| {
                                d.on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.terminal_manager_mut().set_active_pane(&pid_close);
                                    this.terminal_manager_mut().close_active_pane();
                                    cx.notify();
                                }))
                            }),
                    );

                // Combine into tab strip (relative container for zoom indicator)
                let tab_strip = div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .bg(rgb(0x11111b))
                    .border_b_1()
                    .border_color(rgb(0x252530))
                    .child(tabs_row)
                    .child(actions_row)
                    // Zoom indicator: absolutely centered over the entire tab strip
                    .when(is_zoomed, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left_0()
                                .right_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                // Don't block clicks on tabs/buttons underneath
                                .child(
                                    div()
                                        .px_2()
                                        .py(px(2.0))
                                        .rounded(px(8.0))
                                        .bg(rgb(0x1e1e2e))
                                        .border_1()
                                        .border_color(rgb(0x45475a))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(5.0))
                                        .child(
                                            div()
                                                .w(px(6.0))
                                                .h(px(6.0))
                                                .rounded(px(3.0))
                                                .bg(rgb(0xa6e3a1)) // green for "zoomed" state
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(0xa6adc8))
                                                .child("ZOOMED")
                                        )
                                )
                        )
                    })
                    .into_any_element();

                let active_tab_exited = pane.active_tab_exited();
                let content = if let Some(term) = pane.active_terminal_ref() {
                    if active_tab_exited {
                        render_exited_overlay(term, cursor_blink_on, &metrics, is_active, font_family, font_size, pane_id, cx)
                    } else {
                        crate::gpui_terminal::render_alacritty_terminal(term, cursor_blink_on, &metrics, is_active, font_family, font_size).into_any_element()
                    }
                } else {
                    div().flex_1().flex().items_center().justify_center()
                        .bg(rgb(0x1d1f21))
                        .child(
                            div().flex().flex_col().items_center().gap_2()
                                .child(div().text_sm().text_color(rgb(0x585b70)).child("Starting terminal..."))
                        )
                        .into_any_element()
                };
                (tab_strip, content)
            } else {
                (
                    div().into_any_element(),
                    div().flex_1().bg(rgb(0x1e1e2e)).child("Empty pane").into_any_element(),
                )
            };

            let pid = pane_id.clone();
            let pid_drop = pane_id.clone();
            div()
                .id(gpui::ElementId::Name(pane_id.0.clone().into()))
                .flex_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                .bg(rgb(0x1d1f21))
                // Active pane indicator: only show when multiple panes exist
                // No extra border — active pane is indicated by tab strip's blue underline
                // Tab strip at top (limux style)
                .child(tab_strip)
                // Terminal content
                .child(content)
                // Drag-and-drop: visual feedback when dragging a tab over this pane
                .drag_over::<DragTab>(|style, _, _, _| {
                    style.border_t_2().border_color(rgb(0x585b70))
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
            let handle_px = 6.0_f32;
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
                .child(render_layout(left, manager, active_pane_id, left_w, avail_h, cursor_blink_on, metrics, is_zoomed, renaming_tab, origin_x, origin_y, pane_bounds, font_family, font_size, cx));

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
                        .bg(rgb(0x252530))
                        .group_hover("resize-h", |d| d.w(px(2.0)).bg(rgb(0x585b70)))
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
                .child(render_layout(right, manager, active_pane_id, right_w, avail_h, cursor_blink_on, metrics, is_zoomed, renaming_tab, origin_x + left_w + handle_px, origin_y, pane_bounds, font_family, font_size, cx));

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
            let handle_px = 6.0_f32;
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
                .child(render_layout(top, manager, active_pane_id, avail_w, top_h, cursor_blink_on, metrics, is_zoomed, renaming_tab, origin_x, origin_y, pane_bounds, font_family, font_size, cx));

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
                        .bg(rgb(0x252530))
                        .group_hover("resize-v", |d| d.h(px(2.0)).bg(rgb(0x585b70)))
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
                .child(render_layout(bottom, manager, active_pane_id, avail_w, bottom_h, cursor_blink_on, metrics, is_zoomed, renaming_tab, origin_x, origin_y + top_h + handle_px, pane_bounds, font_family, font_size, cx));

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

/// Render the "Process exited" overlay with Restart/Close buttons.
/// Extracted as a separate function to reduce render_layout's stack frame size
/// (prevents stack overflow on Windows where default stack is 1MB).
#[cfg(feature = "gpui")]
fn render_exited_overlay(
    term: &amux_platform::terminal::alacritty_view::AlacrittyTerminal,
    cursor_blink_on: bool,
    metrics: &crate::gpui_terminal::CellMetrics,
    is_active: bool,
    font_family: &str,
    font_size: f32,
    pane_id: &amux_platform::terminal::manager::PaneId,
    cx: &mut Context<GpuiShellView>,
) -> gpui::AnyElement {
    let terminal_content = crate::gpui_terminal::render_alacritty_terminal(
        term, cursor_blink_on, metrics, is_active, font_family, font_size,
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
                            div().text_sm().text_color(rgb(0x6c7086)).child("Process exited")
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    div()
                                        .id(gpui::ElementId::Name(format!("{}-restart", pane_id.0).into()))
                                        .px_3().py_1().rounded(px(4.0))
                                        .bg(rgb(0x313244))
                                        .hover(|d| d.bg(rgb(0x45475a)))
                                        .cursor_pointer()
                                        .text_sm().text_color(rgb(0xa6e3a1))
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
                                        .bg(rgb(0x313244))
                                        .hover(|d| d.bg(rgb(0x45475a)))
                                        .cursor_pointer()
                                        .text_sm().text_color(rgb(0xf38ba8))
                                        .child("✕ Close")
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.terminal_manager_mut().set_active_pane(&pid_close);
                                            this.terminal_manager_mut().close_active_pane();
                                            cx.notify();
                                        }))
                                )
                        )
                )
        )
        .into_any_element()
}
