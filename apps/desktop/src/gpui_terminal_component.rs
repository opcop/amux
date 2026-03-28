//! GPUI Terminal Component
//! 
//! Interactive terminal component that renders terminal sessions.

#[cfg(feature = "gpui")]
use gpui::{
    rgb, px, div, AnyElement, Element, IntoElement, ParentElement,
    RenderOnce, SharedString, Styled,
};
#[cfg(feature = "gpui")]
use gpui::Context;

#[cfg(feature = "gpui")]
use amux_platform::terminal::{
    Cell, Color, Cursor, TerminalEmulator, TerminalSessionManager,
    keyboard_to_pty, TerminalLaunchProfile, ShellKind, WorkspaceTarget,
};

/// Character cell dimensions (in pixels)
const CELL_WIDTH: f32 = 9.0;
const CELL_HEIGHT: f32 = 17.0;

/// Terminal component state
#[cfg(feature = "gpui")]
pub struct GpuiTerminalComponent {
    /// Terminal session manager
    manager: TerminalSessionManager,
    /// Active session ID
    active_session: Option<amux_core::TerminalSessionId>,
    /// Scroll offset (lines)
    scroll_offset: usize,
    /// Terminal dimensions in cells
    cols: usize,
    rows: usize,
}

/// Color scheme for terminal rendering (Windows Terminal style)
#[cfg(feature = "gpui")]
#[derive(Clone)]
pub struct TerminalColorScheme {
    pub background: gpui::Rgba,
    pub foreground: gpui::Rgba,
    pub cursor: gpui::Rgba,
    pub cursor_bg: gpui::Rgba,
    pub selection_bg: gpui::Rgba,
    pub ansi_black: gpui::Rgba,
    pub ansi_red: gpui::Rgba,
    pub ansi_green: gpui::Rgba,
    pub ansi_yellow: gpui::Rgba,
    pub ansi_blue: gpui::Rgba,
    pub ansi_magenta: gpui::Rgba,
    pub ansi_cyan: gpui::Rgba,
    pub ansi_white: gpui::Rgba,
    pub ansi_bright_black: gpui::Rgba,
    pub ansi_bright_red: gpui::Rgba,
    pub ansi_bright_green: gpui::Rgba,
    pub ansi_bright_yellow: gpui::Rgba,
    pub ansi_bright_blue: gpui::Rgba,
    pub ansi_bright_magenta: gpui::Rgba,
    pub ansi_bright_cyan: gpui::Rgba,
    pub ansi_bright_white: gpui::Rgba,
}

#[cfg(feature = "gpui")]
impl Default for TerminalColorScheme {
    fn default() -> Self {
        Self {
            background: rgb(0x0c0c0c),        // Dark background
            foreground: rgb(0xcccccc),        // Light gray text
            cursor: rgb(0xffffff),            // White cursor
            cursor_bg: rgb(0xffffff),         // Cursor background
            selection_bg: rgb(0x264f78),       // Blue selection
            ansi_black: rgb(0x0c0c0c),
            ansi_red: rgb(0xc50f1f),
            ansi_green: rgb(0x13a10e),
            ansi_yellow: rgb(0xc19c00),
            ansi_blue: rgb(0x0037da),
            ansi_magenta: rgb(0x881798),
            ansi_cyan: rgb(0x3a96dd),
            ansi_white: rgb(0xcccccc),
            ansi_bright_black: rgb(0x767676),
            ansi_bright_red: rgb(0xe74856),
            ansi_bright_green: rgb(0x16c60c),
            ansi_bright_yellow: rgb(0xf9f1a5),
            ansi_bright_blue: rgb(0x3b78ff),
            ansi_bright_magenta: rgb(0xb4009e),
            ansi_bright_cyan: rgb(0x61d6d6),
            ansi_bright_white: rgb(0xf2f2f2),
        }
    }
}

#[cfg(feature = "gpui")]
impl TerminalColorScheme {
    /// Get RGB color for terminal color
    pub fn get_color(&self, color: &Color, is_bg: bool) -> gpui::Rgba {
        match color {
            Color::Default => {
                if is_bg { self.background } else { self.foreground }
            }
            Color::Black => self.ansi_black,
            Color::Red => self.ansi_red,
            Color::Green => self.ansi_green,
            Color::Yellow => self.ansi_yellow,
            Color::Blue => self.ansi_blue,
            Color::Magenta => self.ansi_magenta,
            Color::Cyan => self.ansi_cyan,
            Color::White => self.ansi_white,
            Color::BrightBlack => self.ansi_bright_black,
            Color::BrightRed => self.ansi_bright_red,
            Color::BrightGreen => self.ansi_bright_green,
            Color::BrightYellow => self.ansi_bright_yellow,
            Color::BrightBlue => self.ansi_bright_blue,
            Color::BrightMagenta => self.ansi_bright_magenta,
            Color::BrightCyan => self.ansi_bright_cyan,
            Color::BrightWhite => self.ansi_bright_white,
            Color::Indexed(i) => self.indexed_color(*i),
            Color::Rgb(r, g, b) => gpui::Rgba {
                r: *r as f32 / 255.0,
                g: *g as f32 / 255.0,
                b: *b as f32 / 255.0,
                a: 1.0,
            },
        }
    }

    /// Get color from 256-color palette
    fn indexed_color(&self, index: u8) -> gpui::Rgba {
        if index < 16 {
            // Standard colors
            match index {
                0 => self.ansi_black,
                1 => self.ansi_red,
                2 => self.ansi_green,
                3 => self.ansi_yellow,
                4 => self.ansi_blue,
                5 => self.ansi_magenta,
                6 => self.ansi_cyan,
                7 => self.ansi_white,
                8 => self.ansi_bright_black,
                9 => self.ansi_bright_red,
                10 => self.ansi_bright_green,
                11 => self.ansi_bright_yellow,
                12 => self.ansi_bright_blue,
                13 => self.ansi_bright_magenta,
                14 => self.ansi_bright_cyan,
                15 => self.ansi_bright_white,
                _ => self.foreground,
            }
        } else if index < 232 {
            // 216 color cube
            let i = index - 16;
            let r = ((i / 36) * 51) as u8;
            let g = (((i / 6) % 6) * 51) as u8;
            let b = ((i % 6) * 51) as u8;
            gpui::Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
        } else {
            // Grayscale
            let gray = ((index - 232) * 10 + 8) as u8;
            gpui::Rgba { r: gray as f32 / 255.0, g: gray as f32 / 255.0, b: gray as f32 / 255.0, a: 1.0 }
        }
    }
}

#[cfg(feature = "gpui")]
impl GpuiTerminalComponent {
    /// Create a new terminal component
    pub fn new() -> Self {
        Self {
            manager: TerminalSessionManager::new(),
            active_session: None,
            scroll_offset: 0,
            cols: 80,
            rows: 24,
        }
    }

    /// Create a new terminal with a shell
    pub fn with_shell(target: WorkspaceTarget, shell: ShellKind) -> Result<Self, String> {
        let mut this = Self::new();
        this.create_session(target, shell)?;
        Ok(this)
    }

    /// Create a new terminal session
    pub fn create_session(&mut self, target: WorkspaceTarget, shell: ShellKind) -> Result<(), String> {
        let profile = TerminalLaunchProfile {
            target,
            shell,
            cwd: None,
            env: std::collections::BTreeMap::new(),
            title: Some("Terminal".to_string()),
        };
        
        let session_id = self.manager.create_session(profile)?;
        self.active_session = Some(session_id);
        Ok(())
    }

    /// Write input to the active terminal
    pub fn write_input(&mut self, key: &str, ctrl: bool, shift: bool, alt: bool) -> Result<(), String> {
        if let Some(session_id) = &self.active_session {
            let data = keyboard_to_pty(key, ctrl, shift, alt);
            self.manager.write_input(session_id, &data)?;
            
            // Poll for output
            self.manager.poll_output(session_id)?;
        }
        Ok(())
    }

    /// Poll for terminal output
    pub fn poll(&mut self) -> Result<(), String> {
        if let Some(session_id) = &self.active_session {
            self.manager.poll_output(session_id)?;
        }
        Ok(())
    }

    /// Get the terminal dimensions
    pub fn dimensions(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    /// Set terminal dimensions
    pub fn set_dimensions(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        
        if let Some(session_id) = &self.active_session {
            let _ = self.manager.resize(session_id, cols as u16, rows as u16);
        }
    }

    /// Scroll the terminal view
    pub fn scroll_by(&mut self, delta: i32) {
        if delta < 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as usize);
        } else {
            self.scroll_offset += delta as usize;
        }
    }

    /// Get the pixel size for the terminal
    pub fn pixel_size(&self) -> (f32, f32) {
        (
            (self.cols as f32) * CELL_WIDTH,
            (self.rows as f32) * CELL_HEIGHT,
        )
    }

    /// Render the terminal content
    pub fn render_content(&self) -> impl IntoElement {
        let color_scheme = TerminalColorScheme::default();
        
        if let Some(session_id) = &self.active_session {
            if let Some(session) = self.manager.get(session_id) {
                return self.render_terminal_grid(session.emulator(), &color_scheme);
            }
        }
        
        // No session - show empty terminal
        div()
            .bg(color_scheme.background)
            .w(px((self.cols as f32) * CELL_WIDTH))
            .h(px((self.rows as f32) * CELL_HEIGHT))
    }

    /// Render the terminal grid
    fn render_terminal_grid(&self, emulator: &TerminalEmulator, color_scheme: &TerminalColorScheme) -> impl IntoElement {
        let (cols, rows) = emulator.dimensions();
        let grid = emulator.grid();
        let cursor = emulator.cursor();

        div()
            .bg(color_scheme.background)
            .relative()
            .w(px((cols as f32) * CELL_WIDTH))
            .h(px((rows as f32) * CELL_HEIGHT))
            .flex()
            .flex_col()
            .font_family("Cascadia Code, Consolas, Courier New, monospace".to_string())
            .text_size(px(CELL_HEIGHT))
            .line_height(px(CELL_HEIGHT))
            .children((0..rows).map(|y| {
                self.render_row(y, grid, cursor, color_scheme)
            }))
    }

    /// Render a single row of the terminal
    fn render_row(
        &self,
        row_idx: usize,
        grid: &[Vec<Cell>],
        cursor: &Cursor,
        color_scheme: &TerminalColorScheme,
    ) -> impl IntoElement {
        let row = grid.get(row_idx).unwrap_or(&vec![Cell::default(); self.cols]);
        let color_scheme = color_scheme.clone();
        
        // Collect text runs with consistent styling
        let mut runs: Vec<(usize, usize, gpui::Rgba, gpui::Rgba)> = Vec::new();
        let mut current_fg = color_scheme.foreground;
        let mut current_bg = color_scheme.background;
        let mut run_start = 0;
        
        for x in 0..self.cols.min(row.len()) {
            let cell = &row[x];
            let fg = color_scheme.get_color(&cell.fg, false);
            let bg = color_scheme.get_color(&cell.bg, true);
            
            if fg != current_fg || bg != current_bg {
                if run_start < x {
                    runs.push((run_start, x, current_fg, current_bg));
                }
                current_fg = fg;
                current_bg = bg;
                run_start = x;
            }
        }
        
        // Add final run
        if run_start < self.cols {
            runs.push((run_start, self.cols, current_fg, current_bg));
        }
        
        // Build the row
        div()
            .relative()
            .w(px((self.cols as f32) * CELL_WIDTH))
            .h(px(CELL_HEIGHT))
            .overflow_hidden()
            .children(runs.into_iter().map(|(start, end, fg, bg)| {
                let text: String = row[start..end.min(row.len())]
                    .iter()
                    .map(|c| c.ch)
                    .collect();
                
                div()
                    .absolute()
                    .left(px((start as f32) * CELL_WIDTH))
                    .top(px(0.0))
                    .w(px(((end - start) as f32) * CELL_WIDTH))
                    .h(px(CELL_HEIGHT))
                    .bg(bg)
                    .text_color(fg)
                    .child(text)
            }))
            // Cursor overlay
            .child(if cursor.visible && cursor.y == row_idx && cursor.x < self.cols {
                let cursor_text = row.get(cursor.x).map(|c| c.ch).unwrap_or(' ');
                Some(
                    div()
                        .absolute()
                        .left(px((cursor.x as f32) * CELL_WIDTH))
                        .top(px(0.0))
                        .w(px(CELL_WIDTH))
                        .h(px(CELL_HEIGHT))
                        .bg(color_scheme.cursor)
                        .text_color(color_scheme.cursor_bg)
                        .child(cursor_text.to_string())
                )
            } else {
                None
            })
    }
}

#[cfg(feature = "gpui")]
impl Default for GpuiTerminalComponent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "gpui")]
impl Drop for GpuiTerminalComponent {
    fn drop(&mut self) {
        // Clean up sessions
        for session_id in self.manager.session_ids() {
            let _ = self.manager.kill(&session_id);
        }
    }
}
