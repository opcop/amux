//! Embedded browser pane using WebView2 (via wry).
//!
//! The WebView2 loads pages directly (no iframe). A GPUI-rendered URL bar
//! sits above it. Clicking the URL bar opens a URL input popup.

#[cfg(feature = "gpui")]
use std::rc::Rc;
#[cfg(feature = "gpui")]
use std::cell::{Cell, RefCell};
#[cfg(feature = "gpui")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "gpui")]
use gpui::*;

// ─── Browser Pane State ────────────────────────────────────────

#[cfg(feature = "gpui")]
pub struct BrowserPaneState {
    pub url: String,
    pub title: String,
    pub width: f32,
    webview: Option<Rc<wry::WebView>>,
    visible: bool,
    bounds: Bounds<Pixels>,
    /// URLs from target="_blank" / window.open that should be navigated in-place.
    pending_nav: Arc<Mutex<Option<String>>>,
    /// Current URL updated by navigation_handler (for syncing to URL bar).
    current_url: Arc<Mutex<Option<String>>>,
}

#[cfg(feature = "gpui")]
impl BrowserPaneState {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            title: String::new(),
            width: 680.0,
            webview: None,
            visible: true,
            bounds: Bounds::default(),
            pending_nav: Arc::new(Mutex::new(None)),
            current_url: Arc::new(Mutex::new(None)),
        }
    }

    pub fn init_webview(&mut self, raw_handle: raw_window_handle::RawWindowHandle) {
        let window_handle = unsafe {
            raw_window_handle::WindowHandle::borrow_raw(raw_handle)
        };

        let pending = self.pending_nav.clone();
        let nav_url = self.current_url.clone();
        let builder = wry::WebViewBuilder::new()
            .with_url(&self.url)
            .with_transparent(false)
            .with_devtools(true)
            // Allow all navigation; capture the URL for syncing to the address bar.
            .with_navigation_handler(move |url| {
                if let Ok(mut slot) = nav_url.lock() {
                    *slot = Some(url);
                }
                true
            })
            // Intercept target="_blank" / window.open — queue the URL for
            // in-place navigation instead of opening the system browser.
            .with_new_window_req_handler(move |url, _features| {
                if let Ok(mut slot) = pending.lock() {
                    *slot = Some(url);
                }
                wry::NewWindowResponse::Deny
            })
            // JS: convert _blank links to same-window navigation as a first line
            // of defense (fires before new_window_req_handler for most clicks).
            .with_initialization_script(
                r#"(function(){
                    document.addEventListener('click', function(e) {
                        var a = e.target.closest ? e.target.closest('a') : null;
                        if (a && a.href && (a.target === '_blank' || a.target === '_new')) {
                            e.preventDefault();
                            e.stopPropagation();
                            window.location.href = a.href;
                        }
                    }, true);
                })();"#
            );

        match builder.build_as_child(&window_handle) {
            Ok(webview) => {
                self.webview = Some(Rc::new(webview));
                self.visible = true;
                self.bounds = Bounds::default();
            }
            Err(e) => {
                eprintln!("[amux-browser] failed to create WebView2: {}", e);
            }
        }
    }

    /// Check for and process any pending navigation from new-window requests.
    /// Call this from the 60fps timer.
    pub fn process_pending_nav(&mut self) {
        let url = {
            if let Ok(mut slot) = self.pending_nav.lock() {
                slot.take()
            } else {
                None
            }
        };
        if let Some(url) = url {
            self.navigate(&url);
        }
    }

    /// Take the latest navigated URL (if any) for syncing to the URL bar.
    pub fn take_current_url(&mut self) -> Option<String> {
        if let Ok(mut slot) = self.current_url.lock() {
            let url = slot.take();
            if let Some(ref u) = url {
                self.url = u.clone();
            }
            url
        } else {
            None
        }
    }

    pub fn navigate(&mut self, url: &str) {
        self.url = url.to_string();
        if let Some(ref wv) = self.webview {
            let _ = wv.load_url(url);
        }
    }

    pub fn go_back(&self) {
        if let Some(ref wv) = self.webview {
            let _ = wv.evaluate_script("history.back();");
        }
    }

    pub fn go_forward(&self) {
        if let Some(ref wv) = self.webview {
            let _ = wv.evaluate_script("history.forward();");
        }
    }

    pub fn reload(&self) {
        if let Some(ref wv) = self.webview {
            let _ = wv.evaluate_script("location.reload();");
        }
    }

    pub fn show(&mut self) {
        if let Some(ref wv) = self.webview {
            let _ = wv.set_visible(true);
        }
        self.visible = true;
    }

    pub fn hide(&mut self) {
        if let Some(ref wv) = self.webview {
            let _ = wv.set_visible(false);
        }
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool { self.visible }

    pub fn toggle_visible(&mut self) {
        if self.visible { self.hide(); } else { self.show(); }
    }

    pub fn sync_bounds(&mut self, bounds: Bounds<Pixels>) {
        if self.bounds == bounds { return; }
        self.bounds = bounds;
        if let Some(ref wv) = self.webview {
            let _ = wv.set_bounds(wry::Rect {
                position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(
                    bounds.origin.x.into(), bounds.origin.y.into(),
                )),
                size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(
                    bounds.size.width.into(), bounds.size.height.into(),
                )),
            });
            if self.visible { let _ = wv.set_visible(true); }
        }
    }

    pub fn is_initialized(&self) -> bool { self.webview.is_some() }
    pub fn force_bounds_update(&mut self) { self.bounds = Bounds::default(); }

    pub fn focus_parent(&self) {
        if let Some(ref wv) = self.webview { let _ = wv.focus_parent(); }
    }

    pub fn open_devtools(&self) {
        #[cfg(debug_assertions)]
        if let Some(ref wv) = self.webview { wv.open_devtools(); }
    }
}

#[cfg(feature = "gpui")]
impl Drop for BrowserPaneState {
    fn drop(&mut self) { self.hide(); }
}

// ─── Browser Tab Entry (desktop-layer state per browser tab) ──

/// Per-browser-tab state held by GpuiShellView.
/// Keyed by `browser_id` in the `browser_tabs` HashMap.
#[cfg(feature = "gpui")]
pub struct BrowserTabEntry {
    pub browser: BrowserPaneState,
    pub url_input: gpui::Entity<gpui_component::input::InputState>,
    pub bounds_cell: Rc<Cell<Option<Bounds<Pixels>>>>,
}

// ─── Render ────────────────────────────────────────────────────

/// Render browser tab content (URL bar + WebView2 canvas) inside a pane.
#[cfg(feature = "gpui")]
pub fn render_browser_tab_content(
    url_input: gpui::Entity<gpui_component::input::InputState>,
    bounds_cell: Rc<Cell<Option<Bounds<Pixels>>>>,
    browser_id: u64,
    content_w: f32,
    content_h: f32,
    cx: &mut Context<crate::gpui_entry::GpuiShellView>,
) -> impl IntoElement {
    use crate::gpui_entry::GpuiShellView;
    use gpui_component::input::Input;

    let bid = browser_id;
    div()
        .flex_1()
        .flex()
        .flex_col()
        .w_full()
        .overflow_hidden()
        .bg(rgb(0x1d1f21))
        // URL bar
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .h(px(32.0))
                .px_2()
                .gap_1()
                .bg(rgb(0x282a2e))
                .border_b_1()
                .border_color(rgb(0x373b41))
                // Back
                .child(nav_btn("browser-back", "\u{25C0}", cx.listener(move |this: &mut GpuiShellView, _, _, cx| {
                    if let Some(e) = this.browser_tabs.get(&bid) { e.browser.go_back(); } cx.notify();
                })))
                // Forward
                .child(nav_btn("browser-fwd", "\u{25B6}", cx.listener(move |this: &mut GpuiShellView, _, _, cx| {
                    if let Some(e) = this.browser_tabs.get(&bid) { e.browser.go_forward(); } cx.notify();
                })))
                // Reload
                .child(nav_btn("browser-reload", "\u{21BB}", cx.listener(move |this: &mut GpuiShellView, _, _, cx| {
                    if let Some(e) = this.browser_tabs.get(&bid) { e.browser.reload(); } cx.notify();
                })))
                // URL bar
                .child(
                    div()
                        .flex_1()
                        .mx_1()
                        .bg(rgb(0x1d1f21))
                        .rounded(px(4.0))
                        .child(
                            Input::new(&url_input)
                                .cleanable(true)
                                .appearance(false)
                        )
                )
                // DevTools
                .child(nav_btn_styled("browser-devtools", "F12", 0xb5bd68,
                    cx.listener(move |this: &mut GpuiShellView, _, _, cx| {
                        if let Some(e) = this.browser_tabs.get(&bid) { e.browser.open_devtools(); } cx.notify();
                    })))
                // Close tab
                .child(nav_btn_styled("browser-close", "\u{2715}", 0xcc6666,
                    cx.listener(|this: &mut GpuiShellView, _, _, cx| {
                        this.close_browser(); cx.notify();
                    })))
        )
        // WebView content area — canvas with exact pixel size (like the terminal canvas).
        // URL bar takes 32px; remaining height goes to the WebView2 content.
        .child(
            canvas(
                move |bounds, _window, _cx| { bounds_cell.set(Some(bounds)); bounds },
                |_, _, _, _| {},
            ).w(px(content_w)).h(px((content_h - 32.0).max(0.0)))
        )
}

#[cfg(feature = "gpui")]
fn nav_btn(
    id: &'static str,
    label: &'static str,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .px(px(6.0)).py(px(2.0)).rounded(px(4.0))
        .text_sm().text_color(rgb(0x969896))
        .hover(|d| d.bg(rgb(0x373b41)).text_color(rgb(0xc5c8c6)))
        .cursor_pointer()
        .child(label)
        .on_click(handler)
}

#[cfg(feature = "gpui")]
fn nav_btn_styled(
    id: &'static str,
    label: &'static str,
    hover_color: u32,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .px(px(6.0)).py(px(2.0)).rounded(px(4.0))
        .text_sm().text_color(rgb(0x969896))
        .hover(|d| d.bg(rgb(0x373b41)).text_color(rgb(hover_color)))
        .cursor_pointer()
        .child(label)
        .on_click(handler)
}

