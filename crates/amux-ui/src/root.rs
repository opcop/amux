use std::path::PathBuf;
use std::sync::Arc;

use amux_core::{PaneId, SplitAxis, TabId};
use amux_platform::HostPlatform;

use crate::{
    commands::UiAction,
    controller::AppController,
    render::{AppRenderer, TextRenderer},
    AppSnapshot, UiState,
};

/// Caller-provided startup intent for `DesktopApp::startup`.
///
/// The product startup flow has three orthogonal inputs:
///   - Whether the user passed an explicit workspace path on the command line.
///   - Whether a previously persisted session can be restored from disk.
///   - The empty/welcome fallback when neither of the above produced a workspace.
///
/// This struct keeps those inputs explicit so `main.rs` doesn't have to know
/// how the controller layers them — and so future flags (e.g. "ignore stored
/// session", "force welcome screen") can be added without rewriting callers.
#[derive(Clone, Debug, Default)]
pub struct StartupOptions {
    /// Optional workspace folder to open immediately. When `None`, the
    /// startup flow falls back to "restore session" → "empty welcome".
    pub workspace: Option<PathBuf>,
}

/// Result of [`DesktopApp::startup`], used by `main.rs` to print a banner
/// that reflects what actually happened (no silent fallbacks).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupResult {
    pub mode: StartupMode,
    pub workspace_count: usize,
}

/// Discriminator for the path the startup flow took.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupMode {
    /// User passed `--workspace <path>` (or equivalent) and we opened it.
    /// `path` is the resolved workspace folder.
    OpenedWorkspace { path: PathBuf },
    /// No explicit workspace; we restored an existing session from disk.
    Restored,
    /// No explicit workspace and no restorable session, but we auto-
    /// opened a default workspace rooted at `$HOME` so the user gets a
    /// working terminal on launch instead of a blank window. Opt-in
    /// "empty/welcome" is still available via [`StartupMode::Empty`]
    /// when `HOME` / `USERPROFILE` resolves to something that isn't a
    /// real directory.
    DefaultHome { path: PathBuf },
    /// Truly empty startup — no session, no `$HOME` resolved. The
    /// user is expected to use Ctrl+Shift+N or the command palette
    /// `workspace open <path>` flow to get going. This path is now
    /// rare in practice (HOME is nearly always set) and mostly a
    /// safety valve for broken test environments.
    Empty,
}

#[derive(Clone, Debug)]
pub struct DesktopApp {
    name: String,
    state: UiState,
    controller: AppController,
}

impl DesktopApp {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            state: UiState::default(),
            controller: AppController::new(&name),
        }
    }

    /// Create with real filesystem and terminal backends
    pub fn with_real_backends(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            state: UiState::default(),
            controller: AppController::with_real_fs(&name),
        }
    }

    pub fn with_platform(name: impl Into<String>, platform: Arc<dyn HostPlatform>) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            state: UiState::default(),
            controller: AppController::with_platform(&name, platform),
        }
    }

    pub fn banner(&self) -> String {
        format!("{} bootstrap ready", self.name)
    }

    /// Run the product startup flow.
    ///
    /// Replaces the historical `bootstrap_demo` entry point with an honest,
    /// non-opinionated launch:
    ///
    /// 1. If `options.workspace` is `Some`, open it.
    /// 2. Otherwise try to restore a previously persisted session.
    /// 3. Otherwise enter the empty / welcome state — the GPUI shell will
    ///    render an empty workspace and the user can use Ctrl+Shift+N or
    ///    the command palette to open a folder.
    ///
    /// The opinionated demo flow (auto-open cwd, mock README, auto-launch
    /// codex agent, force a vertical split) is now gated behind
    /// `seed_demo_state`, which is only available in `cfg(test)` or with
    /// the `demo-bootstrap` feature, so production builds cannot regress
    /// to it by accident.
    pub fn startup(&mut self, options: StartupOptions) -> StartupResult {
        if let Some(path) = options.workspace {
            // An explicit workspace path is the strongest signal of intent;
            // we still restore the session first so the user's other
            // workspaces / layout state survive across launches, then open
            // the requested folder on top.
            let _ = self.controller.restore_session_if_present(&mut self.state);
            self.controller.open_local_workspace(&mut self.state, path.clone());
            return StartupResult {
                mode: StartupMode::OpenedWorkspace { path },
                workspace_count: self.state.session.workspaces.len(),
            };
        }

        if self.controller.restore_session_if_present(&mut self.state) {
            return StartupResult {
                mode: StartupMode::Restored,
                workspace_count: self.state.session.workspaces.len(),
            };
        }

        // No explicit workspace, no restorable session — fall back to
        // auto-opening a default workspace at `$HOME` so amux comes up
        // as a usable terminal instead of a blank window. This is the
        // UX unblock that replaces the old "click Open Workspace to
        // use amux" friction; group-aware sidebar puts the default
        // workspace under the unnamed default group so the visual
        // result is a single flat row, just like the pre-group
        // behavior of an open workspace.
        if let Some(home) = Self::resolve_home_dir() {
            self.controller
                .open_local_workspace(&mut self.state, home.clone());
            return StartupResult {
                mode: StartupMode::DefaultHome { path: home },
                workspace_count: self.state.session.workspaces.len(),
            };
        }

        StartupResult {
            mode: StartupMode::Empty,
            workspace_count: 0,
        }
    }

    /// Resolve the user's home directory as a `PathBuf`. Tries
    /// `HOME` first (Unix), then `USERPROFILE` (Windows), and
    /// finally returns `None` if neither resolves to an actual
    /// directory — in which case the caller falls back to
    /// [`StartupMode::Empty`] rather than guessing `/`.
    fn resolve_home_dir() -> Option<PathBuf> {
        let raw = std::env::var("HOME")
            .ok()
            .or_else(|| std::env::var("USERPROFILE").ok())?;
        let path = PathBuf::from(raw);
        if path.is_dir() {
            Some(path)
        } else {
            None
        }
    }

    /// Test/dev-only seed of the opinionated demo workspace.
    ///
    /// Available in unit tests via `cfg(test)` and behind the
    /// `demo-bootstrap` Cargo feature for ad-hoc developer use. The
    /// production `main.rs` MUST go through `startup` instead.
    #[cfg(test)]
    pub fn seed_demo_state(&mut self) {
        self.controller.seed_demo_state(&mut self.state);
    }

    pub fn dispatch(&mut self, action: UiAction) -> Vec<amux_core::Event> {
        self.controller.dispatch(&mut self.state, action)
    }

    pub fn run_command(&mut self, input: &str) -> Result<String, String> {
        self.controller.run_command(&mut self.state, input)
    }

    pub fn set_command_palette_query(&mut self, query: impl Into<String>) {
        let _ = self.dispatch(UiAction::SetCommandPaletteQuery(query.into()));
    }

    pub fn clear_command_palette_query(&mut self) {
        let _ = self.dispatch(UiAction::ClearCommandPaletteQuery);
    }

    pub fn append_command_palette_query(&mut self, segment: impl Into<String>) {
        let _ = self.dispatch(UiAction::AppendCommandPaletteQuery(segment.into()));
    }

    pub fn backspace_command_palette_query(&mut self) {
        let _ = self.dispatch(UiAction::BackspaceCommandPaletteQuery);
    }

    pub fn select_next_palette_item(&mut self) {
        let count = crate::commands::filtered_palette_commands_for(
            &self.state.command_palette_query,
            &self.controller.platform_capabilities(),
        )
        .len();
        if count == 0 {
            let _ = self.dispatch(UiAction::SetCommandPaletteSelectedIndex(0));
            return;
        }
        let next = (self.state.command_palette_selected_index + 1) % count;
        let _ = self.dispatch(UiAction::SetCommandPaletteSelectedIndex(next));
    }

    pub fn select_previous_palette_item(&mut self) {
        let count = crate::commands::filtered_palette_commands_for(
            &self.state.command_palette_query,
            &self.controller.platform_capabilities(),
        )
        .len();
        if count == 0 {
            let _ = self.dispatch(UiAction::SetCommandPaletteSelectedIndex(0));
            return;
        }
        let current = self.state.command_palette_selected_index.min(count - 1);
        let previous = if current == 0 { count - 1 } else { current - 1 };
        let _ = self.dispatch(UiAction::SetCommandPaletteSelectedIndex(previous));
    }

    /// Get the command string of the currently selected palette item.
    pub fn selected_palette_command_str(&self) -> Option<String> {
        let commands = crate::commands::filtered_palette_commands_for(
            &self.state.command_palette_query,
            &self.controller.platform_capabilities(),
        );
        if commands.is_empty() {
            return None;
        }
        let index = self.state.command_palette_selected_index
            .min(commands.len().saturating_sub(1));
        Some(commands[index].command.clone())
    }

    pub fn execute_selected_palette_command(&mut self) -> Result<String, String> {
        let commands = crate::commands::filtered_palette_commands_for(
            &self.state.command_palette_query,
            &self.controller.platform_capabilities(),
        );
        if commands.is_empty() {
            return Err("no command matches the current palette query".into());
        }
        let index = self
            .state
            .command_palette_selected_index
            .min(commands.len().saturating_sub(1));
        let command = commands[index].command.clone();
        self.run_command(&command)
    }

    pub fn activate_workspace(&mut self, workspace_id: &str) -> Result<(), String> {
        self.controller
            .activate_workspace(&mut self.state, workspace_id)
    }

    pub fn rename_workspace(&mut self, workspace_id: &str, new_name: &str) -> Result<String, String> {
        self.controller
            .rename_workspace(&mut self.state, workspace_id, new_name)
    }

    pub fn split_active_pane(&mut self, axis: SplitAxis) -> Result<(), String> {
        self.controller.split_active_pane(&mut self.state, axis)
    }

    pub fn focus_pane(&mut self, pane_id: PaneId) -> Result<(), String> {
        self.controller.focus_pane(&mut self.state, pane_id)
    }

    pub fn activate_tab(&mut self, pane_id: PaneId, tab_id: TabId) -> Result<(), String> {
        self.controller
            .activate_tab(&mut self.state, pane_id, tab_id)
    }

    pub fn close_tab(&mut self, pane_id: PaneId, tab_id: TabId) -> Result<(), String> {
        self.controller.close_tab(&mut self.state, pane_id, tab_id)
    }

    pub fn snapshot(&mut self) -> AppSnapshot {
        self.controller.snapshot(&mut self.state)
    }

    pub fn render_with<R: AppRenderer>(&mut self, renderer: &R) -> R::Output {
        let name = self.name.clone();
        let snapshot = self.snapshot();
        renderer.render(&name, &snapshot)
    }

    pub fn render_text_ui(&mut self) -> String {
        self.render_with(&TextRenderer)
    }

    pub fn session_path(&self) -> PathBuf {
        self.controller.session_path()
    }

    pub fn pick_workspace_folder(&self) -> Result<Option<PathBuf>, String> {
        self.controller.pick_workspace_folder()
    }

    pub fn open_local_workspace(&mut self, path: PathBuf) {
        let _ = self.dispatch(UiAction::OpenLocalWorkspace(path));
    }
}

#[cfg(test)]
mod tests {
    use super::{DesktopApp, StartupMode, StartupOptions};

    /// Create a test app with a unique session path to avoid
    /// cross-test interference. The controller writes its session
    /// to `~/.<slug>` (via `default_session_dir`), not
    /// `std::env::temp_dir()/<slug>-session` as the old cleanup
    /// here assumed. Clean both for safety — any test that persists
    /// a session needs the first path gone, and the second path is
    /// cheap to unlink even if it never existed.
    fn test_app(suffix: &str) -> DesktopApp {
        let name = format!("amux-test-{}", suffix);
        let slug = name.to_ascii_lowercase().replace(' ', "-");
        if let Some(home) = amux_platform::real_user_home() {
            let _ = std::fs::remove_dir_all(home.join(format!(".{slug}")));
        }
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("{}-session", name)),
        );
        DesktopApp::new(name)
    }

    #[test]
    fn startup_with_no_session_opens_default_home() {
        // Supersedes the original CP6 `Empty` assertion: amux now
        // auto-opens a workspace at `$HOME` on empty launches so the
        // user gets a working terminal instead of a blank window.
        // The original CP6 anti-regressions that still matter are:
        //   * No mock README.md seeded (open_files stays empty).
        //   * No opinionated demo surface (no codex auto-launch,
        //     no vertical split seeded).
        // The "don't auto-open cwd" clause has been *deliberately
        // relaxed*: we still don't touch `current_dir()` (which is
        // `/` for macOS .app launches), we open the user's home
        // directory instead, and the new `StartupMode::DefaultHome`
        // variant exists precisely to make that transition audible
        // in telemetry / banners.
        let mut app = test_app("startup-empty");
        let result = app.startup(StartupOptions::default());
        match &result.mode {
            StartupMode::DefaultHome { path } => {
                assert!(path.is_dir(), "resolved home must be a real dir");
            }
            StartupMode::Empty => {
                // Acceptable only in environments where neither
                // HOME nor USERPROFILE resolves to a directory —
                // extremely rare in practice but we keep the path
                // open so CI runners with a broken env don't panic.
            }
            other => panic!("unexpected mode: {other:?}"),
        }

        let snapshot = app.snapshot();
        assert!(
            snapshot.open_files.is_empty(),
            "default-home startup must not auto-open any files"
        );
    }

    #[test]
    fn startup_with_explicit_workspace_opens_it() {
        // CP6 regression: when main.rs forwards `--workspace <path>`,
        // the resulting StartupResult must reflect that and the session
        // must contain the requested workspace.
        let mut app = test_app("startup-explicit");
        let temp = std::env::temp_dir().join("amux-test-startup-explicit-ws");
        let _ = std::fs::create_dir_all(&temp);

        let result = app.startup(StartupOptions {
            workspace: Some(temp.clone()),
        });

        match result.mode {
            StartupMode::OpenedWorkspace { path } => {
                assert_eq!(path, temp);
            }
            other => panic!("expected OpenedWorkspace, got {other:?}"),
        }
        assert!(result.workspace_count >= 1);
    }

    #[test]
    fn command_router_can_launch_agent_and_open_file() {
        let mut app = test_app("launch");
        app.seed_demo_state();

        let launched = app
            .run_command("agent claude")
            .expect("agent command should work");
        let opened = app
            .run_command("file open notes.md")
            .expect("file command should work");

        assert!(launched.contains("claude"));
        assert!(opened.contains("notes.md"));
        let snapshot = app.snapshot();
        assert!(snapshot
            .open_files
            .iter()
            .any(|file| file.relative_path == "notes.md"));
    }

    #[test]
    fn command_router_reuses_existing_agent_and_editor_tabs() {
        let mut app = test_app("reuse");
        app.seed_demo_state();

        let before = app.snapshot();
        let before_open_files = before.open_files.len();
        let before_agent_tabs = count_surface_kind(&before.active_workspace, "agent");

        app.run_command("agent codex")
            .expect("agent command should work");
        app.run_command("file open README.md")
            .expect("file command should work");

        let after = app.snapshot();
        assert_eq!(after.open_files.len(), before_open_files);
        assert_eq!(
            count_surface_kind(&after.active_workspace, "agent"),
            before_agent_tabs
        );
    }

    #[test]
    fn selected_palette_command_executes_filtered_command() {
        let mut app = test_app("palette-exec");
        app.seed_demo_state();
        app.set_command_palette_query("claude");

        let result = app
            .execute_selected_palette_command()
            .expect("selected palette command should execute");

        assert!(result.contains("claude"));
    }

    #[test]
    fn palette_query_can_be_built_incrementally() {
        let mut app = test_app("palette-query");
        app.append_command_palette_query("agent");
        app.append_command_palette_query("claude");

        let snapshot = app.snapshot();
        assert_eq!(snapshot.command_palette_query, "agent claude");

        app.backspace_command_palette_query();
        let snapshot = app.snapshot();
        assert_eq!(snapshot.command_palette_query, "agent claud");
    }

    #[test]
    fn palette_selection_wraps_around_filtered_commands() {
        // The "agent" query may match a varying number of palette commands
        // as the catalog evolves, so derive the expected wrap target from
        // the actual filtered count instead of hard-coding it. The test's
        // intent is just "select_previous from index 0 wraps to the last
        // filtered item, and select_next wraps it back to 0".
        let mut app = test_app("palette-wrap");
        app.set_command_palette_query("agent");

        let filtered = crate::commands::filtered_palette_commands_for(
            &app.state.command_palette_query,
            &app.controller.platform_capabilities(),
        );
        let count = filtered.len();
        assert!(count >= 2, "expected at least 2 palette commands matching `agent`, got {count}");
        let last_index = count - 1;

        app.select_previous_palette_item();
        let snapshot = app.snapshot();
        assert_eq!(snapshot.command_palette_selected_index, last_index);

        app.select_next_palette_item();
        let snapshot = app.snapshot();
        assert_eq!(snapshot.command_palette_selected_index, 0);
    }

    fn count_surface_kind(workspace: &Option<crate::WorkspaceSnapshot>, kind: &str) -> usize {
        fn count_layout(layout: &crate::LayoutSnapshot, kind: &str) -> usize {
            match layout {
                crate::LayoutSnapshot::Pane(pane) => pane
                    .tabs
                    .iter()
                    .filter(|tab| tab.surface_kind == kind)
                    .count(),
                crate::LayoutSnapshot::Split(split) => {
                    count_layout(&split.first, kind) + count_layout(&split.second, kind)
                }
            }
        }

        workspace
            .as_ref()
            .map(|workspace| count_layout(&workspace.layout, kind))
            .unwrap_or(0)
    }
}
