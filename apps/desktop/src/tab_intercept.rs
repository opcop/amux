//! Handle Tab key for terminal auto-completion on macOS.
//!
//! On macOS, AppKit's `NSWindow` intercepts the Tab key for keyboard
//! focus navigation (key view loop). GPUI's `performKeyEquivalent:`
//! handler processes the event but returns NO because propagation is
//! not stopped by the app-level key listener. AppKit then consumes
//! Tab for its key-view loop — the `\t` byte never reaches the PTY.
//!
//! This module stores a pointer (as `usize`) to the currently-active
//! `AlacrittyTerminal`, updated each frame from the main thread. On
//! macOS, an `NSEvent` local monitor intercepts Tab *before* AppKit's
//! key-view-loop sees it, forwarding `\t` (0x09) directly to the PTY.
//!
//! ## Safety
//!
//! All reads and writes to the pointer happen on the main Cocoa run
//! loop — there is no data race. The pointer is cleared before the
//! terminal is dropped.
//!
//! On non-macOS targets this module is a no-op.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Encoded pointer to the active terminal. Zero = none.
static ACTIVE_TERMINAL: AtomicUsize = AtomicUsize::new(0);

/// Update the active terminal pointer. Called from the 60fps tick.
pub fn set_active_terminal(ptr: usize) {
    ACTIVE_TERMINAL.store(ptr, Ordering::Relaxed);
}

/// Send `\t` (0x09) to the active terminal's PTY.
/// Returns true if a terminal was active and input was sent.
pub fn dispatch_tab() -> bool {
    let ptr = ACTIVE_TERMINAL.load(Ordering::Relaxed);
    if ptr != 0 {
        // SAFETY: ptr is a valid AlacrittyTerminal reference set by
        // set_active_terminal from the main thread. The terminal
        // outlives any NSEvent monitor callback, and both run on the
        // main Cocoa run loop.
        unsafe {
            (*(ptr as *const amux_platform::terminal::alacritty_view::AlacrittyTerminal))
                .send_input(b"\t");
        }
        true
    } else {
        false
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install() {}

#[cfg(target_os = "macos")]
pub fn install() {
    use std::ptr::NonNull;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};

    unsafe {
        let handler = move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let event_ref = event.as_ref();
            const KVK_TAB: u16 = 48;
            if event_ref.keyCode() != KVK_TAB {
                return event.as_ptr();
            }
            let flags = event_ref.modifierFlags();
            // Only intercept bare Tab / Shift+Tab.
            if flags.intersects(
                NSEventModifierFlags::Control
                    | NSEventModifierFlags::Command
                    | NSEventModifierFlags::Option,
            ) {
                return event.as_ptr();
            }
            if dispatch_tab() {
                std::ptr::null_mut() // consume event
            } else {
                event.as_ptr() // pass through
            }
        };

        let block = block2::StackBlock::new(handler).copy();

        let _monitor: Option<Retained<objc2::runtime::AnyObject>> =
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::KeyDown,
                &block,
            );

        // Leak the monitor so it lives for the entire app lifetime.
        if let Some(monitor) = _monitor {
            std::mem::forget(monitor);
        }
    }
}
