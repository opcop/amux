// Hide the console window on Windows when running as a GUI app
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "gpui")]
mod gpui_clipboard;
mod crash;
mod gpui_config;
#[cfg(feature = "gpui")]
mod gpui_entry;
#[cfg(feature = "gpui")]
mod gpui_input_handler;
#[cfg(feature = "gpui")]
mod gpui_layout_renderer;
#[cfg(feature = "gpui")]
mod gpui_preview;
#[cfg(feature = "gpui")]
mod gpui_vibe_tools;
mod gpui_workspace_persistence;
#[cfg(feature = "gpui")]
mod gpui_status_bar;
#[cfg(feature = "gpui")]
mod gpui_terminal;
#[cfg(feature = "gpui")]
mod gpui_workspace_sidebar;
#[cfg(feature = "gpui")]
mod gpui_browser;
#[cfg(not(feature = "gpui"))]
mod text_entry;

fn main() {
    // Install panic hook first so any subsequent panic — including
    // during startup — lands in ~/.amux/logs/crash for post-mortem.
    crash::install(crash::crash_log_dir());

    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = parse_cli(&raw_args);

    let mut app = if parsed.demo_mode {
        #[cfg(debug_assertions)]
        eprintln!("mode: in-memory demo (pass --real or omit --demo for production)");
        amux_ui::DesktopApp::new("AMUX")
    } else {
        amux_ui::DesktopApp::with_platform("AMUX", amux_platform::current_host_platform())
    };

    let startup = app.startup(amux_ui::StartupOptions {
        workspace: parsed.workspace.clone(),
    });

    // Startup banner — only in debug builds. Release builds go
    // straight to the GUI with no console output, so double-clicking
    // the binary on macOS doesn't flash a terminal window.
    #[cfg(debug_assertions)]
    {
        for command in &parsed.commands {
            match app.run_command(command) {
                Ok(message) => println!("command: {message}"),
                Err(err) => eprintln!("command error: {err}"),
            }
        }

        println!("{}", app.banner());
        println!("session: {}", app.session_path().display());
        match &startup.mode {
            amux_ui::StartupMode::OpenedWorkspace { path } => {
                println!(
                    "startup: opened workspace {} ({} total)",
                    path.display(),
                    startup.workspace_count
                );
            }
            amux_ui::StartupMode::Restored => {
                println!(
                    "startup: restored {} workspace(s) from session",
                    startup.workspace_count
                );
            }
            amux_ui::StartupMode::Empty => {
                println!(
                    "startup: empty session — use Ctrl+Shift+N or the command palette \
                     (`workspace open <path>`) to open a folder"
                );
            }
        }
        println!();
    }

    // In release mode, still process inline commands silently.
    #[cfg(not(debug_assertions))]
    {
        for command in &parsed.commands {
            let _ = app.run_command(command);
        }
    }

    #[cfg(feature = "gpui")]
    {
        let config = gpui_config::AmuxConfig::load();
        gpui_entry::run(&app, config);
    }

    #[cfg(not(feature = "gpui"))]
    {
        text_entry::run(&mut app);
    }
}

/// Result of parsing the desktop CLI argument list.
struct ParsedCli {
    /// `--demo` opts into the in-memory demo mode (no real PTY / FS).
    /// Default is production mode with real platform backends.
    demo_mode: bool,
    /// Optional explicit workspace path. Accepts both `--workspace <path>`
    /// and a single positional argument that resolves to an existing
    /// directory. Anything else is treated as an inline command (legacy
    /// behavior — used for the `command: ...` lines printed at startup).
    workspace: Option<std::path::PathBuf>,
    /// Inline commands to run after startup. Preserves the historical
    /// "throw the rest at the command router" behavior so existing
    /// scripts keep working.
    commands: Vec<String>,
}

fn parse_cli(args: &[String]) -> ParsedCli {
    let mut demo_mode = false;
    let mut workspace: Option<std::path::PathBuf> = None;
    let mut commands: Vec<String> = Vec::new();

    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            // --demo: opt into in-memory mode (tests / demos).
            // --real: accepted for backwards compat but is now the default.
            "--demo" => demo_mode = true,
            "--real" => {}
            "--workspace" | "-w" => {
                if let Some(path) = iter.next() {
                    workspace = Some(std::path::PathBuf::from(path));
                } else {
                    eprintln!("error: --workspace requires a path argument");
                }
            }
            other => {
                // Positional arg: if it resolves to an existing directory and
                // we haven't already locked in a workspace, treat it as the
                // workspace folder. Otherwise fall through to the legacy
                // "inline command" bucket.
                let candidate = std::path::PathBuf::from(other);
                if workspace.is_none() && candidate.is_dir() {
                    workspace = Some(candidate);
                } else {
                    commands.push(other.to_string());
                }
            }
        }
    }

    ParsedCli {
        demo_mode,
        workspace,
        commands,
    }
}
