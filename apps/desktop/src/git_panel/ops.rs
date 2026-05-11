//! Mutating git operations the diff panel needs: stage, unstage, commit.
//!
//! All operations shell out to the system `git` binary (consistent with the
//! "shell out, don't link" red line) and run synchronously from the caller's
//! thread. Callers running on the GPUI main thread should wrap these in
//! `smol::unblock` so a slow filesystem can't stall the renderer — the
//! `spawn_*` helpers in `gpui_entry.rs` do exactly that.

use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// Stage `path` in `repo_root`. Equivalent to `git add -- <path>`. Works for
/// modified, deleted, and untracked files — `git add` figures out the
/// right operation. The path is repo-relative.
pub(crate) fn run_git_add(repo_root: &Path, path: &str) -> io::Result<()> {
    let output = Command::new("git")
        .arg("add")
        .arg("--")
        .arg(path)
        .current_dir(repo_root)
        .output()?;
    succeed_or_io_err(output, "git add")
}

/// Unstage `path` in `repo_root`. Equivalent to `git restore --staged --
/// <path>`. The worktree is untouched — only the index entry is reset to
/// match HEAD.
pub(crate) fn run_git_restore_staged(repo_root: &Path, path: &str) -> io::Result<()> {
    let output = Command::new("git")
        .arg("restore")
        .arg("--staged")
        .arg("--")
        .arg(path)
        .current_dir(repo_root)
        .output()?;
    succeed_or_io_err(output, "git restore --staged")
}

/// Create a commit with `message`. The message is fed via stdin (`git commit
/// -F -`) to avoid shell escaping concerns when it contains quotes, newlines,
/// or non-ASCII characters. Returns an error if there's nothing staged or git
/// itself fails (e.g. missing `user.email`).
pub(crate) fn run_git_commit(repo_root: &Path, message: &str) -> io::Result<()> {
    let mut child = Command::new("git")
        .arg("commit")
        .arg("-F")
        .arg("-")
        .current_dir(repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    {
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "git commit stdin unavailable")
        })?;
        stdin.write_all(message.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    succeed_or_io_err(output, "git commit")
}

/// Stage everything reportable as a change in `repo_root` — modified,
/// deleted, AND untracked — via `git add -A`. Mirrors the user's mental
/// model of "stage all changes in the panel": every entry visible in the
/// diff panel's file list (which excludes ignored files since polling runs
/// with `--untracked-files=normal`) ends up in the index.
///
/// Distinct from `git add -u` (tracked changes only) and `git add .` (only
/// inside the current cwd). `-A` is repo-wide and includes untracked, which
/// matches what users mean by "Stage All".
pub(crate) fn run_git_stage_all(repo_root: &Path) -> io::Result<()> {
    let output = Command::new("git")
        .arg("add")
        .arg("-A")
        .current_dir(repo_root)
        .output()?;
    succeed_or_io_err(output, "git add -A")
}

/// Apply a unified-diff `patch` to the index only (`git apply --cached`).
/// When `reverse` is true, applies the inverse — used to unstage a
/// previously-staged hunk. Patches must be self-contained: include
/// `--- a/<path>` / `+++ b/<path>` headers and `@@ ... @@` hunk headers
/// the same way `git diff` would emit them. Errors flow through verbatim
/// from git's stderr so the UI can surface "patch does not apply" / "no
/// such file" without re-parsing.
pub(crate) fn run_git_apply_cached(
    repo_root: &Path,
    patch: &str,
    reverse: bool,
) -> io::Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("apply").arg("--cached");
    if reverse {
        cmd.arg("--reverse");
    }
    cmd.arg("--whitespace=nowarn")
        .current_dir(repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    {
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "git apply stdin unavailable")
        })?;
        stdin.write_all(patch.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    succeed_or_io_err(
        output,
        if reverse {
            "git apply --cached --reverse"
        } else {
            "git apply --cached"
        },
    )
}

/// Push the current branch to its configured upstream. Equivalent to plain
/// `git push` (no remote/refspec) so the user's existing tracking config is
/// authoritative. Fails verbatim with whatever `git push` writes to stderr —
/// network errors, "no upstream", "ref out of date" all flow through
/// unchanged so the UI can render them without re-interpreting.
pub(crate) fn run_git_push(repo_root: &Path) -> io::Result<()> {
    let output = Command::new("git")
        .arg("push")
        .current_dir(repo_root)
        .output()?;
    succeed_or_io_err(output, "git push")
}

fn succeed_or_io_err(output: std::process::Output, label: &str) -> io::Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Some commands report failures on stdout (commit "nothing to commit" is
    // the canonical example), so include both streams in the surfaced error.
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(io::Error::new(
        io::ErrorKind::Other,
        format!(
            "{label} exited with {}: {}",
            output.status,
            if !stderr.trim().is_empty() {
                stderr.trim().to_string()
            } else if !stdout.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                "(no output)".to_string()
            }
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::TempDir;

    /// Build a fresh git repo with `user.email` / `user.name` set and an
    /// initial commit so subsequent operations can run. Returns the TempDir
    /// (caller holds it; dropping cleans up).
    fn init_test_repo() -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        run(root, &["init", "--initial-branch=main"]);
        run(root, &["config", "user.email", "test@example.com"]);
        run(root, &["config", "user.name", "Test User"]);
        run(root, &["config", "commit.gpgsign", "false"]);

        fs::write(root.join("README.md"), "initial\n").unwrap();
        run(root, &["add", "README.md"]);
        run(root, &["commit", "-m", "initial commit"]);

        dir
    }

    fn run(repo_root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .expect("git invocation failed");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn list_porcelain(repo_root: &Path) -> String {
        let output = Command::new("git")
            .args(["status", "--porcelain=v2", "--branch"])
            .current_dir(repo_root)
            .output()
            .expect("git status");
        String::from_utf8(output.stdout).unwrap()
    }

    #[test]
    fn run_git_add_stages_a_modified_file() {
        let dir = init_test_repo();
        let root = dir.path();

        fs::write(root.join("README.md"), "changed\n").unwrap();
        // Sanity: modified but not staged.
        let before = list_porcelain(root);
        assert!(before.contains(" .M "), "expected unstaged change: {before}");

        run_git_add(root, "README.md").expect("git add succeeds");

        let after = list_porcelain(root);
        assert!(after.contains("M. "), "expected staged change: {after}");
    }

    #[test]
    fn run_git_add_stages_an_untracked_file() {
        let dir = init_test_repo();
        let root = dir.path();

        fs::write(root.join("new.txt"), "hello\n").unwrap();
        let before = list_porcelain(root);
        assert!(
            before.contains("? new.txt"),
            "expected untracked entry: {before}"
        );

        run_git_add(root, "new.txt").expect("git add succeeds for new file");

        let after = list_porcelain(root);
        assert!(
            after.contains("A. ") && after.contains("new.txt"),
            "expected staged add: {after}"
        );
    }

    #[test]
    fn run_git_restore_staged_unstages_a_file() {
        let dir = init_test_repo();
        let root = dir.path();

        fs::write(root.join("README.md"), "changed\n").unwrap();
        run_git_add(root, "README.md").unwrap();
        // Confirm it's staged.
        let staged = list_porcelain(root);
        assert!(staged.contains("M. "), "expected staged: {staged}");

        run_git_restore_staged(root, "README.md").expect("git restore --staged succeeds");

        let unstaged = list_porcelain(root);
        assert!(
            unstaged.contains(" .M "),
            "expected back to unstaged after restore: {unstaged}"
        );
    }

    #[test]
    fn run_git_commit_creates_a_commit_from_stdin_message() {
        let dir = init_test_repo();
        let root = dir.path();

        fs::write(root.join("README.md"), "v2\n").unwrap();
        run_git_add(root, "README.md").unwrap();

        run_git_commit(root, "second commit").expect("git commit succeeds");

        // Worktree is clean now.
        let after = list_porcelain(root);
        assert!(
            !after.contains(" .M ") && !after.contains("M. "),
            "expected clean status after commit: {after}"
        );

        // Latest commit message matches.
        let log = Command::new("git")
            .args(["log", "-1", "--pretty=%s"])
            .current_dir(root)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&log.stdout).trim(),
            "second commit"
        );
    }

    #[test]
    fn run_git_commit_accepts_multiline_message() {
        let dir = init_test_repo();
        let root = dir.path();

        fs::write(root.join("README.md"), "v3\n").unwrap();
        run_git_add(root, "README.md").unwrap();

        let msg = "summary line\n\nlonger body with\nmultiple lines";
        run_git_commit(root, msg).expect("multi-line commit succeeds");

        let log = Command::new("git")
            .args(["log", "-1", "--pretty=%B"])
            .current_dir(root)
            .output()
            .unwrap();
        let logged = String::from_utf8_lossy(&log.stdout);
        assert!(logged.contains("summary line"), "got: {logged}");
        assert!(logged.contains("longer body"), "got: {logged}");
    }

    #[test]
    fn run_git_commit_errors_when_nothing_staged() {
        let dir = init_test_repo();
        let root = dir.path();

        let err = run_git_commit(root, "empty").expect_err("should fail with nothing to commit");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("nothing")
                || msg.contains("no changes")
                || msg.contains("untracked")
                || msg.contains("commit"),
            "expected nothing-to-commit error, got: {msg}"
        );
    }

    #[test]
    fn run_git_add_errors_for_missing_path() {
        let dir = init_test_repo();
        let root = dir.path();

        let err = run_git_add(root, "does-not-exist.txt").expect_err("should fail");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("did not match")
                || msg.contains("pathspec")
                || msg.contains("no such")
                || msg.contains("exit"),
            "expected pathspec error, got: {msg}"
        );
    }

    /// Create a bare repo at `<tmpdir>/origin.git` and clone-init a working
    /// repo at `<tmpdir>/work` that pushes to it. Returns the TempDir (caller
    /// keeps it alive) and the working repo path.
    fn init_repo_with_remote() -> (TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let origin = dir.path().join("origin.git");
        let work = dir.path().join("work");

        std::fs::create_dir_all(&origin).unwrap();
        std::fs::create_dir_all(&work).unwrap();

        // Bare remote.
        run(&origin, &["init", "--bare", "--initial-branch=main"]);

        // Working repo with the bare as upstream.
        run(&work, &["init", "--initial-branch=main"]);
        run(&work, &["config", "user.email", "test@example.com"]);
        run(&work, &["config", "user.name", "Test User"]);
        run(&work, &["config", "commit.gpgsign", "false"]);
        let origin_url = origin.to_string_lossy().into_owned();
        run(&work, &["remote", "add", "origin", &origin_url]);

        fs::write(work.join("README.md"), "initial\n").unwrap();
        run(&work, &["add", "README.md"]);
        run(&work, &["commit", "-m", "initial"]);
        // Set upstream + push so the next `git push` (no args) works.
        run(&work, &["push", "--set-upstream", "origin", "main"]);

        (dir, work)
    }

    #[test]
    fn run_git_apply_cached_stages_a_specific_hunk() {
        let dir = init_test_repo();
        let root = dir.path();

        // Seed a file with a generous gap between two regions so the two
        // edits below get serialized as TWO distinct hunks (default git
        // diff context is 3 lines, so we need >6 unchanged lines between
        // the two edits to keep the hunks separate).
        let baseline: String = std::iter::once("alpha".to_string())
            .chain((1..=20).map(|i| format!("line {i}")))
            .chain(std::iter::once("delta".to_string()))
            .chain((21..=40).map(|i| format!("line {i}")))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(root.join("multi.txt"), &baseline).unwrap();
        run(root, &["add", "multi.txt"]);
        run(root, &["commit", "-m", "seed"]);

        // Modify the two distant lines.
        let modified = baseline.replace("alpha", "ALPHA").replace("delta", "DELTA");
        fs::write(root.join("multi.txt"), &modified).unwrap();

        // Grab the full unified diff so we can carve out just hunk 1.
        let diff = Command::new("git")
            .args(["diff", "--no-color", "--", "multi.txt"])
            .current_dir(root)
            .output()
            .unwrap();
        let full = String::from_utf8(diff.stdout).unwrap();
        assert!(full.contains("@@"), "expected hunks in diff: {full}");

        // Crude carving: take everything up to but not including the second
        // `@@` line. The first `@@` appears right after the file headers.
        let mut lines = full.lines().peekable();
        let mut patch = String::new();
        let mut at_count = 0;
        while let Some(line) = lines.next() {
            if line.starts_with("@@") {
                at_count += 1;
                if at_count == 2 {
                    break;
                }
            }
            patch.push_str(line);
            patch.push('\n');
        }
        assert!(at_count >= 1, "carving failed: {patch}");

        // Apply just hunk 1.
        run_git_apply_cached(root, &patch, false).expect("apply cached succeeds");

        // Index should now have hunk 1's change (ALPHA) but not hunk 2's
        // (DELTA still unstaged).
        let cached = Command::new("git")
            .args(["diff", "--cached", "--no-color", "--", "multi.txt"])
            .current_dir(root)
            .output()
            .unwrap();
        let cached_text = String::from_utf8_lossy(&cached.stdout);
        assert!(
            cached_text.contains("+ALPHA"),
            "hunk 1 should be staged: {cached_text}"
        );
        assert!(
            !cached_text.contains("+DELTA"),
            "hunk 2 must remain unstaged: {cached_text}"
        );

        // Worktree still differs from the index — DELTA is present in the
        // file, but not staged.
        let worktree = Command::new("git")
            .args(["diff", "--no-color", "--", "multi.txt"])
            .current_dir(root)
            .output()
            .unwrap();
        let worktree_text = String::from_utf8_lossy(&worktree.stdout);
        assert!(
            worktree_text.contains("+DELTA"),
            "hunk 2 still expected in worktree: {worktree_text}"
        );
    }

    #[test]
    fn run_git_apply_cached_reverse_unstages_a_hunk() {
        let dir = init_test_repo();
        let root = dir.path();

        // Stage a change, capture the staged diff, then reverse-apply it to
        // unstage. Equivalent to clicking "−" on a hunk that was previously
        // staged.
        fs::write(root.join("README.md"), "v2\n").unwrap();
        run_git_add(root, "README.md").unwrap();

        let cached = Command::new("git")
            .args(["diff", "--cached", "--no-color", "--", "README.md"])
            .current_dir(root)
            .output()
            .unwrap();
        let patch = String::from_utf8(cached.stdout).unwrap();
        assert!(!patch.is_empty(), "should have a staged diff to reverse");

        run_git_apply_cached(root, &patch, true).expect("reverse apply succeeds");

        let status = list_porcelain(root);
        assert!(
            status.contains(" .M "),
            "expected back to unstaged after reverse apply: {status}"
        );
        assert!(
            !status.contains("M. "),
            "expected nothing staged after reverse apply: {status}"
        );
    }

    #[test]
    fn run_git_apply_cached_errors_on_malformed_patch() {
        let dir = init_test_repo();
        let root = dir.path();

        let bad = "this is not a unified diff\nat all\n";
        let err = run_git_apply_cached(root, bad, false).expect_err("should fail");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("patch") || msg.contains("input") || msg.contains("exit"),
            "expected patch-related error, got: {msg}"
        );
    }

    #[test]
    fn run_git_stage_all_stages_modified_and_untracked_together() {
        let dir = init_test_repo();
        let root = dir.path();

        fs::write(root.join("README.md"), "changed\n").unwrap();
        fs::write(root.join("new1.txt"), "a\n").unwrap();
        fs::write(root.join("new2.txt"), "b\n").unwrap();

        let before = list_porcelain(root);
        assert!(before.contains(" .M "), "expected unstaged change: {before}");
        assert!(before.contains("? new1.txt"), "expected untracked: {before}");

        run_git_stage_all(root).expect("git add -A succeeds");

        let after = list_porcelain(root);
        // Modified file is now staged (M.).
        assert!(after.contains("M. "), "expected staged modify: {after}");
        // Untracked files are now added (A.).
        assert!(
            after.matches("A. ").count() >= 2,
            "expected two staged adds: {after}"
        );
        // No untracked left.
        assert!(
            !after.contains("? new"),
            "expected no untracked remaining: {after}"
        );
    }

    #[test]
    fn run_git_push_sends_new_commit_to_upstream() {
        let (_dir, work) = init_repo_with_remote();

        fs::write(work.join("README.md"), "v2\n").unwrap();
        run_git_add(&work, "README.md").unwrap();
        run_git_commit(&work, "second").unwrap();

        // After commit, ahead should be 1.
        let ab = Command::new("git")
            .args(["rev-list", "--left-right", "--count", "origin/main...HEAD"])
            .current_dir(&work)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&ab.stdout).trim().ends_with("1")
                || String::from_utf8_lossy(&ab.stdout).trim().contains("\t1"),
            "expected to be 1 ahead before push, got: {}",
            String::from_utf8_lossy(&ab.stdout)
        );

        run_git_push(&work).expect("push succeeds");

        // After push, nothing ahead.
        let ab_after = Command::new("git")
            .args(["rev-list", "--left-right", "--count", "origin/main...HEAD"])
            .current_dir(&work)
            .output()
            .unwrap();
        let line = String::from_utf8_lossy(&ab_after.stdout);
        assert!(
            line.trim() == "0\t0" || line.trim().ends_with("0"),
            "expected 0 ahead/behind after push, got: {line}"
        );
    }

    #[test]
    fn run_git_push_errors_without_upstream() {
        let dir = init_test_repo();
        let root = dir.path();

        // No `origin` configured — push must fail loudly.
        let err = run_git_push(root).expect_err("push should fail with no upstream");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("no upstream")
                || msg.contains("no configured push destination")
                || msg.contains("origin")
                || msg.contains("does not appear to be a git repository")
                || msg.contains("exit"),
            "expected upstream-related error, got: {msg}"
        );
    }

    #[test]
    fn run_git_commit_message_with_special_chars_via_stdin() {
        let dir = init_test_repo();
        let root = dir.path();

        fs::write(root.join("README.md"), "v4\n").unwrap();
        run_git_add(root, "README.md").unwrap();

        // Quotes, backticks, dollar signs, emoji — would all be hazardous via
        // shell `-m`. Stdin bypasses the shell entirely.
        let msg = "fix(scope): handle `$PATH` with \"quotes\" 🎉";
        run_git_commit(root, msg).expect("special chars commit succeeds");

        let log = Command::new("git")
            .args(["log", "-1", "--pretty=%s"])
            .current_dir(root)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&log.stdout).trim(), msg);
    }
}
