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
    /// Selection is preserved after copy (matching Terminal.app behavior)
    /// so the user can see what was copied.
    pub(crate) fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            let text = term.with_term(|t| t.selection_to_string());
            if let Some(text) = text {
                if !text.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                }
            }
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
                        return;
                    }
                    // Image save failed — fall through to text paste
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
    /// BMP images are converted to PNG since most AI tools don't accept BMP.
    fn save_clipboard_image(&self, image: &gpui::Image) -> Option<String> {
        let dir = Self::amux_dir().join("screenshots");
        std::fs::create_dir_all(&dir).ok()?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        // BMP → convert to PNG; all other formats save directly
        if matches!(image.format, gpui::ImageFormat::Bmp) {
            let filename = format!("screenshot_{}.png", timestamp);
            let path = dir.join(&filename);
            if let Some(png_bytes) = Self::bmp_to_png(&image.bytes) {
                std::fs::write(&path, &png_bytes).ok()?;
                return Some(path.to_string_lossy().to_string());
            }
            // Fallback: save as bmp if conversion fails
            let path = dir.join(format!("screenshot_{}.bmp", timestamp));
            std::fs::write(&path, &image.bytes).ok()?;
            return Some(path.to_string_lossy().to_string());
        }

        let ext = match image.format {
            gpui::ImageFormat::Png => "png",
            gpui::ImageFormat::Jpeg => "jpg",
            gpui::ImageFormat::Gif => "gif",
            gpui::ImageFormat::Webp => "webp",
            _ => "png",
        };
        let filename = format!("screenshot_{}.{}", timestamp, ext);
        let path = dir.join(&filename);
        std::fs::write(&path, &image.bytes).ok()?;
        Some(path.to_string_lossy().to_string())
    }

    /// Convert BMP bytes to PNG bytes using the image crate.
    fn bmp_to_png(bmp_bytes: &[u8]) -> Option<Vec<u8>> {
        let img = image::load_from_memory_with_format(bmp_bytes, image::ImageFormat::Bmp).ok()?;
        let mut png_buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png_buf, image::ImageFormat::Png).ok()?;
        Some(png_buf.into_inner())
    }

    /// Initiate "Send to Pane": grab the current selection and either send
    /// directly (if only one other pane) or open the pane picker.
    pub(crate) fn start_send_to_pane(&mut self, _cx: &mut Context<Self>) {
        // 1. Get selected text from active terminal
        let text = self.terminal_manager_mut().active_terminal()
            .and_then(|term| term.with_term(|t| t.selection_to_string()))
            .unwrap_or_default();
        if text.is_empty() { return; }

        // 2. Get list of other panes
        let active_pid = self.terminal_manager().active_pane_id().cloned();
        let targets = match active_pid {
            Some(ref pid) => self.terminal_manager().other_pane_summaries(pid),
            None => return,
        };
        if targets.is_empty() { return; }

        // 3. Only one other pane — send directly, skip picker
        if targets.len() == 1 {
            let target_id = targets[0].0.clone();
            self.terminal_manager_mut().send_text_to_pane(&target_id, &text);
            if let Some(term) = self.terminal_manager_mut().active_terminal() {
                term.with_term_mut(|t| { t.selection = None; });
            }
            return;
        }

        // 4. Multiple panes — open picker
        self.pane_picker = Some(crate::gpui_entry::PanePickerState {
            text,
            targets,
            selected_index: 0,
        });
    }

    /// Execute the pane picker selection — send text to chosen pane and close picker.
    pub(crate) fn execute_pane_picker(&mut self) {
        if let Some(picker) = self.pane_picker.take() {
            if let Some((target_id, _)) = picker.targets.get(picker.selected_index) {
                self.terminal_manager_mut().send_text_to_pane(target_id, &picker.text);
                if let Some(term) = self.terminal_manager_mut().active_terminal() {
                    term.with_term_mut(|t| { t.selection = None; });
                }
            }
        }
    }

    /// Send text to active terminal with bracketed paste support.
    pub(crate) fn send_paste_text(&mut self, text: &str) {
        if let Some(term) = self.terminal_manager_mut().active_terminal() {
            term.scroll_to_bottom();
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
