//! Preview-panel keyboard shortcuts and file-watcher plumbing.
//!
//! Three features live here, grouped because they all pivot on "the
//! focused pane's active tab is a Preview tab":
//!
//! * **`Y` — copy full document**: reads the backing file and writes
//!   it to the clipboard. Equivalent to clicking the header Copy
//!   button, but without leaving the keyboard.
//!
//! * **`c` — copy first code block**: scans the preview's parsed
//!   elements for the first `CodeBlock` and copies its raw text.
//!   Handles both pure-code files (one block == whole file) and
//!   markdown with fenced code. "Nearest" semantics (mdterm's `c`
//!   behavior) would require threading the current viewport offset
//!   through; for now we take the first block, which covers the
//!   common one-code-block-per-doc case. Multi-block docs can still
//!   use Y + manual select.
//!
//! * **Auto-reload via `notify`**: single shared `RecommendedWatcher`
//!   created lazily on the first preview open. Each preview tab open
//!   calls `preview_watch_path`; close calls `preview_unwatch_path`.
//!   `poll_preview_reloads` drains the channel each render tick and
//!   schedules a background `PreviewState::load` for any path that
//!   still has a live tab entry. Handles atomic-save inode churn
//!   (write-tmp + rename) by re-watching on remove events.

#[cfg(feature = "gpui")]
use gpui::Context;
#[cfg(feature = "gpui")]
use notify::{EventKind, RecursiveMode, Watcher};

#[cfg(feature = "gpui")]
use crate::gpui_entry::GpuiShellView;
#[cfg(feature = "gpui")]
use crate::gpui_preview::PreviewElement;

#[cfg(feature = "gpui")]
impl GpuiShellView {
    /// Path of the focused pane's active tab, if it's a Preview tab.
    /// Returns `None` when the active tab is a Terminal or Browser —
    /// which is how callers gate preview-only shortcuts.
    pub(crate) fn active_preview_path(&self) -> Option<String> {
        use amux_platform::terminal::manager::TabKind;
        let pid = self.terminal_manager().active_pane_id()?;
        let pane = self.terminal_manager().get_pane(pid)?;
        match pane.active_tab_kind()? {
            TabKind::Preview { path } => Some(path.clone()),
            _ => None,
        }
    }

    /// Copy the entire backing file to the clipboard.
    ///
    /// Re-reads from disk instead of reconstructing from the parsed
    /// `PreviewState`: the parsed form drops whitespace-only lines and
    /// normalizes markdown, which is not what a user expects when they
    /// press Y to grab the raw file.
    pub(crate) fn preview_copy_full_document(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.active_preview_path() else { return };
        if let Ok(content) = std::fs::read_to_string(&path) {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(content));
        }
    }

    /// Copy the first code block in the active preview to the
    /// clipboard.
    ///
    /// For pure-code files the single `CodeBlock` element covers the
    /// whole file, so this matches `Y`. For markdown it copies just
    /// the first fenced block — useful when a spec opens with a
    /// snippet the reader wants to paste into a terminal.
    pub(crate) fn preview_copy_first_code_block(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.active_preview_path() else { return };
        let Some(preview) = self.preview_tabs.get(&path) else { return };
        for el in &preview.elements {
            if let PreviewElement::CodeBlock { formatted_lines, .. } = el {
                // `formatted_lines[i].1` is the raw line text after
                // token reassembly — joining with newlines reproduces
                // the source block's textual content (minus the fence
                // markers, which aren't useful for pasting).
                let text: String = formatted_lines
                    .iter()
                    .map(|(_num, code, _color)| code.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                return;
            }
        }
    }

    /// Start watching `path` for changes. Lazily creates the shared
    /// watcher on first call. No-ops if the watcher can't be built
    /// (missing platform backend, permission error) — auto-reload is
    /// a convenience, not a correctness requirement.
    pub(crate) fn preview_watch_path(&mut self, path: &str) {
        if self.preview_watcher.is_none() {
            let tx = self.preview_reload_tx.clone();
            match notify::RecommendedWatcher::new(
                move |res| {
                    let _ = tx.send(res);
                },
                notify::Config::default(),
            ) {
                Ok(w) => self.preview_watcher = Some(w),
                Err(e) => {
                    eprintln!("[amux-preview] failed to create watcher: {e}");
                    return;
                }
            }
        }
        if let Some(w) = self.preview_watcher.as_mut() {
            let _ = w.watch(std::path::Path::new(path), RecursiveMode::NonRecursive);
        }
    }

    /// Stop watching `path`. Safe to call on paths that were never
    /// watched (e.g. when the watcher failed to initialize).
    pub(crate) fn preview_unwatch_path(&mut self, path: &str) {
        if let Some(w) = self.preview_watcher.as_mut() {
            let _ = w.unwatch(std::path::Path::new(path));
        }
        // Drop the list state for this preview as well — no sense
        // carrying a stale ListState around once the tab is gone.
        self.preview_list_states.remove(path);
    }

    /// Ensure a `ListState` exists for every open markdown preview,
    /// with the correct item count. Call once per render tick before
    /// `render_layout`, so every preview has a state ready to render
    /// against. Handles three cases:
    ///
    /// * New preview: create a fresh `ListState` with the current
    ///   element count.
    /// * Element count changed (auto-reload swapped in a differently-
    ///   sized document): call `reset(new_count)` so the list's
    ///   internal item tree matches the new elements vec.
    /// * Unchanged: leave the existing state alone so scroll position
    ///   is preserved.
    ///
    /// Pure-code previews (single `CodeBlock`) still use the
    /// `UniformListScrollHandle` path — we skip them here.
    pub(crate) fn sync_preview_list_states(&mut self) {
        for (path, preview) in &self.preview_tabs {
            // Only markdown / mixed-element previews use `list`. Pure
            // code files route through the uniform_list path and
            // don't need a ListState.
            let is_code_only = matches!(
                preview.elements.as_slice(),
                [crate::gpui_preview::PreviewElement::CodeBlock { .. }]
            );
            if is_code_only {
                continue;
            }
            let needed = preview.elements.len();
            match self.preview_list_states.get(path) {
                Some(state) if state.item_count() == needed => {}
                Some(state) => {
                    state.reset(needed);
                }
                None => {
                    // `overdraw` = 100px: render ~1 extra viewport of
                    // content above/below the visible area so
                    // scrolling doesn't flash unrendered elements
                    // during momentum.
                    let state = gpui::ListState::new(
                        needed,
                        gpui::ListAlignment::Top,
                        gpui::px(100.0),
                    );
                    self.preview_list_states.insert(path.clone(), state);
                }
            }
        }
    }

    /// Drain pending filesystem events. For every path we still have
    /// a preview tab for, spawn a background reload. Called from the
    /// render-loop tick in `app_bootstrap.rs`.
    ///
    /// Atomic saves (write tmp + rename) produce a remove event that
    /// kills the inotify watch on the old inode; we re-watch in that
    /// case so subsequent changes still fire.
    pub(crate) fn poll_preview_reloads(&mut self, cx: &mut Context<Self>) {
        let mut changed: Vec<String> = Vec::new();
        let mut need_rewatch: Vec<String> = Vec::new();
        while let Ok(res) = self.preview_reload_rx.try_recv() {
            let Ok(event) = res else { continue };
            let is_modify = matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_));
            let is_remove = matches!(event.kind, EventKind::Remove(_));
            if !is_modify && !is_remove && !matches!(event.kind, EventKind::Any) {
                continue;
            }
            for path in event.paths {
                let s = path.to_string_lossy().to_string();
                if self.preview_tabs.contains_key(&s) {
                    if is_remove {
                        need_rewatch.push(s.clone());
                    }
                    changed.push(s);
                }
            }
        }
        changed.sort();
        changed.dedup();
        for path in need_rewatch {
            // Re-attach the watch on the new inode. Drop any stale
            // error from unwatch — the old inode may already be gone.
            self.preview_unwatch_path(&path);
            self.preview_watch_path(&path);
        }
        for path in changed {
            self.spawn_preview_reload(cx, path);
        }
    }

    /// Re-hydrate `preview_tabs` for every `TabKind::Preview` that
    /// survived in a restored layout. Called once on startup from
    /// the render-tick loop. Without this, a preview tab saved in
    /// `layouts.json` comes back with its pane-tree entry intact
    /// but an empty `preview_tabs` HashMap, and the renderer falls
    /// through to the "Preview: <path>" placeholder div forever.
    ///
    /// For each unique path: inserts the synchronous loading
    /// placeholder, attaches the filesystem watcher, and kicks off
    /// a background `PreviewState::load` that swaps in the real
    /// content when it finishes. Same pipeline as the initial
    /// `open_preview_file` path, minus the `add_preview_tab` step —
    /// the tab already exists in the pane tree.
    pub(crate) fn restore_preview_tabs_from_layouts(&mut self, cx: &mut Context<Self>) {
        use amux_platform::terminal::manager::TabKind;

        let mut paths: Vec<String> = Vec::new();
        for tm in self.workspace_terminals.values() {
            for pane in tm.all_panes() {
                for tab in &pane.tabs {
                    if let TabKind::Preview { path } = &tab.kind {
                        paths.push(path.clone());
                    }
                }
            }
        }
        // Dedup — the same path might live in multiple workspaces or
        // multiple panes within a workspace.
        paths.sort();
        paths.dedup();
        if paths.is_empty() {
            return;
        }

        for path in paths {
            // Skip anything already present (defensive: idempotent
            // even if the latch is bypassed somehow).
            if self.preview_tabs.contains_key(&path) {
                continue;
            }
            self.preview_tabs.insert(
                path.clone(),
                crate::gpui_preview::PreviewState::loading_placeholder(&path),
            );
            self.preview_watch_path(&path);
            self.spawn_preview_reload(cx, path);
        }
    }

    /// Background reload of a single preview path. Same pattern as
    /// `preview_open::open_preview_file`'s initial load — `unblock`
    /// keeps the pulldown-cmark / syntect work off the UI thread so
    /// a 100k-line log being re-saved doesn't stall a frame.
    fn spawn_preview_reload(&mut self, cx: &mut Context<Self>, path: String) {
        let task_path = path.clone();
        cx.spawn(async move |this, cx| {
            let loaded = smol::unblock(move || {
                crate::gpui_preview::PreviewState::load(&task_path)
            })
            .await;
            let _ = this.update(cx, move |view: &mut GpuiShellView, cx| {
                // Only swap if the tab still exists. The user may have
                // closed the preview between the event firing and the
                // reload completing.
                if !view.preview_tabs.contains_key(&path) {
                    return;
                }
                if let Some(state) = loaded {
                    view.preview_tabs.insert(path, state);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
