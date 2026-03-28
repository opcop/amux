//! Terminal emulator - ANSI parsing and terminal grid management
//!
//! This module provides a simple terminal emulator that parses ANSI escape
//! sequences and maintains a cell grid for rendering.

use std::collections::VecDeque;

/// Number of columns in the terminal grid
pub const DEFAULT_COLS: usize = 80;
/// Number of rows in the terminal scrollback buffer
pub const SCROLLBACK_LINES: usize = 10000;

/// A single character cell in the terminal
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub strikethrough: bool,
    /// True if this cell is the right-half placeholder of a wide (CJK) character.
    /// The actual character is in the cell to the left.
    pub wide_continuation: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            inverse: false,
            strikethrough: false,
            wide_continuation: false,
        }
    }
}

impl Cell {
    pub fn new(ch: char) -> Self {
        Self {
            ch,
            ..Default::default()
        }
    }
}

/// Terminal colors (ANSI 256-color palette index, or RGB)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Color {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    /// Convert to RGB tuple
    pub fn to_rgb(&self) -> (u8, u8, u8) {
        match self {
            Color::Default => (255, 255, 255),
            Color::Black => (0, 0, 0),
            Color::Red => (205, 0, 0),
            Color::Green => (0, 205, 0),
            Color::Yellow => (205, 205, 0),
            Color::Blue => (0, 0, 238),
            Color::Magenta => (205, 0, 205),
            Color::Cyan => (0, 205, 205),
            Color::White => (229, 229, 229),
            Color::BrightBlack => (127, 127, 127),
            Color::BrightRed => (255, 0, 0),
            Color::BrightGreen => (0, 255, 0),
            Color::BrightYellow => (255, 255, 0),
            Color::BrightBlue => (0, 0, 255),
            Color::BrightMagenta => (255, 0, 255),
            Color::BrightCyan => (0, 255, 255),
            Color::BrightWhite => (255, 255, 255),
            Color::Indexed(i) => indexed_to_rgb(*i),
            Color::Rgb(r, g, b) => (*r, *g, *b),
        }
    }
}

/// Convert 256-color indexed color to RGB
fn indexed_to_rgb(index: u8) -> (u8, u8, u8) {
    if index < 16 {
        // Standard colors (0-15)
        match index {
            0 => (0, 0, 0),        // black
            1 => (205, 0, 0),      // red
            2 => (0, 205, 0),      // green
            3 => (205, 205, 0),    // yellow
            4 => (0, 0, 238),      // blue
            5 => (205, 0, 205),    // magenta
            6 => (0, 205, 205),    // cyan
            7 => (229, 229, 229),  // white
            8 => (127, 127, 127),  // bright black
            9 => (255, 0, 0),      // bright red
            10 => (0, 255, 0),     // bright green
            11 => (255, 255, 0),   // bright yellow
            12 => (0, 0, 255),     // bright blue
            13 => (255, 0, 255),   // bright magenta
            14 => (0, 255, 255),   // bright cyan
            15 => (255, 255, 255), // bright white
            _ => (255, 255, 255),
        }
    } else if index < 232 {
        // 216 color cube (16-231)
        let i = index - 16;
        let r = (i / 36) * 51;
        let g = ((i / 6) % 6) * 51;
        let b = (i % 6) * 51;
        (r, g, b)
    } else {
        // Grayscale (232-255)
        let gray = (index - 232) * 10 + 8;
        (gray, gray, gray)
    }
}

/// Terminal cursor position
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub x: usize,
    pub y: usize,
    pub visible: bool,
    pub blinking: bool,
}

/// Text selection range in the terminal
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub start: Option<(usize, usize)>,
    pub end: Option<(usize, usize)>,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.start.is_none() || self.end.is_none()
    }

    pub fn clear(&mut self) {
        self.start = None;
        self.end = None;
    }

    pub fn set(&mut self, start: (usize, usize), end: (usize, usize)) {
        self.start = Some(start);
        self.end = Some(end);
    }

    /// Check if a cell at (x, y) is selected
    pub fn contains(&self, x: usize, y: usize) -> bool {
        let (start, end) = match (&self.start, &self.end) {
            (Some(s), Some(e)) => (s, e),
            _ => return false,
        };

        let (start_x, start_y) = if start.1 < end.1 || (start.1 == end.1 && start.0 <= end.0) {
            (start.0, start.1)
        } else {
            (end.0, end.1)
        };
        let (_, end_y) = if start.1 < end.1 || (start.1 == end.1 && start.0 <= end.0) {
            (end.0, end.1)
        } else {
            (start.0, start.1)
        };

        if y < start_y || y > end_y {
            return false;
        }

        if y == start_y && y == end_y {
            x >= start_x.min(end.0) && x <= start_x.max(end.0)
        } else if y == start_y {
            x >= start.0
        } else if y == end_y {
            x <= end.0
        } else {
            true
        }
    }

    /// Get selected text from the grid
    pub fn get_selected_text(&self, grid: &[Vec<Cell>]) -> String {
        if self.is_empty() {
            return String::new();
        }

        let (start, end) = match (&self.start, &self.end) {
            (Some(s), Some(e)) => (s, e),
            _ => return String::new(),
        };

        let min_y = start.1.min(end.1);
        let max_y = start.1.max(end.1);
        let mut text = String::new();

        for y in min_y..=max_y.min(grid.len().saturating_sub(1)) {
            let row = &grid[y];
            let min_x = if y == min_y { start.0.min(end.0) } else { 0 };
            let max_x = if y == max_y {
                start.0.max(end.0)
            } else {
                row.len().saturating_sub(1)
            };

            for x in min_x..=max_x.min(row.len().saturating_sub(1)) {
                text.push(row[x].ch);
            }
            if y < max_y {
                text.push('\n');
            }
        }

        text.trim_end().to_string()
    }
}

/// ANSI parser state
#[derive(Clone, Debug)]
pub struct ParserState {
    /// Current cursor position
    cursor: Cursor,
    /// Current foreground color
    fg: Color,
    /// Current background color
    bg: Color,
    /// Bold attribute
    bold: bool,
    /// Dim/faint attribute
    dim: bool,
    /// Italic attribute
    italic: bool,
    /// Underline attribute
    underline: bool,
    /// Inverse colors
    inverse: bool,
    /// Strikethrough
    strikethrough: bool,
    /// Saved cursor position
    saved_cursor: Option<Cursor>,
    /// Tab stops
    tab_stops: Vec<usize>,
    /// In escape sequence
    in_escape: bool,
    /// In OSC (Operating System Command) sequence — swallow until BEL or ST
    in_osc: bool,
    /// Current escape parameters
    escape_params: Vec<u16>,
    /// Current escape type
    escape_type: Option<char>,
    /// UTF-8 accumulation buffer (up to 4 bytes)
    utf8_buf: [u8; 4],
    /// How many bytes collected so far for current UTF-8 character
    utf8_len: u8,
    /// How many bytes expected for current UTF-8 character
    utf8_expected: u8,
    /// OSC sequence content buffer
    osc_buf: Vec<u8>,
}

impl Default for ParserState {
    fn default() -> Self {
        let mut tab_stops = Vec::new();
        for i in (0..512).step_by(8) {
            tab_stops.push(i);
        }
        Self {
            cursor: Cursor::default(),
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            inverse: false,
            strikethrough: false,
            saved_cursor: None,
            tab_stops,
            in_escape: false,
            in_osc: false,
            escape_params: Vec::new(),
            escape_type: None,
            utf8_buf: [0; 4],
            utf8_len: 0,
            utf8_expected: 0,
            osc_buf: Vec::new(),
        }
    }
}

/// Terminal emulator that parses ANSI and manages the cell grid
#[derive(Clone)]
pub struct TerminalEmulator {
    /// Terminal dimensions
    cols: usize,
    rows: usize,
    /// The visible grid (current screen)
    grid: Vec<Vec<Cell>>,
    /// Scrollback buffer (history lines above visible area)
    scrollback: VecDeque<Vec<Cell>>,
    /// Parser state
    state: ParserState,
    /// Scroll region (for partial scrolling)
    scroll_top: usize,
    scroll_bottom: usize,
    /// Current scroll offset (0 = at bottom, positive = scrolled up)
    scroll_offset: usize,
    /// Text selection
    selection: Selection,
    /// Terminal title (set via OSC 0/2)
    title: Option<String>,
}

impl TerminalEmulator {
    /// Create a new terminal emulator with default dimensions
    pub fn new() -> Self {
        Self::with_size(DEFAULT_COLS, 24)
    }

    /// Create a new terminal emulator with specific dimensions
    pub fn with_size(cols: usize, rows: usize) -> Self {
        let grid = vec![vec![Cell::default(); cols]; rows];
        Self {
            cols,
            rows,
            grid,
            scrollback: VecDeque::with_capacity(SCROLLBACK_LINES),
            state: ParserState::default(),
            scroll_top: 0,
            scroll_bottom: rows - 1,
            scroll_offset: 0,
            selection: Selection::new(),
            title: None,
        }
    }

    /// Get text selection
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Get the terminal title (set by OSC 0/2)
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Get mutable selection
    pub fn selection_mut(&mut self) -> &mut Selection {
        &mut self.selection
    }

    /// Set selection start (for mouse drag)
    pub fn set_selection_start(&mut self, x: usize, y: usize) {
        self.selection.set((x, y), (x, y));
    }

    /// Update selection end (during mouse drag)
    pub fn set_selection_end(&mut self, x: usize, y: usize) {
        if let Some(start) = &self.selection.start {
            self.selection.set(start.clone(), (x, y));
        }
    }

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// Check if there's an active selection
    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    /// Get selected text
    pub fn get_selection_text(&self) -> String {
        self.selection.get_selected_text(&self.grid)
    }

    /// Get selected text including scrollback
    pub fn get_selection_text_with_scrollback(&self) -> String {
        let mut text = String::new();

        // Add scrollback if selected lines are in scrollback
        if let (Some((_, start_y)), Some((_, end_y))) = (&self.selection.start, &self.selection.end)
        {
            let visible_start = self.rows.saturating_sub(self.scrollback.len());
            if *start_y < visible_start || *end_y < visible_start {
                // Selection includes scrollback - would need more complex handling
            }
        }

        text + &self.selection.get_selected_text(&self.grid)
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: usize, rows: usize) {
        if cols == self.cols && rows == self.rows {
            return;
        }

        let mut new_grid = vec![vec![Cell::default(); cols]; rows];

        // Keep content anchored around cursor position:
        // Copy rows so that the cursor row stays in the same position (or gets clamped)
        let cursor_y = self.state.cursor.y;

        // Determine which source rows to show in the new grid
        // Strategy: keep the cursor at the same row if possible
        let copy_rows = rows.min(self.rows);
        let src_start = if cursor_y >= rows {
            // Cursor would be off screen — scroll to keep it visible at bottom
            cursor_y + 1 - rows
        } else {
            0
        };

        for y in 0..copy_rows {
            let src_y = src_start + y;
            if src_y < self.grid.len() {
                let src_row = &self.grid[src_y];
                for x in 0..cols.min(src_row.len()) {
                    new_grid[y][x] = src_row[x].clone();
                }
            }
        }

        // If shrinking and we skipped top rows, push them to scrollback
        for i in 0..src_start {
            if i < self.grid.len() {
                let mut row = self.grid[i].clone();
                row.resize(cols, Cell::default());
                if self.scrollback.len() >= SCROLLBACK_LINES {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(row);
            }
        }

        self.cols = cols;
        self.rows = rows;
        self.grid = new_grid;
        self.scroll_bottom = rows.saturating_sub(1);

        // Clamp cursor
        self.state.cursor.y = cursor_y.saturating_sub(src_start).min(rows.saturating_sub(1));
        self.state.cursor.x = self.state.cursor.x.min(cols.saturating_sub(1));
        self.state.cursor.y = self.state.cursor.y.min(rows.saturating_sub(1));
    }

    /// Get terminal dimensions
    pub fn dimensions(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    /// Get the visible grid
    pub fn grid(&self) -> &[Vec<Cell>] {
        &self.grid
    }

    /// Get cursor position
    pub fn cursor(&self) -> &Cursor {
        &self.state.cursor
    }

    /// Get scroll offset (0 = at bottom, positive = scrolled up)
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Set scroll offset
    pub fn set_scroll_offset(&mut self, offset: usize) {
        let max_offset = self.scrollback.len();
        self.scroll_offset = offset.min(max_offset);
    }

    /// Clear the visible screen (like Ctrl+L or clear command)
    pub fn clear_screen(&mut self) {
        for row in &mut self.grid {
            for cell in row {
                *cell = Cell::default();
            }
        }
        self.state.cursor.x = 0;
        self.state.cursor.y = 0;
    }

    /// Clear the scrollback buffer (like Ctrl+K)
    pub fn clear_scrollback(&mut self) {
        self.scrollback.clear();
        self.scroll_offset = 0;
    }

    /// Clear both screen and scrollback
    pub fn clear_all(&mut self) {
        self.clear_screen();
        self.clear_scrollback();
    }

    /// Scroll up by n lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = (self.scroll_offset + lines).min(self.scrollback.len());
    }

    /// Scroll down by n lines
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Scroll to the bottom (reset scroll offset)
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Check if currently scrolled up (showing history)
    pub fn is_scrolled(&self) -> bool {
        self.scroll_offset > 0
    }

    /// Get total scrollback lines
    pub fn scrollback_lines(&self) -> usize {
        self.scrollback.len()
    }

    /// Get the grid including scrollback for rendering
    /// Returns rows from scrollback + visible grid
    pub fn visible_grid(&self) -> Vec<&[Cell]> {
        let mut result = Vec::with_capacity(self.rows + self.scroll_offset);

        // Add scrollback lines (from the top)
        let scrollback_start = self.scrollback.len().saturating_sub(self.scroll_offset);
        for i in scrollback_start..self.scrollback.len() {
            result.push(&self.scrollback[i][..]);
        }

        // Add visible grid lines
        for row in &self.grid {
            result.push(&row[..]);
        }

        result
    }

    /// Parse and process input data
    pub fn feed(&mut self, data: &[u8]) {
        for &byte in data {
            self.feed_byte(byte);
        }
    }

    /// Feed a single byte to the parser
    fn feed_byte(&mut self, byte: u8) {
        // UTF-8 multi-byte accumulation in progress
        if self.state.utf8_expected > 0 {
            if byte & 0xC0 == 0x80 {
                // Valid continuation byte
                self.state.utf8_buf[self.state.utf8_len as usize] = byte;
                self.state.utf8_len += 1;
                if self.state.utf8_len == self.state.utf8_expected {
                    // Complete UTF-8 sequence — decode and write
                    let buf = &self.state.utf8_buf[..self.state.utf8_len as usize];
                    if let Ok(s) = std::str::from_utf8(buf) {
                        if let Some(ch) = s.chars().next() {
                            self.write_char(ch);
                        }
                    }
                    self.state.utf8_len = 0;
                    self.state.utf8_expected = 0;
                }
            } else {
                // Invalid continuation — discard buffer, reprocess this byte
                self.state.utf8_len = 0;
                self.state.utf8_expected = 0;
                self.feed_byte(byte);
            }
            return;
        }

        // OSC sequences: collect content until BEL (0x07) or ST (ESC \)
        if self.state.in_osc {
            match byte {
                0x07 => {
                    self.handle_osc();
                    self.state.in_osc = false;
                    self.state.osc_buf.clear();
                }
                0x1B => {
                    self.handle_osc();
                    self.state.in_osc = false;
                    self.state.osc_buf.clear();
                    self.state.in_escape = true;
                    self.state.escape_params.clear();
                    self.state.escape_type = None;
                }
                _ => {
                    if self.state.osc_buf.len() < 4096 {
                        self.state.osc_buf.push(byte);
                    }
                }
            }
            return;
        }

        if self.state.in_escape {
            self.parse_escape(byte);
        } else {
            match byte {
                0x07 => {
                    // Bell
                }
                0x08 => {
                    // Backspace
                    if self.state.cursor.x > 0 {
                        self.state.cursor.x -= 1;
                    }
                }
                0x09 => {
                    // Tab
                    self.handle_tab();
                }
                0x0A | 0x0B | 0x0C => {
                    // Line feed, vertical tab, form feed
                    self.linefeed();
                }
                0x0D => {
                    // Carriage return
                    self.state.cursor.x = 0;
                }
                0x1B => {
                    // Escape character
                    self.state.in_escape = true;
                    self.state.escape_params.clear();
                    self.state.escape_type = None;
                }
                0x7F => {
                    // Delete - ignored
                }
                c if c >= 0x20 && c < 0x7F => {
                    // Printable ASCII
                    self.write_char(c as char);
                }
                c if c >= 0xC0 && c < 0xE0 => {
                    // UTF-8 2-byte start
                    self.state.utf8_buf[0] = c;
                    self.state.utf8_len = 1;
                    self.state.utf8_expected = 2;
                }
                c if c >= 0xE0 && c < 0xF0 => {
                    // UTF-8 3-byte start (CJK characters fall here)
                    self.state.utf8_buf[0] = c;
                    self.state.utf8_len = 1;
                    self.state.utf8_expected = 3;
                }
                c if c >= 0xF0 && c < 0xF8 => {
                    // UTF-8 4-byte start (emoji etc.)
                    self.state.utf8_buf[0] = c;
                    self.state.utf8_len = 1;
                    self.state.utf8_expected = 4;
                }
                _ => {
                    // C1 control codes (0x80-0x9F) or invalid — ignore
                }
            }
        }
    }

    /// Write a character at the current cursor position
    fn write_char(&mut self, ch: char) {
        let wide = is_wide_char(ch);
        let width = if wide { 2 } else { 1 };

        // Need enough room: if wide char at last column, wrap first
        if self.state.cursor.x + width > self.cols {
            self.state.cursor.x = 0;
            self.linefeed();
        }

        let y = self.state.cursor.y;
        let x = self.state.cursor.x;

        let cell = Cell {
            ch,
            fg: if self.state.inverse {
                self.state.bg.clone()
            } else {
                self.state.fg.clone()
            },
            bg: if self.state.inverse {
                self.state.fg.clone()
            } else {
                self.state.bg.clone()
            },
            bold: self.state.bold,
            dim: self.state.dim,
            italic: self.state.italic,
            underline: self.state.underline,
            inverse: self.state.inverse,
            strikethrough: self.state.strikethrough,
            wide_continuation: false,
        };

        self.grid[y][x] = cell;

        // For wide characters, place a continuation marker in the next cell
        if wide && x + 1 < self.cols {
            self.grid[y][x + 1] = Cell {
                ch: ' ',
                wide_continuation: true,
                ..self.grid[y][x].clone()
            };
        }

        self.state.cursor.x += width;
    }

    /// Handle tab character
    fn handle_tab(&mut self) {
        let mut next_tab = None;
        for &tab in &self.state.tab_stops {
            if tab > self.state.cursor.x {
                next_tab = Some(tab);
                break;
            }
        }
        self.state.cursor.x = next_tab.unwrap_or(self.cols - 1);
    }

    /// Line feed (move cursor down, scroll if needed)
    fn linefeed(&mut self) {
        if self.state.cursor.y >= self.scroll_bottom {
            self.scroll();
        } else {
            self.state.cursor.y += 1;
        }
    }

    /// Scroll the screen up by one line
    /// Handle a completed OSC sequence
    fn handle_osc(&mut self) {
        let content = String::from_utf8_lossy(&self.state.osc_buf);
        // OSC format: "Ps;Pt" where Ps is the command number
        if let Some((cmd, payload)) = content.split_once(';') {
            match cmd {
                "0" | "2" => {
                    // OSC 0: set icon name + title, OSC 2: set title
                    self.title = Some(payload.to_string());
                }
                _ => {} // Ignore other OSC commands
            }
        }
    }

    fn scroll(&mut self) {
        // Save the line being scrolled off to scrollback
        if self.scrollback.len() >= SCROLLBACK_LINES {
            self.scrollback.pop_front();
        }
        self.scrollback
            .push_back(self.grid[self.scroll_top].clone());

        // Shift rows up: row[top] = row[top+1], row[top+1] = row[top+2], ...
        for y in self.scroll_top..self.scroll_bottom {
            // Swap rows to avoid leaving empty vecs
            self.grid.swap(y, y + 1);
        }

        // Clear the bottom row (which now holds the old top row's data)
        let cols = self.cols;
        self.grid[self.scroll_bottom] = vec![Cell::default(); cols];
    }

    /// Parse an escape sequence
    fn parse_escape(&mut self, byte: u8) {
        // If we haven't determined the escape type yet (first byte after ESC)
        if self.state.escape_type.is_none() {
            match byte {
                0x5B => {
                    // CSI - Control Sequence Introducer [
                    self.state.escape_type = Some('[');
                    return;
                }
                0x5D => {
                    // OSC - Operating System Command — swallow until BEL or ST
                    self.state.in_osc = true;
                    self.state.in_escape = false;
                    return;
                }
                0x50 => {
                    // DCS - Device Control String — swallow like OSC
                    self.state.in_osc = true;
                    self.state.in_escape = false;
                    return;
                }
                0x5E | 0x5F => {
                    // PM (Privacy Message) / APC (Application Program Command) — swallow
                    self.state.in_osc = true;
                    self.state.in_escape = false;
                    return;
                }
                0x28 | 0x29 | 0x2A | 0x2B => {
                    // G0/G1/G2/G3 charset designation — next byte is the charset ID, skip it
                    self.state.escape_type = Some('(');
                    return;
                }
                0x5C => {
                    // ST (String Terminator) — just end escape
                    self.state.in_escape = false;
                    return;
                }
                0x63 => {
                    // RIS - Reset to Initial State
                    self.reset();
                    self.state.in_escape = false;
                    return;
                }
                0x37 => {
                    // DECSC - Save cursor
                    self.state.saved_cursor = Some(self.state.cursor.clone());
                    self.state.in_escape = false;
                    return;
                }
                0x38 => {
                    // DECRC - Restore cursor
                    if let Some(cursor) = self.state.saved_cursor.take() {
                        self.state.cursor = cursor;
                    }
                    self.state.in_escape = false;
                    return;
                }
                0x44 => {
                    // IND - Index (move cursor down, scroll if at bottom)
                    self.linefeed();
                    self.state.in_escape = false;
                    return;
                }
                0x45 => {
                    // NEL - Next Line
                    self.state.cursor.x = 0;
                    self.linefeed();
                    self.state.in_escape = false;
                    return;
                }
                0x4D => {
                    // RI - Reverse Index (move cursor up, scroll if at top)
                    if self.state.cursor.y == self.scroll_top {
                        self.scroll_down_line();
                    } else if self.state.cursor.y > 0 {
                        self.state.cursor.y -= 1;
                    }
                    self.state.in_escape = false;
                    return;
                }
                _ => {
                    // Unknown single-char escape — just ignore
                    self.state.in_escape = false;
                    return;
                }
            }
        }

        // Charset designation: consume exactly one more byte
        if self.state.escape_type == Some('(') {
            // The byte specifies which charset (B=US-ASCII, 0=line drawing, etc.)
            // We don't implement charset switching, just consume and done
            self.state.in_escape = false;
            return;
        }

        // We're in CSI mode (escape_type == Some('['))
        match byte {
            0x20..=0x2F => {
                // Intermediate bytes — silently consume (part of CSI sequence)
                // These modify the meaning of the final byte (e.g., `ESC [ 0 $ x`)
            }
            0x30..=0x3F => {
                // Parameter bytes: digits, semicolons, and private-mode chars (? > < =)
                let c = byte as char;
                if c.is_ascii_digit() {
                    let param = c.to_digit(10).unwrap() as u16;
                    if self.state.escape_params.is_empty() {
                        self.state.escape_params.push(param);
                    } else if let Some(last) = self.state.escape_params.last_mut() {
                        *last = *last * 10 + param;
                    }
                } else if c == ';' {
                    self.state.escape_params.push(0);
                }
                // '?' '>' '<' '=' are private-mode indicators — silently ignored
            }
            0x40..=0x7E => {
                // Final byte — execute CSI command
                let final_char = byte as char;
                self.execute_csi(final_char);
                self.state.in_escape = false;
            }
            _ => {
                // Invalid byte in CSI — abort
                self.state.in_escape = false;
            }
        }
    }

    /// Execute a CSI (Control Sequence Introducer) sequence
    fn execute_csi(&mut self, final_char: char) {
        // Copy params to avoid borrow checker issues
        let params: Vec<u16> = self.state.escape_params.clone();
        let param = |i: usize, default: u16| -> u16 { params.get(i).copied().unwrap_or(default) };

        match final_char {
            'A' => {
                // Cursor up
                let n = param(0, 1) as usize;
                self.state.cursor.y = self.state.cursor.y.saturating_sub(n);
            }
            'B' => {
                // Cursor down
                let n = param(0, 1) as usize;
                self.state.cursor.y = (self.state.cursor.y + n).min(self.rows - 1);
            }
            'C' => {
                // Cursor forward
                let n = param(0, 1) as usize;
                self.state.cursor.x = (self.state.cursor.x + n).min(self.cols - 1);
            }
            'D' => {
                // Cursor back
                let n = param(0, 1) as usize;
                self.state.cursor.x = self.state.cursor.x.saturating_sub(n);
            }
            'E' => {
                // CNL - Cursor Next Line
                let n = param(0, 1) as usize;
                self.state.cursor.y = (self.state.cursor.y + n).min(self.rows - 1);
                self.state.cursor.x = 0;
            }
            'F' => {
                // CPL - Cursor Previous Line
                let n = param(0, 1) as usize;
                self.state.cursor.y = self.state.cursor.y.saturating_sub(n);
                self.state.cursor.x = 0;
            }
            'G' | '`' => {
                // CHA - Cursor Character Absolute (move to column N)
                let col = param(0, 1) as usize;
                self.state.cursor.x = col.saturating_sub(1).min(self.cols - 1);
            }
            'H' | 'f' => {
                // CUP - Cursor Position
                let row = param(0, 1) as usize;
                let col = param(1, 1) as usize;
                self.state.cursor.y = row.saturating_sub(1).min(self.rows - 1);
                self.state.cursor.x = col.saturating_sub(1).min(self.cols - 1);
            }
            'I' => {
                // CHT - Cursor Horizontal Tab (move forward N tab stops)
                let n = param(0, 1) as usize;
                for _ in 0..n {
                    self.handle_tab();
                }
            }
            'J' => {
                // Erase in display
                let mode = param(0, 0) as u8;
                self.erase_display(mode);
            }
            'K' => {
                // Erase in line
                let mode = param(0, 0) as u8;
                self.erase_line(mode);
            }
            'm' => {
                // SGR - Select Graphic Rendition
                self.parse_sgr(&params);
            }
            'r' => {
                // Set scrolling region
                let top = param(0, 1) as usize;
                let bottom = param(1, self.rows as u16) as usize;
                self.scroll_top = top.saturating_sub(1).min(self.rows - 1);
                self.scroll_bottom = bottom.saturating_sub(1).min(self.rows - 1);
                self.state.cursor.x = 0;
                self.state.cursor.y = self.scroll_top;
            }
            'S' => {
                // Scroll up
                let n = param(0, 1) as usize;
                for _ in 0..n {
                    self.scroll_up_line();
                }
            }
            'T' => {
                // Scroll down
                let n = param(0, 1) as usize;
                for _ in 0..n {
                    self.scroll_down_line();
                }
            }
            'd' => {
                // VPA - Vertical Position Absolute (move to row N)
                let row = param(0, 1) as usize;
                self.state.cursor.y = row.saturating_sub(1).min(self.rows - 1);
            }
            'l' => {
                // Reset mode (DECRST)
                // Silently accept — modes like ?25l (hide cursor), ?2004l (bracketed paste) etc.
            }
            'h' => {
                // Set mode (DECSET)
                // Silently accept — modes like ?25h (show cursor), ?2004h (bracketed paste) etc.
            }
            'n' => {
                // Device status report
                // Could implement cursor position report
            }
            's' => {
                // Save cursor position
                self.state.saved_cursor = Some(self.state.cursor.clone());
            }
            'u' => {
                // Restore cursor position
                if let Some(cursor) = self.state.saved_cursor.take() {
                    self.state.cursor = cursor;
                }
            }
            'X' => {
                // Erase characters
                let n = param(0, 1) as usize;
                for x in self.state.cursor.x..(self.state.cursor.x + n).min(self.cols) {
                    self.grid[self.state.cursor.y][x] = Cell::default();
                }
            }
            '@' => {
                // Insert characters
                let n = param(0, 1) as usize;
                let y = self.state.cursor.y;
                let start = self.state.cursor.x;
                for x in (start..self.cols).rev() {
                    if x >= n {
                        self.grid[y][x] = self.grid[y][x - n].clone();
                    } else {
                        self.grid[y][x] = Cell::default();
                    }
                }
            }
            'P' => {
                // Delete characters
                let n = param(0, 1) as usize;
                let y = self.state.cursor.y;
                let start = self.state.cursor.x;
                for x in start..self.cols {
                    if x + n < self.cols {
                        self.grid[y][x] = self.grid[y][x + n].clone();
                    } else {
                        self.grid[y][x] = Cell::default();
                    }
                }
            }
            'M' => {
                // Delete lines
                let n = param(0, 1) as usize;
                let y = self.state.cursor.y;
                for i in y..=self.scroll_bottom {
                    if i + n <= self.scroll_bottom {
                        self.grid[i] = self.grid[i + n].clone();
                    } else {
                        self.grid[i] = vec![Cell::default(); self.cols];
                    }
                }
            }
            'L' => {
                // Insert lines
                let n = param(0, 1) as usize;
                let y = self.state.cursor.y;
                for i in (y..=self.scroll_bottom).rev() {
                    if i >= n {
                        self.grid[i] = self.grid[i - n].clone();
                    } else {
                        self.grid[i] = vec![Cell::default(); self.cols];
                    }
                }
            }
            _ => {}
        }
    }

    /// Parse SGR (Select Graphic Rendition) sequence
    fn parse_sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.reset_attributes();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            let p = params[i];
            match p {
                0 => self.reset_attributes(),
                1 => self.state.bold = true,
                2 => self.state.dim = true,
                3 => self.state.italic = true,
                4 => self.state.underline = true,
                7 => self.state.inverse = true,
                9 => self.state.strikethrough = true,
                22 => { self.state.bold = false; self.state.dim = false; }
                23 => self.state.italic = false,
                24 => self.state.underline = false,
                27 => self.state.inverse = false,
                29 => self.state.strikethrough = false,
                30..=37 => self.state.fg = color_from_code((p - 30) as u8),
                38 => {
                    // Extended foreground color
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        // 256-color mode
                        self.state.fg = Color::Indexed(params[i + 2] as u8);
                        i += 2;
                    } else if i + 4 < params.len() && params[i + 1] == 2 {
                        // True color mode
                        self.state.fg = Color::Rgb(
                            params[i + 2] as u8,
                            params[i + 3] as u8,
                            params[i + 4] as u8,
                        );
                        i += 4;
                    }
                }
                39 => self.state.fg = Color::Default,
                40..=47 => self.state.bg = color_from_code((p - 40) as u8),
                48 => {
                    // Extended background color
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.state.bg = Color::Indexed(params[i + 2] as u8);
                        i += 2;
                    } else if i + 4 < params.len() && params[i + 1] == 2 {
                        self.state.bg = Color::Rgb(
                            params[i + 2] as u8,
                            params[i + 3] as u8,
                            params[i + 4] as u8,
                        );
                        i += 4;
                    }
                }
                49 => self.state.bg = Color::Default,
                90..=97 => self.state.fg = color_from_code((p - 90 + 8) as u8),
                100..=107 => self.state.bg = color_from_code((p - 100 + 8) as u8),
                _ => {}
            }
            i += 1;
        }
    }

    /// Erase display (mode: 0=below, 1=above, 2=all, 3=scrollback)
    fn erase_display(&mut self, mode: u8) {
        match mode {
            0 => {
                // Below cursor
                for y in self.state.cursor.y..self.rows {
                    for x in if y == self.state.cursor.x {
                        self.state.cursor.x
                    } else {
                        0
                    }..self.cols
                    {
                        self.grid[y][x] = Cell::default();
                    }
                }
            }
            1 => {
                // Above cursor
                for y in 0..=self.state.cursor.y {
                    for x in 0..if y == self.state.cursor.y {
                        self.state.cursor.x + 1
                    } else {
                        self.cols
                    } {
                        self.grid[y][x] = Cell::default();
                    }
                }
            }
            2 | 3 => {
                // All or scrollback
                for row in &mut self.grid {
                    for cell in row {
                        *cell = Cell::default();
                    }
                }
                if mode == 3 {
                    self.scrollback.clear();
                }
            }
            _ => {}
        }
    }

    /// Erase line (mode: 0=right, 1=left, 2=all)
    fn erase_line(&mut self, mode: u8) {
        let y = self.state.cursor.y;
        match mode {
            0 => {
                for x in self.state.cursor.x..self.cols {
                    self.grid[y][x] = Cell::default();
                }
            }
            1 => {
                for x in 0..=self.state.cursor.x {
                    self.grid[y][x] = Cell::default();
                }
            }
            2 => {
                for x in 0..self.cols {
                    self.grid[y][x] = Cell::default();
                }
            }
            _ => {}
        }
    }

    /// Scroll up by one line (within scroll region)
    fn scroll_up_line(&mut self) {
        for y in self.scroll_top..self.scroll_bottom {
            self.grid.swap(y, y + 1);
        }
        let cols = self.cols;
        self.grid[self.scroll_bottom] = vec![Cell::default(); cols];
    }

    /// Scroll down by one line (within scroll region)
    fn scroll_down_line(&mut self) {
        for y in (self.scroll_top + 1..=self.scroll_bottom).rev() {
            self.grid.swap(y, y - 1);
        }
        let cols = self.cols;
        self.grid[self.scroll_top] = vec![Cell::default(); cols];
    }

    /// Reset all attributes to default
    fn reset_attributes(&mut self) {
        self.state.fg = Color::Default;
        self.state.bg = Color::Default;
        self.state.bold = false;
        self.state.dim = false;
        self.state.italic = false;
        self.state.underline = false;
        self.state.inverse = false;
        self.state.strikethrough = false;
    }

    /// Reset terminal to initial state
    fn reset(&mut self) {
        self.grid = vec![vec![Cell::default(); self.cols]; self.rows];
        self.scrollback.clear();
        self.state = ParserState::default();
        self.scroll_top = 0;
        self.scroll_bottom = self.rows - 1;
    }

    /// Get a slice of visible rows (from scrollback if offset > 0)
    pub fn visible_rows(&self, offset: usize, count: usize) -> Vec<&[Cell]> {
        let mut rows = Vec::new();

        // First get from scrollback
        let scrollback_start = self.scrollback.len().saturating_sub(offset);
        for i in scrollback_start..self.scrollback.len() {
            if rows.len() >= count {
                break;
            }
            if let Some(row) = self.scrollback.get(i) {
                rows.push(row.as_slice());
            }
        }

        // Then from visible grid
        for row in &self.grid {
            if rows.len() >= count {
                break;
            }
            rows.push(row.as_slice());
        }

        rows
    }
}

impl Default for TerminalEmulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine if a character is "wide" (occupies 2 terminal cells).
/// Covers CJK Unified Ideographs, Hangul, fullwidth forms, and common wide ranges.
fn is_wide_char(ch: char) -> bool {
    let cp = ch as u32;
    matches!(cp,
        // CJK Unified Ideographs
        0x4E00..=0x9FFF |
        // CJK Extension A
        0x3400..=0x4DBF |
        // CJK Extension B-F
        0x20000..=0x2A6DF |
        0x2A700..=0x2EBEF |
        // CJK Compatibility Ideographs
        0xF900..=0xFAFF |
        // Hangul Syllables
        0xAC00..=0xD7AF |
        // Fullwidth forms
        0xFF01..=0xFF60 |
        0xFFE0..=0xFFE6 |
        // CJK Symbols and Punctuation
        0x3000..=0x303F |
        // Hiragana
        0x3040..=0x309F |
        // Katakana
        0x30A0..=0x30FF |
        // Bopomofo
        0x3100..=0x312F |
        // Enclosed CJK
        0x3200..=0x32FF |
        // CJK Compatibility
        0x3300..=0x33FF |
        // Kangxi Radicals
        0x2F00..=0x2FDF |
        // CJK Radicals Supplement
        0x2E80..=0x2EFF
    )
}

/// Convert ANSI color code to Color
fn color_from_code(code: u8) -> Color {
    match code % 16 {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::White,
        8 => Color::BrightBlack,
        9 => Color::BrightRed,
        10 => Color::BrightGreen,
        11 => Color::BrightYellow,
        12 => Color::BrightBlue,
        13 => Color::BrightMagenta,
        14 => Color::BrightCyan,
        15 => Color::BrightWhite,
        _ => Color::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_characters() {
        let mut term = TerminalEmulator::with_size(10, 3);
        term.feed(b"Hello");
        assert_eq!(term.grid()[0][0].ch, 'H');
        assert_eq!(term.grid()[0][4].ch, 'o');
    }

    #[test]
    fn test_carriage_return() {
        let mut term = TerminalEmulator::with_size(10, 3);
        term.feed(b"Hello\r");
        term.feed(b"World");
        assert_eq!(term.grid()[0][0].ch, 'W');
        assert_eq!(term.grid()[0][4].ch, 'd');
    }

    #[test]
    fn test_ansi_colors() {
        let mut term = TerminalEmulator::with_size(10, 3);
        term.feed(b"\x1b[31mRed\x1b[0m Normal");
        // ANSI 31 = red foreground
        assert!(matches!(term.grid()[0][0].fg, Color::Red));
        // After reset, should be default
        assert!(matches!(term.grid()[0][4].fg, Color::Default));
    }

    #[test]
    fn test_cursor_movement() {
        let mut term = TerminalEmulator::with_size(10, 5);
        term.feed(b"\x1b[5;3H"); // Move to row 5, col 3 (1-indexed)
        assert_eq!(term.cursor().x, 2);
        assert_eq!(term.cursor().y, 4);
    }

    #[test]
    fn test_osc_sequences_are_swallowed() {
        let mut term = TerminalEmulator::with_size(20, 3);
        // OSC terminated by BEL
        term.feed(b"\x1b]0;My Title\x07Hello");
        assert_eq!(term.grid()[0][0].ch, 'H');
        assert_eq!(term.grid()[0][4].ch, 'o');

        // OSC terminated by ST (ESC \)
        let mut term2 = TerminalEmulator::with_size(20, 3);
        term2.feed(b"\x1b]2;Window Title\x1b\\World");
        assert_eq!(term2.grid()[0][0].ch, 'W');
    }

    #[test]
    fn test_multiple_osc_then_text() {
        let mut term = TerminalEmulator::with_size(40, 3);
        // Simulates bash startup: multiple OSC sequences followed by prompt
        term.feed(b"\x1b]0;user@host:~\x07\x1b]7;file://host/home/user\x07$ ");
        // Only "$ " should appear on screen
        assert_eq!(term.grid()[0][0].ch, '$');
        assert_eq!(term.grid()[0][1].ch, ' ');
    }

    #[test]
    fn test_scroll_does_not_corrupt_grid() {
        let mut term = TerminalEmulator::with_size(10, 3);
        // Fill 3 rows and then add a 4th to trigger scroll
        // Use \r\n (CR+LF) to move cursor to column 0 on each new line
        term.feed(b"AAA\r\nBBB\r\nCCC\r\nDDD");
        // After scroll: row 0=BBB, row 1=CCC, row 2=DDD
        assert_eq!(term.grid()[0][0].ch, 'B');
        assert_eq!(term.grid()[1][0].ch, 'C');
        assert_eq!(term.grid()[2][0].ch, 'D');
        // Grid should still have proper-sized rows (no empty vecs)
        assert_eq!(term.grid()[0].len(), 10);
        assert_eq!(term.grid()[1].len(), 10);
        assert_eq!(term.grid()[2].len(), 10);
    }

    #[test]
    fn test_many_lines_do_not_crash() {
        let mut term = TerminalEmulator::with_size(80, 24);
        // Simulate `ls` output — many lines forcing repeated scrolls
        for i in 0..100 {
            term.feed(format!("file_{:03}.txt\n", i).as_bytes());
        }
        // Should not crash, and grid should still be valid
        assert_eq!(term.grid().len(), 24);
        for row in term.grid() {
            assert_eq!(row.len(), 80);
        }
    }

    #[test]
    fn test_charset_designation_swallowed() {
        let mut term = TerminalEmulator::with_size(10, 3);
        // ESC ( B — select US ASCII charset, should NOT write 'B' to grid
        term.feed(b"\x1b(BHello");
        assert_eq!(term.grid()[0][0].ch, 'H');
        assert_eq!(term.grid()[0][4].ch, 'o');
    }

    #[test]
    fn test_csi_intermediate_bytes_swallowed() {
        let mut term = TerminalEmulator::with_size(10, 3);
        // CSI with intermediate bytes — should not leak
        term.feed(b"\x1b[0 qHi");
        assert_eq!(term.grid()[0][0].ch, 'H');
        assert_eq!(term.grid()[0][1].ch, 'i');
    }

    #[test]
    fn test_utf8_chinese_characters() {
        let mut term = TerminalEmulator::with_size(20, 3);
        term.feed("你好世界".as_bytes());
        // Wide chars: 你(0,1) 好(2,3) 世(4,5) 界(6,7)
        assert_eq!(term.grid()[0][0].ch, '你');
        assert!(term.grid()[0][1].wide_continuation);
        assert_eq!(term.grid()[0][2].ch, '好');
        assert!(term.grid()[0][3].wide_continuation);
        assert_eq!(term.grid()[0][4].ch, '世');
        assert_eq!(term.grid()[0][6].ch, '界');
    }

    #[test]
    fn test_utf8_mixed_with_ascii() {
        let mut term = TerminalEmulator::with_size(20, 3);
        term.feed("Hi你好".as_bytes());
        // H(0) i(1) 你(2,3) 好(4,5)
        assert_eq!(term.grid()[0][0].ch, 'H');
        assert_eq!(term.grid()[0][1].ch, 'i');
        assert_eq!(term.grid()[0][2].ch, '你');
        assert!(term.grid()[0][3].wide_continuation);
        assert_eq!(term.grid()[0][4].ch, '好');
    }

    #[test]
    fn test_utf8_with_ansi_sequences() {
        let mut term = TerminalEmulator::with_size(20, 3);
        // Red "错误" then reset " OK"
        term.feed("\x1b[31m错误\x1b[0m OK".as_bytes());
        // 错(0,1) 误(2,3) space(4) O(5) K(6)
        assert_eq!(term.grid()[0][0].ch, '错');
        assert!(matches!(term.grid()[0][0].fg, Color::Red));
        assert_eq!(term.grid()[0][2].ch, '误');
        assert_eq!(term.grid()[0][5].ch, 'O');
        assert!(matches!(term.grid()[0][5].fg, Color::Default));
    }

    #[test]
    fn test_cha_cursor_character_absolute() {
        let mut term = TerminalEmulator::with_size(20, 3);
        term.feed(b"Hello World");
        // CHA: move cursor to column 1 (1-indexed)
        term.feed(b"\x1b[1G");
        assert_eq!(term.cursor().x, 0);
        // CHA: move to column 7
        term.feed(b"\x1b[7G");
        assert_eq!(term.cursor().x, 6);
    }
}
