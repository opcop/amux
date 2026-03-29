#[cfg(feature = "gpui")]
use gpui::{rgb, FontWeight, IntoElement, div, prelude::*};

/// Runtime status bar data collected from actual terminal state
#[cfg(feature = "gpui")]
pub struct StatusBarData {
    pub workspace_name: String,
    pub pane_count: usize,
    pub tab_count: usize,
    pub shell_name: String,
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
        .py_1()
        .bg(rgb(0x0a0a14))
        .border_t_1()
        .border_color(rgb(0x1a1a2a))
        .text_xs()
        .text_color(rgb(0x7f849c))
        // Left section
        .child(
            div()
                .flex()
                .gap_4()
                .items_center()
                // Workspace name
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .items_center()
                        .child(div().w_1().h_1().rounded_full().bg(rgb(0x89b4fa)))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0xcdd6f4))
                                .child(workspace.clone()),
                        ),
                )
                // Pane/Tab counts
                .child(
                    div()
                        .text_color(rgb(0x585b70))
                        .child(format!("panes:{} tabs:{}", pane_count, tab_count)),
                ),
        )
        // Right section
        .child(
            div()
                .flex()
                .gap_4()
                .items_center()
                .child(
                    div()
                        .text_color(rgb(0x585b70))
                        .child(format!("shell: {}", shell)),
                ),
        )
}
