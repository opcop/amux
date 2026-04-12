//! Pure scrollback search logic.
//!
//! Everything in this module operates on plain data — a
//! `SearchState` and a borrowed `alacritty_terminal::Term`. No
//! GPUI types, no `GpuiShellView`, no `self`. The wrappers in
//! `gpui_entry.rs` just do the "take state, grab active terminal,
//! delegate, put state back" glue so these functions stay trivially
//! unit-testable.
//!
//! ## Design
//!
//! * `rebuild` is called on every query or mode change. It walks
//!   the full scrollback up to `SEARCH_MATCH_CAP` and populates
//!   `state.matches`. Navigation between hits (`Enter` /
//!   `Shift+Enter`) then just cycles `state.current` and calls
//!   `apply_current` — the expensive work happens once per edit.
//! * Smart case: all-lowercase queries get `(?i)` prepended, both
//!   for `Literal` (after metachar escaping) and `Regex` modes.
//!   Fuzzy mode is always case-insensitive.
//! * On `Regex` mode compile failure, `state.error = true` and
//!   `matches` is left empty — the UI paints a red `err`.

#![cfg(feature = "gpui")]

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::Term;

use crate::state::{SearchMode, SearchState, SEARCH_MATCH_CAP};

/// Smart-case regex pattern builder. In `Literal` mode the query
/// is escaped first so `.` / `*` etc. are matched literally. In
/// either mode, if the query is all lowercase we prepend `(?i)` so
/// users don't have to think about case.
pub(crate) fn build_regex_pattern(query: &str, mode: SearchMode) -> String {
    let base: String = if mode == SearchMode::Literal {
        query
            .chars()
            .flat_map(|c| {
                if "\\^$.|?*+()[]{}".contains(c) {
                    vec!['\\', c]
                } else {
                    vec![c]
                }
            })
            .collect()
    } else {
        query.to_string()
    };
    let has_upper = query.chars().any(|c| c.is_ascii_uppercase());
    let already_flagged = query.starts_with("(?i)") || query.starts_with("(?-i)");
    if has_upper || already_flagged {
        base
    } else {
        format!("(?i){}", base)
    }
}

/// Enumerate regex matches across the entire terminal buffer
/// (scrollback + viewport), capped at `SEARCH_MATCH_CAP`.
fn collect_regex_matches<T: EventListener>(
    t: &Term<T>,
    regex: &mut RegexSearch,
) -> (Vec<Match>, bool) {
    let mut out = Vec::new();
    let mut origin = Point::new(t.topmost_line(), Column(0));
    let end = Point::new(t.bottommost_line(), Column(t.columns().saturating_sub(1)));

    while out.len() < SEARCH_MATCH_CAP {
        let Some(m) = t.search_next(regex, origin, Direction::Right, Side::Left, None) else {
            break;
        };
        // Stop once we walk past the bottom of the buffer.
        if *m.start() > end {
            break;
        }
        // Advance origin past the end of this match to avoid
        // re-matching the same span. Zero-width matches are not
        // possible with RegexSearch but we still bump by one
        // column defensively.
        let next_col = m.end().column.0 + 1;
        let next_line = m.end().line;
        origin = if next_col >= t.columns() {
            if next_line >= t.bottommost_line() {
                out.push(m);
                break;
            }
            Point::new(Line(next_line.0 + 1), Column(0))
        } else {
            Point::new(next_line, Column(next_col))
        };
        out.push(m);
    }

    let truncated = out.len() == SEARCH_MATCH_CAP;
    (out, truncated)
}

/// Fuzzy (subsequence) matcher. Walks every line in scrollback +
/// viewport and records a match for each line whose characters
/// contain the query letters in order, case-insensitive. Returns
/// a match spanning the first → last matched character on that
/// line so the existing "scroll + select" path can highlight it
/// unchanged.
fn collect_fuzzy_matches<T: EventListener>(t: &Term<T>, query: &str) -> (Vec<Match>, bool) {
    let needle: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    if needle.is_empty() {
        return (Vec::new(), false);
    }

    let cols = t.columns();
    let top = t.topmost_line().0;
    let bot = t.bottommost_line().0;
    let mut out = Vec::new();

    for line_i32 in top..=bot {
        if out.len() >= SEARCH_MATCH_CAP {
            break;
        }
        let line = Line(line_i32);
        let start_pt = Point::new(line, Column(0));
        let end_pt = Point::new(line, Column(cols.saturating_sub(1)));
        let text = t.bounds_to_string(start_pt, end_pt);

        // Single-pass subsequence match: record the column of the
        // first and last matched needle char. If we run out of
        // haystack before matching the whole needle, reject.
        let mut needle_idx = 0usize;
        let mut first_col: Option<usize> = None;
        let mut last_col: usize = 0;
        for (col, ch) in text.chars().enumerate() {
            if needle_idx >= needle.len() {
                break;
            }
            if ch.to_lowercase().next() == Some(needle[needle_idx]) {
                if first_col.is_none() {
                    first_col = Some(col);
                }
                last_col = col;
                needle_idx += 1;
            }
        }
        if needle_idx == needle.len() {
            if let Some(fc) = first_col {
                let fc = fc.min(cols.saturating_sub(1));
                let lc = last_col.min(cols.saturating_sub(1));
                let s = Point::new(line, Column(fc));
                let e = Point::new(line, Column(lc));
                out.push(s..=e);
            }
        }
    }
    let truncated = out.len() == SEARCH_MATCH_CAP;
    (out, truncated)
}

/// Rebuild `state.matches` from its current query + mode against
/// the given terminal. Clears `current`, `truncated`, `error`, and
/// any existing selection. Called on every query edit or mode
/// toggle from the input handler. Cheap when the query is empty —
/// just clears.
pub(crate) fn rebuild<T: EventListener>(state: &mut SearchState, t: &mut Term<T>) {
    state.matches.clear();
    state.current = 0;
    state.truncated = false;
    state.error = false;

    if state.query.is_empty() {
        t.selection = None;
        return;
    }

    match state.mode {
        SearchMode::Literal | SearchMode::Regex => {
            let pattern = build_regex_pattern(&state.query, state.mode);
            let mut regex = match RegexSearch::new(&pattern) {
                Ok(r) => r,
                Err(_) => {
                    state.error = true;
                    return;
                }
            };
            let (matches, truncated) = collect_regex_matches(t, &mut regex);
            state.matches = matches;
            state.truncated = truncated;
        }
        SearchMode::Fuzzy => {
            let (matches, truncated) = collect_fuzzy_matches(t, &state.query);
            state.matches = matches;
            state.truncated = truncated;
        }
    }
}

/// Scroll the terminal so the match at `state.current` is visible
/// and highlight it via a `Simple` selection. No-op when matches
/// is empty.
pub(crate) fn apply_current<T: EventListener>(state: &SearchState, t: &mut Term<T>) {
    if state.matches.is_empty() {
        return;
    }
    let idx = state.current.min(state.matches.len() - 1);
    let m = state.matches[idx].clone();

    let line_i32 = m.start().line.0;
    if line_i32 < 0 {
        let needed = (-line_i32) as usize;
        let display_offset = t.grid().display_offset();
        if needed > display_offset {
            t.scroll_display(Scroll::Delta((needed - display_offset) as i32));
        }
    } else if t.grid().display_offset() > 0 {
        t.scroll_display(Scroll::Bottom);
    }
    let mut sel = Selection::new(SelectionType::Simple, *m.start(), Side::Left);
    sel.update(*m.end(), Side::Right);
    t.selection = Some(sel);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Smart case: all-lowercase query gets `(?i)`, anything with an
    // uppercase letter stays as-is. Applies to both Literal and
    // Regex modes, with Literal additionally escaping metachars.
    #[test]
    fn smart_case_literal_lower() {
        assert_eq!(build_regex_pattern("error", SearchMode::Literal), "(?i)error");
    }

    #[test]
    fn smart_case_literal_mixed() {
        assert_eq!(build_regex_pattern("Error", SearchMode::Literal), "Error");
    }

    #[test]
    fn literal_escapes_regex_metachars() {
        assert_eq!(
            build_regex_pattern("a.b*c", SearchMode::Literal),
            "(?i)a\\.b\\*c"
        );
    }

    #[test]
    fn regex_mode_passes_through_metachars() {
        assert_eq!(
            build_regex_pattern("a.b*c", SearchMode::Regex),
            "(?i)a.b*c"
        );
    }

    #[test]
    fn explicit_case_flag_respected() {
        assert_eq!(
            build_regex_pattern("(?i)ERROR", SearchMode::Regex),
            "(?i)ERROR"
        );
    }

    #[test]
    fn build_pattern_is_pure_no_self() {
        // Regression guard: this function must not depend on any
        // caller context. If it ever grows a parameter beyond
        // (query, mode), reconsider whether it still belongs here.
        let _ = build_regex_pattern("hello", SearchMode::Fuzzy);
        let _ = build_regex_pattern("", SearchMode::Literal);
    }
}
