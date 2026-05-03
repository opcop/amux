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
    // Check for a selection: terminal selection (alacritty) takes
    // priority; fallback is a non-empty preview text selection.
    let has_terminal_selection = view
        .terminal_manager()
        .active_terminal_ref()
        .and_then(|t| t.with_term(|term| term.selection_to_string()))
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let has_preview_selection = view.has_preview_text_selection();
    let has_selection = has_terminal_selection || has_preview_selection;

    #[cfg(target_os = "macos")]
    mod shortcut_labels {
        pub const COPY: &str = "⌘⇧C";
        pub const CLEAR: &str = "⌘K";
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
        pub const CLEAR: &str = "Ctrl+K";
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

    // "Open Selection as File" — shown only when the right-click
    // captured a selection that resolved to a real file. The
    // resolution happened once at menu-open time and is stored in
    // `context_menu.selection_path`, so `build_items` pays no FS
    // cost here. When there's no selection or it didn't resolve,
    // the row is omitted entirely (rather than shown disabled) to
    // keep the menu compact in the common case.
    let selection_resolved = view
        .context_menu
        .as_ref()
        .and_then(|m| m.selection_path.as_deref())
        .is_some();

    let mut items: Vec<ContextMenuItem> = Vec::with_capacity(10);
    if selection_resolved {
        items.push(ContextMenuItem::action("Open Selection as File", None, true).separator());
    }
    items.extend([
        ContextMenuItem::action("Copy", Some(COPY), has_selection),
        ContextMenuItem::action("Select All", None, true),
        ContextMenuItem::action("Paste", Some(PASTE), true).separator(),
        ContextMenuItem::action("Send to Pane", Some(SEND), multi_pane && has_selection),
        ContextMenuItem::action("Split Right", Some(SPLIT_RIGHT), true),
        ContextMenuItem::action("Split Down", Some(SPLIT_DOWN), true).separator(),
        ContextMenuItem::action("New Tab", Some(NEW_TAB), true),
        ContextMenuItem::action("Close Pane", Some(CLOSE_PANE), multi_pane),
        if view.zoomed_pane.is_some() {
            ContextMenuItem::action("Restore Pane", Some(ZOOM), true)
        } else {
            ContextMenuItem::action("Zoom Pane", Some(ZOOM), multi_pane)
        },
        ContextMenuItem::action("Clear Buffer", Some(CLEAR), true).separator(),
        // Workspace-level actions. These used to live in a command
        // palette (`Cmd+Shift+P` → "layout template …", "workspace
        // open …") but the palette UI was never mounted in the
        // render tree — leaving the shortcuts entirely hidden.
        // Restoring them here so the features remain discoverable
        // until the palette itself is properly wired.
        ContextMenuItem::action("Apply Layout...", None, true),
        ContextMenuItem::action("Edit Startup Script...", None, true),
        ContextMenuItem::action("Open Workspace...", None, view.model.local_workspace_supported),
    ]);
    items
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
    let selection_path = view
        .context_menu
        .as_ref()
        .and_then(|m| m.selection_path.clone());
    view.context_menu = None;
    if let Some(pid) = source_pane {
        view.terminal_manager_mut().set_active_pane(&pid);
    }

    match label {
        // Accept both the historical "Open Workspace" label
        // (still used by renderer-side menus when no workspace is
        // loaded — see module-level comment) and the new
        // "Open Workspace..." label used by the terminal context
        // menu. Same dispatch target.
        "Open Workspace" | "Open Workspace..." => view.prompt_open_local_workspace(cx),
        "Apply Layout..." => view.open_template_picker(),
        "Edit Startup Script..." => view.edit_startup_file(),
        "Open Selection as File" => {
            if let Some(path) = selection_path {
                crate::preview_open::open_preview_file(view, cx, &path);
            }
        }
        "Copy" => {
            // Preview text selection takes precedence (same priority
            // as the Cmd+C keyboard handler). Without this, right-click
            // → Copy on a preview silently does nothing because
            // `copy_selection` only reads terminal selections.
            if view.has_preview_text_selection() {
                view.copy_preview_selection(cx);
            } else {
                view.copy_selection(cx);
            }
        }
        "Select All" => view.select_all_in_terminal(cx),
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
        "Clear Buffer" => {
            if let Some(term) = view.terminal_manager_mut().active_terminal() {
                term.with_term_mut(|t| {
                    use alacritty_terminal::vte::ansi::Handler;
                    t.clear_screen(alacritty_terminal::vte::ansi::ClearMode::Saved);
                    t.clear_screen(alacritty_terminal::vte::ansi::ClearMode::All);
                });
            }
        }
        _ => {}
    }

    view.context_menu = None;
    cx.notify();
}
