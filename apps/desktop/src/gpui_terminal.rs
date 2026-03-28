//! GPUI Terminal Renderer
//! 
//! Renders terminal emulator output using GPUI elements.

#[cfg(feature = "gpui")]
use gpui::{
    rgb, px, div, AnyElement, IntoElement, ParentElement,
    Styled,
};
#[cfg(feature = "gpui")]
use gpui::Rgba;
#[cfg(feature = "gpui")]
use amux_platform::terminal::emulator::{
    Cell, Color, Cursor, TerminalEmulator, DEFAULT_COLS,
};

/// Character cell dimensions (in pixels)
pub const CELL_WIDTH: f32 = 8.4;
pub const CELL_HEIGHT: f32 = 20.0;

/// Terminal view state for GPUI rendering
#[cfg(feature = "gpui")]
#[derive(Clone)]
pub struct GpuiTerminalView {
    emulator: TerminalEmulator,
    scroll_offset: usize,
    visible_rows: usize,
    visible_cols: usize,
}

/// Color scheme for terminal rendering
#[cfg(feature = "gpui")]
#[derive(Clone)]
pub struct TerminalColorScheme {
    pub background: gpui::Rgba,
    pub foreground: gpui::Rgba,
    pub cursor: gpui::Rgba,
    pub cursor_text: gpui::Rgba,
    pub selection_bg: gpui::Rgba,
    pub selection_fg: gpui::Rgba,
}

#[cfg(feature = "gpui")]
impl Default for TerminalColorScheme {
    fn default() -> Self {
        Self {
            background: rgb(0x1e1e2e),      // Catppuccin Mocha base
            foreground: rgb(0xcdd6f4),      // Catppuccin Mocha text
            cursor: rgb(0x89b4fa),           // Catppuccin blue
            cursor_text: rgb(0x1e1e2e),     // Catppuccin base
            selection_bg: rgb(0x45475a),   // Catppuccin surface1
            selection_fg: rgb(0xcdd6f4),    // Catppuccin text
        }
    }
}

#[cfg(feature = "gpui")]
impl TerminalColorScheme {
    /// Get the color for a terminal color
    pub fn get_color(&self, color: &Color, is_background: bool) -> gpui::Rgba {
        match color {
            Color::Default => {
                if is_background {
                    self.background
                } else {
                    self.foreground
                }
            }
            Color::Black => rgb(0x000000),
            Color::Red => rgb(0xcd3131),
            Color::Green => rgb(0x0d9c39),
            Color::Yellow => rgb(0xc5c329),
            Color::Blue => rgb(0x2472c8),
            Color::Magenta => rgb(0xb05cc5),
            Color::Cyan => rgb(0x11a8c7),
            Color::White => rgb(0xe5e5e5),
            Color::BrightBlack => rgb(0x666666),
            Color::BrightRed => rgb(0xf14c4c),
            Color::BrightGreen => rgb(0x23d18b),
            Color::BrightYellow => rgb(0xf5f543),
            Color::BrightBlue => rgb(0x3b8eea),
            Color::BrightMagenta => rgb(0xd670d6),
            Color::BrightCyan => rgb(0x29b8db),
            Color::BrightWhite => rgb(0xffffff),
            Color::Indexed(i) => indexed_to_rgb(*i),
            Color::Rgb(r, g, b) => Rgba { r: *r as f32 / 255.0, g: *g as f32 / 255.0, b: *b as f32 / 255.0, a: 1.0 },
        }
    }
}

/// Convert 256-color indexed color to RGB
#[cfg(feature = "gpui")]
fn indexed_to_rgb(index: u8) -> gpui::Rgba {
    if index < 16 {
        match index {
            0 => rgb(0x000000),
            1 => rgb(0xcd3131),
            2 => rgb(0x0d9c39),
            3 => rgb(0xc5c329),
            4 => rgb(0x2472c8),
            5 => rgb(0xb05cc5),
            6 => rgb(0x11a8c7),
            7 => rgb(0xe5e5e5),
            8 => rgb(0x666666),
            9 => rgb(0xf14c4c),
            10 => rgb(0x23d18b),
            11 => rgb(0xf5f543),
            12 => rgb(0x3b8eea),
            13 => rgb(0xd670d6),
            14 => rgb(0x29b8db),
            15 => rgb(0xffffff),
            _ => rgb(0xffffff),
        }
    } else if index < 232 {
        let i = index - 16;
        let r = ((i / 36) * 51) as u8;
        let g = (((i / 6) % 6) * 51) as u8;
        let b = ((i % 6) * 51) as u8;
        Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
    } else {
        let gray = ((index - 232) * 10 + 8) as u8;
        Rgba { r: gray as f32 / 255.0, g: gray as f32 / 255.0, b: gray as f32 / 255.0, a: 1.0 }
    }
}

#[cfg(feature = "gpui")]
impl GpuiTerminalView {
    /// Create a new terminal view with default dimensions
    pub fn new() -> Self {
        Self {
            emulator: TerminalEmulator::new(),
            scroll_offset: 0,
            visible_rows: 24,
            visible_cols: DEFAULT_COLS,
        }
    }

    /// Create a terminal view with specific dimensions
    pub fn with_size(cols: usize, rows: usize) -> Self {
        Self {
            emulator: TerminalEmulator::with_size(cols, rows),
            scroll_offset: 0,
            visible_rows: rows,
            visible_cols: cols,
        }
    }

    /// Feed data to the terminal emulator
    pub fn feed(&mut self, data: &[u8]) {
        self.emulator.feed(data);
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.emulator.resize(cols, rows);
        self.visible_cols = cols;
        self.visible_rows = rows;
    }

    /// Get the terminal dimensions
    pub fn dimensions(&self) -> (usize, usize) {
        self.emulator.dimensions()
    }

    /// Get cursor position
    pub fn cursor(&self) -> &Cursor {
        self.emulator.cursor()
    }

    /// Set scroll offset
    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    /// Get scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Scroll by delta lines
    pub fn scroll_by(&mut self, delta: i32) {
        if delta < 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as usize);
        } else {
            self.scroll_offset += delta as usize;
        }
    }

    /// Get the emulator reference
    pub fn emulator(&self) -> &TerminalEmulator {
        &self.emulator
    }

    /// Render the terminal as GPUI elements
    pub fn render_to_elements(&self) -> TerminalRenderElements {
        let (cols, rows) = self.emulator.dimensions();
        let grid = self.emulator.grid();
        let cursor = self.emulator.cursor();
        let color_scheme = TerminalColorScheme::default();

        TerminalRenderElements {
            grid: grid.to_vec(),
            cursor: cursor.clone(),
            cols,
            rows: rows.min(self.visible_rows),
            scrollback_len: 0,
            color_scheme,
        }
    }
}

#[cfg(feature = "gpui")]
impl Default for GpuiTerminalView {
    fn default() -> Self {
        Self::new()
    }
}

/// Rendered terminal elements ready for GPUI rendering
#[cfg(feature = "gpui")]
pub struct TerminalRenderElements {
    grid: Vec<Vec<Cell>>,
    cursor: Cursor,
    cols: usize,
    rows: usize,
    scrollback_len: usize,
    color_scheme: TerminalColorScheme,
}

#[cfg(feature = "gpui")]
impl TerminalRenderElements {
    /// Get the minimum size hint for this terminal
    pub fn min_size(&self) -> (f32, f32) {
        (
            (self.cols as f32) * CELL_WIDTH,
            (self.rows as f32) * CELL_HEIGHT,
        )
    }
}

/// Render the terminal as GPUI div elements.
///
/// Uses simple flex row layout — one div per row, styled text runs inline.
#[cfg(feature = "gpui")]
pub fn render_terminal(emulator: &TerminalEmulator, cursor: &Cursor, cursor_blink_on: bool) -> impl IntoElement {
    let (cols, rows) = emulator.dimensions();
    let is_scrolled = emulator.is_scrolled();
    let scroll_grid;
    let grid: &[Vec<Cell>];
    let display_rows;
    if is_scrolled {
        let visible = emulator.visible_grid();
        // Convert &[Cell] rows to Vec<Cell> for owned storage
        scroll_grid = visible.iter().map(|row| row.to_vec()).collect::<Vec<_>>();
        display_rows = scroll_grid.len().min(rows);
        grid = &scroll_grid;
    } else {
        grid = emulator.grid();
        display_rows = rows.min(grid.len());
    };
    // Only show cursor when not scrolled back and blink is on
    let show_cursor = cursor.visible && !is_scrolled && cursor_blink_on;
    let cs = TerminalColorScheme::default();
    let selection = emulator.selection();

    div()
        .bg(cs.background)
        .flex()
        .flex_col()
        .flex_1()
        .overflow_hidden()
        .p_1()
        .font_family("Cascadia Code, Consolas, DejaVu Sans Mono, monospace".to_string())
        .text_sm()
        .children((0..display_rows.min(rows)).map(|y| {
            let row = if y < grid.len() {
                &grid[y]
            } else {
                return div().h(px(CELL_HEIGHT)).into_any_element();
            };

            // Build styled runs for this row
            let mut spans: Vec<AnyElement> = Vec::new();
            let mut run_text = String::new();
            let mut run_fg = cs.get_color(&Color::Default, false);
            let mut run_bg = cs.get_color(&Color::Default, true);
            let col_limit = cols.min(row.len());

            for x in 0..col_limit {
                let cell = &row[x];

                // Skip continuation cells of wide characters
                if cell.wide_continuation {
                    continue;
                }

                let is_cursor = show_cursor && cursor.x == x && cursor.y == y;
                let is_selected = selection.contains(x, y);

                let cell_fg = if is_cursor {
                    cs.cursor_text
                } else if is_selected {
                    cs.selection_fg
                } else {
                    let mut c = cs.get_color(&cell.fg, false);
                    if cell.dim {
                        // Dim: reduce brightness by ~50%
                        c.r *= 0.5;
                        c.g *= 0.5;
                        c.b *= 0.5;
                    }
                    c
                };
                let cell_bg = if is_cursor {
                    cs.cursor
                } else if is_selected {
                    cs.selection_bg
                } else {
                    cs.get_color(&cell.bg, true)
                };

                // Style changed — flush current run
                if cell_fg != run_fg || cell_bg != run_bg {
                    if !run_text.is_empty() {
                        spans.push(
                            div()
                                .text_color(run_fg)
                                .bg(run_bg)
                                .child(run_text.clone())
                                .into_any_element(),
                        );
                        run_text.clear();
                    }
                    run_fg = cell_fg;
                    run_bg = cell_bg;
                }

                run_text.push(cell.ch);
            }

            // Flush last run
            if !run_text.is_empty() {
                spans.push(
                    div()
                        .text_color(run_fg)
                        .bg(run_bg)
                        .child(run_text)
                        .into_any_element(),
                );
            }

            div()
                .flex()
                .flex_row()
                .h(px(CELL_HEIGHT))
                .children(spans)
                .into_any_element()
        }))
}

/// Render terminal as a simple text representation
/// 
/// This is useful for debug views or fallback rendering.
#[cfg(feature = "gpui")]
pub fn render_terminal_text(emulator: &TerminalEmulator) -> Vec<String> {
    let grid = emulator.grid();
    let cursor = emulator.cursor();
    
    grid.iter()
        .enumerate()
        .map(|(y, row)| {
            let chars: String = row.iter()
                .map(|cell| cell.ch)
                .collect();
            
            // Add cursor indicator
            if cursor.visible && cursor.y == y {
                let x = cursor.x.min(row.len().saturating_sub(1));
                let mut chars: Vec<char> = chars.chars().collect();
                if !chars.is_empty() {
                    chars[x] = '█';
                }
                chars.into_iter().collect()
            } else {
                chars
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_feed() {
        let mut term = GpuiTerminalView::new();
        term.feed(b"Hello, World!");
        let (cols, rows) = term.dimensions();
        assert_eq!(cols, 80);
        assert_eq!(rows, 24);
    }

    #[test]
    fn test_terminal_text_render() {
        let mut term = GpuiTerminalView::new();
        term.feed(b"Test");
        let lines = render_terminal_text(term.emulator());
        assert_eq!(lines[0].trim(), "Test");
    }
}
