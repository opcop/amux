//! Directory resolution for AMUX configuration and user paths.
//!
//! There are TWO conceptually different "home directories" that AMUX needs:
//!
//! 1. **AMUX's own config root** ([`amux_home_dir`]) — where session.json,
//!    layouts.json, config.toml, screenshots/, workspaces/, templates/ live.
//!    This is *amux-private* and may be redirected via the `AMUX_HOME` env
//!    var so users can put it under a dotfiles repo, on shared NAS, or under
//!    an XDG-style layout. Smoke tests and CI also use `AMUX_HOME` to
//!    isolate AMUX's state without polluting the host's `~/.amux/`.
//!
//! 2. **The user's real OS home directory** ([`real_user_home`]) — used
//!    for `~` expansion in user-typed paths (`~/projects/foo`) and for
//!    the value that PTY child processes inherit as `HOME`. This MUST
//!    keep pointing at the user's real home, otherwise downstream CLIs
//!    like `claude`, `gh`, `git`, and `ssh` lose access to their
//!    on-disk credentials and config.
//!
//! Keeping these two concepts separate is what allows AMUX to be both
//! isolatable (for tests, multi-tenant servers, dotfiles workflows) and
//! a good citizen of the host system (no broken auth in spawned shells).
//!
//! ## Important: do NOT propagate `AMUX_HOME` into PTY children explicitly
//!
//! The `RealTerminalBackend` builds child commands via `portable_pty`
//! which inherits the parent process environment by default. We do *not*
//! re-set `HOME` for PTY children, and we never override `HOME` in the
//! amux process itself. The result is that when amux runs with a custom
//! `AMUX_HOME`, the PTY child still sees the real `HOME` exactly as the
//! user's shell would see it.

use std::path::PathBuf;

/// Environment variable that overrides amux's own config directory.
///
/// Documented here so the constant is the single source of truth and
/// shows up in `cargo doc`. Users and tooling should reference this
/// name verbatim.
pub const AMUX_HOME_ENV: &str = "AMUX_HOME";

/// Resolve the directory amux uses to store its own state.
///
/// Resolution order:
///
/// 1. The `AMUX_HOME` environment variable, if set and non-empty. The
///    value is used directly as the config root (no `.amux` suffix
///    appended). This is the production-grade override.
/// 2. The user's real home directory plus `.amux` (`~/.amux` on Unix,
///    `%USERPROFILE%\.amux` on Windows). This is the default that all
///    existing installations rely on.
/// 3. A subdirectory of the OS temp dir as a last-resort fallback when
///    no home directory can be determined (e.g. minimal CI containers).
pub fn amux_home_dir() -> PathBuf {
    if let Ok(custom) = std::env::var(AMUX_HOME_ENV) {
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
    }
    if let Some(home) = real_user_home() {
        return home.join(".amux");
    }
    std::env::temp_dir().join("amux-home")
}

/// Resolve the user's real OS home directory.
///
/// Used for `~` expansion in user-typed paths and as the value PTY
/// children should see in `HOME`. Distinct from [`amux_home_dir`],
/// which represents amux's *own* config root and may be redirected by
/// `AMUX_HOME`.
///
/// Returns `None` if the platform-appropriate env var is missing or
/// empty (very rare on a real desktop session, but worth handling).
pub fn real_user_home() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each test below mutates the env, so they are intentionally
    /// serialized via a single `#[test]` function. Splitting them would
    /// race because tests in the same crate run in parallel.
    #[test]
    fn amux_home_resolution_order() {
        // Snapshot real env so we can restore.
        let original_amux_home = std::env::var(AMUX_HOME_ENV).ok();
        let restore = || {
            // SAFETY: tests run single-threaded inside this function.
            unsafe {
                match &original_amux_home {
                    Some(v) => std::env::set_var(AMUX_HOME_ENV, v),
                    None => std::env::remove_var(AMUX_HOME_ENV),
                }
            }
        };

        // 1. Explicit override wins.
        // SAFETY: see comment above.
        unsafe {
            std::env::set_var(AMUX_HOME_ENV, "/tmp/amux-test-explicit");
        }
        assert_eq!(
            amux_home_dir(),
            PathBuf::from("/tmp/amux-test-explicit"),
            "AMUX_HOME should be honored verbatim with no .amux suffix"
        );

        // 2. Empty AMUX_HOME falls through to the home-dir branch.
        // SAFETY: see comment above.
        unsafe {
            std::env::set_var(AMUX_HOME_ENV, "");
        }
        let resolved = amux_home_dir();
        assert!(
            resolved.ends_with(".amux") || resolved.starts_with(std::env::temp_dir()),
            "empty AMUX_HOME should fall through to ~/.amux or temp fallback, got {resolved:?}"
        );

        restore();
    }
}
