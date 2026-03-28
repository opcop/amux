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
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub strikethrough: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
            strikethrough: false,
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
            0 => (0, 0, 0),           // black
            1 => (205, 0, 0),         // red
            2 => (0, 205, 0),         // green
            3 => (205, 205, 0),       // yellow
            4 => (0, 0, 238),         // blue
            5 => (205, 0, 205),       // magenta
            6 => (0, 205, 205),       // cyan
            7 => (229, 229, 229),     // white
            8 => (127, 127, 127),     // bright black
            9 => (255, 0, 0),         // bright red
            10 => (0, 255, 0),        // bright green
            11 => (255, 255, 0),      // bright yellow
            12 => (0, 0, 255),        // bright blue
            13 => (255, 0, 255),      // bright magenta
            14 => (0, 255, 255),      // bright cyan
            15 => (255, 255, 255),    // bright white
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
        }
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: usize, rows: usize) {
        if cols == self.cols && rows == self.rows {
            return;
        }

        let mut new_grid = vec![vec![Cell::default(); cols]; rows];
        
        // Copy existing content
        for y in 0..rows.min(self.rows) {
            for x in 0..cols.min(self.cols) {
                new_grid[y][x] = self.grid[y][x].clone();
            }
        }

        self.cols = cols;
        self.rows = rows;
        self.grid = new_grid;
        self.scroll_bottom = rows.saturating_sub(1);

        // Clamp cursor
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
        // OSC sequences: swallow everything until BEL (0x07) or ST (ESC \)
        if self.state.in_osc {
            match byte {
                0x07 => {
                    // BEL terminates OSC
                    self.state.in_osc = false;
                }
                0x1B => {
                    // Could be start of ST (ESC \) — mark escape and let parse_escape handle
                    self.state.in_osc = false;
                    self.state.in_escape = true;
                    self.state.escape_params.clear();
                    self.state.escape_type = None;
                }
                _ => {
                    // Swallow all OSC content
                }
            }
            return;
        }

        if self.state.in_escape {
            self.parse_escape(byte);
        } else {
            match byte {
                0x07 => {
                    // Bell - could trigger notification
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
                c => {
                    // Handle other bytes as UTF-8 continuation or unknown
                    // For simplicity, just write as character if printable
                    if c >= 0x80 {
                        self.write_char(c as char);
                    }
                }
            }
        }
    }

    /// Write a character at the current cursor position
    fn write_char(&mut self, ch: char) {
        let x = self.state.cursor.x;
        let y = self.state.cursor.y;

        if x >= self.cols {
            // Wrap to next line
            self.state.cursor.x = 0;
            self.linefeed();
        }

        let y = self.state.cursor.y;
        let x = self.state.cursor.x;

        // Handle attributes
        let mut cell = Cell {
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
            italic: self.state.italic,
            underline: self.state.underline,
            inverse: self.state.inverse,
            strikethrough: self.state.strikethrough,
        };

        self.grid[y][x] = cell;

        self.state.cursor.x += 1;
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
    fn scroll(&mut self) {
        // Save the line being scrolled off to scrollback
        if self.scrollback.len() >= SCROLLBACK_LINES {
            self.scrollback.pop_front();
        }
        self.scrollback.push_back(self.grid[self.scroll_top].clone());

        // Move all lines up
        for y in self.scroll_top..self.scroll_bottom {
            self.grid[y] = std::mem::take(&mut self.grid[y + 1]);
            // Fill with empty cells
            for cell in &mut self.grid[y] {
                *cell = Cell::default();
            }
        }

        // Clear the new line at bottom
        for cell in &mut self.grid[self.scroll_bottom] {
            *cell = Cell::default();
        }
    }

    /// Parse an escape sequence
    fn parse_escape(&mut self, byte: u8) {
        match byte {
            0x5B => {
                // CSI - Control Sequence Introducer [
                self.state.escape_type = Some('[');
            }
            0x5D => {
                // OSC - Operating System Command — swallow until BEL or ST
                self.state.in_osc = true;
                self.state.in_escape = false;
                return;
            }
            0x28 | 0x29 => {
                // G0/G1 charset
                self.state.in_escape = false;
            }
            0x63 => {
                // RIS - Reset to Initial State
                self.reset();
                self.state.in_escape = false;
            }
            0x37 | 0x38 => {
                // Save/Restore cursor position
                if byte == 0x37 {
                    self.state.saved_cursor = Some(self.state.cursor.clone());
                } else if let Some(cursor) = self.state.saved_cursor.take() {
                    self.state.cursor = cursor;
                }
                self.state.in_escape = false;
            }
            0x30..=0x3F => {
                // Parameter bytes
                if let Some(c) = char::from_u32(byte as u32) {
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
                }
            }
            0x40..=0x7E => {
                // Final byte
                let final_char = byte as char;
                self.execute_csi(final_char);
                self.state.in_escape = false;
            }
            _ => {
                // Unknown escape sequence - ignore
                self.state.in_escape = false;
            }
        }
    }

    /// Execute a CSI (Control Sequence Introducer) sequence
    fn execute_csi(&mut self, final_char: char) {
        // Copy params to avoid borrow checker issues
        let params: Vec<u16> = self.state.escape_params.clone();
        let param = |i: usize, default: u16| -> u16 {
            params.get(i).copied().unwrap_or(default)
        };

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
            'H' | 'f' => {
                // Cursor position
                let row = param(0, 1) as usize;
                let col = param(1, 1) as usize;
                self.state.cursor.y = row.saturating_sub(1).min(self.rows - 1);
                self.state.cursor.x = col.saturating_sub(1).min(self.cols - 1);
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
            'l' => {
                // Reset mode
                if let Some(&1) = params.get(0) {
                    // Reset origin mode - not implemented
                }
            }
            'h' => {
                // Set mode
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
                3 => self.state.italic = true,
                4 => self.state.underline = true,
                7 => self.state.inverse = true,
                9 => self.state.strikethrough = true,
                22 => self.state.bold = false,
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
                    for x in if y == self.state.cursor.x { self.state.cursor.x } else { 0 }..self.cols {
                        self.grid[y][x] = Cell::default();
                    }
                }
            }
            1 => {
                // Above cursor
                for y in 0..=self.state.cursor.y {
                    for x in 0..if y == self.state.cursor.y { self.state.cursor.x + 1 } else { self.cols } {
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
            self.grid[y] = std::mem::take(&mut self.grid[y + 1]);
            for cell in &mut self.grid[y] {
                *cell = Cell::default();
            }
        }
        for cell in &mut self.grid[self.scroll_bottom] {
            *cell = Cell::default();
        }
    }

    /// Scroll down by one line (within scroll region)
    fn scroll_down_line(&mut self) {
        for y in (self.scroll_top..=self.scroll_bottom).rev() {
            self.grid[y] = std::mem::take(&mut self.grid[y - 1]);
            for cell in &mut self.grid[y] {
                *cell = Cell::default();
            }
        }
    }

    /// Reset all attributes to default
    fn reset_attributes(&mut self) {
        self.state.fg = Color::Default;
        self.state.bg = Color::Default;
        self.state.bold = false;
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
}
