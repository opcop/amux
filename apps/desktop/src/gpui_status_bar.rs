#[cfg(feature = "gpui")]
use gpui::{rgb, px, FontWeight, IntoElement, div, prelude::*};
#[cfg(feature = "gpui")]
use crate::theme;

/// Summary of a single agent's status for the status bar
#[cfg(feature = "gpui")]
pub struct AgentSummary {
    pub name: String,
    pub status_icon: &'static str,
    pub color: u32,
}

/// Runtime status bar data collected from actual terminal state
#[cfg(feature = "gpui")]
pub struct StatusBarData {
    pub workspace_name: String,
    pub pane_count: usize,
    pub tab_count: usize,
    pub shell_name: String,
    pub agents: Vec<AgentSummary>,
    /// If the last startup found crash reports on disk, this holds
    /// the count so the status bar can surface a passive warning.
    /// `None` when there are no crashes to notify about.
    pub crash_notice: Option<usize>,
    /// Pre-formatted debug stats line (frame time, glyph cache hit
    /// rate). Populated from `metrics::snapshot()` when
    /// `AMUX_DEBUG_STATS=1`; `None` otherwise.
    pub debug_stats: Option<String>,
}

#[cfg(feature = "gpui")]
pub fn render_status_bar(data: &StatusBarData) -> impl IntoElement {
    let workspace = &data.workspace_name;
    let pane_count = data.pane_count;
    let tab_count = data.tab_count;
    let shell = &data.shell_name;

    div()
        .flex()
        .justify_between()
        .items_center()
        .px_3()
        .pt(px(8.0)) // breathing room above the bar
        .h(px(34.0)) // 26px content + 8px top padding
        .bg(rgb(crate::theme::SURFACE))
        .border_t_1()
        .border_color(rgb(crate::theme::SURFACE_RAISED))
        .text_xs()
        .text_color(rgb(crate::theme::TEXT_DIM))
        // Left section
        .child(
            div()
                .flex()
                .gap_3()
                .items_center()
                // Workspace indicator
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .items_center()
                        .child(
                            div()
                                .w(px(6.0))
                                .h(px(6.0))
                                .rounded_full()
                                .bg(rgb(crate::theme::SUCCESS))  // green dot = active
                        )
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(crate::theme::TEXT))
                                .child(workspace.clone()),
                        ),
                )
                // Separator
                .child(div().w_px().h(px(12.0)).bg(rgb(crate::theme::BORDER)))
                // Pane/Tab counts
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div().text_color(rgb(crate::theme::TEXT_DIM))
                                .child(format!("{} {}", pane_count, if pane_count == 1 { "pane" } else { "panes" }))
                        )
                        .child(
                            div().text_color(rgb(crate::theme::TEXT_DIM)).child("·")
                        )
                        .child(
                            div().text_color(rgb(crate::theme::TEXT_DIM))
                                .child(format!("{} {}", tab_count, if tab_count == 1 { "tab" } else { "tabs" }))
                        ),
                ),
        )
        // Right section
        .child(
            div()
                .flex()
                .gap_3()
                .items_center()
                // Debug stats HUD (AMUX_DEBUG_STATS=1). Monospace-ish,
                // dim, read-only — no borders, never interactive.
                .children(match data.debug_stats.as_deref() {
                    Some(s) => vec![
                        div()
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(theme::RADIUS_SM))
                            .bg(rgb(theme::SURFACE_DIM))
                            .text_color(rgb(theme::INFO))
                            .child(s.to_string())
                            .into_any_element(),
                        div().w_px().h(px(12.0)).bg(rgb(crate::theme::BORDER)).into_any_element(),
                    ],
                    None => Vec::new(),
                })
                // Crash notice (shown when ~/.amux/logs/crash has entries).
                // Passive — points the user at the log directory.
                .children(match data.crash_notice {
                    Some(n) if n > 0 => vec![
                        div()
                            .flex()
                            .gap(px(4.0))
                            .items_center()
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(theme::RADIUS_SM))
                            .bg(rgb(theme::DANGER_BG))
                            .border_1()
                            .border_color(rgb(theme::DANGER))
                            .text_color(rgb(theme::DANGER_BRIGHT))
                            .child(format!(
                                "⚠ {} crash log{} — see ~/.amux/logs/crash",
                                n,
                                if n == 1 { "" } else { "s" }
                            ))
                            .into_any_element(),
                        div().w_px().h(px(12.0)).bg(rgb(crate::theme::BORDER)).into_any_element(),
                    ],
                    _ => Vec::new(),
                })
                // Agent status indicators
                .children(if data.agents.is_empty() {
                    Vec::new()
                } else {
                    let mut els = vec![
                        div().w_px().h(px(12.0)).bg(rgb(crate::theme::BORDER)).into_any_element(),
                    ];
                    for agent in &data.agents {
                        els.push(
                            div()
                                .flex()
                                .gap(px(4.0))
                                .items_center()
                                .child(
                                    div().text_color(rgb(agent.color))
                                        .child(agent.status_icon)
                                )
                                .child(
                                    div().text_color(rgb(crate::theme::TEXT_DIM))
                                        .child(agent.name.clone())
                                )
                                .into_any_element(),
                        );
                    }
                    els
                })
                .child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(3.0))
                        .bg(rgb(crate::theme::SURFACE_DIM))
                        .text_color(rgb(crate::theme::TEXT_DIM))
                        .child(shell.clone()),
                ),
        )
}
