//! In-document search for the preview panel (`/`, `n`, `N`).
//!
//! Supports both code-file and markdown previews.
//!
//! Workflow:
//! * `/` while a preview is active → open the search bar at the
//!   bottom of the panel. The bar is modal: typed chars build up the
//!   query, Enter commits, Escape closes.
//! * On commit, we compute all matches (case-insensitive literal match)
//!   and for code files center the first hit via scroll. For markdown,
//!   matches are counted and the user scrolls manually.
//! * `n` / `N` cycle through matches.
//!
//! Literal-only for now (no regex).

#[cfg(feature = "gpui")]
use gpui::Context;

#[cfg(feature = "gpui")]
use crate::gpui_entry::GpuiShellView;
#[cfg(feature = "gpui")]
use crate::gpui_preview::{PreviewElement, PreviewState};

/// A single match within a preview.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct PreviewMatch {
    /// Index into `CodeBlock::formatted_lines` (code) or
    /// `PreviewState.elements` (markdown).
    pub line_idx: usize,
    /// Character offsets into the text of that line/element.
    #[allow(dead_code)]
    pub char_start: usize,
    #[allow(dead_code)]
    pub char_end: usize,
}

/// Preview search state. Owned by `GpuiShellView` when a search is
/// active; dropped on Escape or tab switch.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct PreviewSearchState {
    /// Path of the preview this search belongs to. Searches do not
    /// follow the user across preview tabs — switching tabs drops
    /// the state, mirroring how mdterm rebuilds search on file
    /// switch.
    pub path: String,
    pub query: String,
    /// Input mode: chars build up the query, Enter commits. After
    /// commit the bar stays visible for `n`/`N` navigation until
    /// Escape closes the whole thing.
    pub input_active: bool,
    pub matches: Vec<PreviewMatch>,
    pub current_idx: usize,
}

#[cfg(feature = "gpui")]
impl PreviewSearchState {
    pub fn new(path: String) -> Self {
        Self {
            path,
            query: String::new(),
            input_active: true,
            matches: Vec::new(),
            current_idx: 0,
        }
    }

    /// Recompute matches against the current query. Called after
    /// every edit to the query string. Works for both code and markdown.
    pub fn rebuild(&mut self, preview: &PreviewState) {
        self.matches.clear();
        self.current_idx = 0;
        if self.query.is_empty() {
            return;
        }
        let query_lower = self.query.to_lowercase();

        // Try code-file path first (single CodeBlock element)
        if let Some(code_lines) = code_lines_of(preview) {
            for (line_idx, code) in code_lines.iter().enumerate() {
                self.find_in_text(code, line_idx, &query_lower);
            }
            return;
        }

        // Markdown path: search each element's text content
        for (el_idx, el) in preview.elements.iter().enumerate() {
            let text = element_text(el);
            if !text.is_empty() {
                self.find_in_text(&text, el_idx, &query_lower);
            }
        }
    }

    fn find_in_text(&mut self, text: &str, idx: usize, query_lower: &str) {
        let text_lower = text.to_lowercase();
        let mut pos = 0usize;
        while pos < text_lower.len() {
            let Some(found) = text_lower[pos..].find(query_lower) else { break };
            let byte_start = pos + found;
            let byte_end = byte_start + query_lower.len();
            let char_start = text_lower[..byte_start].chars().count();
            let char_end = text_lower[..byte_end].chars().count();
            self.matches.push(PreviewMatch {
                line_idx: idx,
                char_start,
                char_end,
            });
            pos = byte_end;
        }
    }

    pub fn next(&mut self) {
        if !self.matches.is_empty() {
            self.current_idx = (self.current_idx + 1) % self.matches.len();
        }
    }

    pub fn prev(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current_idx = self
            .current_idx
            .checked_sub(1)
            .unwrap_or(self.matches.len() - 1);
    }

    /// Line index of the currently-selected match, if any. Returned
    /// to the render layer so it can paint that row's background and
    /// scroll the list to it.
    pub fn current_line(&self) -> Option<usize> {
        self.matches.get(self.current_idx).map(|m| m.line_idx)
    }
}

/// If `preview` is a pure-code file (single `CodeBlock` element),
/// return a borrowed view of its lines. Used both by match
/// collection and by the "is this preview searchable?" gate.
#[cfg(feature = "gpui")]
pub fn code_lines_of(preview: &PreviewState) -> Option<Vec<&str>> {
    match preview.elements.as_slice() {
        [PreviewElement::CodeBlock { formatted_lines, .. }] => {
            Some(formatted_lines.iter().map(|(_, t, _)| t.as_str()).collect())
        }
        _ => None,
    }
}

/// Extract plain text from a single preview element for search.
#[cfg(feature = "gpui")]
fn element_text(el: &PreviewElement) -> String {
    let mut out = String::new();
    match el {
        PreviewElement::Heading { text, .. } => {
            out.push_str(text);
        }
        PreviewElement::Paragraph { spans } | PreviewElement::Blockquote { spans } => {
            for s in spans { out.push_str(&s.text); }
        }
        PreviewElement::CodeBlock { formatted_lines, .. } => {
            for (_, text, _) in formatted_lines {
                out.push_str(text);
                out.push('\n');
            }
        }
        PreviewElement::ListItem { spans, .. } => {
            for s in spans { out.push_str(&s.text); }
        }
        PreviewElement::Table { headers, rows } => {
            for row in headers { for span in row { out.push_str(&span.text); out.push(' '); } }
            for row in rows { for cell in row { for span in cell { out.push_str(&span.text); } out.push(' '); } }
        }
        PreviewElement::HorizontalRule => {}
    }
    out
}

/// Whether a preview has any searchable text content.
#[cfg(feature = "gpui")]
pub fn is_searchable(preview: &PreviewState) -> bool {
    !preview.elements.is_empty()
}

#[cfg(feature = "gpui")]
impl GpuiShellView {
    /// Open (or re-open) search on the active preview.
    pub(crate) fn preview_search_open(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.active_preview_path() else { return };
        let searchable = self
            .preview_tabs
            .get(&path)
            .map(|s| is_searchable(s))
            .unwrap_or(false);
        if !searchable {
            return;
        }
        // Re-opening an existing search (e.g. user pressed `/` twice)
        // re-enters input mode with the current query preserved. That
        // matches the mdterm shape where Escape is the only way to
        // fully drop the state.
        if let Some(state) = self.preview_search.as_mut()
            && state.path == path
        {
            state.input_active = true;
        } else {
            self.preview_search = Some(PreviewSearchState::new(path));
        }
        cx.notify();
    }

    /// Drop search state entirely. Also scrolls the list back to the
    /// top? No — keep the current scroll position; mdterm does the
    /// same (Escape clears the query and highlights, not the view).
    pub(crate) fn preview_search_close(&mut self, cx: &mut Context<Self>) {
        self.preview_search = None;
        cx.notify();
    }

    /// Append `text` to the current query (typed while `input_active`).
    pub(crate) fn preview_search_input(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(state) = self.preview_search.as_mut() else { return };
        state.query.push_str(text);
        let path = state.path.clone();
        if let Some(preview) = self.preview_tabs.get(&path) {
            if let Some(state) = self.preview_search.as_mut() {
                state.rebuild(preview);
                self.preview_search_scroll_to_current();
            }
        }
        cx.notify();
    }

    pub(crate) fn preview_search_backspace(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.preview_search.as_mut() else { return };
        state.query.pop();
        let path = state.path.clone();
        if let Some(preview) = self.preview_tabs.get(&path) {
            if let Some(state) = self.preview_search.as_mut() {
                state.rebuild(preview);
                self.preview_search_scroll_to_current();
            }
        }
        cx.notify();
    }

    /// Commit the query: exit input mode but keep the bar visible
    /// for `n`/`N` navigation. Centers the first match.
    pub(crate) fn preview_search_commit(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.preview_search.as_mut() {
            state.input_active = false;
            self.preview_search_scroll_to_current();
        }
        cx.notify();
    }

    pub(crate) fn preview_search_next(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.preview_search.as_mut() {
            state.next();
            self.preview_search_scroll_to_current();
        }
        cx.notify();
    }

    pub(crate) fn preview_search_prev(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.preview_search.as_mut() {
            state.prev();
            self.preview_search_scroll_to_current();
        }
        cx.notify();
    }

    /// Center the current match line in the list viewport. No-op if
    /// no matches (so the user sees the "0 matches" counter without
    /// the view jumping).
    fn preview_search_scroll_to_current(&self) {
        let Some(state) = self.preview_search.as_ref() else { return };
        let Some(line) = state.current_line() else { return };
        self.preview_scroll_handle
            .scroll_to_item(line, gpui::ScrollStrategy::Center);
    }
}

#[cfg(all(test, feature = "gpui"))]
mod tests {
    use super::*;
    use crate::gpui_preview::{PreviewElement, PreviewState};

    fn code_preview(lines: &[&str]) -> PreviewState {
        // Build a CodeBlock that mirrors what `format_code_block`
        // produces at load time: numbered rows with the raw code
        // text as the middle tuple field. `line_idx` coordinates in
        // `PreviewMatch` are indices into this `formatted_lines`
        // vector, which is exactly what the renderer sees too.
        let total = lines.len();
        let formatted: Vec<(String, String, u32)> = lines
            .iter()
            .enumerate()
            .map(|(i, l)| (format!("{}", i + 1), l.to_string(), 0xffffff))
            .collect();
        PreviewState {
            file_path: "/tmp/test.rs".into(),
            file_name: "test.rs".into(),
            elements: vec![PreviewElement::CodeBlock {
                language: "rust".into(),
                formatted_lines: formatted,
                total_lines: total,
            }],
            headings: Vec::new(),
            generation: 1,
        }
    }

    #[test]
    fn finds_literal_matches_case_insensitive() {
        let preview = code_preview(&[
            "fn main() {",
            "    let Foo = bar();",
            "    foo();",
            "}",
        ]);
        let mut s = PreviewSearchState::new("/tmp/test.rs".into());
        s.query = "foo".into();
        s.rebuild(&preview);
        assert_eq!(s.matches.len(), 2);
        assert_eq!(s.matches[0].line_idx, 1);
        assert_eq!(s.matches[1].line_idx, 2);
    }

    #[test]
    fn multiple_hits_per_line() {
        let preview = code_preview(&["foo foo bar foo"]);
        let mut s = PreviewSearchState::new("/tmp/test.rs".into());
        s.query = "foo".into();
        s.rebuild(&preview);
        assert_eq!(s.matches.len(), 3);
        assert!(s.matches.iter().all(|m| m.line_idx == 0));
        // Character offsets advance across the line.
        assert_eq!(s.matches[0].char_start, 0);
        assert_eq!(s.matches[1].char_start, 4);
        assert_eq!(s.matches[2].char_start, 12);
    }

    #[test]
    fn empty_query_yields_no_matches() {
        let preview = code_preview(&["anything"]);
        let mut s = PreviewSearchState::new("/tmp/test.rs".into());
        s.query = String::new();
        s.rebuild(&preview);
        assert_eq!(s.matches.len(), 0);
    }

    #[test]
    fn next_prev_wraps_around() {
        let preview = code_preview(&["x", "x", "x"]);
        let mut s = PreviewSearchState::new("/tmp/test.rs".into());
        s.query = "x".into();
        s.rebuild(&preview);
        assert_eq!(s.current_idx, 0);
        s.next();
        assert_eq!(s.current_idx, 1);
        s.next();
        assert_eq!(s.current_idx, 2);
        s.next();
        assert_eq!(s.current_idx, 0, "next at last should wrap to 0");
        s.prev();
        assert_eq!(s.current_idx, 2, "prev at 0 should wrap to last");
    }

    #[test]
    fn non_code_preview_returns_none_from_code_lines_of() {
        let preview = PreviewState {
            file_path: "/tmp/test.md".into(),
            file_name: "test.md".into(),
            elements: vec![
                PreviewElement::Heading { level: 1, text: "hi".into() },
                PreviewElement::Paragraph { spans: vec![] },
            ],
            headings: Vec::new(),
            generation: 1,
        };
        assert!(code_lines_of(&preview).is_none());
    }

    #[test]
    fn multibyte_char_offsets_are_character_based() {
        // The lowered-byte find() returns byte offsets; we convert
        // them back to character offsets so later inline-highlight
        // rendering (which slices by `char_indices`) lands on glyph
        // boundaries. Regressing this would split a multi-byte
        // codepoint like 你 (3 UTF-8 bytes) mid-sequence.
        let preview = code_preview(&["你好 world 你好"]);
        let mut s = PreviewSearchState::new("/tmp/test.rs".into());
        s.query = "你好".into();
        s.rebuild(&preview);
        assert_eq!(s.matches.len(), 2);
        // First 你好 starts at character 0.
        assert_eq!(s.matches[0].char_start, 0);
        assert_eq!(s.matches[0].char_end, 2);
        // Second 你好 at character 9 (你好 + space + world + space = 9 chars).
        assert_eq!(s.matches[1].char_start, 9);
        assert_eq!(s.matches[1].char_end, 11);
    }
}
