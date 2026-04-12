//! Drag "ghost" views: small floating rectangles GPUI renders
//! under the cursor while a drag is in flight.
//!
//! Two kinds:
//!   * `DragTab` — a single terminal tab being dragged between
//!     panes by its title pill.
//!   * `DragWorkspace` — a workspace sidebar row being reordered.
//!
//! Each is a plain data struct with an `impl Render` that draws a
//! styled div containing the dragged entity's label. The actual
//! drop handling (reordering the tab list, moving the tab to a new
//! pane, reordering the workspace) stays inside the render closures
//! in `gpui_entry.rs` / `gpui_layout_renderer.rs` where it has
//! `cx.listener` access.
//!
//! Colors come from `crate::theme` tokens rather than raw hex —
//! this file was drifting through the Tomorrow Night palette by
//! hand so it's the simplest place to earn a bit more consistency
//! while we're moving it anyway.

#![cfg(feature = "gpui")]

use gpui::{rgb, px, div, Context, IntoElement, Render, Window, prelude::*};

use crate::theme;

/// Drag data for tab drag-and-drop between panes.
#[derive(Clone)]
pub(crate) struct DragTab {
    pub(crate) source_pane: amux_platform::terminal::manager::PaneId,
    pub(crate) tab_index: usize,
    pub(crate) title: String,
}

impl Render for DragTab {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py(px(4.0))
            .bg(rgb(theme::SURFACE_RAISED))
            .border_1()
            .border_color(rgb(theme::TEXT_DIM))
            .rounded(px(theme::RADIUS_SM))
            .text_xs()
            .text_color(rgb(theme::TEXT))
            .shadow_md()
            .child(self.title.clone())
    }
}

/// Drag data for workspace reordering in the sidebar.
#[derive(Clone)]
pub(crate) struct DragWorkspace {
    pub(crate) name: String,
    pub(crate) index: usize,
}

impl Render for DragWorkspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py(px(4.0))
            .bg(rgb(theme::SURFACE_RAISED))
            .border_1()
            .border_color(rgb(theme::TEXT_DIM))
            .rounded(px(theme::RADIUS_SM))
            .text_sm()
            .text_color(rgb(theme::TEXT))
            .shadow_md()
            .child(self.name.clone())
    }
}
