//! Command palette → gpui-layer action dispatch.
//!
//! Some palette entries map to `amux_core::Command` / `AppCommand`
//! variants that amux-ui already knows how to execute
//! (`switch workspace 1`, `pane resize-left`, `autosave enable`, …).
//! Others need state that only exists in the gpui layer — the
//! active `GpuiShellView`, the window handle, the file picker —
//! and must be handled by calling methods on `GpuiShellView`
//! directly. This module is the single place that routes those.
//!
//! ## How it's wired
//!
//! `gpui_input_handler::on_global_key_down` intercepts the palette
//! `Enter` key, grabs the currently-selected command string, and
//! calls `dispatch(view, cmd, window, cx)`. If this function
//! returns `true` it means the action was handled here and the
//! caller must NOT also route the command through the usual
//! `execute_selected_palette_command` path — doing so would
//! double-fire or error out.
//!
//! If it returns `false` the command is a regular
//! `parse_command`-routed action and the caller falls through to
//! the existing dispatch.
//!
//! ## Why free fn, not method
//!
//! Consistent with `crate::search::rebuild`, `crate::menu::dispatch`,
//! `crate::preview_open::open_preview_file`, etc. The "take
//! `&mut GpuiShellView` as a parameter, not `self`" shape keeps
//! `gpui_entry.rs` from accreting another 100 lines of impl block
//! and makes this module trivially greppable — every
//! palette-exclusive handler lives in one file.

#![cfg(feature = "gpui")]

use gpui::{Context, Window};

use crate::gpui_entry::GpuiShellView;

/// Execute a palette command if it's one of the gpui-layer-only
/// actions. Returns `true` if handled, `false` if the caller
/// should fall through to `execute_selected_palette_command`.
///
/// Every handler closes the palette first (via the
/// `ToggleCommandPalette` UiAction) so the user doesn't have to
/// `Esc` out after firing a one-shot action.
pub(crate) fn dispatch(
    view: &mut GpuiShellView,
    cmd: &str,
    window: &mut Window,
    cx: &mut Context<GpuiShellView>,
) -> bool {
    // Handle the `layout template <name>` family before the
    // exact-match branch. The suffix is arbitrary so it can't sit
    // in a `match cmd { ... }` literal.
    if let Some(_name) = cmd.strip_prefix("layout template ") {
        close_palette(view);
        view.open_template_picker();
        view.refresh_model();
        return true;
    }

    match cmd {
        // ─── General ──────────────────────────────────────────────
        "find" => {
            close_palette(view);
            view.search_state = Some(crate::state::SearchState::new());
        }
        "quit" => {
            close_palette(view);
            cx.quit();
            return true;
        }
        "browser" => {
            close_palette(view);
            view.open_browser("", window, cx);
        }
        "sidebar toggle" => {
            close_palette(view);
            view.sidebar_state.collapsed = !view.sidebar_state.collapsed;
        }
        "sidebar mode" => {
            close_palette(view);
            use crate::gpui_workspace_sidebar::SidebarMode;
            view.sidebar_state.mode = match view.sidebar_state.mode {
                SidebarMode::Workspaces => SidebarMode::Agents,
                SidebarMode::Agents => SidebarMode::Workspaces,
            };
            if view.sidebar_state.collapsed {
                view.sidebar_state.collapsed = false;
            }
        }
        "scrollback clear" => {
            close_palette(view);
            if let Some(term) = view.terminal_manager_mut().active_terminal() {
                term.with_term_mut(|t| {
                    use alacritty_terminal::vte::ansi::Handler;
                    t.clear_screen(alacritty_terminal::vte::ansi::ClearMode::Saved);
                    t.clear_screen(alacritty_terminal::vte::ansi::ClearMode::All);
                });
            }
        }

        // ─── Workspace ────────────────────────────────────────────
        "workspace new" => {
            close_palette(view);
            view.prompt_open_local_workspace(cx);
        }
        "workspace edit-startup" => {
            close_palette(view);
            view.edit_startup_file();
        }

        // ─── Agent ────────────────────────────────────────────────
        // Note: "agent <id>" (e.g. "agent claude") is handled by
        // parse_command as a real provider-launch. We use
        // "launch agent" here so the generic picker never
        // collides with the `["agent", <id>]` parse pattern.
        "launch agent" => {
            close_palette(view);
            view.open_agent_picker();
        }

        // ─── Pane ─────────────────────────────────────────────────
        "pane new-tab" => {
            close_palette(view);
            let env = view.capture_active_env();
            view.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
            view.spawn_with_captured_env(&env);
        }
        "pane close" => {
            close_palette(view);
            view.cleanup_pane_tab_entries();
            let _ = view.terminal_manager_mut().close_active_pane();
        }
        "pane zoom" => {
            close_palette(view);
            view.toggle_zoom();
        }
        "pane equalize" => {
            close_palette(view);
            view.terminal_manager_mut().equalize_splits();
        }
        "pane send" => {
            close_palette(view);
            view.start_send_to_pane(cx);
        }

        // ─── Layout ───────────────────────────────────────────────
        "layout save-as-template" => {
            close_palette(view);
            let ws_name = view
                .model
                .active_workspace_name
                .clone()
                .unwrap_or_else(|| view.active_workspace_id.clone());
            view.save_current_as_template(&ws_name);
        }

        // Fall through — not a gpui-layer command.
        _ => return false,
    }

    view.refresh_model();
    true
}

/// Dismiss the command palette. Called at the top of every
/// handler so the user gets the "run this action then close the
/// palette" behavior they expect.
fn close_palette(view: &mut GpuiShellView) {
    let _ = view.app.dispatch(amux_ui::UiAction::ToggleCommandPalette);
}
