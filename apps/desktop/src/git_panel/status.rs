use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::model::{FileStatus, GitFileEntry, WorkspaceGitState};

/// Default timeout for git invocations. The UI polls in the background and a
/// hanging `git status` would freeze the sidebar, so we abandon any single call
/// that runs longer than this.
pub(crate) const GIT_TIMEOUT: Duration = Duration::from_secs(3);

/// Resolve the repository root for `cwd` by invoking `git rev-parse
/// --show-toplevel`. Returns `None` when `cwd` is not inside a git repository
/// or the `git` binary is unavailable.
pub(crate) fn detect_repo_root(cwd: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .current_dir(cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

/// Run `git status --porcelain=v2 --branch` against `repo_root` and parse the
/// result into a `WorkspaceGitState`. Returns an error if the `git` invocation
/// fails; callers should treat that as a transient condition and skip the
/// current poll cycle without surfacing the error to the user.
pub(crate) fn run_git_status(repo_root: &Path) -> io::Result<WorkspaceGitState> {
    let output = Command::new("git")
        .arg("status")
        .arg("--porcelain=v2")
        .arg("--branch")
        .arg("--untracked-files=normal")
        .arg("--no-renames")
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "git status exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut state = parse_porcelain_v2(&stdout);
    state.root = repo_root.to_path_buf();
    Ok(state)
}

/// Parse the textual output of `git status --porcelain=v2 --branch` into a
/// `WorkspaceGitState`. Pure function for ease of unit testing. Unknown lines
/// are skipped silently — porcelain v2 reserves additional record types that
/// future git versions may emit.
pub(crate) fn parse_porcelain_v2(input: &str) -> WorkspaceGitState {
    let mut state = WorkspaceGitState::default();

    for line in input.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            parse_header(rest, &mut state);
        } else if let Some(rest) = line.strip_prefix("1 ") {
            if let Some(entry) = parse_ordinary(rest) {
                state.files.push(entry);
            }
        } else if let Some(rest) = line.strip_prefix("2 ") {
            if let Some(entry) = parse_renamed(rest) {
                state.files.push(entry);
            }
        } else if let Some(rest) = line.strip_prefix("u ") {
            if let Some(entry) = parse_unmerged(rest) {
                state.files.push(entry);
            }
        } else if let Some(path) = line.strip_prefix("? ") {
            state.files.push(GitFileEntry {
                path: PathBuf::from(path),
                index_status: FileStatus::Unmodified,
                worktree_status: FileStatus::Untracked,
                orig_path: None,
            });
        } else if let Some(path) = line.strip_prefix("! ") {
            state.files.push(GitFileEntry {
                path: PathBuf::from(path),
                index_status: FileStatus::Unmodified,
                worktree_status: FileStatus::Ignored,
                orig_path: None,
            });
        }
    }

    state
        .files
        .sort_by(|a, b| a.path.cmp(&b.path));
    state
}

fn parse_header(rest: &str, state: &mut WorkspaceGitState) {
    let mut parts = rest.splitn(2, ' ');
    let key = parts.next().unwrap_or("");
    let value = parts.next().unwrap_or("").trim();

    match key {
        "branch.head" => {
            if value == "(detached)" {
                state.detached = true;
                state.branch = None;
            } else {
                state.branch = Some(value.to_string());
            }
        }
        "branch.upstream" => {
            if !value.is_empty() {
                state.upstream = Some(value.to_string());
            }
        }
        "branch.ab" => {
            // Format: "+<ahead> -<behind>"
            for token in value.split_whitespace() {
                if let Some(num) = token.strip_prefix('+') {
                    state.ahead = num.parse().unwrap_or(0);
                } else if let Some(num) = token.strip_prefix('-') {
                    state.behind = num.parse().unwrap_or(0);
                }
            }
        }
        _ => {}
    }
}

fn parse_ordinary(rest: &str) -> Option<GitFileEntry> {
    // 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
    // 8 space-separated fields; path is the 8th and may contain spaces, so use
    // splitn(8) to keep the trailing path intact.
    let mut iter = rest.splitn(8, ' ');
    let xy = iter.next()?;
    let _sub = iter.next()?;
    let _mh = iter.next()?;
    let _mi = iter.next()?;
    let _mw = iter.next()?;
    let _hh = iter.next()?;
    let _hi = iter.next()?;
    let path = iter.next()?;

    let mut chars = xy.chars();
    let index = FileStatus::from_porcelain_char(chars.next()?);
    let worktree = FileStatus::from_porcelain_char(chars.next()?);

    Some(GitFileEntry {
        path: PathBuf::from(path),
        index_status: index,
        worktree_status: worktree,
        orig_path: None,
    })
}

fn parse_renamed(rest: &str) -> Option<GitFileEntry> {
    // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>\t<origPath>
    // 9 space-separated fields; trailing path/orig combo may contain spaces.
    let mut iter = rest.splitn(9, ' ');
    let xy = iter.next()?;
    let _sub = iter.next()?;
    let _mh = iter.next()?;
    let _mi = iter.next()?;
    let _mw = iter.next()?;
    let _hh = iter.next()?;
    let _hi = iter.next()?;
    let _xscore = iter.next()?;
    let path_and_orig = iter.next()?;

    let mut chars = xy.chars();
    let index = FileStatus::from_porcelain_char(chars.next()?);
    let worktree = FileStatus::from_porcelain_char(chars.next()?);

    let (path, orig) = path_and_orig.split_once('\t')?;

    Some(GitFileEntry {
        path: PathBuf::from(path),
        index_status: index,
        worktree_status: worktree,
        orig_path: Some(PathBuf::from(orig)),
    })
}

fn parse_unmerged(rest: &str) -> Option<GitFileEntry> {
    // u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
    // 10 space-separated fields; trailing path may contain spaces.
    let mut iter = rest.splitn(10, ' ');
    let xy = iter.next()?;
    for _ in 0..8 {
        iter.next()?;
    }
    let path = iter.next()?;

    let mut chars = xy.chars();
    let index = FileStatus::from_porcelain_char(chars.next()?);
    let worktree = FileStatus::from_porcelain_char(chars.next()?);

    Some(GitFileEntry {
        path: PathBuf::from(path),
        index_status: index,
        worktree_status: worktree,
        orig_path: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry<'a>(state: &'a WorkspaceGitState, path: &str) -> &'a GitFileEntry {
        state
            .files
            .iter()
            .find(|f| f.path == PathBuf::from(path))
            .unwrap_or_else(|| panic!("no entry for {path}: {:?}", state.files))
    }

    #[test]
    fn parses_clean_repo_on_main_with_upstream() {
        let input = "\
# branch.oid abc123
# branch.head main
# branch.upstream origin/main
# branch.ab +0 -0
";
        let state = parse_porcelain_v2(input);
        assert_eq!(state.branch.as_deref(), Some("main"));
        assert_eq!(state.upstream.as_deref(), Some("origin/main"));
        assert_eq!(state.ahead, 0);
        assert_eq!(state.behind, 0);
        assert!(!state.detached);
        assert!(state.is_clean());
    }

    #[test]
    fn parses_ahead_behind() {
        let input = "\
# branch.oid abc123
# branch.head feature
# branch.upstream origin/feature
# branch.ab +3 -2
";
        let state = parse_porcelain_v2(input);
        assert_eq!(state.ahead, 3);
        assert_eq!(state.behind, 2);
    }

    #[test]
    fn parses_detached_head() {
        let input = "\
# branch.oid abc123
# branch.head (detached)
";
        let state = parse_porcelain_v2(input);
        assert!(state.detached);
        assert!(state.branch.is_none());
    }

    #[test]
    fn parses_modified_and_staged_file() {
        // "MM" = staged Modified + worktree Modified
        let input = "\
# branch.head main
1 MM N... 100644 100644 100644 abc def src/main.rs
";
        let state = parse_porcelain_v2(input);
        let e = entry(&state, "src/main.rs");
        assert_eq!(e.index_status, FileStatus::Modified);
        assert_eq!(e.worktree_status, FileStatus::Modified);
    }

    #[test]
    fn parses_added_file_unstaged() {
        let input = "\
# branch.head main
1 .A N... 000000 000000 100644 0000 0000 new.rs
";
        let state = parse_porcelain_v2(input);
        let e = entry(&state, "new.rs");
        assert_eq!(e.index_status, FileStatus::Unmodified);
        assert_eq!(e.worktree_status, FileStatus::Added);
    }

    #[test]
    fn parses_deleted_file() {
        let input = "\
# branch.head main
1 D. N... 100644 000000 000000 abc 0000 gone.rs
";
        let state = parse_porcelain_v2(input);
        let e = entry(&state, "gone.rs");
        assert_eq!(e.index_status, FileStatus::Deleted);
    }

    #[test]
    fn parses_untracked_file() {
        let input = "\
# branch.head main
? scratch.txt
";
        let state = parse_porcelain_v2(input);
        let e = entry(&state, "scratch.txt");
        assert_eq!(e.worktree_status, FileStatus::Untracked);
    }

    #[test]
    fn parses_renamed_file_with_orig_path() {
        // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> R100 newpath\toldpath
        let input = "\
# branch.head main
2 R. N... 100644 100644 100644 abc def R100 src/new.rs\tsrc/old.rs
";
        let state = parse_porcelain_v2(input);
        let e = entry(&state, "src/new.rs");
        assert_eq!(e.index_status, FileStatus::Renamed);
        assert_eq!(e.orig_path.as_deref(), Some(Path::new("src/old.rs")));
    }

    #[test]
    fn parses_path_with_spaces() {
        let input = "\
# branch.head main
1 M. N... 100644 100644 100644 abc def docs/file with spaces.md
";
        let state = parse_porcelain_v2(input);
        let e = entry(&state, "docs/file with spaces.md");
        assert_eq!(e.index_status, FileStatus::Modified);
    }

    #[test]
    fn files_are_sorted_by_path() {
        let input = "\
# branch.head main
1 M. N... 100644 100644 100644 abc def zebra.rs
1 M. N... 100644 100644 100644 abc def alpha.rs
? middle.rs
";
        let state = parse_porcelain_v2(input);
        let paths: Vec<&str> = state
            .files
            .iter()
            .map(|f| f.path.to_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["alpha.rs", "middle.rs", "zebra.rs"]);
    }

    #[test]
    fn badge_label_for_clean_synced_repo_is_none() {
        let input = "\
# branch.head main
# branch.upstream origin/main
# branch.ab +0 -0
";
        let state = parse_porcelain_v2(input);
        assert_eq!(state.badge_label(), None);
    }

    #[test]
    fn badge_label_shows_pending_count_when_dirty() {
        let input = "\
# branch.head main
# branch.upstream origin/main
# branch.ab +5 -0
1 M. N... 100644 100644 100644 abc def a.rs
1 .M N... 100644 100644 100644 abc def b.rs
? c.rs
";
        let state = parse_porcelain_v2(input);
        // Even though we're ahead by 5, the pending file count wins.
        assert_eq!(state.badge_label().as_deref(), Some("3"));
        assert_eq!(state.pending_count(), 3);
    }

    #[test]
    fn badge_label_shows_arrows_when_clean_but_diverged() {
        let input = "\
# branch.head main
# branch.upstream origin/main
# branch.ab +2 -3
";
        let state = parse_porcelain_v2(input);
        assert_eq!(state.badge_label().as_deref(), Some("↑2 ↓3"));
    }

    #[test]
    fn badge_label_shows_ahead_only() {
        let input = "\
# branch.head main
# branch.upstream origin/main
# branch.ab +4 -0
";
        let state = parse_porcelain_v2(input);
        assert_eq!(state.badge_label().as_deref(), Some("↑4"));
    }

    #[test]
    fn changed_count_excludes_untracked() {
        let input = "\
# branch.head main
1 M. N... 100644 100644 100644 abc def edited.rs
? untracked.rs
";
        let state = parse_porcelain_v2(input);
        // 2 entries total, but only 1 counts as "changed" for the badge.
        assert_eq!(state.files.len(), 2);
        assert_eq!(state.changed_count(), 1);
    }

    #[test]
    fn is_clean_for_empty_input() {
        let state = parse_porcelain_v2("");
        assert!(state.is_clean());
        assert_eq!(state.changed_count(), 0);
        assert!(state.branch.is_none());
        assert!(!state.detached);
    }

    #[test]
    fn unknown_record_types_are_ignored() {
        let input = "\
# branch.head main
1 M. N... 100644 100644 100644 abc def real.rs
$ unknown record type
xyz garbage
";
        let state = parse_porcelain_v2(input);
        assert_eq!(state.files.len(), 1);
        assert_eq!(state.files[0].path, PathBuf::from("real.rs"));
    }

    #[test]
    fn detect_repo_root_in_this_repo() {
        // The test process is run inside the amux repo, so this should resolve.
        let cwd = std::env::current_dir().expect("cwd");
        let root = detect_repo_root(&cwd);
        assert!(
            root.is_some(),
            "expected detect_repo_root to find a git root from {cwd:?}"
        );
        let root = root.unwrap();
        assert!(root.join(".git").exists() || root.join(".git").is_file());
    }

    #[test]
    fn detect_repo_root_returns_none_outside_repo() {
        // /tmp is (almost certainly) not inside a git repo on CI/dev machines.
        // If it ever is, this test will fail loudly rather than silently.
        let tmp = std::env::temp_dir();
        let root = detect_repo_root(&tmp);
        assert!(
            root.is_none(),
            "expected /tmp to be outside any repo, but got {root:?}"
        );
    }
}
