//! Cross-platform clipboard service backed by `arboard`.
//!
//! Single implementation shared by Windows / macOS / Linux platform adapters.
//! Each call lazily acquires a `Clipboard` handle so we don't hold a long-lived
//! native resource — the underlying X11/Wayland/AppKit/Win32 connections do
//! not always behave well when kept open across the entire process lifetime.
//!
//! Image payloads are passed through as raw RGBA8, matching what arboard
//! receives from the OS. Encoding decisions (PNG / JPEG / blit into a GPUI
//! image) are intentionally left to the desktop shell.

use arboard::Clipboard;

use crate::services::{ClipboardImage, ClipboardService};

#[derive(Clone, Debug, Default)]
pub struct ArboardClipboardService;

impl ArboardClipboardService {
    pub fn new() -> Self {
        Self
    }

    fn open() -> Result<Clipboard, String> {
        Clipboard::new().map_err(|err| format!("clipboard unavailable: {err}"))
    }
}

impl ClipboardService for ArboardClipboardService {
    fn read_text(&self) -> Result<Option<String>, String> {
        let mut clipboard = Self::open()?;
        match clipboard.get_text() {
            Ok(text) => Ok(Some(text)),
            // ContentNotAvailable is a normal "no text on clipboard" signal,
            // not an error worth surfacing to the UI.
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(err) => Err(format!("clipboard read failed: {err}")),
        }
    }

    fn write_text(&self, text: &str) -> Result<(), String> {
        let mut clipboard = Self::open()?;
        clipboard
            .set_text(text.to_owned())
            .map_err(|err| format!("clipboard write failed: {err}"))
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, String> {
        let mut clipboard = Self::open()?;
        let image = match clipboard.get_image() {
            Ok(img) => img,
            Err(arboard::Error::ContentNotAvailable) => return Ok(None),
            Err(err) => return Err(format!("clipboard image read failed: {err}")),
        };
        Ok(Some(ClipboardImage {
            width: image.width as u32,
            height: image.height as u32,
            rgba: image.bytes.into_owned(),
        }))
    }
}
