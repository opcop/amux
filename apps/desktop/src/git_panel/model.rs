use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FileStatus {
    #[default]
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Ignored,
    Unmerged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GitFileEntry {
    pub(crate) path: PathBuf,
    /// Status in the index (staged changes).
    pub(crate) index_status: FileStatus,
    /// Status in the worktree (unstaged changes).
    pub(crate) worktree_status: FileStatus,
    /// For renames/copies: original path before the change.
    pub(crate) orig_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkspaceGitState {
    /// Absolute path to the repository root (output of `git rev-parse --show-toplevel`).
    pub(crate) root: PathBuf,
    /// Current branch name. `None` for detached HEAD.
    pub(crate) branch: Option<String>,
    /// Upstream tracking branch (e.g. `origin/main`).
    pub(crate) upstream: Option<String>,
    /// Commits ahead of upstream.
    pub(crate) ahead: u32,
    /// Commits behind upstream.
    pub(crate) behind: u32,
    /// True if HEAD is detached (no current branch).
    pub(crate) detached: bool,
    /// All changed/untracked files, sorted by path.
    pub(crate) files: Vec<GitFileEntry>,
}

impl WorkspaceGitState {
    /// True when there are no changes in the worktree or index.
    pub(crate) fn is_clean(&self) -> bool {
        self.files.is_empty()
    }

    /// Total number of changed (non-untracked, non-ignored) files.
    pub(crate) fn changed_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| {
                !matches!(
                    f.worktree_status,
                    FileStatus::Untracked | FileStatus::Ignored
                ) || !matches!(f.index_status, FileStatus::Unmodified)
            })
            .count()
    }

    /// Count of all entries reported by `git status`, including untracked
    /// files. Ignored files are excluded since the polling runs with
    /// `--untracked-files=normal` and never asks for them.
    pub(crate) fn pending_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| !matches!(f.worktree_status, FileStatus::Ignored))
            .count()
    }

    /// Short label to display next to a workspace in the sidebar.
    ///
    /// Returns `None` when the workspace is fully clean and synced — the
    /// caller should render nothing in that case rather than an empty pill.
    /// When the worktree has pending changes, prefers the change count over
    /// the ahead/behind arrows since uncommitted work is what the user is
    /// most likely to be reviewing right now.
    pub(crate) fn badge_label(&self) -> Option<String> {
        let pending = self.pending_count();
        if pending > 0 {
            return Some(pending.to_string());
        }
        match (self.ahead, self.behind) {
            (0, 0) => None,
            (a, 0) => Some(format!("↑{a}")),
            (0, b) => Some(format!("↓{b}")),
            (a, b) => Some(format!("↑{a} ↓{b}")),
        }
    }
}

impl FileStatus {
    /// Parse a single porcelain v2 status character. Returns `Unmodified` for `.`.
    pub(crate) fn from_porcelain_char(c: char) -> Self {
        match c {
            '.' => Self::Unmodified,
            'M' => Self::Modified,
            'A' => Self::Added,
            'D' => Self::Deleted,
            'R' => Self::Renamed,
            'C' => Self::Copied,
            'T' => Self::TypeChanged,
            'U' => Self::Unmerged,
            _ => Self::Unmodified,
        }
    }

    /// Single-character label suitable for sidebar badges and file list rows.
    pub(crate) fn badge(&self) -> &'static str {
        match self {
            Self::Unmodified => " ",
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Copied => "C",
            Self::TypeChanged => "T",
            Self::Untracked => "?",
            Self::Ignored => "!",
            Self::Unmerged => "U",
        }
    }
}
