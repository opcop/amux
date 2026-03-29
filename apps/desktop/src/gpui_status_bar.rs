#[cfg(feature = "gpui")]
use gpui::{rgb, px, FontWeight, IntoElement, div, prelude::*};

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
        .h(px(26.0))
        .bg(rgb(0x11111b))
        .border_t_1()
        .border_color(rgb(0x252530))
        .text_xs()
        .text_color(rgb(0x6c7086))
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
                                .bg(rgb(0xa6e3a1))  // green dot = active
                        )
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0xcdd6f4))
                                .child(workspace.clone()),
                        ),
                )
                // Separator
                .child(div().w_px().h(px(12.0)).bg(rgb(0x313244)))
                // Pane/Tab counts
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div().text_color(rgb(0x7f849c))
                                .child(format!("{} panes", pane_count))
                        )
                        .child(
                            div().text_color(rgb(0x585b70)).child("·")
                        )
                        .child(
                            div().text_color(rgb(0x7f849c))
                                .child(format!("{} tabs", tab_count))
                        ),
                ),
        )
        // Right section
        .child(
            div()
                .flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(3.0))
                        .bg(rgb(0x1e1e2e))
                        .text_color(rgb(0x7f849c))
                        .child(shell.clone()),
                ),
        )
}
