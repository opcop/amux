#[cfg(feature = "gpui")]
use amux_ui::GpuiWindowModel;
#[cfg(feature = "gpui")]
use gpui::{rgb, FontWeight, IntoElement, div, prelude::*};

#[cfg(feature = "gpui")]
pub fn render_status_bar(model: &GpuiWindowModel) -> impl IntoElement {
    let workspace = model
        .active_workspace_name
        .clone()
        .unwrap_or_else(|| "No workspace".into());
    let wsl_distro = model.status_wsl_distro.as_ref();
    let split_count = model.status_split_count;
    let shell = model.status_terminal_shell.as_ref();
    let cpu_usage = model.status_cpu_usage.as_ref();
    let memory_usage = model.status_memory_usage.as_ref();
    let load_color = model.status_load_color.as_ref();
    let save_status = &model.status_save;

    div()
        .flex()
        .justify_between()
        .items_center()
        .px_3()
        .py_1()
        .bg(rgb(0x141414))
        .border_t_1()
        .border_color(rgb(0x2a2a2a))
        .text_xs()
        .text_color(rgb(0xb3b3b3))
        // Left section: workspace info
        .child(
            div()
                .flex()
                .gap_4()
                .items_center()
                // Workspace name with indicator
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .items_center()
                        .child(div().w_1().h_1().rounded_full().bg(rgb(0x0091ff)))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0xffffff))
                                .child(workspace.clone()),
                        ),
                )
                // WSL indicator
                .child({
                    if let Some(distro) = wsl_distro {
                        div()
                            .px_2()
                            .py_px()
                            .rounded_sm()
                            .bg(rgb(0x1a3a2a))
                            .text_color(rgb(0x10b981))
                            .child(format!("WSL:{}", distro))
                    } else {
                        div()
                    }
                })
                // Split count
                .child(format!("splits:{}", split_count))
                // System metrics
                .child({
                    div()
                        .flex()
                        .gap_3()
                        .items_center()
                        .child({
                            if let Some(cpu) = cpu_usage {
                                let cpu_color = if let Some(color) = load_color {
                                    match color.as_str() {
                                        "red" => rgb(0xef4444),
                                        "yellow" => rgb(0xf59e0b),
                                        _ => rgb(0x10b981),
                                    }
                                } else {
                                    rgb(0x10b981)
                                };
                                div()
                                    .flex()
                                    .gap_1()
                                    .items_center()
                                    .child(div().w_1().h_1().rounded_full().bg(cpu_color))
                                    .child(format!("CPU {}", cpu))
                            } else {
                                div()
                            }
                        })
                        .child({
                            if let Some(mem) = memory_usage {
                                div()
                                    .flex()
                                    .gap_1()
                                    .items_center()
                                    .child(format!("MEM {}", mem))
                            } else {
                                div()
                            }
                        })
                }),
        )
        // Right section: status indicators
        .child(
            div()
                .flex()
                .gap_4()
                .items_center()
                // Terminal shell
                .child({
                    if let Some(s) = shell {
                        div()
                            .text_color(rgb(0x666666))
                            .child(format!("shell:{}", s))
                    } else {
                        div()
                    }
                })
                // Save status
                .child({
                    let color = if save_status.contains("unsaved") {
                        rgb(0xf59e0b)
                    } else if save_status.contains("saving") {
                        rgb(0x3b82f6)
                    } else {
                        rgb(0x10b981)
                    };
                    div()
                        .flex()
                        .gap_1()
                        .items_center()
                        .child(div().w_1().h_1().rounded_full().bg(color))
                        .child(save_status.clone())
                }),
        )
}
