// Shared UI components for GPUI views
// This module provides reusable components across all GPUI panels

#[cfg(feature = "gpui")]
use gpui::{rgb, Div, FontWeight, IntoElement, Stateful, div, prelude::*};

/// Base toolbar container with consistent styling
#[cfg(feature = "gpui")]
pub fn toolbar_container() -> gpui::Div {
    div()
        .flex()
        .gap_2()
}

/// Base panel builder with title and background color
#[cfg(feature = "gpui")]
pub fn base_panel(title: &str, background: gpui::Rgba) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(0xd6d3d1))
        .bg(background)
        .p_3()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_sm()
                .child(title.to_string()),
        )
}

/// Generic panel renderer for displaying titled content lines
#[cfg(feature = "gpui")]
pub fn render_content_panel(
    title: &str,
    lines: &[String],
    background: gpui::Rgba,
    compact: bool,
) -> impl IntoElement {
    let mut panel = base_panel(title, background);

    let lines = if lines.is_empty() {
        vec!["Empty".to_string()]
    } else {
        lines.to_vec()
    };

    let max_lines = if compact { 8 } else { 12 };
    for line in lines.into_iter().take(max_lines) {
        panel = panel.child(div().text_sm().text_color(rgb(0x4b5563)).child(line));
    }

    panel
}

/// Action row with optional active state and clickability
#[cfg(feature = "gpui")]
pub fn action_row(
    id: impl Into<String>,
    label: impl Into<String>,
    active: bool,
    clickable: bool,
) -> Stateful<Div> {
    let label: String = label.into();
    div()
        .id(id.into())
        .px_2()
        .py_1()
        .rounded_sm()
        .when(clickable, |this| this.cursor_pointer())
        .bg(if active { rgb(0xdbe7f0) } else { rgb(0xf8f5ef) })
        .when(clickable, |this| this.hover(|style| style.bg(rgb(0xe7dfd1))))
        .text_sm()
        .text_color(rgb(0x4b5563))
        .child(label)
}

/// Action button for toolbar and controls
#[cfg(feature = "gpui")]
pub fn action_button(id: impl Into<String>, label: impl Into<String>) -> Stateful<Div> {
    let label: String = label.into();
    div()
        .id(id.into())
        .px_2()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .bg(rgb(0xe7dfd1))
        .hover(|style| style.bg(rgb(0xd6cfc1)))
        .text_sm()
        .text_color(rgb(0x1f2933))
        .child(label)
}

/// Single metric card
#[cfg(feature = "gpui")]
pub fn metric_card(label: &str, value: impl Into<String>) -> impl IntoElement {
    let value: String = value.into();
    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(0xd6d3d1))
        .bg(rgb(0xfffbf5))
        .p_3()
        .child(div().text_sm().text_color(rgb(0x6b7280)).child(label.to_string()))
        .child(
            div()
                .font_weight(FontWeight::BOLD)
                .text_xl()
                .child(value),
        )
}
