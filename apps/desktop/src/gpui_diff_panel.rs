//! Right-side overlay panel that shows uncommitted changes for the active
//! workspace and lets the user review per-file diffs without leaving Amux.
//!
//! Scope policy: this module owns the **UI state** and **rendering** of the
//! diff panel. The actual `git` invocations and the unified-diff parser live
//! in `crate::git_panel::diff` so the data layer can be unit-tested without
//! GPUI in the loop. The workspace-wide change list comes from
//! `crate::git_panel::status`, which the existing 2s poll loop in
//! `gpui_entry::spawn_git_status_poll_loop` keeps fresh.
//!
//! Architecture note: the original design (see `docs/diff-panel-design.md`)
//! considered folding diff rendering into `gpui_preview.rs`. That file is
//! already ~87 KB managing markdown / syntax / search / TOC / text selection;
//! mixing in another panel's state machine would double its surface. Diff
//! gets its own state struct and renderer that **shares the right-side dock
//! position** with preview but keeps state isolated.

#![cfg(feature = "gpui")]

use std::path::PathBuf;

use gpui::{div, prelude::*, px, rgb, Context, IntoElement, Styled};

use crate::git_panel::diff::{DiffFile, DiffLine};
use crate::git_panel::model::{FileStatus, GitFileEntry, WorkspaceGitState};
use crate::gpui_entry::GpuiShellView;

/// State of the right-side diff overlay. `Some(...)` on `GpuiShellView`
/// means the panel is open; `None` means closed.
///
/// The panel is **workspace-pinned** at open time: switching the active
/// workspace via the sidebar does *not* automatically retarget the panel,
/// because the user typically opens diff specifically to review the
/// workspace they were just working in. They can close + reopen to jump.
#[derive(Clone, Debug)]
pub(crate) struct DiffPanelState {
    /// Workspace this panel is bound to.
    pub(crate) workspace_id: String,
    /// Cached repo root, resolved when the panel opens.
    pub(crate) repo_root: PathBuf,
    /// Currently-selected file (repo-relative path). `None` until the user
    /// clicks an entry or the panel auto-selects the first changed file.
    pub(crate) selected_path: Option<String>,
    /// Whether the displayed diff is for the staging index (`git diff --cached`)
    /// rather than the worktree (`git diff`). V1 hardcodes `false`; the
    /// staged toggle ships in a later slice.
    pub(crate) show_staged: bool,
    /// True while a diff load is in flight — drives the "Loading…" line.
    pub(crate) loading: bool,
    /// Cached parsed diff for `selected_path`. Cleared when the selection
    /// changes; replaced by the background loader on completion. The vec is
    /// usually a single file (we always invoke `git diff -- <path>`), but
    /// the runner returns a `Vec` to stay symmetric with the parser.
    pub(crate) loaded_diff: Option<Vec<DiffFile>>,
    /// Last load error message, surfaced inline so the user can see why the
    /// diff is empty (e.g. file not in repo, git stderr).
    pub(crate) load_error: Option<String>,
    /// Commit message input. Survives across renders so the user's typing is
    /// preserved; created when the panel opens (needs a `&mut Window`).
    pub(crate) commit_input: gpui::Entity<gpui_component::input::InputState>,
    /// True while a `git commit` is in flight — disables the commit button
    /// and message input so a fast double-click can't fire twice.
    pub(crate) committing: bool,
    /// Last commit error to surface in the footer. Cleared whenever the user
    /// edits the message or starts a new commit attempt.
    pub(crate) commit_error: Option<String>,
    /// First-click latch for push confirmation. Set when the user clicks the
    /// Push button once; the second click within the same panel session
    /// actually invokes `git push`. Cleared on push success/failure or when
    /// the panel closes.
    pub(crate) push_confirming: bool,
    /// True while a `git push` is in flight.
    pub(crate) pushing: bool,
    /// Last push error (stderr verbatim, no parsing per design doc).
    pub(crate) push_error: Option<String>,
}

impl DiffPanelState {
    /// Build state for a freshly-opened panel. `commit_input` must be created
    /// by the caller via `cx.new(|cx| InputState::new(window, cx))` because
    /// `InputState::new` requires a `&mut Window` which freestanding code
    /// inside async closures can't reach.
    pub(crate) fn new(
        workspace_id: String,
        repo_root: PathBuf,
        selected_path: Option<String>,
        commit_input: gpui::Entity<gpui_component::input::InputState>,
    ) -> Self {
        Self {
            workspace_id,
            repo_root,
            selected_path,
            show_staged: false,
            loading: false,
            loaded_diff: None,
            load_error: None,
            commit_input,
            committing: false,
            commit_error: None,
            push_confirming: false,
            pushing: false,
            push_error: None,
        }
    }
}

/// Render the diff panel as a full-screen overlay anchored to the right edge.
///
/// Layout (right slide-out style):
///   ┌────────────────────────────────────────────────────────────┐
///   │ backdrop (semi-transparent, click closes panel)            │
///   │     ┌──────────────────────────────────────────────────┐   │
///   │     │ header: "Git Diff — <workspace>"          [×]    │   │
///   │     ├─────────────────┬────────────────────────────────┤   │
///   │     │ change list     │ unified diff for selected file │   │
///   │     │ M src/main.rs   │ @@ -1,3 +1,4 @@                │   │
///   │     │ A docs/foo.md   │  use foo;                      │   │
///   │     │ ?  scratch.txt  │ +use bar;                      │   │
///   │     │                 │  use baz;                      │   │
///   │     └─────────────────┴────────────────────────────────┘   │
///   └────────────────────────────────────────────────────────────┘
///
/// `git_state` is the cached `git status` result from the polling loop —
/// passed in so this function stays a pure render (no I/O, no view mutations
/// during paint).
pub(crate) fn render_diff_panel(
    state: &DiffPanelState,
    git_state: Option<&WorkspaceGitState>,
    workspace_name: &str,
    cx: &mut Context<GpuiShellView>,
) -> impl IntoElement {
    let file_rows = render_file_list(state, git_state, cx);
    let body = render_diff_body(state, git_state, cx);
    let workspace_label = workspace_name.to_string();

    div()
        .id("diff-panel-backdrop")
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .bg(gpui::rgba(0x000000aa))
        .flex()
        .items_stretch()
        .justify_end()
        // Block ALL mouse events from bubbling to the root shell view.
        // Without this, the user moving / dragging inside the modal still
        // hits the terminal pane's selection start (blue selection rect
        // appears behind the modal) and the split-handle resize hit-test
        // (lets them drag panes around as if the panel weren't there). The
        // root handlers don't see the modal as "blocking" — GPUI relies on
        // explicit stop_propagation, not z-index. `on_click` (already wired
        // for backdrop close) still fires because click is synthesized
        // from a matched mouse_down + mouse_up on the same element, and
        // stopping propagation in mouse_down doesn't cancel that
        // synthesis.
        .on_mouse_down(gpui::MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(gpui::MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(gpui::MouseButton::Middle, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_up(gpui::MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_up(gpui::MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_move(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_scroll_wheel(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_click(cx.listener(|this, _event, _window, cx| {
            this.diff_panel = None;
            cx.notify();
        }))
        .child(
            div()
                .id("diff-panel-modal")
                .w(px(880.0))
                .max_w(px(1200.0))
                .h_full()
                .bg(rgb(crate::theme::SURFACE))
                .border_l_1()
                .border_color(rgb(crate::theme::BORDER))
                .shadow_lg()
                .flex()
                .flex_col()
                .overflow_hidden()
                // Swallow clicks inside the panel so they don't reach the
                // backdrop click-to-close listener above.
                .on_click(|_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(render_header(workspace_label, cx))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_1()
                        .overflow_hidden()
                        .child(file_rows)
                        .child(body),
                ),
        )
}

fn render_header(workspace_label: String, cx: &mut Context<GpuiShellView>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(rgb(crate::theme::BORDER))
        .child(
            div()
                .flex()
                .flex_row()
                .items_baseline()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(crate::theme::TEXT))
                        .child("Git Diff"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(crate::theme::TEXT_DIM))
                        .child(format!("— {workspace_label}")),
                ),
        )
        .child(
            div()
                .id("diff-panel-close")
                .px(px(8.0))
                .py(px(2.0))
                .rounded(px(crate::theme::RADIUS_SM))
                .text_sm()
                .text_color(rgb(crate::theme::TEXT_DIM))
                .hover(|d| {
                    d.bg(rgb(crate::theme::SURFACE_RAISED))
                        .text_color(rgb(crate::theme::TEXT))
                })
                .cursor_pointer()
                .child("×")
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.diff_panel = None;
                    cx.notify();
                })),
        )
}

fn render_file_list(
    state: &DiffPanelState,
    git_state: Option<&WorkspaceGitState>,
    cx: &mut Context<GpuiShellView>,
) -> gpui::AnyElement {
    let mut col = div()
        .w(px(240.0))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .bg(rgb(crate::theme::SURFACE_DIM))
        .border_r_1()
        .border_color(rgb(crate::theme::BORDER))
        .overflow_hidden();

    let files: &[GitFileEntry] = git_state.map(|s| s.files.as_slice()).unwrap_or(&[]);

    col = col.child(
        div()
            .px_3()
            .py_2()
            .text_xs()
            .text_color(rgb(crate::theme::TEXT_DIM))
            .child(format!("Changes ({})", files.len())),
    );

    if files.is_empty() {
        col = col.child(
            div()
                .px_3()
                .py_2()
                .text_xs()
                .text_color(rgb(crate::theme::TEXT_DISABLED))
                .child("clean — nothing to review"),
        );
    } else {
        let mut list = div().flex().flex_col().flex_1().overflow_y_hidden();
        for entry in files {
            let path_str = entry.path.to_string_lossy().into_owned();
            let is_selected = state.selected_path.as_deref() == Some(path_str.as_str());
            let badge = effective_badge(entry);
            let badge_color = badge_color(entry);
            let row_path = path_str.clone();
            let can_stage = has_unstaged_changes(entry);
            let can_unstage = has_staged_changes(entry);
            let stage_path = path_str.clone();
            let unstage_path = path_str.clone();
            list = list.child(
                div()
                    .id(gpui::ElementId::Name(
                        format!("diff-file-{}", path_str).into(),
                    ))
                    .group(format!("diff-row-{}", path_str))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .px_3()
                    .py(px(4.0))
                    .when(is_selected, |d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
                    .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
                    .cursor_pointer()
                    .child(
                        div()
                            .w(px(14.0))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(badge_color))
                            .child(badge),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_sm()
                            .text_color(rgb(crate::theme::TEXT))
                            .child(path_str.clone()),
                    )
                    .when(can_unstage, |d| {
                        d.child(stage_button(
                            format!("diff-unstage-{}", path_str),
                            "−",
                            "unstage",
                            crate::theme::TEXT_DIM,
                            crate::theme::DANGER,
                            cx.listener(move |this, _event, _window, cx| {
                                this.unstage_diff_file(unstage_path.clone(), cx);
                            }),
                        ))
                    })
                    .when(can_stage, |d| {
                        d.child(stage_button(
                            format!("diff-stage-{}", path_str),
                            "+",
                            "stage",
                            crate::theme::TEXT_DIM,
                            crate::theme::SUCCESS,
                            cx.listener(move |this, _event, _window, cx| {
                                this.stage_diff_file(stage_path.clone(), cx);
                            }),
                        ))
                    })
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.select_diff_file(row_path.clone(), cx);
                    })),
            );
        }
        col = col.child(list);
    }

    col.into_any_element()
}

/// Small `+` / `−` icon-style button inside a file row. Click-stops
/// propagation so it doesn't trigger the row's select-file handler.
/// `title` is currently unused — kept on the signature so a future tooltip
/// pass has the label ready.
fn stage_button<F>(
    id: String,
    label: &'static str,
    _title: &'static str,
    base_color: u32,
    hover_color: u32,
    on_click: F,
) -> impl IntoElement
where
    F: Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
{
    div()
        .id(gpui::ElementId::Name(id.into()))
        .px(px(5.0))
        .rounded(px(crate::theme::RADIUS_SM))
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(base_color))
        .hover(|d| {
            d.bg(rgb(crate::theme::SURFACE_DIM))
                .text_color(rgb(hover_color))
        })
        .cursor_pointer()
        .child(label)
        .on_mouse_down(gpui::MouseButton::Left, |_e, _w, cx| {
            // Stop the row's parent click handler from firing alongside the
            // button click — the user clicked the icon, not the row.
            cx.stop_propagation();
        })
        .on_click(on_click)
}

fn has_staged_changes(entry: &GitFileEntry) -> bool {
    !matches!(entry.index_status, FileStatus::Unmodified)
}

fn has_unstaged_changes(entry: &GitFileEntry) -> bool {
    !matches!(
        entry.worktree_status,
        FileStatus::Unmodified | FileStatus::Ignored
    )
}

fn render_diff_body(
    state: &DiffPanelState,
    git_state: Option<&WorkspaceGitState>,
    cx: &mut Context<GpuiShellView>,
) -> gpui::AnyElement {
    // The body is now a vertical stack: the diff content (flex_1, scrollable)
    // on top, the commit footer pinned at the bottom. Always render the
    // footer — the commit input survives across selection changes so the
    // user's typing isn't lost when they click around the file list.
    let diff_content = render_diff_content(state, cx);
    let footer = render_commit_footer(state, git_state, cx);

    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(rgb(crate::theme::SURFACE))
        .child(diff_content)
        .child(footer)
        .into_any_element()
}

fn render_diff_content(
    state: &DiffPanelState,
    cx: &mut Context<GpuiShellView>,
) -> gpui::AnyElement {
    let mut body = div().flex_1().flex().flex_col().overflow_y_hidden();

    let Some(path) = state.selected_path.as_deref() else {
        body = body.child(
            div()
                .px_4()
                .py_3()
                .text_sm()
                .text_color(rgb(crate::theme::TEXT_DIM))
                .child("Pick a file on the left to view its diff."),
        );
        return body.into_any_element();
    };

    body = body.child(
        div()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(rgb(crate::theme::BORDER_DIM))
            .text_xs()
            .text_color(rgb(crate::theme::TEXT_DIM))
            .child(path.to_string()),
    );

    if state.loading {
        body = body.child(
            div()
                .px_4()
                .py_3()
                .text_sm()
                .text_color(rgb(crate::theme::TEXT_DIM))
                .child("Loading…"),
        );
        return body.into_any_element();
    }

    if let Some(err) = state.load_error.as_deref() {
        body = body.child(
            div()
                .px_4()
                .py_3()
                .text_sm()
                .text_color(rgb(crate::theme::DANGER))
                .child(format!("git diff failed: {err}")),
        );
        return body.into_any_element();
    }

    let Some(diff_files) = state.loaded_diff.as_ref() else {
        body = body.child(
            div()
                .px_4()
                .py_3()
                .text_sm()
                .text_color(rgb(crate::theme::TEXT_DIM))
                .child("(no diff loaded yet)"),
        );
        return body.into_any_element();
    };

    if diff_files.is_empty() {
        body = body.child(
            div()
                .px_4()
                .py_3()
                .text_sm()
                .text_color(rgb(crate::theme::TEXT_DIM))
                .child(
                    "git diff returned no hunks — the file may be untracked. Stage it first to see content.",
                ),
        );
        return body.into_any_element();
    }

    let scroll = div()
        .id("diff-panel-scroll")
        .flex_1()
        .overflow_y_scroll()
        .font_family("monospace")
        .text_xs();
    let scroll = render_diff_files(scroll, diff_files, cx);
    body = body.child(scroll);
    body.into_any_element()
}

/// Bottom-pinned commit composer. Always present so the message survives
/// across file-list clicks. Empty messages disable the button; in-flight
/// commits dim it and show "Committing…". Errors render in red below the
/// row of action buttons.
fn render_commit_footer(
    state: &DiffPanelState,
    git_state: Option<&WorkspaceGitState>,
    cx: &mut Context<GpuiShellView>,
) -> impl IntoElement {
    use gpui_component::Sizable;

    // IMPORTANT: do NOT call `state.commit_input.read(cx)` from this render
    // path. Reading an entity inside `Render::render()` double-leases it and
    // breaks every InputState in the same tree — the user's keystrokes stop
    // reaching the field, and the focus chain silently stalls. (See the
    // crate-wide memory note `gpui — never entity.read() inside
    // Render::render()`.) The commit message itself is read inside the
    // click handler instead — that runs after render, in an event context
    // where re-borrowing the entity is safe.
    let busy = state.committing;

    let staged_count = git_state
        .map(|s| s.files.iter().filter(|f| has_staged_changes(f)).count())
        .unwrap_or(0);
    let unstaged_count = git_state
        .map(|s| s.files.iter().filter(|f| has_unstaged_changes(f)).count())
        .unwrap_or(0);
    // A commit needs at least one staged file. We can't gate on
    // "non-empty message" here without reading the InputState (forbidden in
    // render); the empty-message check lives inside `commit_from_diff_panel`
    // and surfaces as a toast / `commit_error` if the user hits the button
    // with an empty message.
    let commit_enabled = staged_count > 0 && !busy;
    // Stage-all-and-commit is offered whenever there's any change to capture
    // — staged OR unstaged. (If everything is already staged, the operation
    // is a no-op `git add -A` followed by a regular commit, which is fine.)
    let total_changes = staged_count + unstaged_count;
    let stage_all_enabled = total_changes > 0 && !busy;

    // Push affordance is hidden when there's no upstream — there's nothing
    // to push to. When upstream exists but ahead == 0, the button is
    // disabled (visible-but-greyed) so the user knows where push would go.
    let ahead = git_state.map(|s| s.ahead).unwrap_or(0);
    let has_upstream = git_state.and_then(|s| s.upstream.as_ref()).is_some();
    let push_enabled = has_upstream && ahead > 0 && !state.pushing;

    let staged_label = match staged_count {
        0 => "No files staged — click + on a file to stage it.".to_string(),
        1 => "1 file staged — only this will be committed.".to_string(),
        n => format!("{n} files staged — only these will be committed."),
    };
    let staged_color = if staged_count == 0 {
        crate::theme::TEXT_DIM
    } else {
        crate::theme::SUCCESS
    };

    let mut footer = div()
        .flex_shrink_0()
        .border_t_1()
        .border_color(rgb(crate::theme::BORDER))
        .bg(rgb(crate::theme::SURFACE_DIM))
        .px_4()
        .py_3()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_xs()
                .text_color(rgb(staged_color))
                .child(staged_label),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(crate::theme::TEXT_DIM))
                .child("Commit message"),
        )
        .child(
            div().child(
                gpui_component::input::Input::new(&state.commit_input)
                    .small()
                    .cleanable(false)
                    .appearance(true),
            ),
        );

    if let Some(err) = state.commit_error.as_deref() {
        footer = footer.child(
            div()
                .text_xs()
                .text_color(rgb(crate::theme::DANGER))
                .child(format!("commit failed: {err}")),
        );
    }

    if let Some(err) = state.push_error.as_deref() {
        footer = footer.child(
            div()
                .text_xs()
                .text_color(rgb(crate::theme::DANGER))
                .child(format!("push failed: {err}")),
        );
    }

    let commit_label = if busy { "Committing…" } else { "Commit" };
    let push_label = match (state.pushing, state.push_confirming, ahead, has_upstream) {
        (true, _, _, _) => "Pushing…".to_string(),
        (_, true, n, _) => format!("Confirm push ↑{n}"),
        (_, false, n, true) if n > 0 => format!("Push ↑{n}"),
        (_, false, _, true) => "Push".to_string(),
        (_, false, _, false) => "Push (no upstream)".to_string(),
    };

    let stage_all_label = if busy {
        "Committing…".to_string()
    } else if unstaged_count == 0 && staged_count > 0 {
        // Everything is already staged — the stage-all action is then just
        // a regular commit. Surface that so the button label doesn't lie.
        format!("Stage all & Commit ({staged_count})")
    } else {
        format!("Stage all & Commit ({total_changes})")
    };

    footer = footer.child(
        div()
            .flex()
            .flex_row()
            .justify_end()
            .items_center()
            .gap(px(6.0))
            // Stage All on the left (the "I just want to capture everything"
            // shortcut), then plain Commit (only staged), then Push. Order
            // reads left→right as "scope of action" growing: nothing extra
            // staged → only staged → push to remote.
            // `.on_click` is attached ONLY when the button is enabled —
            // attaching it unconditionally lets greyed-out buttons still
            // fire their action, which surprised the user. Gating the
            // listener attach keeps disabled buttons fully inert (no
            // pointer cursor, no hover bg, no click). The action handlers
            // themselves still re-check their guards in case the state
            // races between paint and click on the same frame.
            .child(
                div()
                    .id("diff-stage-all-commit-btn")
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(crate::theme::RADIUS_SM))
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .when(stage_all_enabled, |d| {
                        d.bg(rgb(crate::theme::SUCCESS))
                            .text_color(rgb(crate::theme::SURFACE))
                            .hover(|d| d.bg(rgb(crate::theme::ACCENT)))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                this.stage_all_and_commit_from_diff_panel(window, cx);
                            }))
                    })
                    .when(!stage_all_enabled, |d| {
                        d.bg(rgb(crate::theme::SURFACE_RAISED))
                            .text_color(rgb(crate::theme::TEXT_DISABLED))
                    })
                    .child(stage_all_label),
            )
            .child(
                div()
                    .id("diff-commit-btn")
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(crate::theme::RADIUS_SM))
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .when(commit_enabled, |d| {
                        d.bg(rgb(crate::theme::ACCENT))
                            .text_color(rgb(crate::theme::SURFACE))
                            .hover(|d| d.bg(rgb(crate::theme::INFO)))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                this.commit_from_diff_panel(window, cx);
                            }))
                    })
                    .when(!commit_enabled, |d| {
                        d.bg(rgb(crate::theme::SURFACE_RAISED))
                            .text_color(rgb(crate::theme::TEXT_DISABLED))
                    })
                    .child(commit_label),
            )
            .child(
                div()
                    .id("diff-push-btn")
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(crate::theme::RADIUS_SM))
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .when(push_enabled && state.push_confirming, |d| {
                        // Confirm state: warning yellow — eye-catching so the
                        // user knows the next click is the real thing.
                        d.bg(rgb(crate::theme::WARNING))
                            .text_color(rgb(crate::theme::SURFACE))
                            .hover(|d| d.bg(rgb(crate::theme::WARNING)))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.push_from_diff_panel(cx);
                            }))
                    })
                    .when(push_enabled && !state.push_confirming, |d| {
                        d.bg(rgb(crate::theme::SURFACE_RAISED))
                            .text_color(rgb(crate::theme::TEXT))
                            .hover(|d| d.bg(rgb(crate::theme::BORDER)))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.push_from_diff_panel(cx);
                            }))
                    })
                    .when(!push_enabled, |d| {
                        d.bg(rgb(crate::theme::SURFACE_RAISED))
                            .text_color(rgb(crate::theme::TEXT_DISABLED))
                    })
                    .child(push_label),
            ),
    );

    footer
}

fn render_diff_files<E>(
    mut col: E,
    files: &[DiffFile],
    cx: &mut Context<GpuiShellView>,
) -> E
where
    E: ParentElement,
{
    for (file_idx, file) in files.iter().enumerate() {
        if file.is_binary {
            col = col.child(
                div()
                    .px_4()
                    .py_2()
                    .text_color(rgb(crate::theme::TEXT_DIM))
                    .child("Binary file — no textual diff."),
            );
            continue;
        }

        // Pick the syntax language from the file's display path. Falls back
        // to "" for unknown extensions, which makes `highlight_line` emit
        // one plain token covering the whole line — visually identical to
        // the pre-highlight rendering.
        let lang = crate::gpui_preview::detect_language(file.display_path());

        for (hunk_idx, hunk) in file.hunks.iter().enumerate() {
            let header_text = match hunk.function_context.as_deref() {
                Some(ctx) => format!(
                    "@@ -{},{} +{},{} @@ {}",
                    hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count, ctx
                ),
                None => format!(
                    "@@ -{},{} +{},{} @@",
                    hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
                ),
            };
            let hunk_id = format!("hunk-{file_idx}-{hunk_idx}");
            col = col.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .px_4()
                    .py_px()
                    .bg(gpui::rgba(0x81a2be20)) // ACCENT @ ~12% alpha
                    .text_color(rgb(crate::theme::ACCENT))
                    .child(div().flex_1().child(header_text))
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("stage-{hunk_id}").into()))
                            .px(px(5.0))
                            .rounded(px(crate::theme::RADIUS_SM))
                            .text_color(rgb(crate::theme::TEXT_DIM))
                            .hover(|d| {
                                d.bg(rgb(crate::theme::SURFACE_DIM))
                                    .text_color(rgb(crate::theme::SUCCESS))
                            })
                            .cursor_pointer()
                            .child("+ stage hunk")
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.apply_diff_hunk(file_idx, hunk_idx, false, cx);
                            })),
                    ),
            );

            for line in &hunk.lines {
                let (prefix, prefix_color, bg) = match line {
                    DiffLine::Added(_) => (
                        "+",
                        crate::theme::SUCCESS,
                        gpui::rgba(0xb5bd6826), // SUCCESS @ ~15% alpha
                    ),
                    DiffLine::Removed(_) => (
                        "-",
                        crate::theme::DANGER,
                        gpui::rgba(0xcc666626), // DANGER @ ~15% alpha
                    ),
                    DiffLine::Context(_) => (
                        " ",
                        crate::theme::TEXT_DIM,
                        gpui::rgba(0x00000000), // transparent
                    ),
                };
                // Tokenize the line CONTENT (without the +/- prefix) so that
                // keyword/string/number coloring runs over the actual code.
                // The prefix character is rendered as a separate span in
                // its own diff-status color so it stays unambiguous even on
                // tinted backgrounds.
                let content = line.text();
                let tokens = crate::gpui_preview::highlight_line(content, lang.as_str());
                let mut row = div()
                    .px_4()
                    .py_px()
                    .bg(bg)
                    .flex()
                    .flex_row()
                    .child(
                        div()
                            .w(px(10.0))
                            .flex_shrink_0()
                            .text_color(rgb(prefix_color))
                            .child(prefix.to_string()),
                    );
                if tokens.is_empty() {
                    // Empty diff line (e.g. blank "+" or "-") — still emit
                    // the row so vertical spacing matches surrounding ones.
                    row = row.child(
                        div()
                            .text_color(rgb(crate::theme::TEXT))
                            .child(content.to_string()),
                    );
                } else {
                    for tok in tokens {
                        let color = tok.color();
                        let text = tok.text().to_string();
                        row = row.child(div().text_color(rgb(color)).child(text));
                    }
                }
                col = col.child(row);
            }
        }
    }
    col
}

fn effective_badge(entry: &GitFileEntry) -> String {
    // Prefer the worktree status if it conveys a change; otherwise fall back
    // to the index status. Untracked is shown as `?`.
    let status = if matches!(entry.worktree_status, FileStatus::Unmodified) {
        entry.index_status
    } else {
        entry.worktree_status
    };
    status.badge().to_string()
}

fn badge_color(entry: &GitFileEntry) -> u32 {
    let status = if matches!(entry.worktree_status, FileStatus::Unmodified) {
        entry.index_status
    } else {
        entry.worktree_status
    };
    match status {
        FileStatus::Added | FileStatus::Untracked => crate::theme::SUCCESS,
        FileStatus::Deleted => crate::theme::DANGER,
        FileStatus::Modified | FileStatus::TypeChanged => crate::theme::WARNING,
        FileStatus::Renamed | FileStatus::Copied => crate::theme::ACCENT,
        FileStatus::Unmerged => crate::theme::DANGER,
        FileStatus::Ignored | FileStatus::Unmodified => crate::theme::TEXT_DIM,
    }
}
