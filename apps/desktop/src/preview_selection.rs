//! Character-level text selection + clipboard copy for the preview
//! panel's markdown render path. See
//! `plans/preview-text-selection-spec.md` for the full design.
//!
//! Current steps delivered:
//! * Step 1: selection state types + coord conversion.
//! * Step 2: `SelectableText` wrapper replacing every text-producing
//!   div in `render_element`, passthrough to `StyledText`.
//! * Step 3: mouse-driven selection state (anchor/head in content
//!   coordinates, bounds cached via `on_prepaint`).
//! * Step 4: `SelectableText` promoted from `IntoElement` to
//!   `Element` with custom `paint`. Computes per-character selection
//!   ranges against the stored anchor/head and paints translucent
//!   selection background quads over the intersecting text. Mirrors
//!   gpui-component's `Inline` strategy but decoupled from their
//!   private types.

#[cfg(feature = "gpui")]
use std::cell::RefCell;
#[cfg(feature = "gpui")]
use std::ops::Range;
#[cfg(feature = "gpui")]
use std::rc::Rc;

#[cfg(feature = "gpui")]
use gpui::{
    App, BorderStyle, Bounds, Edges, Element, GlobalElementId, Half, HighlightStyle, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString, StyledText, Window,
    point, px, quad, transparent_black,
};

/// Persistent selection state for a single active markdown preview.
///
/// Scoped to a `(path, generation)` pair: switching preview tabs or
/// reloading the file invalidates the state (see
/// `plans/preview-text-selection-spec.md` §3 "Auto-reload interaction").
///
/// Coordinates in `anchor` / `head` are **content-relative**:
/// `window_pos - bounds.origin - scroll_offset`. Storing content
/// coords is the only way selection survives a scroll — window-space
/// coords would drift as the viewport moves.
///
/// `bounds` is the window-space rect of the markdown body container,
/// updated every frame via `on_prepaint` on the root div. It's the
/// reference point for every window↔content conversion.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct PreviewSelectionState {
    pub path: String,
    /// Matches `PreviewState.generation` at the moment the selection
    /// was started. Mismatch on any render tick means the underlying
    /// document was reloaded; selection must be dropped because
    /// element indices and byte offsets may have shifted.
    pub generation: u64,
    pub anchor: Option<Point<Pixels>>,
    pub head: Option<Point<Pixels>>,
    pub is_selecting: bool,
    pub bounds: Bounds<Pixels>,
}

#[cfg(feature = "gpui")]
impl PreviewSelectionState {
    /// Create an empty state bound to a preview path + load
    /// generation. No anchor/head yet — those get populated on the
    /// first mouse-down inside the preview body.
    pub fn new(path: String, generation: u64) -> Self {
        Self {
            path,
            generation,
            anchor: None,
            head: None,
            is_selecting: false,
            bounds: Bounds::default(),
        }
    }

    /// True when both endpoints are set and non-coincident. Coincident
    /// endpoints mean the user clicked without dragging; treating that
    /// as "no selection" avoids copying a zero-width range.
    pub fn has_nonempty_selection(&self) -> bool {
        match (self.anchor, self.head) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        }
    }
}

/// A contiguous byte range of selected text within one text run of
/// the preview. Produced by `SelectableText::paint` during the render
/// pass (Step 4), consumed by `extract_selected_text` when the user
/// hits `Cmd/Ctrl+C` (Step 5).
///
/// Indexing is byte-based (not char-based) to match `TextLayout`'s
/// API and slice-friendly extraction. Offsets are always on UTF-8
/// boundaries because the element-text-rendering side walks
/// `str::char_indices`, never arbitrary byte positions.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineSelection {
    pub location: TextLocation,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// Identifies *which* text run inside `PreviewState.elements` a
/// `SelectableText` belongs to. Used to map selection byte ranges
/// back to the originating element during extraction.
///
/// Encoding convention (interpreted by the extractor per element kind):
/// * Heading / Paragraph / ListItem / Blockquote: `sub_idx = 0` —
///   their text is a single concatenated run.
/// * CodeBlock: `sub_idx = line index` into `formatted_lines`.
/// * Table: `sub_idx = row * col_count + col` — flattened cell
///   index, with row `0` being the header row.
#[cfg(feature = "gpui")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextLocation {
    pub element_idx: usize,
    pub sub_idx: usize,
}

#[cfg(feature = "gpui")]
impl TextLocation {
    pub fn new(element_idx: usize, sub_idx: usize) -> Self {
        Self { element_idx, sub_idx }
    }
}

/// Shared side-channel from `SelectableText::paint` →
/// `extract_selected_text` (consumed on `Cmd/Ctrl+C`).
///
/// Every frame where selection is live, each text run that
/// intersects the selection rect pushes one `InlineSelection` here
/// describing its byte range. The copy handler then walks the sink
/// and rebuilds the selected plain text from the preview's
/// `PreviewElement` tree.
///
/// `Rc<RefCell>` not `Arc<Mutex>`: GPUI is single-threaded on the
/// main event loop, paint never runs concurrently with copy. The
/// cheaper Rc/RefCell is sufficient and matches gpui-component's
/// `InlineState` choice.
///
/// Cleared at the top of every `Render::render` tick (before any
/// paint). Stale ranges from a prior frame would otherwise leak
/// into the next copy.
#[cfg(feature = "gpui")]
pub type SelectionRangeSink = Rc<RefCell<Vec<InlineSelection>>>;

/// Per-frame selection geometry handed to every `SelectableText`.
///
/// Built once in `render_layout`'s Preview-tab arm from
/// `PreviewSelectionState.anchor/head` + current list scroll offset +
/// panel bounds, so every text run's paint can do a cheap window-coord
/// hit test without re-deriving the rect. `None` at the render-site
/// call means "no selection active — don't paint highlights."
///
/// Color is captured here too, sourced from `TerminalTheme::selection`
/// for visual consistency with the terminal pane's selection. `sink`
/// is where each intersecting text run pushes its byte range.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct SelectionRenderCtx {
    pub start_window: Point<Pixels>,
    pub end_window: Point<Pixels>,
    pub background: Hsla,
    pub sink: SelectionRangeSink,
}

/// A selectable text run in the preview body.
///
/// Implements `Element` directly (not `IntoElement`) so its `paint`
/// step can overlay selection background quads on top of the text.
/// The render path stays one-to-one with the previous `IntoElement`
/// passthrough — all existing call sites construct a
/// `SelectableText`, attach highlights + optional selection ctx,
/// and hand it to a parent div's `.child(...)`.
#[cfg(feature = "gpui")]
pub struct SelectableText {
    pub location: TextLocation,
    pub text: SharedString,
    pub highlights: Vec<(Range<usize>, HighlightStyle)>,
    pub selection_ctx: Option<SelectionRenderCtx>,
    /// Set during `request_layout` and reused in `paint`. We can't
    /// construct it until we know the default `TextStyle` (only
    /// available from `Window`), so this is an `Option` until
    /// request_layout fills it in.
    styled_text: Option<StyledText>,
}

#[cfg(feature = "gpui")]
impl SelectableText {
    pub fn new(location: TextLocation, text: impl Into<SharedString>) -> Self {
        Self {
            location,
            text: text.into(),
            highlights: Vec::new(),
            selection_ctx: None,
            styled_text: None,
        }
    }

    pub fn with_highlights(
        mut self,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    ) -> Self {
        self.highlights = highlights;
        self
    }

    /// Attach the current frame's selection geometry. `None` means no
    /// selection is active; the element still renders, just without
    /// any highlight overlay.
    pub fn with_selection_ctx(mut self, ctx: Option<SelectionRenderCtx>) -> Self {
        self.selection_ctx = ctx;
        self
    }
}

#[cfg(feature = "gpui")]
impl IntoElement for SelectableText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(feature = "gpui")]
impl Element for SelectableText {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // Build the concrete StyledText now that we have the
        // surrounding TextStyle. Using `with_default_highlights` folds
        // the default style + per-span HighlightStyles into runs in
        // one shot — simpler than manually interleaving runs.
        let text_style = window.text_style();
        let mut styled = StyledText::new(self.text.clone())
            .with_default_highlights(&text_style, self.highlights.clone());
        let (layout_id, _) = styled.request_layout(None, None, window, cx);
        self.styled_text = Some(styled);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> () {
        if let Some(ref mut styled) = self.styled_text {
            styled.prepaint(None, None, bounds, &mut (), window, cx);
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        // Order of paint: text first, selection quad on top. The
        // quad is translucent (theme selection color + ~0.4 alpha)
        // so the underlying text stays readable. This mirrors the
        // macOS native convention and gpui-component's `Inline`.
        let Some(styled) = self.styled_text.as_mut() else { return };
        let text_layout = styled.layout().clone();
        styled.paint(None, None, bounds, &mut (), &mut (), window, cx);

        let Some(ctx) = self.selection_ctx.as_ref() else { return };
        // Walk every character's window-space rect; accumulate a
        // contiguous byte range for those inside the selection rect.
        let line_height = window.line_height();
        let selection_range = compute_selection_byte_range(
            &self.text,
            &text_layout,
            ctx.start_window,
            ctx.end_window,
            line_height,
        );
        let Some(range) = selection_range else { return };
        // Record this text run's selected byte range for the copy
        // handler to consume on Cmd/Ctrl+C. Paint runs on the main
        // thread; the sink is a single-threaded Rc<RefCell<Vec>>
        // so this is a borrow-and-push, no locking.
        ctx.sink.borrow_mut().push(InlineSelection {
            location: self.location,
            byte_start: range.start,
            byte_end: range.end,
        });
        paint_selection_bg(
            range,
            &text_layout,
            &bounds,
            line_height,
            ctx.background,
            window,
        );
    }
}

/// Walk the text byte-by-character, asking `TextLayout` for each
/// character's position, and return a contiguous `(byte_start,
/// byte_end)` range covering every character whose center falls
/// inside the selection rect. Mirrors gpui-component's
/// `Inline::layout_selections` but pared down — we don't track links
/// or hover, just selection.
///
/// Returns `None` when no character is inside the rect (e.g. the
/// selection sits entirely above or below this text run).
#[cfg(feature = "gpui")]
fn compute_selection_byte_range(
    text: &str,
    text_layout: &gpui::TextLayout,
    selection_start: Point<Pixels>,
    selection_end: Point<Pixels>,
    line_height: Pixels,
) -> Option<Range<usize>> {
    let mut selection: Option<Range<usize>> = None;
    let mut offset = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let Some(pos) = text_layout.position_for_index(offset) else {
            offset += c.len_utf8();
            continue;
        };
        // Width of this char: distance to the next index's position
        // on the same visual line. If the next index is on a
        // wrapped line, fall back to half line_height as a
        // reasonable approximation.
        let mut char_width = line_height.half();
        if let Some(next_pos) = text_layout.position_for_index(offset + c.len_utf8()) {
            if next_pos.y == pos.y {
                char_width = next_pos.x - pos.x;
            }
        }
        if point_in_text_selection(pos, char_width, selection_start, selection_end, line_height) {
            let next_offset = offset + c.len_utf8();
            match selection.as_mut() {
                None => selection = Some(offset..next_offset),
                Some(r) => r.end = next_offset,
            }
        }
        offset += c.len_utf8();
    }
    selection
}

/// Paint the selection background as 1-3 quads depending on whether
/// the selection is single-line, spans two lines, or covers a middle
/// block between top and bottom partial lines.
#[cfg(feature = "gpui")]
fn paint_selection_bg(
    selection: Range<usize>,
    text_layout: &gpui::TextLayout,
    bounds: &Bounds<Pixels>,
    line_height: Pixels,
    background: Hsla,
    window: &mut Window,
) {
    let Some(start_pos) = text_layout.position_for_index(selection.start) else { return };
    let Some(end_pos) = text_layout.position_for_index(selection.end) else { return };
    let paint = |rect: Bounds<Pixels>, window: &mut Window| {
        window.paint_quad(quad(
            rect,
            px(0.0),
            background,
            Edges::default(),
            transparent_black(),
            BorderStyle::default(),
        ));
    };
    if start_pos.y == end_pos.y {
        // Single line: one quad from start.x → end.x at start.y.
        paint(
            Bounds::from_corners(
                start_pos,
                point(end_pos.x, end_pos.y + line_height),
            ),
            window,
        );
        return;
    }
    // Multi-line: first partial line, optional middle block, last partial line.
    paint(
        Bounds::from_corners(
            start_pos,
            point(bounds.right(), start_pos.y + line_height),
        ),
        window,
    );
    if end_pos.y > start_pos.y + line_height {
        paint(
            Bounds::from_corners(
                point(bounds.left(), start_pos.y + line_height),
                point(bounds.right(), end_pos.y),
            ),
            window,
        );
    }
    paint(
        Bounds::from_corners(
            point(bounds.left(), end_pos.y),
            point(end_pos.x, end_pos.y + line_height),
        ),
        window,
    );
}

/// Hit test: does the selection rect defined by `(selection_start,
/// selection_end)` cover the character at `pos` with width
/// `char_width`? Handles three cases:
///
/// 1. Point is on a line fully between top and bottom — selected.
/// 2. Point is on the top line — selected if x ≥ selection_start.x.
/// 3. Point is on the bottom line — selected if x ≤ selection_end.x.
///
/// The `point_in_line` helper checks `pos.y` falls within the
/// selection endpoint's rendered line using `line_height` tolerance.
/// If both endpoints are on the same rendered line, degenerate to a
/// simple left/right x test.
///
/// Copied (not vendored) from gpui-component's Inline — the algorithm
/// is small and well-tested, but licensing-wise we re-implement to
/// avoid importing private types. See the gpui-component reference
/// commit in `plans/preview-text-selection-spec.md` §4 Layer 4.
#[cfg(feature = "gpui")]
fn point_in_text_selection(
    pos: Point<Pixels>,
    char_width: Pixels,
    selection_start: Point<Pixels>,
    selection_end: Point<Pixels>,
    line_height: Pixels,
) -> bool {
    let point_in_line = |pt: Point<Pixels>| pt.y >= pos.y && pt.y < pos.y + line_height;
    let top = selection_start.y.min(selection_end.y);
    let bottom = selection_start.y.max(selection_end.y);
    // Test the character's midpoint — covers edge cases where the
    // drag endpoint lands exactly on a char's left edge.
    let x = pos.x + char_width.half();

    // Entirely above or below the selection band.
    if pos.y + line_height <= top || pos.y > bottom {
        return false;
    }

    // Both endpoints on the same rendered line of this text run:
    // single-axis test.
    if point_in_line(selection_start) && point_in_line(selection_end) {
        let left = selection_start.x.min(selection_end.x);
        let right = selection_start.x.max(selection_end.x);
        return x >= left && x <= right;
    }

    let (top_pt, bottom_pt) = if selection_start.y < selection_end.y {
        (selection_start, selection_end)
    } else {
        (selection_end, selection_start)
    };
    if point_in_line(top_pt) {
        // Partial top line — include everything to the right of the
        // starting x.
        return x >= top_pt.x;
    }
    if point_in_line(bottom_pt) {
        // Partial bottom line — include everything to the left of
        // the ending x.
        return x <= bottom_pt.x;
    }
    // Fully inside the middle block.
    true
}

// ─── Selection control methods on the view ─────────────────────

#[cfg(feature = "gpui")]
impl crate::gpui_entry::GpuiShellView {
    /// Resolve the preview path for the active pane's active tab, or
    /// return `None` if the active tab isn't a Preview. Duplicated
    /// here as a small helper so selection code stays decoupled from
    /// `preview_shortcuts.rs`'s `active_preview_path` — the two would
    /// otherwise form a subtle cross-module tie even though they
    /// share zero semantic intent beyond "look up the same thing".
    ///
    /// Kept crate-private; no stability guarantee.
    pub(crate) fn selection_active_preview_path(&self) -> Option<String> {
        self.active_preview_path()
    }

    /// True when the active pane shows a preview tab with a non-empty
    /// text selection recorded by the most recent paint pass.
    pub(crate) fn has_preview_text_selection(&self) -> bool {
        self.active_preview_path().is_some()
            && !self.preview_selection_ranges.borrow().is_empty()
    }

    /// Mouse-down handler for the markdown preview body. Starts a
    /// fresh selection at the click point, discarding any previous
    /// one. Called from the root body div's `on_mouse_down` listener.
    pub(crate) fn preview_selection_mouse_down(
        &mut self,
        window_pos: Point<Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        // Guard: TOC overlay owns all mouse input while it's visible.
        // A click there navigates / closes the overlay; it must not
        // also kick off a selection in the preview underneath.
        if self.preview_toc.is_some() {
            return;
        }
        let Some(path) = self.selection_active_preview_path() else { return };
        // Ensure the pane holding this preview tab is focused so that
        // Cmd+C routes through the copy handler (which checks the
        // active pane's active tab).
        let active_is_preview = {
            let tm = self.terminal_manager();
            tm.active_pane_id()
                .and_then(|pid| tm.get_pane(pid))
                .and_then(|p| p.active_tab_kind())
                .map(|k| matches!(k, amux_platform::terminal::manager::TabKind::Preview { path } if path == &*path))
                .unwrap_or(false)
        };
        if !active_is_preview {
            // Find the pane that has this preview as its active tab and focus it.
            let target: Option<amux_platform::terminal::manager::PaneId> = {
                let tm = self.terminal_manager();
                tm.pane_iter()
                    .find_map(|(id, pane)| {
                        match pane.active_tab_kind() {
                            Some(amux_platform::terminal::manager::TabKind::Preview { path: p }) if p == &path => Some(id.clone()),
                            _ => None,
                        }
                    })
            };
            if let Some(pid) = target {
                self.terminal_manager_mut().set_active_pane(&pid);
            }
        }
        let Some(bounds) = self.preview_body_bounds else {
            // Bounds not yet captured (first frame). Drop the click.
            // The next frame will have bounds set and the user can
            // retry; no state gets corrupted by this skip.
            return;
        };
        let scroll_offset = self.preview_list_scroll_offset(&path);
        let content_pos = window_to_content(window_pos, bounds, scroll_offset);

        // Capture the preview's generation counter at mouse-down.
        // Any later reload will bump PreviewState.generation and
        // `invalidate_preview_selection_if_stale` (called from the
        // render tick) will drop this selection. Default to 0 when
        // the preview state somehow doesn't exist; the next tick's
        // invalidation will catch the mismatch anyway.
        let generation = self
            .preview_tabs
            .get(&path)
            .map(|p| p.generation)
            .unwrap_or(0);
        let mut state = PreviewSelectionState::new(path, generation);
        state.bounds = bounds;
        state.anchor = Some(content_pos);
        state.head = Some(content_pos);
        state.is_selecting = true;
        self.preview_selection = Some(state);
        cx.notify();
    }

    /// Mouse-move handler: extend the selection's head while the
    /// button is held.
    pub(crate) fn preview_selection_mouse_move(
        &mut self,
        window_pos: Point<Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(state) = self.preview_selection.as_ref() else { return };
        if !state.is_selecting {
            return;
        }
        let Some(bounds) = self.preview_body_bounds else { return };
        let path = state.path.clone();
        let scroll_offset = self.preview_list_scroll_offset(&path);
        let content_pos = window_to_content(window_pos, bounds, scroll_offset);
        if let Some(state) = self.preview_selection.as_mut() {
            state.head = Some(content_pos);
            state.bounds = bounds;
        }
        cx.notify();
    }

    /// Mouse-up handler: stop extending, but keep the selection
    /// visible so the user can read / `Cmd+C` it.
    pub(crate) fn preview_selection_mouse_up(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(state) = self.preview_selection.as_mut()
            && state.is_selecting
        {
            state.is_selecting = false;
            // A "click without drag" (anchor == head, which
            // `has_nonempty_selection` already reports as empty)
            // leaves selection state in place but contributes nothing
            // visible. Step 4's paint gates on `has_nonempty_selection`
            // so no highlight appears; Step 7 will drop the state
            // entirely in the click-vs-drag polish pass.
            cx.notify();
        }
    }

    /// Drop selection state. Called on Escape, on tab switch, or on
    /// mouse-down outside the body region.
    pub(crate) fn preview_selection_clear(&mut self, cx: &mut gpui::Context<Self>) {
        if self.preview_selection.is_some() {
            self.preview_selection = None;
            cx.notify();
        }
    }

    /// Extract the currently-selected text from the active preview
    /// and write it to the system clipboard. Called by the Cmd/Ctrl+C
    /// handler in `gpui_input_handler` when the active tab is Preview
    /// and a non-empty selection exists.
    ///
    /// Reads byte ranges from `preview_selection_ranges` (populated
    /// by the most recent paint) and walks the preview's
    /// `PreviewElement` tree to slice out the text.
    pub(crate) fn copy_preview_selection(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(path) = self.active_preview_path() else { return };
        let Some(preview) = self.preview_tabs.get(&path) else { return };
        let ranges = self.preview_selection_ranges.borrow();
        let text = extract_selected_text(preview, &ranges);
        drop(ranges);
        if text.is_empty() {
            return;
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
    }

    /// Called at the top of every render tick, before any paint runs,
    /// to drop byte ranges collected on the previous frame. Ranges
    /// that survive into the next frame would double-copy and
    /// mis-attribute bytes to post-scroll text.
    pub(crate) fn clear_preview_selection_ranges(&self) {
        self.preview_selection_ranges.borrow_mut().clear();
    }

    /// Drop the active selection when it no longer belongs to the
    /// currently-focused preview. Three triggers, all checked every
    /// render tick:
    ///
    /// * **Tab closed**: `preview_tabs.get(state.path)` is `None` —
    ///   no document to map bytes against, clear.
    /// * **Tab reloaded**: `PreviewState.generation` has advanced
    ///   since mouse-down captured it — byte offsets may point into
    ///   stale text, clear (see `plans/preview-text-selection-spec.md`
    ///   §3 "Auto-reload interaction").
    /// * **Focus moved away from this preview**: the active pane's
    ///   active tab is no longer this selection's preview — either
    ///   the user switched to a terminal tab, clicked a different
    ///   preview tab, or focused another pane. Keeping the stored
    ///   selection invisible-but-live would lead to surprising
    ///   copy-paste behavior on tab-switch-back.
    ///
    /// Cheap: two map lookups per tick when a selection is active,
    /// zero work when it isn't.
    pub(crate) fn invalidate_preview_selection_if_stale(&mut self) {
        let Some(state) = self.preview_selection.as_ref() else { return };
        let stale_by_state = match self.preview_tabs.get(&state.path) {
            None => true,
            Some(p) => p.generation != state.generation,
        };
        let stale_by_focus = self
            .active_preview_path()
            .map(|p| p != state.path)
            .unwrap_or(true);
        if stale_by_state || stale_by_focus {
            self.preview_selection = None;
            self.preview_selection_ranges.borrow_mut().clear();
        }
    }

    /// Current scroll offset of the markdown list for `path`, if it
    /// has an associated `ListState`. Code-only previews don't have
    /// one (they use the uniform_list path) — selection in that path
    /// is out of scope per the spec, so `Point::default()` is fine.
    fn preview_list_scroll_offset(&self, path: &str) -> Point<Pixels> {
        self.preview_list_states
            .get(path)
            .map(|s| s.scroll_px_offset_for_scrollbar())
            .unwrap_or_default()
    }
}

// ─── Coordinate conversion ──────────────────────────────────────

/// Convert a window-space mouse position into the selection state's
/// content coordinate space. Inverse of `content_to_window`.
///
/// Stored anchor/head coords are computed this way so that scrolling
/// the list doesn't drag the selection highlight along with the
/// viewport — the selection stays pinned to the text it was started
/// on. `scroll_offset` is whatever the list reports as its current
/// scroll displacement; for a non-scrollable preview it should be
/// `Point::default()`.
#[cfg(feature = "gpui")]
pub fn window_to_content(
    window_pos: Point<Pixels>,
    bounds: Bounds<Pixels>,
    scroll_offset: Point<Pixels>,
) -> Point<Pixels> {
    window_pos - bounds.origin - scroll_offset
}

/// Convert a content-space coordinate back to window space. Used by
/// the paint path when comparing stored selection endpoints against
/// per-character rects that `TextLayout::position_for_index` returns
/// in window space.
#[cfg(feature = "gpui")]
pub fn content_to_window(
    content_pos: Point<Pixels>,
    bounds: Bounds<Pixels>,
    scroll_offset: Point<Pixels>,
) -> Point<Pixels> {
    content_pos + bounds.origin + scroll_offset
}

// ─── Selected-text extraction ──────────────────────────────────

/// Build the clipboard string for a non-empty selection by walking
/// `ranges` in document order, slicing each text run's rendered
/// string by byte offsets, and joining adjacent entries with the
/// appropriate separator.
///
/// Separator logic matches the spec §4 "Block-type conventions":
/// * Same element, same sub_idx: no separator (ranges are already
///   merged by paint into one entry per run — this case shouldn't
///   show up, but the code is defensive).
/// * Adjacent CodeBlock lines (same element, sub_idx differs by 1):
///   `\n`.
/// * Table cells in the same row (same element, same row): `" "`.
/// * Table cells across rows, or across elements: `\n`.
///
/// Leading/trailing whitespace is trimmed so a user dragging across
/// a block's empty trailing line doesn't end up pasting a dangling
/// newline.
#[cfg(feature = "gpui")]
pub fn extract_selected_text(
    preview: &crate::gpui_preview::PreviewState,
    ranges: &[InlineSelection],
) -> String {
    let mut sorted: Vec<InlineSelection> = ranges.to_vec();
    sorted.sort_by_key(|r| (r.location.element_idx, r.location.sub_idx, r.byte_start));

    let mut out = String::new();
    let mut prev: Option<TextLocation> = None;
    for r in &sorted {
        let Some(el) = preview.elements.get(r.location.element_idx) else { continue };
        let Some(run_text) = text_run_for(el, r.location.sub_idx) else { continue };
        // Clamp the end to run_text.len() to survive any future race
        // where the document reloads mid-frame and byte offsets
        // outrun the new run length. Step 6's generation counter
        // invalidates selection on reload, but belt-and-suspenders.
        let byte_end = r.byte_end.min(run_text.len());
        if r.byte_start >= byte_end {
            continue;
        }
        // Byte-range must land on char boundaries — upstream only
        // produces them that way, but guard against pathological
        // inputs that would otherwise panic on slicing.
        if !run_text.is_char_boundary(r.byte_start) || !run_text.is_char_boundary(byte_end) {
            continue;
        }
        let sliced = &run_text[r.byte_start..byte_end];
        if sliced.is_empty() {
            continue;
        }

        if let Some(prev) = prev {
            out.push_str(separator_between(preview, prev, r.location));
        }
        out.push_str(sliced);
        prev = Some(r.location);
    }
    out.trim().to_string()
}

/// Plain text for a single (element, sub_idx) text run — the same
/// string that `SelectableText` was asked to render. Returns `None`
/// when sub_idx doesn't map to a text run (out of bounds, or element
/// kind has no text at all like HorizontalRule).
#[cfg(feature = "gpui")]
fn text_run_for(
    el: &crate::gpui_preview::PreviewElement,
    sub_idx: usize,
) -> Option<String> {
    use crate::gpui_preview::PreviewElement;
    match el {
        PreviewElement::Heading { text, .. } if sub_idx == 0 => Some(text.clone()),
        PreviewElement::Paragraph { spans }
        | PreviewElement::Blockquote { spans }
        | PreviewElement::ListItem { spans, .. }
            if sub_idx == 0 =>
        {
            Some(spans.iter().map(|s| s.text.as_str()).collect())
        }
        PreviewElement::CodeBlock { formatted_lines, .. } => {
            formatted_lines.get(sub_idx).map(|(_, text, _)| text.clone())
        }
        PreviewElement::Table { headers, rows } => {
            // Encoding from render_element: row=0 is headers, row>=1
            // is rows[row-1]. sub_idx = row * col_count + col.
            let col_count = headers.len();
            if col_count == 0 {
                return None;
            }
            let row = sub_idx / col_count;
            let col = sub_idx % col_count;
            let cell_spans = if row == 0 {
                headers.get(col)?
            } else {
                rows.get(row - 1)?.get(col)?
            };
            Some(cell_spans.iter().map(|s| s.text.as_str()).collect())
        }
        _ => None,
    }
}

/// Separator inserted between two consecutive InlineSelections in
/// the sorted extraction walk. See `extract_selected_text`'s doc for
/// the rules.
#[cfg(feature = "gpui")]
fn separator_between(
    preview: &crate::gpui_preview::PreviewState,
    prev: TextLocation,
    curr: TextLocation,
) -> &'static str {
    if prev == curr {
        return "";
    }
    if prev.element_idx != curr.element_idx {
        return "\n";
    }
    // Same element, different sub_idx.
    use crate::gpui_preview::PreviewElement;
    match preview.elements.get(prev.element_idx) {
        Some(PreviewElement::Table { headers, .. }) => {
            let col_count = headers.len();
            if col_count == 0 {
                return "\n";
            }
            if prev.sub_idx / col_count == curr.sub_idx / col_count {
                " "
            } else {
                "\n"
            }
        }
        _ => "\n",
    }
}

// ─── Tests for extraction ──────────────────────────────────────

#[cfg(all(test, feature = "gpui"))]
mod extraction_tests {
    use super::*;
    use crate::gpui_preview::{PreviewElement, PreviewState, TextSpan};

    fn span(text: &str) -> TextSpan {
        TextSpan {
            text: text.into(),
            bold: false,
            italic: false,
            code: false,
            link_url: None,
        }
    }

    fn make_preview(elements: Vec<PreviewElement>) -> PreviewState {
        PreviewState {
            file_path: "/tmp/t.md".into(),
            file_name: "t.md".into(),
            elements,
            headings: Vec::new(),
            generation: 1,
        }
    }

    fn sel(e: usize, s: usize, b0: usize, b1: usize) -> InlineSelection {
        InlineSelection {
            location: TextLocation::new(e, s),
            byte_start: b0,
            byte_end: b1,
        }
    }

    #[test]
    fn single_span_slice() {
        let preview = make_preview(vec![PreviewElement::Paragraph {
            spans: vec![span("hello world")],
        }]);
        let got = extract_selected_text(&preview, &[sel(0, 0, 6, 11)]);
        assert_eq!(got, "world");
    }

    #[test]
    fn cross_span_concatenates_plain_text() {
        // Spans "bold " + "rest" — selection covers both. Bold markup
        // is lost (spec: "rendered glyphs only").
        let preview = make_preview(vec![PreviewElement::Paragraph {
            spans: vec![
                TextSpan { bold: true, ..span("bold ") },
                span("rest"),
            ],
        }]);
        let got = extract_selected_text(&preview, &[sel(0, 0, 0, 9)]);
        assert_eq!(got, "bold rest");
    }

    #[test]
    fn cross_element_joins_with_newline() {
        let preview = make_preview(vec![
            PreviewElement::Heading { level: 1, text: "Title".into() },
            PreviewElement::Paragraph { spans: vec![span("body")] },
        ]);
        let got = extract_selected_text(
            &preview,
            &[sel(0, 0, 0, 5), sel(1, 0, 0, 4)],
        );
        assert_eq!(got, "Title\nbody");
    }

    #[test]
    fn codeblock_lines_join_with_newline() {
        let preview = make_preview(vec![PreviewElement::CodeBlock {
            language: "rust".into(),
            formatted_lines: vec![
                ("1".into(), "fn main() {".into(), 0xffffff),
                ("2".into(), "    let x = 1;".into(), 0xffffff),
                ("3".into(), "}".into(), 0xffffff),
            ],
            total_lines: 3,
        }]);
        let got = extract_selected_text(
            &preview,
            &[
                sel(0, 0, 0, 11),
                sel(0, 1, 0, 14),
                sel(0, 2, 0, 1),
            ],
        );
        assert_eq!(got, "fn main() {\n    let x = 1;\n}");
    }

    #[test]
    fn table_cells_same_row_join_with_space() {
        // 2x2 table: headers [A, B], rows [[1, 2]]. Select A (row 0
        // col 0) + B (row 0 col 1) + 1 (row 1 col 0) + 2 (row 1 col 1).
        let preview = make_preview(vec![PreviewElement::Table {
            headers: vec![vec![span("A")], vec![span("B")]],
            rows: vec![vec![vec![span("1")], vec![span("2")]]],
        }]);
        // col_count=2. sub_idx: row 0 = 0..2 (A, B), row 1 = 2..4 (1, 2).
        let got = extract_selected_text(
            &preview,
            &[
                sel(0, 0, 0, 1),
                sel(0, 1, 0, 1),
                sel(0, 2, 0, 1),
                sel(0, 3, 0, 1),
            ],
        );
        // Same row → space; across rows → newline.
        assert_eq!(got, "A B\n1 2");
    }

    #[test]
    fn empty_ranges_produce_empty_string() {
        let preview = make_preview(vec![PreviewElement::Paragraph {
            spans: vec![span("hello")],
        }]);
        assert_eq!(extract_selected_text(&preview, &[]), "");
    }

    #[test]
    fn whitespace_only_slice_trims_to_empty() {
        let preview = make_preview(vec![PreviewElement::Paragraph {
            spans: vec![span("   ")],
        }]);
        assert_eq!(extract_selected_text(&preview, &[sel(0, 0, 0, 3)]), "");
    }

    #[test]
    fn cjk_byte_boundaries_preserved() {
        // `你好` is 2 CJK chars, 6 UTF-8 bytes (each 3 bytes). Select
        // both chars by byte range 0..6. Mid-codepoint offsets
        // (1, 2, 4, 5) must never appear in real paint output, but
        // guard explicitly.
        let preview = make_preview(vec![PreviewElement::Paragraph {
            spans: vec![span("你好world")],
        }]);
        let got = extract_selected_text(&preview, &[sel(0, 0, 0, 6)]);
        assert_eq!(got, "你好");
    }

    #[test]
    fn emoji_byte_boundaries_preserved() {
        // "🎉" is 4 UTF-8 bytes. Select just the emoji.
        let preview = make_preview(vec![PreviewElement::Paragraph {
            spans: vec![span("🎉party")],
        }]);
        let got = extract_selected_text(&preview, &[sel(0, 0, 0, 4)]);
        assert_eq!(got, "🎉");
    }

    #[test]
    fn out_of_bounds_byte_end_is_clamped() {
        // Paint shouldn't produce this, but defensive.
        let preview = make_preview(vec![PreviewElement::Paragraph {
            spans: vec![span("ab")],
        }]);
        let got = extract_selected_text(&preview, &[sel(0, 0, 0, 999)]);
        assert_eq!(got, "ab");
    }

    #[test]
    fn mid_codepoint_byte_offset_is_skipped() {
        // Byte offset 1 falls inside "你" (3 bytes). Range 1..6 is
        // invalid — extraction must skip rather than panic on slicing.
        let preview = make_preview(vec![PreviewElement::Paragraph {
            spans: vec![span("你好")],
        }]);
        let got = extract_selected_text(&preview, &[sel(0, 0, 1, 6)]);
        assert_eq!(got, "");
    }
}

#[cfg(all(test, feature = "gpui"))]
mod tests {
    use super::*;
    use gpui::{point, size};

    fn bounds_at(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds {
            origin: point(px(x), px(y)),
            size: size(px(w), px(h)),
        }
    }

    #[test]
    fn window_to_content_subtracts_origin_and_scroll() {
        let bounds = bounds_at(100.0, 50.0, 400.0, 300.0);
        let scroll = point(px(0.0), px(20.0));
        let got = window_to_content(point(px(150.0), px(200.0)), bounds, scroll);
        // (150, 200) - origin(100, 50) - scroll(0, 20) = (50, 130)
        assert_eq!(got, point(px(50.0), px(130.0)));
    }

    #[test]
    fn content_to_window_adds_origin_and_scroll() {
        let bounds = bounds_at(100.0, 50.0, 400.0, 300.0);
        let scroll = point(px(0.0), px(20.0));
        let got = content_to_window(point(px(50.0), px(130.0)), bounds, scroll);
        // (50, 130) + origin(100, 50) + scroll(0, 20) = (150, 200)
        assert_eq!(got, point(px(150.0), px(200.0)));
    }

    #[test]
    fn round_trip_identity() {
        // Any window pos, round-tripped through content space and
        // back, must land exactly where it started. If this breaks,
        // selection stored on mouse-down won't match paint-time hit
        // tests, and highlights will render off by a few pixels.
        let bounds = bounds_at(37.5, 12.25, 400.0, 300.0);
        let scroll = point(px(0.0), px(75.0));
        for (x, y) in &[
            (0.0, 0.0),
            (200.0, 150.0),
            (-5.0, 10.0), // mouse above bounds — still must round-trip
            (1000.0, 1000.0),
        ] {
            let original = point(px(*x), px(*y));
            let content = window_to_content(original, bounds, scroll);
            let back = content_to_window(content, bounds, scroll);
            assert_eq!(
                back, original,
                "round-trip failed for ({x}, {y})"
            );
        }
    }

    #[test]
    fn round_trip_identity_zero_scroll() {
        let bounds = bounds_at(0.0, 0.0, 400.0, 300.0);
        let scroll = Point::default();
        let original = point(px(42.0), px(99.0));
        let back = content_to_window(
            window_to_content(original, bounds, scroll),
            bounds,
            scroll,
        );
        assert_eq!(back, original);
    }

    #[test]
    fn has_nonempty_selection_requires_both_endpoints_distinct() {
        let mut s = PreviewSelectionState::new("/tmp/x.md".into(), 0);
        assert!(!s.has_nonempty_selection(), "no endpoints → no selection");

        s.anchor = Some(point(px(10.0), px(20.0)));
        assert!(!s.has_nonempty_selection(), "only anchor → no selection");

        s.head = Some(point(px(10.0), px(20.0)));
        assert!(
            !s.has_nonempty_selection(),
            "coincident endpoints → no selection (click without drag)"
        );

        s.head = Some(point(px(10.0), px(21.0)));
        assert!(s.has_nonempty_selection(), "distinct endpoints → selection");
    }

    #[test]
    fn generation_counter_is_strictly_monotonic() {
        // Every call to next_preview_generation returns a strictly
        // larger value. The invalidator depends on `old != new` ⇒
        // reload; equal generations must only occur when the state
        // hasn't been rebuilt. A non-monotonic counter would let
        // two unrelated loads share a generation, masking reloads
        // and leaving stale selections live.
        let a = crate::gpui_preview::next_preview_generation();
        let b = crate::gpui_preview::next_preview_generation();
        let c = crate::gpui_preview::next_preview_generation();
        assert!(a < b, "generation must increase: {a} < {b}");
        assert!(b < c, "generation must increase: {b} < {c}");
    }
}
