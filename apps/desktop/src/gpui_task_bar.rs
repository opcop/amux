//! Task Bar UI — renders the compare task progress bar above terminal panes.

#[cfg(feature = "gpui")]
use gpui::{rgb, px, div, prelude::*, Styled, FontWeight, IntoElement};

#[cfg(feature = "gpui")]
use crate::gpui_compare_task::{CompareTask, ComparePhase, CompareAgentStatus};
use crate::gpui_entry::GpuiShellView;

/// Render the Task Bar. Returns None if no active task.
#[cfg(feature = "gpui")]
pub fn render_task_bar(
    task: &CompareTask,
    cx: &mut gpui::Context<GpuiShellView>,
) -> gpui::AnyElement {
    match task.phase {
        ComparePhase::Running => render_running(task, cx),
        ComparePhase::Review => render_review(task, cx),
        _ => div().into_any_element(),
    }
}

/// Running phase: show agent progress bars
#[cfg(feature = "gpui")]
fn render_running(
    task: &CompareTask,
    cx: &mut gpui::Context<GpuiShellView>,
) -> gpui::AnyElement {
    let prompt_preview: String = if task.prompt.len() > 60 {
        format!("{}...", &task.prompt[..57])
    } else {
        task.prompt.clone()
    };

    let all_done = task.agents.iter().all(|a| {
        matches!(a.status, CompareAgentStatus::Done | CompareAgentStatus::Error | CompareAgentStatus::Waiting)
    });

    div()
        .id("task-bar")
        .w_full()
        .flex_shrink_0()
        .bg(rgb(0x1e1e2e))
        .border_b_1()
        .border_color(rgb(0x313244))
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .gap_1()
        // Header line
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x89b4fa))
                        .child("⚡ COMPARE")
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x7f849c))
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(prompt_preview)
                )
                .child(
                    div()
                        .id("task-bar-dismiss")
                        .text_xs()
                        .text_color(rgb(0x585b70))
                        .px(px(4.0))
                        .rounded(px(3.0))
                        .cursor_pointer()
                        .hover(|d| d.text_color(rgb(0xf38ba8)).bg(rgb(0x313244)))
                        .child("✕")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.dismiss_compare_task();
                            cx.notify();
                        }))
                )
        )
        // Agent status rows
        .children(task.agents.iter().map(|agent| {
            let color = rgb(agent.status.color());
            let icon = agent.status.icon();
            let status_text = match &agent.status {
                CompareAgentStatus::Pending => "Pending...",
                CompareAgentStatus::Running => "Working...",
                CompareAgentStatus::Waiting => "Waiting for input",
                CompareAgentStatus::Done => "Completed",
                CompareAgentStatus::Error => "Error",
            };

            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div().text_xs().text_color(color).w(px(14.0)).child(icon.to_string())
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xcdd6f4))
                        .w(px(100.0))
                        .child(agent.display_name.clone())
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(color)
                        .child(status_text.to_string())
                )
        }))
        // "Review Results" button when all done
        .when(all_done, |d| {
            d.child(
                div()
                    .flex()
                    .justify_end()
                    .mt_1()
                    .child(
                        div()
                            .id("task-bar-review")
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0x1e1e2e))
                            .bg(rgb(0xa6e3a1))
                            .px_2()
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(0xb8d89e)))
                            .child("Results Ready — Review ▸")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(ref mut task) = this.compare_task {
                                    task.phase = ComparePhase::Review;
                                }
                                cx.notify();
                            }))
                    )
            )
        })
        .into_any_element()
}

/// Review phase: show completion summary
#[cfg(feature = "gpui")]
fn render_review(
    task: &CompareTask,
    cx: &mut gpui::Context<GpuiShellView>,
) -> gpui::AnyElement {
    div()
        .id("task-bar-review-phase")
        .w_full()
        .flex_shrink_0()
        .bg(rgb(0x1e1e2e))
        .border_b_1()
        .border_color(rgb(0x313244))
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .gap_1()
        // Header
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0xa6e3a1))
                        .child("✓ COMPARE COMPLETE")
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x7f849c))
                        .flex_1()
                        .child("Review each agent's output in the panes below, then dismiss.")
                )
                .child(
                    div()
                        .id("task-bar-done")
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0xcdd6f4))
                        .bg(rgb(0x45475a))
                        .px_2()
                        .py(px(3.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(0x585b70)))
                        .child("Done")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.dismiss_compare_task();
                            cx.notify();
                        }))
                )
        )
        // Agent result summary
        .children(task.agents.iter().map(|agent| {
            let color = rgb(agent.status.color());
            let icon = agent.status.icon();

            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_xs().text_color(color).child(icon.to_string()))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0xcdd6f4))
                        .child(agent.display_name.clone())
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x7f849c))
                        .child(if agent.status == CompareAgentStatus::Done {
                            "— see pane output".to_string()
                        } else {
                            format!("— {}", agent.status.icon())
                        })
                )
        }))
        .into_any_element()
}

/// Render the compare setup dialog (overlay)
#[cfg(feature = "gpui")]
pub fn render_compare_setup(
    setup: &crate::gpui_compare_task::CompareSetupState,
    cx: &mut gpui::Context<GpuiShellView>,
) -> gpui::AnyElement {
    let can_start = setup.can_start();
    let prompt_display = if setup.prompt.is_empty() {
        "▎ Enter your requirement...".to_string()
    } else {
        format!("{}▎", setup.prompt)
    };
    let prompt_color = if setup.prompt.is_empty() { rgb(0x7f849c) } else { rgb(0xcdd6f4) };

    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        // Backdrop
        .child(
            div()
                .id("cmp-backdrop")
                .absolute()
                .inset_0()
                .bg(gpui::Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.5 })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.compare_setup = None;
                    cx.notify();
                }))
        )
        // Dialog panel
        .child(
            div()
                .id("cmp-dialog")
                .w(px(480.0))
                .bg(rgb(0x1e1e2e))
                .border_1()
                .border_color(rgb(0x45475a))
                .rounded(px(8.0))
                .px_4()
                .py_3()
                .flex()
                .flex_col()
                .gap_3()
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    // Keep focus on main view so EntityInputHandler continues to receive text
                    this.focus_handle.focus(window, cx);
                }))
                // Title
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x89b4fa))
                        .child("⚡ Compare Agents")
                )
                // Prompt input area
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div().text_xs().text_color(rgb(0x7f849c)).child("Requirement")
                        )
                        .child(
                            div()
                                .w_full()
                                .min_h(px(60.0))
                                .px_2()
                                .py(px(6.0))
                                .bg(rgb(0x313244))
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(rgb(0x45475a))
                                .text_sm()
                                .text_color(prompt_color)
                                .child(prompt_display)
                        )
                )
                // Agent selection
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div().text_xs().text_color(rgb(0x7f849c))
                                .child(format!("Select Agents ({}/{})", setup.selected_count(), setup.agents.len()))
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(6.0))
                                .children(setup.agents.iter().enumerate().map(|(idx, (id, name, selected))| {
                                    let bg = if *selected { rgb(0x313244) } else { rgb(0x1e1e2e) };
                                    let border = if *selected { rgb(0x89b4fa) } else { rgb(0x45475a) };
                                    let text_c = if *selected { rgb(0xcdd6f4) } else { rgb(0x7f849c) };
                                    let check = if *selected { "✓ " } else { "" };

                                    div()
                                        .id(gpui::ElementId::Name(format!("cmp-agent-{}", idx).into()))
                                        .px_2()
                                        .py(px(4.0))
                                        .bg(bg)
                                        .border_1()
                                        .border_color(border)
                                        .rounded(px(4.0))
                                        .text_xs()
                                        .text_color(text_c)
                                        .cursor_pointer()
                                        .hover(|d| d.bg(rgb(0x313244)))
                                        .child(format!("{}{}", check, name))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(ref mut setup) = this.compare_setup {
                                                setup.toggle_agent(idx);
                                            }
                                            cx.notify();
                                        }))
                                }))
                        )
                )
                // Action buttons
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            div()
                                .id("cmp-cancel")
                                .text_xs()
                                .text_color(rgb(0x7f849c))
                                .px_3()
                                .py(px(5.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .hover(|d| d.bg(rgb(0x313244)).text_color(rgb(0xcdd6f4)))
                                .child("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.compare_setup = None;
                                    cx.notify();
                                }))
                        )
                        .child(
                            div()
                                .id("cmp-start")
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if can_start { rgb(0x1e1e2e) } else { rgb(0x585b70) })
                                .bg(if can_start { rgb(0x89b4fa) } else { rgb(0x313244) })
                                .px_3()
                                .py(px(5.0))
                                .rounded(px(4.0))
                                .when(can_start, |d| d
                                    .cursor_pointer()
                                    .hover(|d| d.bg(rgb(0x7aa2d0)))
                                )
                                .child("Start Compare")
                                .when(can_start, |d| {
                                    d.on_click(cx.listener(|this, _, _, cx| {
                                        this.start_compare_task();
                                        cx.notify();
                                    }))
                                })
                        )
                )
        )
        .into_any_element()
}
