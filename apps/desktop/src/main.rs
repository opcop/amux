// Hide the console window on Windows when running as a GUI app
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "gpui")]
mod gpui_command_bar;
#[cfg(feature = "gpui")]
mod gpui_command_palette;
#[cfg(feature = "gpui")]
mod gpui_components;
#[cfg(feature = "gpui")]
mod gpui_clipboard;
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
mod gpui_keyboard_shortcuts;
#[cfg(feature = "gpui")]
mod gpui_status_bar;
#[cfg(feature = "gpui")]
mod gpui_surface_views;
#[cfg(feature = "gpui")]
mod gpui_terminal;
#[cfg(feature = "gpui")]
mod gpui_workspace_sidebar;
#[cfg(feature = "gpui")]
mod gpui_browser;
#[cfg(not(feature = "gpui"))]
mod text_entry;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Check for --real flag to use real filesystem/terminal backends
    let use_real = args.iter().any(|a| a == "--real");
    let commands: Vec<&str> = args
        .iter()
        .filter(|a| *a != "--real")
        .map(|s| s.as_str())
        .collect();

    let mut app = if use_real {
        eprintln!("mode: real filesystem + terminal backends");
        amux_ui::DesktopApp::with_real_backends("AMUX")
    } else {
        amux_ui::DesktopApp::new("AMUX")
    };

    app.bootstrap_demo();

    for command in commands {
        match app.run_command(command) {
            Ok(message) => println!("command: {message}"),
            Err(err) => eprintln!("command error: {err}"),
        }
    }

    println!("{}", app.banner());
    println!("session: {}", app.session_path().display());
    println!();

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
