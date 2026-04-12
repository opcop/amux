//! Terminal right-click context menu: item model, builder, and
//! action dispatch.
//!
//! This module is a straightforward extraction of the three menu
//! concerns that used to sit inline in `gpui_entry.rs`:
//!
//!   * `ContextMenuItem` — the label/shortcut/enabled descriptor
//!     that `gpui_layout_renderer::render_context_menu` takes.
//!   * `build_items` — constructs the item list for the *terminal*
//!     context menu (as opposed to the `+▾` or tab menus, which
//!     are their own pickers).
//!   * `dispatch` — matches the clicked label to the corresponding
//!     `GpuiShellView` method and fires it.
//!
//! Why a free function and not `impl GpuiShellView`?
//!
//! The dispatch is a 50-line `match label { ... }` whose only job
//! is forwarding to already-public helpers on `GpuiShellView`.
//! Lifting it out of the 3600-line entry file means the menu's
//! action vocabulary has a single obvious home, and the entry
//! file's impl block sheds one more unrelated concern. The
//! function takes `&mut GpuiShellView` explicitly so the call
//! site is symmetric with `crate::search::rebuild(&mut state,
//! term)` — both follow the "pure function, borrow passed in"
//! pattern this crate is migrating toward.

#![cfg(feature = "gpui")]

use amux_platform::terminal::manager::SplitDirection;
use gpui::{Context, Window};

use crate::gpui_entry::GpuiShellView;

/// One row in the right-click context menu.
#[derive(Clone)]
pub(crate) struct ContextMenuItem {
    pub(crate) label: &'static str,
    pub(crate) shortcut: Option<&'static str>,
    pub(crate) enabled: bool,
    pub(crate) separator_after: bool,
}

impl ContextMenuItem {
    pub(crate) fn action(
        label: &'static str,
        shortcut: Option<&'static str>,
        enabled: bool,
    ) -> Self {
        Self { label, shortcut, enabled, separator_after: false }
    }

    pub(crate) fn separator(mut self) -> Self {
        self.separator_after = true;
        self
    }
}

/// Build the terminal context menu for the current `GpuiShellView`
/// state. The set of enabled items depends on:
///
///   * whether the active terminal has a non-empty selection,
///   * whether there are multiple panes to target,
///   * whether a pane is currently zoomed.
///
/// The shortcut labels are per-platform compile-time constants so
/// no `format!` runs on the render path — this used to allocate
/// seven+ `String`s per frame when it lived inline.
pub(crate) fn build_items(view: &GpuiShellView) -> Vec<ContextMenuItem> {
    let has_selection = view
        .terminal_manager()
        .active_terminal_ref()
        .and_then(|t| t.with_term(|term| term.selection_to_string()))
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    #[cfg(target_os = "macos")]
    mod shortcut_labels {
        pub const COPY: &str = "⌘⇧C";
        pub const SEND: &str = "⌘⇧Enter";
        pub const PASTE: &str = "⌘V";
        pub const SPLIT_RIGHT: &str = "⌘⇧\\";
        pub const SPLIT_DOWN: &str = "⌘⇧D";
        pub const NEW_TAB: &str = "⌘⇧T";
        pub const CLOSE_PANE: &str = "⌘⇧W";
        pub const ZOOM: &str = "⌘⇧F";
    }
    #[cfg(not(target_os = "macos"))]
    mod shortcut_labels {
        pub const COPY: &str = "Ctrl+Shift+C";
        pub const SEND: &str = "Ctrl+Shift+Enter";
        pub const PASTE: &str = "Ctrl+V";
        pub const SPLIT_RIGHT: &str = "Ctrl+Shift+\\";
        pub const SPLIT_DOWN: &str = "Ctrl+Shift+D";
        pub const NEW_TAB: &str = "Ctrl+Shift+T";
        pub const CLOSE_PANE: &str = "Ctrl+Shift+W";
        pub const ZOOM: &str = "Ctrl+Shift+F";
    }
    use shortcut_labels::*;

    let multi_pane = view.terminal_manager().total_panes() > 1;
    vec![
        ContextMenuItem::action("Copy", Some(COPY), has_selection),
        ContextMenuItem::action("Paste", Some(PASTE), true).separator(),
        ContextMenuItem::action("Send to Pane", Some(SEND), multi_pane),
        ContextMenuItem::action("Split Right", Some(SPLIT_RIGHT), true),
        ContextMenuItem::action("Split Down", Some(SPLIT_DOWN), true).separator(),
        ContextMenuItem::action("New Tab", Some(NEW_TAB), true),
        ContextMenuItem::action("Close Pane", Some(CLOSE_PANE), multi_pane),
        if view.zoomed_pane.is_some() {
            ContextMenuItem::action("Restore Pane", Some(ZOOM), true)
        } else {
            ContextMenuItem::action("Zoom Pane", Some(ZOOM), multi_pane)
        },
    ]
}

/// Execute a context menu action by label.
///
/// The menu stores which pane was active when it opened
/// (`context_menu.source_pane`). Before dispatching, we restore
/// that pane as active — otherwise a slow right-click + move +
/// choose sequence would run the action against whichever pane
/// the cursor happened to be over at click time.
///
/// Labels that don't match a known action are silently ignored so
/// that renderer-side menus (e.g. the "Open Workspace" entry shown
/// when no workspace is loaded) can reuse this dispatch without a
/// separate path.
pub(crate) fn dispatch(
    view: &mut GpuiShellView,
    label: &str,
    _window: &mut Window,
    cx: &mut Context<GpuiShellView>,
) {
    let source_pane = view.context_menu.as_ref().and_then(|m| m.source_pane.clone());
    view.context_menu = None;
    if let Some(pid) = source_pane {
        view.terminal_manager_mut().set_active_pane(&pid);
    }

    match label {
        "Open Workspace" => view.prompt_open_local_workspace(cx),
        "Copy" => view.copy_selection(cx),
        "Send to Pane" => view.start_send_to_pane(cx),
        "Paste" => view.paste_clipboard(cx),
        "Split Right" => {
            let env = view.capture_active_env();
            view.terminal_manager_mut().split_active_pane(SplitDirection::Horizontal);
            view.spawn_with_captured_env(&env);
        }
        "Split Down" => {
            let env = view.capture_active_env();
            view.terminal_manager_mut().split_active_pane(SplitDirection::Vertical);
            view.spawn_with_captured_env(&env);
        }
        "New Tab" => {
            let env = view.capture_active_env();
            view.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
            view.spawn_with_captured_env(&env);
        }
        "Close Pane" => {
            view.zoomed_pane = None; // unzoom on close
            view.cleanup_pane_tab_entries();
            view.terminal_manager_mut().close_active_pane();
        }
        "Zoom Pane" | "Restore Pane" => view.toggle_zoom(),
        _ => {}
    }

    view.context_menu = None;
    cx.notify();
}
