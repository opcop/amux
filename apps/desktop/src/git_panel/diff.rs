use std::io;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// One line inside a hunk. `Context` lines appear unchanged in both sides;
/// `Removed` lines are only in the old version; `Added` lines are only in the
/// new version. The string never contains the leading `+`/`-`/` ` marker or a
/// trailing newline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "snake_case")]
pub(crate) enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}

impl DiffLine {
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Context(s) | Self::Added(s) | Self::Removed(s) => s,
        }
    }
}

/// A single `@@ ... @@` hunk parsed from a unified diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DiffHunk {
    pub(crate) old_start: u32,
    pub(crate) old_count: u32,
    pub(crate) new_start: u32,
    pub(crate) new_count: u32,
    /// Optional context shown after the second `@@` (typically a function
    /// signature when `diff.context` is enabled).
    pub(crate) function_context: Option<String>,
    pub(crate) lines: Vec<DiffLine>,
}

/// One file's worth of diff. `old_path` is `None` for newly added files
/// (where git reports `--- /dev/null`); `new_path` is `None` for deletions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DiffFile {
    pub(crate) old_path: Option<String>,
    pub(crate) new_path: Option<String>,
    pub(crate) hunks: Vec<DiffHunk>,
    /// True when git reported `Binary files ... differ` for this entry.
    /// In that case `hunks` is empty.
    pub(crate) is_binary: bool,
}

impl DiffFile {
    /// Display path — prefers the new path (where the user is now), falls
    /// back to the old path for deletions. Returns `<unknown>` only if the
    /// diff is malformed.
    pub(crate) fn display_path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("<unknown>")
    }
}

/// Rebuild a self-contained unified-diff patch for a single hunk inside a
/// `DiffFile`. The output is valid input for `git apply --cached` — file
/// headers (`--- a/path` / `+++ b/path`) are emitted using the file's old
/// and new paths, falling back to `/dev/null` for adds and deletes the
/// same way git does.
///
/// Used by the diff panel's per-hunk stage / unstage buttons. We can't
/// just feed the original `git diff` output minus other hunks — git
/// requires the path headers to be present, and the line numbers in the
/// `@@` header must match the file's actual state for the apply to
/// succeed.
pub(crate) fn build_hunk_patch(file: &DiffFile, hunk: &DiffHunk) -> String {
    let mut out = String::new();
    let old_header = file
        .old_path
        .as_deref()
        .map(|p| format!("a/{p}"))
        .unwrap_or_else(|| "/dev/null".to_string());
    let new_header = file
        .new_path
        .as_deref()
        .map(|p| format!("b/{p}"))
        .unwrap_or_else(|| "/dev/null".to_string());
    // diff --git line: keeps `git apply` happy and lets it figure out the
    // file mode if it ever needs to.
    let diff_path = file
        .new_path
        .as_deref()
        .or(file.old_path.as_deref())
        .unwrap_or("unknown");
    out.push_str(&format!("diff --git a/{diff_path} b/{diff_path}\n"));
    out.push_str(&format!("--- {old_header}\n"));
    out.push_str(&format!("+++ {new_header}\n"));
    out.push_str(&format!(
        "@@ -{},{} +{},{} @@",
        hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
    ));
    if let Some(ctx) = hunk.function_context.as_deref() {
        out.push(' ');
        out.push_str(ctx);
    }
    out.push('\n');
    for line in &hunk.lines {
        match line {
            DiffLine::Added(text) => {
                out.push('+');
                out.push_str(text);
                out.push('\n');
            }
            DiffLine::Removed(text) => {
                out.push('-');
                out.push_str(text);
                out.push('\n');
            }
            DiffLine::Context(text) => {
                out.push(' ');
                out.push_str(text);
                out.push('\n');
            }
        }
    }
    out
}

/// Run `git diff` (or `git diff --cached` when `staged` is true) for a single
/// path inside `repo_root`. The path is repo-relative; passing `None` returns
/// the diff for all changed files. Returns the raw textual output for the
/// caller to feed into `parse_unified_diff`.
pub(crate) fn run_git_diff(
    repo_root: &Path,
    path: Option<&str>,
    staged: bool,
) -> io::Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("diff");
    if staged {
        cmd.arg("--cached");
    }
    // Disable color codes regardless of the user's git config — `parse_unified_diff`
    // wants plain ANSI-free text.
    cmd.arg("--no-color");
    if let Some(p) = path {
        cmd.arg("--");
        cmd.arg(p);
    }
    cmd.current_dir(repo_root);

    let output = cmd.output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "git diff exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parse the textual output of `git diff` (or any unified-diff producer) into
/// per-file structures. Multiple `diff --git` blocks become multiple `DiffFile`
/// entries; an empty input returns an empty vec.
///
/// Pure function — no I/O, no panics on malformed input. Unrecognized lines
/// inside a hunk are silently skipped, which matches what most diff renderers
/// do for git's extended-header lines (`similarity index`, `mode change`, etc.).
pub(crate) fn parse_unified_diff(input: &str) -> Vec<DiffFile> {
    let mut files: Vec<DiffFile> = Vec::new();
    let mut current: Option<DiffFile> = None;
    let mut current_hunk: Option<DiffHunk> = None;

    let mut lines = input.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with("diff --git ") {
            // Boundary — flush the previous file/hunk and start a new one.
            if let Some(hunk) = current_hunk.take() {
                if let Some(file) = current.as_mut() {
                    file.hunks.push(hunk);
                }
            }
            if let Some(file) = current.take() {
                files.push(file);
            }
            current = Some(DiffFile {
                old_path: None,
                new_path: None,
                hunks: Vec::new(),
                is_binary: false,
            });
            continue;
        }

        // Everything below this point assumes we're inside a `diff --git` block.
        let Some(file) = current.as_mut() else {
            continue;
        };

        if let Some(path) = line.strip_prefix("--- ") {
            file.old_path = parse_diff_path(path);
        } else if let Some(path) = line.strip_prefix("+++ ") {
            file.new_path = parse_diff_path(path);
        } else if line.starts_with("Binary files ") && line.ends_with(" differ") {
            file.is_binary = true;
        } else if line.starts_with("@@") {
            // Flush any previous hunk, then start a new one.
            if let Some(hunk) = current_hunk.take() {
                file.hunks.push(hunk);
            }
            current_hunk = parse_hunk_header(line);
        } else if let Some(hunk) = current_hunk.as_mut() {
            // Inside an active hunk — classify by leading char.
            if let Some(rest) = line.strip_prefix('+') {
                hunk.lines.push(DiffLine::Added(rest.to_string()));
            } else if let Some(rest) = line.strip_prefix('-') {
                hunk.lines.push(DiffLine::Removed(rest.to_string()));
            } else if let Some(rest) = line.strip_prefix(' ') {
                hunk.lines.push(DiffLine::Context(rest.to_string()));
            } else if line.starts_with('\\') {
                // "\ No newline at end of file" — discard, the visual surprise
                // is small and tracking it adds a field for almost no payoff.
            }
            // Any other leading char ends the hunk implicitly; the next `@@`
            // or `diff --git` line will start a new one.
        }
    }

    // Flush trailing state.
    if let Some(hunk) = current_hunk.take() {
        if let Some(file) = current.as_mut() {
            file.hunks.push(hunk);
        }
    }
    if let Some(file) = current.take() {
        files.push(file);
    }

    files
}

/// Extract a path from a `--- ` or `+++ ` header. Handles `/dev/null` (returns
/// `None` so callers can treat the file as new/deleted) and strips the leading
/// `a/` or `b/` prefix git applies by default. Anything after a tab is
/// metadata (mtime) and discarded.
fn parse_diff_path(raw: &str) -> Option<String> {
    let trimmed = raw.split('\t').next().unwrap_or(raw).trim();
    if trimmed == "/dev/null" {
        return None;
    }
    let stripped = trimmed
        .strip_prefix("a/")
        .or_else(|| trimmed.strip_prefix("b/"))
        .unwrap_or(trimmed);
    Some(stripped.to_string())
}

/// Parse `@@ -old_start[,old_count] +new_start[,new_count] @@ [context]` into
/// a hunk skeleton (lines vec stays empty). Returns `None` if the header is
/// malformed so the caller can skip the section without crashing.
fn parse_hunk_header(line: &str) -> Option<DiffHunk> {
    // Examples:
    //   @@ -10,5 +12,7 @@ fn foo(...)
    //   @@ -1 +1 @@
    //   @@ -0,0 +1,3 @@
    let rest = line.strip_prefix("@@")?.trim_start();
    let close = rest.find("@@")?;
    let range_section = rest[..close].trim();
    let context_section = rest[close + 2..].trim();

    let mut parts = range_section.split_whitespace();
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;

    let (old_start, old_count) = split_range(old);
    let (new_start, new_count) = split_range(new);

    let function_context = if context_section.is_empty() {
        None
    } else {
        Some(context_section.to_string())
    };

    Some(DiffHunk {
        old_start,
        old_count,
        new_start,
        new_count,
        function_context,
        lines: Vec::new(),
    })
}

/// `"10,5"` → `(10, 5)`. `"10"` → `(10, 1)`. Malformed input yields `(0, 0)`
/// — we'd rather render an empty hunk than panic on unexpected git output.
fn split_range(raw: &str) -> (u32, u32) {
    let mut iter = raw.split(',');
    let start = iter.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let count = iter.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    (start, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_input() {
        assert!(parse_unified_diff("").is_empty());
    }

    #[test]
    fn parses_single_file_single_hunk() {
        let input = "\
diff --git a/src/main.rs b/src/main.rs
index abc..def 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 use foo;
+use bar;
 use baz;
-use qux;
";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(f.old_path.as_deref(), Some("src/main.rs"));
        assert_eq!(f.new_path.as_deref(), Some("src/main.rs"));
        assert!(!f.is_binary);
        assert_eq!(f.hunks.len(), 1);

        let h = &f.hunks[0];
        assert_eq!(h.old_start, 1);
        assert_eq!(h.old_count, 3);
        assert_eq!(h.new_start, 1);
        assert_eq!(h.new_count, 4);
        assert_eq!(h.function_context, None);
        assert_eq!(
            h.lines,
            vec![
                DiffLine::Context("use foo;".to_string()),
                DiffLine::Added("use bar;".to_string()),
                DiffLine::Context("use baz;".to_string()),
                DiffLine::Removed("use qux;".to_string()),
            ]
        );
    }

    #[test]
    fn parses_function_context_in_hunk_header() {
        let input = "\
diff --git a/lib.rs b/lib.rs
--- a/lib.rs
+++ b/lib.rs
@@ -10,3 +10,4 @@ fn process(input: &str) -> String {
     let x = 1;
+    let y = 2;
     x.to_string()
";
        let files = parse_unified_diff(input);
        let h = &files[0].hunks[0];
        assert_eq!(
            h.function_context.as_deref(),
            Some("fn process(input: &str) -> String {")
        );
    }

    #[test]
    fn parses_new_file_with_dev_null_source() {
        let input = "\
diff --git a/new.rs b/new.rs
new file mode 100644
--- /dev/null
+++ b/new.rs
@@ -0,0 +1,2 @@
+fn main() {}
+
";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(f.old_path, None);
        assert_eq!(f.new_path.as_deref(), Some("new.rs"));
        assert_eq!(f.display_path(), "new.rs");
        assert_eq!(f.hunks.len(), 1);
        assert_eq!(f.hunks[0].old_start, 0);
        assert_eq!(f.hunks[0].old_count, 0);
    }

    #[test]
    fn parses_deleted_file_with_dev_null_target() {
        let input = "\
diff --git a/gone.rs b/gone.rs
deleted file mode 100644
--- a/gone.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-fn main() {}
-
";
        let files = parse_unified_diff(input);
        let f = &files[0];
        assert_eq!(f.old_path.as_deref(), Some("gone.rs"));
        assert_eq!(f.new_path, None);
        assert_eq!(f.display_path(), "gone.rs");
    }

    #[test]
    fn parses_binary_file_marker() {
        let input = "\
diff --git a/logo.png b/logo.png
index abc..def 100644
Binary files a/logo.png and b/logo.png differ
";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert!(f.is_binary);
        assert!(f.hunks.is_empty());
    }

    #[test]
    fn parses_multiple_files_in_one_diff() {
        let input = "\
diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1 +1 @@
-old
+new
diff --git a/b.rs b/b.rs
--- a/b.rs
+++ b/b.rs
@@ -1 +1 @@
-foo
+bar
";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].new_path.as_deref(), Some("a.rs"));
        assert_eq!(files[1].new_path.as_deref(), Some("b.rs"));
    }

    #[test]
    fn parses_multiple_hunks_in_one_file() {
        let input = "\
diff --git a/x.rs b/x.rs
--- a/x.rs
+++ b/x.rs
@@ -1,2 +1,2 @@
-aaa
+AAA
 bbb
@@ -10,2 +10,2 @@
-ccc
+CCC
 ddd
";
        let files = parse_unified_diff(input);
        let f = &files[0];
        assert_eq!(f.hunks.len(), 2);
        assert_eq!(f.hunks[0].old_start, 1);
        assert_eq!(f.hunks[1].old_start, 10);
    }

    #[test]
    fn parses_single_line_range_without_count() {
        // Git emits `@@ -5 +5 @@` (no comma) when count is exactly 1.
        let input = "\
diff --git a/x.rs b/x.rs
--- a/x.rs
+++ b/x.rs
@@ -5 +5 @@
-old
+new
";
        let files = parse_unified_diff(input);
        let h = &files[0].hunks[0];
        assert_eq!(h.old_start, 5);
        assert_eq!(h.old_count, 1);
        assert_eq!(h.new_start, 5);
        assert_eq!(h.new_count, 1);
    }

    #[test]
    fn no_newline_marker_is_ignored() {
        let input = "\
diff --git a/x.rs b/x.rs
--- a/x.rs
+++ b/x.rs
@@ -1 +1 @@
-old
\\ No newline at end of file
+new
\\ No newline at end of file
";
        let files = parse_unified_diff(input);
        let lines = &files[0].hunks[0].lines;
        assert_eq!(
            lines,
            &vec![
                DiffLine::Removed("old".to_string()),
                DiffLine::Added("new".to_string()),
            ]
        );
    }

    #[test]
    fn extended_header_lines_do_not_break_parser() {
        // similarity index / rename from / rename to / mode change lines
        // sit between `diff --git` and `---` — they should be skipped.
        let input = "\
diff --git a/old.rs b/new.rs
similarity index 95%
rename from old.rs
rename to new.rs
index abc..def 100644
--- a/old.rs
+++ b/new.rs
@@ -1 +1 @@
-foo
+bar
";
        let files = parse_unified_diff(input);
        let f = &files[0];
        assert_eq!(f.old_path.as_deref(), Some("old.rs"));
        assert_eq!(f.new_path.as_deref(), Some("new.rs"));
        assert_eq!(f.hunks.len(), 1);
    }

    #[test]
    fn display_path_falls_back_to_old_for_deletions() {
        let f = DiffFile {
            old_path: Some("gone.rs".to_string()),
            new_path: None,
            hunks: vec![],
            is_binary: false,
        };
        assert_eq!(f.display_path(), "gone.rs");
    }

    #[test]
    fn display_path_uses_unknown_when_both_paths_missing() {
        let f = DiffFile {
            old_path: None,
            new_path: None,
            hunks: vec![],
            is_binary: false,
        };
        assert_eq!(f.display_path(), "<unknown>");
    }

    #[test]
    fn diff_line_text_strips_marker() {
        assert_eq!(DiffLine::Added("hi".to_string()).text(), "hi");
        assert_eq!(DiffLine::Removed("bye".to_string()).text(), "bye");
        assert_eq!(DiffLine::Context("x".to_string()).text(), "x");
    }

    #[test]
    fn build_hunk_patch_roundtrips_through_parser() {
        let original = "\
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 use foo;
+use bar;
 use baz;
-use qux;
";
        let files = parse_unified_diff(original);
        let f = &files[0];
        let h = &f.hunks[0];

        let patch = build_hunk_patch(f, h);
        // The patch we built should re-parse back to the same single hunk.
        let reparsed = parse_unified_diff(&patch);
        assert_eq!(reparsed.len(), 1);
        assert_eq!(reparsed[0].hunks.len(), 1);
        let rh = &reparsed[0].hunks[0];
        assert_eq!(rh.old_start, 1);
        assert_eq!(rh.old_count, 3);
        assert_eq!(rh.new_start, 1);
        assert_eq!(rh.new_count, 4);
        assert_eq!(rh.lines, h.lines);
    }

    #[test]
    fn build_hunk_patch_uses_dev_null_for_new_files() {
        let f = DiffFile {
            old_path: None,
            new_path: Some("new.rs".to_string()),
            hunks: vec![DiffHunk {
                old_start: 0,
                old_count: 0,
                new_start: 1,
                new_count: 1,
                function_context: None,
                lines: vec![DiffLine::Added("fn main() {}".to_string())],
            }],
            is_binary: false,
        };
        let patch = build_hunk_patch(&f, &f.hunks[0]);
        assert!(patch.contains("--- /dev/null"), "got: {patch}");
        assert!(patch.contains("+++ b/new.rs"), "got: {patch}");
    }

    #[test]
    fn build_hunk_patch_preserves_function_context() {
        let f = DiffFile {
            old_path: Some("lib.rs".to_string()),
            new_path: Some("lib.rs".to_string()),
            hunks: vec![DiffHunk {
                old_start: 10,
                old_count: 3,
                new_start: 10,
                new_count: 4,
                function_context: Some("fn process(input: &str) -> String {".to_string()),
                lines: vec![
                    DiffLine::Context("    let x = 1;".to_string()),
                    DiffLine::Added("    let y = 2;".to_string()),
                    DiffLine::Context("    x.to_string()".to_string()),
                ],
            }],
            is_binary: false,
        };
        let patch = build_hunk_patch(&f, &f.hunks[0]);
        assert!(
            patch.contains("@@ -10,3 +10,4 @@ fn process(input: &str) -> String {"),
            "got: {patch}"
        );
    }

    #[test]
    fn run_git_diff_returns_text_for_this_repo_when_no_changes() {
        // No path arg, no staged. Should succeed against this repo (it may or
        // may not have changes — we only assert the call returns Ok).
        let cwd = std::env::current_dir().expect("cwd");
        let root = super::super::status::detect_repo_root(&cwd).expect("expected repo");
        let result = run_git_diff(&root, None, false);
        assert!(result.is_ok(), "git diff failed: {result:?}");
    }
}
