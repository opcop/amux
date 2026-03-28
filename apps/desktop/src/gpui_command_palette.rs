#[cfg(feature = "gpui")]
use amux_ui::GpuiPaletteCommandItem;
#[cfg(feature = "gpui")]
use gpui::{rgb, px, Div, Stateful, IntoElement, div, prelude::*};

#[cfg(feature = "gpui")]
pub fn render_command_palette(
    open: bool,
    query: &str,
    filters: &[String],
    query_suggestions: &[String],
    commands: &[GpuiPaletteCommandItem],
    selected_index: usize,
    query_controls: impl IntoElement,
    selection_controls: impl IntoElement,
    filter_buttons: impl Fn(&String) -> Stateful<Div>,
    query_buttons: impl Fn(&String) -> Stateful<Div>,
    command_click: impl Fn(&str) -> Stateful<Div>,
) -> impl IntoElement {
    if !open {
        return div().hidden();
    }

    // Query input display styled like an input field
    let query_display = if query.is_empty() {
        div()
            .flex()
            .items_center()
            .px_2()
            .py_1()
            .rounded_sm()
            .border_1()
            .border_color(rgb(0x3a3a3a))
            .bg(rgb(0x1e1e2e))
            .text_sm()
            .text_color(rgb(0x6c7086))
            .child("Type to search commands...")
    } else {
        div()
            .flex()
            .items_center()
            .px_2()
            .py_1()
            .rounded_sm()
            .border_2()
            .border_color(rgb(0x89b4fa))
            .bg(rgb(0x1e1e2e))
            .text_sm()
            .text_color(rgb(0xcdd6f4))
            .child(format!("> {query}"))
    };

    let command_count = commands.len();
    let filtered_label = if query.is_empty() {
        format!("{command_count} commands")
    } else {
        format!("{command_count} matching")
    };

    let mut palette = div()
        .mt_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x313244))
        .bg(rgb(0x181825))
        .shadow_md()
        .p_3()
        .flex()
        .flex_col()
        .gap_2()
        // Header
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(0xcdd6f4))
                        .child("Command Palette"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6c7086))
                        .child(filtered_label),
                ),
        )
        // Query input
        .child(query_display);

    // Filter row
    let mut filter_row = div().flex().gap_1().flex_wrap();
    for filter in filters {
        filter_row = filter_row.child(filter_buttons(filter));
    }

    // Suggestion row
    let mut query_row = div().flex().gap_1().flex_wrap();
    for suggestion in query_suggestions {
        query_row = query_row.child(query_buttons(suggestion));
    }

    palette = palette
        .child(filter_row)
        .child(query_controls)
        .child(query_row)
        .child(selection_controls);

    if commands.is_empty() {
        return palette.child(
            div()
                .py_4()
                .text_sm()
                .text_color(rgb(0x6c7086))
                .child("No commands match the current query"),
        );
    }

    // Render commands grouped by category
    let mut current_category = String::new();
    for (index, cmd) in commands.iter().enumerate() {
        // Category header
        if cmd.category != current_category {
            current_category = cmd.category.clone();
            palette = palette.child(
                div()
                    .mt_1()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(0x89b4fa))
                    .child(current_category.clone()),
            );
        }

        // Command row
        let is_selected = index == selected_index;
        let bg = if is_selected {
            rgb(0x313244)
        } else {
            rgb(0x181825)
        };
        let border_color = if is_selected {
            rgb(0x89b4fa)
        } else {
            rgb(0x181825)
        };

        let mut row = div()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(bg)
            .border_l_2()
            .border_color(border_color);

        // Left side: label + description
        let left = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .text_sm()
                    .text_color(if is_selected {
                        rgb(0xcdd6f4)
                    } else {
                        rgb(0xbac2de)
                    })
                    .child(cmd.label.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x6c7086))
                    .child(cmd.description.clone()),
            );

        row = row.child(left);

        // Right side: keybinding badge
        if let Some(ref kb) = cmd.keybinding {
            row = row.child(
                div()
                    .px_1()
                    .py(px(1.0))
                    .rounded_sm()
                    .bg(rgb(0x313244))
                    .text_xs()
                    .text_color(rgb(0x6c7086))
                    .child(kb.clone()),
            );
        }

        // Wrap in clickable container
        palette = palette.child(command_click(&cmd.command).child(row));
    }

    palette
}
