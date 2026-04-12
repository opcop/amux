//! Application bootstrap: GPUI window open + main event loop.
//!
//! This is the code that used to sit at the bottom of
//! `gpui_entry.rs` as a ~450-line `pub fn run`. It owns three
//! concerns that have nothing to do with the per-frame render or
//! the shell view itself:
//!
//!   1. GPUI `application().run()` entry point — dark theme,
//!      `gpui_component::init`, macOS dock icon, macOS native
//!      menubar with app / Edit menus and their action handlers.
//!   2. The 60 fps timer task that drives PTY output polling,
//!      cursor blink, deferred tool detection, deferred PTY spawn,
//!      browser WebView2 bounds sync, auto-save, and toast expiry.
//!   3. The macOS dock-icon PNG pipeline: decodes the embedded
//!      JPEG, resizes to the Apple-HIG visible square, and applies
//!      a rounded-rectangle "squircle" alpha mask so the icon
//!      matches native Dock neighbours instead of reading as a
//!      ported Windows app.
//!
//! Keeping all of this in one file means `gpui_entry.rs` no longer
//! needs `use smol::Timer`, the macos `objc2` crates, the `image`
//! crate, or the `gpui_component` menu types in its top-level
//! imports. See the surrounding decomposition commits for the
//! broader motivation.

#[cfg(feature = "gpui")]
use amux_ui::DesktopApp;
#[cfg(feature = "gpui")]
use gpui::{App, AppContext, Context, WindowOptions, px};
#[cfg(feature = "gpui")]
use gpui_platform::application;

#[cfg(feature = "gpui")]
use crate::gpui_config::AmuxConfig;
#[cfg(feature = "gpui")]
use crate::gpui_entry::GpuiShellView;

/// Version tag for the squircle generation pipeline. Bump when
/// the algorithm changes (radius, inner-square ratio, AA band,
/// filter choice, ...) so stale on-disk caches are invalidated
/// even when the source JPEG hasn't changed.
#[cfg(all(feature = "gpui", target_os = "macos"))]
const ICON_PIPELINE_VERSION: u32 = 1;

/// Compute the cache path for a given source image. The filename
/// encodes a hash over both `ICON_PIPELINE_VERSION` and the JPEG
/// bytes, so swapping the source art OR the pipeline algorithm
/// produces a new filename — the old cache becomes unreachable
/// and the pipeline re-runs once on next launch.
#[cfg(all(feature = "gpui", target_os = "macos"))]
fn icon_cache_path(jpg_bytes: &[u8]) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ICON_PIPELINE_VERSION.hash(&mut h);
    jpg_bytes.hash(&mut h);
    let key = h.finish();
    amux_platform::amux_home_dir()
        .join("cache")
        .join(format!("dock-icon-{:016x}.png", key))
}

/// Set the macOS Dock icon at runtime.
///
/// On a warm cache (`~/.amux/cache/dock-icon-{hash}.png` exists
/// and is non-empty), the squircle pipeline is skipped entirely
/// — we just decode the cached PNG and hand it to AppKit. On a
/// cold cache or on the first launch after a pipeline/source
/// bump, the full pipeline runs and the result is written to
/// disk for next time, fire-and-forget.
///
/// SAFETY: must be called on the main thread, after `application().run`
/// has bootstrapped NSApplication. The single call site in `run()` is
/// inside the gpui application closure, which guarantees both.
#[cfg(all(feature = "gpui", target_os = "macos"))]
fn set_macos_dock_icon() {
    use objc2::rc::Retained;
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    // Embedded JPEG. Source-of-truth for the macOS Dock icon — keep
    // in sync with the Windows .ico master.
    const ICON_JPG: &[u8] = include_bytes!("../../../assets/icons/amux.jpg");

    let cache_path = icon_cache_path(ICON_JPG);
    let png_bytes = match std::fs::read(&cache_path) {
        Ok(bytes) if !bytes.is_empty() => {
            crate::metrics::startup_phase("dock_icon_cache_hit");
            bytes
        }
        _ => {
            crate::metrics::startup_phase("dock_icon_cache_miss");
            let bytes = match build_squircle_icon_png(ICON_JPG) {
                Ok(bytes) => bytes,
                Err(err) => {
                    eprintln!("[amux-icon] failed to build squircle PNG: {err}");
                    return;
                }
            };
            // Write cache for next run. Uses a temp file + rename
            // so a crash mid-write doesn't leave a half-written
            // PNG that the next launch would happily read as
            // "cache hit" and hand to AppKit. Errors are logged
            // but non-fatal — running without a cache hit is
            // always a valid fallback.
            if let Some(parent) = cache_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let tmp = cache_path.with_extension("png.tmp");
            if let Err(err) = std::fs::write(&tmp, &bytes)
                .and_then(|_| std::fs::rename(&tmp, &cache_path))
            {
                eprintln!("[amux-icon] failed to write icon cache: {err}");
            }
            bytes
        }
    };

    unsafe {
        let Some(mtm) = MainThreadMarker::new() else {
            // Not on the main thread — skip rather than panic. The
            // single intended call site is the gpui application
            // closure which always runs on the main thread, but a
            // defensive fallback keeps non-main-thread test harnesses
            // from blowing up.
            return;
        };
        let data: Retained<NSData> = NSData::dataWithBytes_length(
            png_bytes.as_ptr().cast(),
            png_bytes.len(),
        );
        let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
            eprintln!("[amux-icon] failed to decode squircle PNG into NSImage");
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        app.setApplicationIconImage(Some(&image));
    }
}

/// Apply a macOS Big Sur–style squircle alpha mask + standard icon
/// margin to a JPEG icon, returning PNG bytes.
///
/// macOS app icons are designed against a specific grid (Apple HIG):
///
/// - **Canvas**: e.g. 1024×1024
/// - **Visible square**: 824×824 centered (≈ 80.5% of the canvas side,
///   ≈ 9.77% transparent margin on every side)
/// - **Corner radius**: ~22.5% of the visible square's side
///
/// Every native Mac app's icon honors this grid, so when AMUX shipped
/// a bitmap that filled the entire canvas it appeared ~25% larger
/// than its Dock neighbours and immediately read as "ported app".
///
/// The pipeline below:
///
///   1. Decode the input JPEG to RGBA8.
///   2. Resize it to fit the *inner* visible square (≈ 80% of the
///      canvas side) with high-quality Lanczos filtering.
///   3. Composite onto a transparent canvas of the original size,
///      centered.
///   4. Apply a rounded-rectangle alpha mask matching the visible
///      square + 22.5% corner radius. The mask is anti-aliased over
///      a 1.5px band so the corners stay smooth at Dock-pixel sizes.
///
/// We use a plain rounded rectangle rather than a true superellipse
/// ("squircle"). At icon resolutions the visible difference is well
/// under one pixel, and a rounded rect is trivial to compute without
/// pulling in a path rasterizer.
#[cfg(all(feature = "gpui", target_os = "macos"))]
fn build_squircle_icon_png(jpg_bytes: &[u8]) -> Result<Vec<u8>, String> {
    use image::imageops::FilterType;
    use image::{ImageEncoder, ImageFormat, RgbaImage};

    let src = image::load_from_memory_with_format(jpg_bytes, ImageFormat::Jpeg)
        .map_err(|e| format!("decode jpeg: {e}"))?
        .to_rgba8();
    let (sw, sh) = src.dimensions();

    // Choose a square canvas equal to the input's longer side. This
    // preserves the asset's pixel resolution for the Dock without
    // introducing extra resampling beyond the inner-square downscale
    // below.
    let canvas_side = sw.max(sh);

    // Apple grid: visible square is ~80.5% of the canvas, leaving
    // ~9.77% transparent margin per side. We round to integers so
    // the inner square stays pixel-aligned.
    let inner_side = ((canvas_side as f32) * 0.805).round() as u32;
    let margin = (canvas_side - inner_side) / 2;

    // Resize the source to fill the inner square. Lanczos3 is the
    // best built-in filter the `image` crate ships and is fine for a
    // one-shot startup-time conversion.
    let resized = image::imageops::resize(&src, inner_side, inner_side, FilterType::Lanczos3);

    // Composite onto a fully-transparent canvas at the centered offset.
    let mut canvas: RgbaImage = RgbaImage::new(canvas_side, canvas_side);
    image::imageops::overlay(&mut canvas, &resized, margin as i64, margin as i64);

    // Squircle mask, applied to the inner square only. Outside the
    // inner square the canvas is already fully transparent, so we
    // don't need to clear it explicitly.
    let radius = (inner_side as f32) * 0.225;
    let aa = 1.5_f32; // anti-alias band width in pixels
    let inner_left = margin as f32;
    let inner_top = margin as f32;
    let inner_right = (margin + inner_side) as f32;
    let inner_bottom = (margin + inner_side) as f32;

    for y in margin..(margin + inner_side) {
        for x in margin..(margin + inner_side) {
            // Distance from this pixel center to the nearest edge of
            // the rounded rectangle that occupies the inner square.
            // Negative = inside, positive = outside.
            let cx = x as f32 + 0.5;
            let cy = y as f32 + 0.5;
            // Clamp to the inset rectangle of the rounded shape (the
            // rectangle whose Minkowski sum with a disk of radius
            // `radius` produces the rounded square).
            let qx = cx.clamp(inner_left + radius, inner_right - radius);
            let qy = cy.clamp(inner_top + radius, inner_bottom - radius);
            let dx = cx - qx;
            let dy = cy - qy;
            let dist = (dx * dx + dy * dy).sqrt();
            let signed = dist - radius;
            let alpha_factor = if signed <= -aa {
                1.0
            } else if signed >= 0.0 {
                0.0
            } else {
                // Smoothstep from 1.0 at signed=-aa to 0.0 at signed=0.
                let t = (-signed) / aa;
                t * t * (3.0 - 2.0 * t)
            };
            let pixel = canvas.get_pixel_mut(x, y);
            let new_alpha = (pixel.0[3] as f32 * alpha_factor).round() as u8;
            pixel.0[3] = new_alpha;
        }
    }

    let mut png_buf: Vec<u8> = Vec::with_capacity((canvas_side * canvas_side * 4) as usize);
    image::codecs::png::PngEncoder::new(&mut png_buf)
        .write_image(&canvas, canvas_side, canvas_side, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("encode png: {e}"))?;
    Ok(png_buf)
}

#[cfg(feature = "gpui")]
pub fn run(app: &DesktopApp, config: AmuxConfig) {
    use amux_ui::GpuiRenderer;
    use smol::Timer;

    crate::metrics::startup_phase("bootstrap_run_entry");

    // Required for WebView2 to render correctly inside GPUI's DirectComposition window.
    // SAFETY: called once at startup before any threads are spawned.
    #[cfg(target_os = "windows")]
    unsafe { std::env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "true") };

    let mut app = app.clone();
    let model = app.render_with(&GpuiRenderer);

    application().run(move |cx: &mut App| {
        crate::metrics::startup_phase("gpui_app_run");
        // Initialize gpui-component (registers Input keybindings, theme, etc.)
        gpui_component::init(cx);
        // Set dark theme to match Amux's Tomorrow Night palette
        gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
        crate::metrics::startup_phase("gpui_component_init_done");

        // macOS Dock icon. NSApplication is now alive (gpui's application
        // bootstrap created it before invoking this closure), so we can
        // safely call setApplicationIconImage_. Without this, dev `cargo
        // run` shows the generic Rust binary icon in the Dock, which
        // immediately marks the app as "not a real product". A packaged
        // .app bundle with .icns + Info.plist is still TODO; this hook
        // gives us a real icon for unbundled dev builds and overrides any
        // bundle icon at runtime.
        #[cfg(target_os = "macos")]
        set_macos_dock_icon();
        crate::metrics::startup_phase("dock_icon_done");

        // macOS native menubar. Provides the standard app menu with
        // About / Hide / Quit and an Edit menu with clipboard actions.
        // macOS does NOT auto-inject these; we must add them explicitly
        // and wire them to real gpui Actions that call cx.quit() /
        // cx.hide().
        //
        // The binary is named "amux" (via [[bin]] in Cargo.toml) so the
        // menu bar reads "amux" rather than "amux-desktop".
        #[cfg(target_os = "macos")]
        {
            use gpui::{Menu, MenuItem, NoAction, OsAction, SystemMenuType};

            // Define actions for app menu items. These are dispatched
            // via gpui's global action system; the handlers are
            // registered right after cx.set_menus.
            gpui::actions!(amux, [QuitApp, HideApp, HideOthers, ShowAll, AboutAmux]);

            cx.set_menus(vec![
                Menu::new("Amux").items(vec![
                    MenuItem::action("About Amux", AboutAmux),
                    MenuItem::separator(),
                    MenuItem::os_submenu("Services", SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action("Hide Amux", HideApp),
                    MenuItem::action("Hide Others", HideOthers),
                    MenuItem::action("Show All", ShowAll),
                    MenuItem::separator(),
                    MenuItem::action("Quit Amux", QuitApp),
                ]),
                Menu::new("Edit").items(vec![
                    MenuItem::os_action("Undo", NoAction, OsAction::Undo),
                    MenuItem::os_action("Redo", NoAction, OsAction::Redo),
                    MenuItem::separator(),
                    MenuItem::os_action("Cut", NoAction, OsAction::Cut),
                    MenuItem::os_action("Copy", NoAction, OsAction::Copy),
                    MenuItem::os_action("Paste", NoAction, OsAction::Paste),
                    MenuItem::os_action("Select All", NoAction, OsAction::SelectAll),
                ]),
            ]);

            // Global action handlers — these fire regardless of which
            // view has focus, matching the macOS convention that app
            // menu items are always available.
            cx.on_action(|_: &QuitApp, cx| {
                cx.quit();
            });
            cx.on_action(|_: &HideApp, cx| {
                cx.hide();
            });
            cx.on_action(|_: &HideOthers, cx| {
                cx.hide_other_apps();
            });
            cx.on_action(|_: &ShowAll, cx| {
                cx.unhide_other_apps();
            });
            // About — just a no-op for now; a future iteration can
            // show a modal with version / build info.
            cx.on_action(|_: &AboutAmux, _cx| {});
        }
        crate::metrics::startup_phase("macos_menubar_done");

        let model = model.clone();
        let app = app.clone();
        let config = config.clone();

        // Titlebar styling.
        //
        // On macOS we render the content area edge-to-edge under the
        // titlebar (`appears_transparent: true`) and let the traffic
        // lights overlay sit on top, matching modern macOS apps like
        // Zed / VSCode / Warp. Without this, AMUX shows a chunky opaque
        // titlebar that visually fights with the dark workspace area
        // and immediately reads as "ported Windows app" instead of
        // "native Mac app".
        //
        // On Windows / Linux we keep the standard non-transparent
        // titlebar — those window managers don't have an equivalent
        // overlay convention and the system titlebar is the right
        // choice.
        let titlebar = gpui::TitlebarOptions {
            title: Some("Amux".into()),
            #[cfg(target_os = "macos")]
            appears_transparent: true,
            #[cfg(not(target_os = "macos"))]
            appears_transparent: false,
            // Nudge the traffic lights down slightly so they sit
            // visually centered against the wider top inset of our
            // sidebar / tab strip layout.
            #[cfg(target_os = "macos")]
            traffic_light_position: Some(gpui::Point {
                x: px(12.0),
                y: px(12.0),
            }),
            ..Default::default()
        };
        let window_opts = WindowOptions {
            titlebar: Some(titlebar),
            app_id: Some("amux".to_string()),
            window_min_size: Some(gpui::Size { width: px(480.0), height: px(320.0) }),
            ..Default::default()
        };
        crate::metrics::startup_phase("window_open_requested");
        let window_result = cx.open_window(window_opts, |window, cx| {
            let view = cx.new(|cx| {
                // Start a ~60fps polling timer to drain PTY output into the emulator
                cx.spawn(async move |this, cx| {
                    loop {
                        Timer::after(std::time::Duration::from_millis(16)).await;
                        let result = this.update(cx, |this: &mut GpuiShellView, cx: &mut Context<GpuiShellView>| {
                            let has_drag = this.resize_drag.is_some();
                            // Cursor blink: toggle every ~30 frames (500ms at 60fps)
                            this.cursor_blink_frame = this.cursor_blink_frame.wrapping_add(1);
                            let cursor_blink_toggle = this.cursor_blink_frame % 30 == 0;

                            // Check if any terminal has new output (dirty flag from PTY wakeup)
                            let mut any_dirty = false;
                            'outer: for tm in this.workspace_terminals.values() {
                                for term in tm.all_terminals() {
                                    if term.take_dirty() {
                                        any_dirty = true;
                                        break 'outer;
                                    }
                                }
                            }

                            // Collect which browser_ids should be visible: any browser tab
                            // that is the active tab in its pane (regardless of which pane
                            // has focus). This way the user can see the browser while working
                            // in a different terminal pane.
                            let mut visible_bids: Vec<u64> = Vec::new();
                            for tm in this.workspace_terminals.values() {
                                for pane in tm.all_panes() {
                                    if let Some(amux_platform::terminal::manager::TabKind::Browser { browser_id, .. }) = pane.active_tab_kind() {
                                        visible_bids.push(*browser_id);
                                    }
                                }
                            }

                            // Sync browser WebView2 bounds, visibility, and pending navigations.
                            for (&bid, entry) in this.browser_tabs.iter_mut() {
                                let should_show = visible_bids.contains(&bid);
                                if should_show {
                                    if let Some(bounds) = entry.bounds_cell.get() {
                                        entry.browser.sync_bounds(bounds);
                                    }
                                    if !entry.browser.is_visible() {
                                        entry.browser.show();
                                    }
                                } else if entry.browser.is_visible() {
                                    entry.browser.hide();
                                }
                                entry.browser.process_pending_nav();
                                if let Some(url) = entry.browser.take_current_url() {
                                    this.pending_url_bar_update = Some(url);
                                    cx.notify();
                                }
                            }

                            // Only re-render when needed: new output, cursor blink, or drag
                            if any_dirty || cursor_blink_toggle || has_drag || this.selecting {
                                cx.notify();
                            }
                            // Deferred startup: spawn PTY processes on first frame
                            // Only spawn the active workspace's terminals for fast startup.
                            // Other workspaces spawn on first switch (ensure_workspace_terminal).
                            if !this.terminals_spawned {
                                this.terminals_spawned = true;
                                let (shell, args) = GpuiShellView::default_shell();
                                let default_cwd = GpuiShellView::default_cwd();
                                let active_ws = this.active_workspace_id.clone();
                                if let Some(tm) = this.workspace_terminals.get_mut(&active_ws) {
                                    let pane_ids: Vec<_> = tm.active_layout()
                                        .map(|l| l.pane_ids()).unwrap_or_default();
                                    for pid in pane_ids {
                                        tm.spawn_all_tabs_in_pane(&pid, &shell, &args, default_cwd.as_deref());
                                    }
                                }
                                // Generate agent-prompt.md if it doesn't exist
                                GpuiShellView::ensure_agent_prompt_file();
                                cx.notify();
                            }
                            // Deferred tool detection: launch in background thread on third frame
                            if !this.tools_detected && this.cursor_blink_frame >= 3 {
                                this.tools_detected = true;
                                // Spawn detection in background so it doesn't block rendering
                                cx.spawn(async move |this, cx| {
                                    // Run blocking detection on a background thread
                                    let (tools, wsl) = smol::unblock(|| {
                                        let tools = GpuiShellView::detect_all_vibe_tools();
                                        let wsl = GpuiShellView::wsl_available();
                                        (tools, wsl)
                                    }).await;
                                    let _ = this.update(cx, |view: &mut GpuiShellView, _cx| {
                                        view.detected_vibe_tools = tools;
                                        view.wsl_detected = wsl;
                                    });
                                }).detach();
                            }
                            // Poll terminal activity — only for the active workspace.
                            // Background workspaces keep their dirty flag set by the
                            // PTY event proxy; the sidebar shows a green dot for those.
                            // Full poll (agent status detection) runs only on the
                            // workspace the user is actually looking at.
                            if this.cursor_blink_frame % 4 == 0 {
                                let frame = this.cursor_blink_frame;
                                let active_ws = this.active_workspace_id.clone();
                                if let Some(tm) = this.workspace_terminals.get_mut(&active_ws) {
                                    let notifs = tm.poll_activity();
                                    for n in notifs {
                                        // Auto-expand sidebar when agent needs attention
                                        if matches!(n.new_status, amux_platform::terminal::manager::AgentStatus::Waiting | amux_platform::terminal::manager::AgentStatus::Error) {
                                            this.sidebar_state.collapsed = false;
                                            this.sidebar_state.mode = crate::gpui_workspace_sidebar::SidebarMode::Agents;
                                        }
                                        let msg = format!("{} {} — {}",
                                            n.new_status.icon(),
                                            n.tab_title,
                                            n.new_status.label(),
                                        );
                                        this.toasts.push(crate::state::ToastNotification {
                                            message: msg,
                                            color: n.new_status.color_rgb(),
                                            frame_created: frame,
                                            pane_id: n.pane_id,
                                            tab_index: n.tab_index,
                                        });
                                    }
                                    // Clear activity for the active tab since user is looking at it
                                    this.terminal_manager_mut().clear_active_activity();
                                }
                                // Expire old toasts (after ~3 seconds = 180 frames at 60fps)
                                this.toasts.retain(|t| {
                                    frame.wrapping_sub(t.frame_created) < 180
                                });
                            }
                            // Auto-save layouts every ~5 seconds (300 frames at 60fps)
                            if this.cursor_blink_frame % 300 == 0 {
                                this.save_all_layouts();
                            }
                        });
                        if result.is_err() {
                            break;
                        }
                    }
                })
                .detach();

                GpuiShellView::new(app, model, config, cx)
            });
            // Wrap in gpui-component Root (required for Input component)
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        });

        match window_result {
            Ok(_) => {
                cx.activate(true);
            }
            Err(e) => {
                eprintln!("[amux] failed to open window: {:?}", e);
            }
        }
    });
}

#[cfg(not(feature = "gpui"))]
pub fn run(_: &amux_ui::DesktopApp, _config: crate::gpui_config::AmuxConfig) {}
