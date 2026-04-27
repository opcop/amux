//! Table of contents overlay for the preview panel.
//!
//! Bound keys:
//! * `[` / `]` — jump to the previous / next Markdown heading
//!   directly, no overlay. Uses `scroll_list_to_top` (ListState
//!   `scroll_to`), so the target heading always lands at the top of
//!   the viewport even if it was already partially visible.
//! * `o` / `:` — open the TOC overlay: a modal list of every
//!   heading, filterable by typing, Enter to jump, Esc to close.
//!   mdterm distinguishes `o` (plain TOC) from `:` (fuzzy) as two
//!   visually-distinct UIs; we collapse them into one overlay that
//!   always accepts typing, since the extra UI complexity isn't
//!   worth the pixel budget at amux's panel size.
//!
//! Scope: markdown previews only (code-file previews have no
//! headings — trying to open TOC on a `.rs` file no-ops).

#[cfg(feature = "gpui")]
use gpui::{px, div, rgb, prelude::*, AnyElement, FontWeight, IntoElement, ParentElement, Styled};

#[cfg(feature = "gpui")]
use crate::gpui_entry::GpuiShellView;
#[cfg(feature = "gpui")]
use crate::gpui_preview::{HeadingEntry, PreviewState};

/// Modal state for the TOC overlay.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct TocPickerState {
    /// Preview the overlay is bound to. If the active tab changes
    /// away from this path we drop the state on next Escape.
    pub path: String,
    pub query: String,
    /// Indices into `PreviewState.headings`. When `query` is empty,
    /// all headings are listed in document order. When non-empty,
    /// only headings whose `text` (and section content) contains
    /// the query, case-insensitive.
    pub matches: Vec<usize>,
    pub selected_index: usize,
}

#[cfg(feature = "gpui")]
impl TocPickerState {
    pub fn new(path: String) -> Self {
        Self {
            path,
            query: String::new(),
            matches: Vec::new(),
            selected_index: 0,
        }
    }

    /// Recompute `matches` against the current `query`. Empty query
    /// selects everything in document order, mirroring mdterm's
    /// "TOC shows all when filter is empty" shape.
    pub fn rebuild(&mut self, preview: &PreviewState) {
        self.matches.clear();
        if self.query.is_empty() {
            self.matches.extend(0..preview.headings.len());
        } else {
            let q = self.query.to_lowercase();
            for (i, h) in preview.headings.iter().enumerate() {
                // Match on heading text first, then fall back to
                // section content. This way typing "intro" finds a
                // heading literally titled "Introduction" even when
                // the body mentions "intro" elsewhere — intuitive
                // ranking is heading-first.
                let in_title = h.text.to_lowercase().contains(&q);
                let in_body = h.content_text.to_lowercase().contains(&q);
                if in_title || in_body {
                    self.matches.push(i);
                }
            }
        }
        if self.selected_index >= self.matches.len() {
            self.selected_index = self.matches.len().saturating_sub(1);
        }
    }

    /// Heading entry for the currently-selected row, if any.
    pub fn selected<'a>(&self, preview: &'a PreviewState) -> Option<&'a HeadingEntry> {
        let hi = *self.matches.get(self.selected_index)?;
        preview.headings.get(hi)
    }
}

#[cfg(feature = "gpui")]
impl GpuiShellView {
    /// Open (or re-open) the TOC overlay for the active preview.
    /// No-op on code-file previews — those have no `headings` entries.
    pub(crate) fn preview_toc_open(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(path) = self.active_preview_path() else { return };
        let Some(preview) = self.preview_tabs.get(&path) else { return };
        if preview.headings.is_empty() {
            // No headings → nothing to show. Bail silently; adding a
            // toast here would fire every time the user hits `o` on
            // a `.rs` file by accident.
            return;
        }
        let mut state = TocPickerState::new(path);
        state.rebuild(preview);
        self.preview_toc = Some(state);
        cx.notify();
    }

    pub(crate) fn preview_toc_close(&mut self, cx: &mut gpui::Context<Self>) {
        self.preview_toc = None;
        cx.notify();
    }

    pub(crate) fn preview_toc_input(&mut self, text: &str, cx: &mut gpui::Context<Self>) {
        let Some(state) = self.preview_toc.as_mut() else { return };
        state.query.push_str(text);
        state.selected_index = 0;
        let path = state.path.clone();
        if let Some(preview) = self.preview_tabs.get(&path)
            && let Some(state) = self.preview_toc.as_mut()
        {
            state.rebuild(preview);
        }
        cx.notify();
    }

    pub(crate) fn preview_toc_backspace(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(state) = self.preview_toc.as_mut() else { return };
        state.query.pop();
        state.selected_index = 0;
        let path = state.path.clone();
        if let Some(preview) = self.preview_tabs.get(&path)
            && let Some(state) = self.preview_toc.as_mut()
        {
            state.rebuild(preview);
        }
        cx.notify();
    }

    pub(crate) fn preview_toc_next(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(state) = self.preview_toc.as_mut()
            && !state.matches.is_empty()
        {
            state.selected_index = (state.selected_index + 1) % state.matches.len();
            cx.notify();
        }
    }

    pub(crate) fn preview_toc_prev(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(state) = self.preview_toc.as_mut()
            && !state.matches.is_empty()
        {
            state.selected_index = state
                .selected_index
                .checked_sub(1)
                .unwrap_or(state.matches.len() - 1);
            cx.notify();
        }
    }

    /// Jump to the selected heading and close the overlay.
    pub(crate) fn preview_toc_commit(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(state) = self.preview_toc.as_ref() else { return };
        let path = state.path.clone();
        let target_element_idx = self
            .preview_tabs
            .get(&path)
            .and_then(|p| state.selected(p).map(|h| h.element_idx));
        if let Some(idx) = target_element_idx
            && let Some(list_state) = self.preview_list_states.get(&path)
        {
            scroll_list_to_top(list_state, idx);
        }
        self.preview_toc = None;
        cx.notify();
    }

    /// `[` — jump to the previous heading. "Previous" = the heading
    /// whose element index is strictly less than the current scroll
    /// top's item index. No-wrap: bails if already at the first
    /// heading, matching mdterm's behavior.
    pub(crate) fn preview_jump_heading_prev(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(path) = self.active_preview_path() else { return };
        let Some(preview) = self.preview_tabs.get(&path) else { return };
        let Some(list_state) = self.preview_list_states.get(&path) else { return };
        if preview.headings.is_empty() {
            return;
        }
        let current_top = list_state.logical_scroll_top().item_ix;
        // Largest heading with element_idx < current_top.
        let target = preview
            .headings
            .iter()
            .rev()
            .find(|h| h.element_idx < current_top)
            .map(|h| h.element_idx);
        if let Some(idx) = target {
            scroll_list_to_top(list_state, idx);
            cx.notify();
        }
    }

    /// `]` — jump to the next heading. "Next" = heading whose
    /// element index is strictly greater than the current scroll top.
    pub(crate) fn preview_jump_heading_next(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(path) = self.active_preview_path() else { return };
        let Some(preview) = self.preview_tabs.get(&path) else { return };
        let Some(list_state) = self.preview_list_states.get(&path) else { return };
        if preview.headings.is_empty() {
            return;
        }
        let current_top = list_state.logical_scroll_top().item_ix;
        let target = preview
            .headings
            .iter()
            .find(|h| h.element_idx > current_top)
            .map(|h| h.element_idx);
        if let Some(idx) = target {
            scroll_list_to_top(list_state, idx);
            cx.notify();
        }
    }
}

/// Force the list to put `item_ix` at the top of the viewport.
///
/// **Why not `scroll_to_reveal_item`**: reveal-semantics keep the
/// scroll position unchanged if the target is already visible in
/// the viewport. For heading navigation the user expectation is
/// "jump there" — moving the target to the top of the view — even
/// when it was already partially on screen. `scroll_to` with
/// `offset_in_item = 0` pins the target as the new scroll top and
/// always reflows the view.
#[cfg(feature = "gpui")]
fn scroll_list_to_top(list_state: &gpui::ListState, item_ix: usize) {
    list_state.scroll_to(gpui::ListOffset {
        item_ix,
        offset_in_item: gpui::px(0.0),
    });
}

/// Modal TOC overlay. Rendered on top of the preview panel when
/// `preview_toc` is active and its `path` matches the current
/// preview. See `render_preview_panel` for the mount point.
#[cfg(feature = "gpui")]
pub fn render_toc_overlay(
    state: &TocPickerState,
    preview: &PreviewState,
    cx: &mut gpui::Context<GpuiShellView>,
) -> AnyElement {
    let query_display = if state.query.is_empty() {
        "▎ Filter headings...".to_string()
    } else {
        format!("{}▎", state.query)
    };
    let query_color = if state.query.is_empty() {
        rgb(crate::theme::TEXT_DIM)
    } else {
        rgb(crate::theme::TEXT)
    };

    div()
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .pt(px(80.0))
        // Backdrop — click outside to close.
        .child(
            div()
                .id("preview-toc-backdrop")
                .absolute()
                .inset_0()
                .bg(gpui::Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.4 })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.preview_toc = None;
                    cx.notify();
                })),
        )
        .child(
            div()
                .id("preview-toc-panel")
                .w(px(520.0))
                .max_h(px(420.0))
                .bg(rgb(crate::theme::SURFACE))
                .border_1()
                .border_color(rgb(crate::theme::BORDER))
                .rounded(px(8.0))
                .flex()
                .flex_col()
                .overflow_hidden()
                // Header: filter input + match counter.
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(rgb(crate::theme::SURFACE_RAISED))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(crate::theme::ACCENT))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("TOC"),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .text_color(query_color)
                                .child(query_display),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(crate::theme::TEXT_DIM))
                                .child(format!(
                                    "{}/{}",
                                    state.matches.len(),
                                    preview.headings.len()
                                )),
                        ),
                )
                // Rows.
                .child(
                    div()
                        .id(gpui::ElementId::Name("preview-toc-rows".into()))
                        .flex_1()
                        .overflow_y_scroll()
                        .children(if state.matches.is_empty() {
                            vec![
                                div()
                                    .px_3()
                                    .py_2()
                                    .text_xs()
                                    .text_color(rgb(crate::theme::TEXT_DIM))
                                    .child("No matching headings")
                                    .into_any_element(),
                            ]
                        } else {
                            state
                                .matches
                                .iter()
                                .enumerate()
                                .filter_map(|(row, heading_idx)| {
                                    let h = preview.headings.get(*heading_idx)?;
                                    let is_selected = row == state.selected_index;
                                    Some(render_toc_row(h, is_selected, row, cx))
                                })
                                .collect::<Vec<_>>()
                        }),
                )
                // Footer hint.
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1()
                        .border_t_1()
                        .border_color(rgb(crate::theme::SURFACE_RAISED))
                        .text_xs()
                        .text_color(rgb(crate::theme::TEXT_DIM))
                        .child("↑↓ navigate · Enter jump · Esc close")
                        .child(format!("{} headings", preview.headings.len())),
                ),
        )
        .into_any_element()
}

#[cfg(feature = "gpui")]
fn render_toc_row(
    h: &HeadingEntry,
    is_selected: bool,
    row: usize,
    cx: &mut gpui::Context<GpuiShellView>,
) -> AnyElement {
    // Indent by heading level so hierarchy is visible at a glance.
    // H1 = no indent, each extra level adds 12 px.
    let indent_px = ((h.level.saturating_sub(1)) as f32) * 12.0;
    let bg = if is_selected {
        rgb(crate::theme::SURFACE_RAISED)
    } else {
        rgb(crate::theme::SURFACE)
    };
    let text_color = if is_selected {
        rgb(crate::theme::TEXT)
    } else {
        rgb(crate::theme::TEXT_DIM)
    };
    let level_tag_color = match h.level {
        1 => rgb(0x81a2be), // blue
        2 => rgb(0xb5bd68), // green
        3 => rgb(0xf0c674), // yellow
        _ => rgb(crate::theme::TEXT_DIM),
    };
    let level_label = format!("H{}", h.level);
    let text = h.text.clone();
    let target_element_idx = h.element_idx;
    div()
        .id(gpui::ElementId::Name(format!("toc-row-{}", row).into()))
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py(px(5.0))
        .bg(bg)
        .cursor_pointer()
        .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
        .pl(px(12.0 + indent_px))
        .child(
            div()
                .text_xs()
                .text_color(level_tag_color)
                .font_weight(FontWeight::SEMIBOLD)
                .w(px(22.0))
                .child(level_label),
        )
        .child(
            div()
                .flex_1()
                .text_sm()
                .text_color(text_color)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(text),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            // Mouse-click jump: same path as Enter in the keyboard
            // handler — scroll the list to the heading and close.
            let path = this.preview_toc.as_ref().map(|s| s.path.clone());
            if let Some(path) = path
                && let Some(list_state) = this.preview_list_states.get(&path)
            {
                scroll_list_to_top(list_state, target_element_idx);
            }
            this.preview_toc = None;
            cx.notify();
        }))
        .into_any_element()
}
