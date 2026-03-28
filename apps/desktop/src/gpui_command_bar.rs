#[cfg(feature = "gpui")]
use gpui::{rgb, Div, Stateful, div, prelude::*};

#[cfg(feature = "gpui")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandBarAction {
    pub id: &'static str,
    pub label: &'static str,
    pub command: &'static str,
}

#[cfg(feature = "gpui")]
pub fn default_actions() -> Vec<CommandBarAction> {
    vec![
        CommandBarAction {
            id: "palette",
            label: "Palette",
            command: "palette",
        },
        CommandBarAction {
            id: "split-right",
            label: "Split Right",
            command: "pane split-right",
        },
        CommandBarAction {
            id: "split-down",
            label: "Split Down",
            command: "pane split-down",
        },
        CommandBarAction {
            id: "agent-codex",
            label: "Agent Codex",
            command: "agent codex",
        },
        CommandBarAction {
            id: "agent-claude",
            label: "Agent Claude",
            command: "agent claude",
        },
        CommandBarAction {
            id: "open-readme",
            label: "Open README",
            command: "file open README.md",
        },
        CommandBarAction {
            id: "open-notes",
            label: "Open Notes",
            command: "file open notes.md",
        },
    ]
}

#[cfg(feature = "gpui")]
pub fn command_bar_button(id: &str, label: &str) -> Stateful<Div> {
    div()
        .id(format!("command-{id}"))
        .px_2()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .bg(rgb(0xe7dfd1))
        .hover(|style| style.bg(rgb(0xd6cfc1)))
        .text_sm()
        .text_color(rgb(0x1f2933))
        .child(label.to_string())
}
