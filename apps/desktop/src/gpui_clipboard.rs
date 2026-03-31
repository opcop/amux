//! Clipboard operations — copy, paste, smart paste (image detection)
//!
//! Handles text copy/paste with bracketed paste support,
//! and screenshot paste with auto-save and WSL path conversion.

#[cfg(feature = "gpui")]
use gpui::Context;

#[cfg(feature = "gpui")]
use crate::gpui_entry::GpuiShellView;

#[cfg(feature = "gpui")]
impl GpuiShellView {
    /// Copy selected text to clipboard via alacritty's selection.
    pub(crate) fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            let text = term.with_term(|t| t.selection_to_string());
            if let Some(text) = text {
                if !text.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                }
            }
            // Clear selection after copy
            term.with_term_mut(|t| { t.selection = None; });
        }
    }

    /// Paste from clipboard into terminal
    pub(crate) fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let text = cx.read_from_clipboard()
            .and_then(|item| item.text().map(|s| s.to_string()));
        if let Some(text) = text {
            self.send_paste_text(&text);
        }
    }

    /// Smart paste: if clipboard has an image, save it and insert the file path
    /// formatted for the current AI tool. If clipboard has text, paste normally.
    pub(crate) fn smart_paste(&mut self, cx: &mut Context<Self>) {
        let item = match cx.read_from_clipboard() {
            Some(item) => item,
            None => return,
        };

        // Check for image first
        for entry in item.entries() {
            if let gpui::ClipboardEntry::Image(image) = entry {
                if !image.bytes.is_empty() {
                    if let Some(path) = self.save_clipboard_image(image) {
                        // Detect which AI tool is running and format accordingly
                        let formatted = self.format_image_path_for_tool(&path);
                        self.send_paste_text(&formatted);
                    }
                    return;
                }
            }
        }

        // Fallback to text paste
        if let Some(text) = item.text() {
            self.send_paste_text(&text);
        }
    }

    /// Format the image path for the current terminal context.
    /// On Windows: always provide both Windows and WSL paths, since the terminal
    /// might be running a WSL program (claude, opencode) via wsl.exe.
    fn format_image_path_for_tool(&self, path: &str) -> String {
        if cfg!(target_os = "windows") && path.len() >= 2 && path.as_bytes()[1] == b':' {
            // Windows path detected — convert to WSL format since most Vibe Coding
            // tools run inside WSL. WSL can also read Windows paths via /mnt/,
            // so the WSL path works everywhere.
            Self::windows_path_to_wsl(path)
        } else {
            path.to_string()
        }
    }

    /// Save a clipboard image to ~/.amux/screenshots/ and return the path string.
    fn save_clipboard_image(&self, image: &gpui::Image) -> Option<String> {
        let dir = Self::amux_dir().join("screenshots");
        std::fs::create_dir_all(&dir).ok()?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ext = match image.format {
            gpui::ImageFormat::Png => "png",
            gpui::ImageFormat::Jpeg => "jpg",
            gpui::ImageFormat::Gif => "gif",
            gpui::ImageFormat::Webp => "webp",
            gpui::ImageFormat::Bmp => "bmp",
            _ => "png",
        };
        let filename = format!("screenshot_{}.{}", timestamp, ext);
        let path = dir.join(&filename);
        std::fs::write(&path, &image.bytes).ok()?;
        Some(path.to_string_lossy().to_string())
    }

    /// Send text to active terminal with bracketed paste support.
    pub(crate) fn send_paste_text(&mut self, text: &str) {
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            let bracketed = term.with_term(|t| {
                t.mode().contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE)
            });
            if bracketed {
                term.send_input(b"\x1b[200~");
            }
            term.send_input(text.as_bytes());
            if bracketed {
                term.send_input(b"\x1b[201~");
            }
        }
    }
}
