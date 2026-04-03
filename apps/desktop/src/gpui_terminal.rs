//! GPUI Terminal Renderer — Canvas-based, pixel-perfect rendering
//!
//! Renders terminal content from alacritty_terminal using GPUI's canvas element.
//! Backgrounds are painted with `window.paint_quad()`, text with `ShapedLine::paint()`.
//! Cell dimensions are measured from actual font metrics — no hardcoded constants.

#[cfg(feature = "gpui")]
use gpui::{
    canvas, px, rgb, point, size, Bounds, Font, FontStyle, FontWeight, Hsla, IntoElement, Pixels,
    Point, Rgba, SharedString, Size, Styled, Window,
};

/// Left padding (in pixels) so terminal content doesn't hug the pane edge.
/// Applied in both rendering and mouse hit-testing.
pub const TERMINAL_LEFT_PADDING: f32 = 4.0;

// ─── Glyph Cache ───────────────────────────────────────────────

/// Thread-local shaped text cache to avoid re-shaping unchanged text runs.
/// Key: u64 hash of (text_content, style_bits) — avoids String allocation on lookup.
/// Uses hash-keyed cache with generation-based partial eviction (retains recent half).
#[cfg(feature = "gpui")]
mod glyph_cache {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};

    /// Style bits packed into a u8 for cache key
    pub fn style_key(bold: bool, italic: bool, underline: u8, strikethrough: bool) -> u8 {
        (bold as u8)
            | ((italic as u8) << 1)
            | ((underline & 0x7) << 2)
            | ((strikethrough as u8) << 5)
    }

    /// Compute a hash key from text + style without allocating a String.
    fn hash_key(text: &str, style: u8) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        style.hash(&mut hasher);
        hasher.finish()
    }

    struct CacheEntry {
        shaped: gpui::ShapedLine,
        generation: u64,
    }

    struct Cache {
        entries: HashMap<u64, CacheEntry>,
        generation: u64,
    }

    thread_local! {
        static CACHE: RefCell<Cache> = RefCell::new(Cache {
            entries: HashMap::with_capacity(2048),
            generation: 0,
        });
    }

    /// Look up a shaped line in the cache (zero-allocation).
    pub fn get(text: &str, style: u8) -> Option<gpui::ShapedLine> {
        let key = hash_key(text, style);
        CACHE.with(|c| {
            let mut cache = c.borrow_mut();
            let cur_gen = cache.generation;
            if let Some(entry) = cache.entries.get_mut(&key) {
                entry.generation = cur_gen; // mark as recently used
                Some(entry.shaped.clone())
            } else {
                None
            }
        })
    }

    /// Insert a shaped line into the cache.
    pub fn insert(text: &str, style: u8, shaped: gpui::ShapedLine) {
        let key = hash_key(text, style);
        CACHE.with(|c| {
            let mut cache = c.borrow_mut();
            // Evict stale entries when cache grows too large (keep recent half)
            if cache.entries.len() > 8192 {
                let cutoff = cache.generation.saturating_sub(1);
                cache.entries.retain(|_, e| e.generation > cutoff);
                cache.generation += 1;
            }
            let cur_gen = cache.generation;
            cache.entries.insert(key, CacheEntry {
                shaped,
                generation: cur_gen,
            });
        });
    }

    /// Clear the cache (call when font changes).
    pub fn clear() {
        CACHE.with(|c| {
            let mut cache = c.borrow_mut();
            cache.entries.clear();
            cache.generation += 1;
        });
    }
}

// ─── Terminal Theme ─────────────────────────────────────────────

/// Terminal color scheme — 16 ANSI colors + special colors.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct TerminalTheme {
    /// ANSI colors 0-15 (black, red, green, yellow, blue, magenta, cyan, white,
    /// then bright variants)
    pub ansi: [u32; 16],
    /// Dim ANSI colors 0-7
    pub dim: [u32; 8],
    /// Default foreground
    pub fg: u32,
    /// Default background
    pub bg: u32,
    /// Cursor color
    pub cursor: u32,
    /// Cursor text (under block cursor)
    pub cursor_text: u32,
    /// Selection highlight
    pub selection: u32,
}

#[cfg(feature = "gpui")]
impl TerminalTheme {
    /// Tomorrow Night (default)
    pub fn tomorrow_night() -> Self {
        Self {
            ansi: [
                0x1d1f21, 0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xc5c8c6,
                0x969896, 0xd54e53, 0xb9ca4a, 0xe7c547, 0x7aa6da, 0xc397d8, 0x70c0b1, 0xffffff,
            ],
            dim: [0x131515, 0x864343, 0x777e45, 0x9f834d, 0x556b7e, 0x75627c, 0x5c7e7a, 0x828482],
            fg: 0xc5c8c6,
            bg: 0x1d1f21,
            cursor: 0xf5f5f5,
            cursor_text: 0x1d1f21,
            selection: 0x3a5a8f,
        }
    }

    /// Catppuccin Mocha
    pub fn catppuccin_mocha() -> Self {
        Self {
            ansi: [
                0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xcba6f7, 0x94e2d5, 0xbac2de,
                0x585b70, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xcba6f7, 0x94e2d5, 0xa6adc8,
            ],
            dim: [0x313244, 0x874c5e, 0x5e8060, 0x8a7d61, 0x4e6589, 0x6e5d87, 0x537d74, 0x6c7086],
            fg: 0xcdd6f4,
            bg: 0x1e1e2e,
            cursor: 0xf5e0dc,
            cursor_text: 0x1e1e2e,
            selection: 0x45475a,
        }
    }

    /// Dracula
    pub fn dracula() -> Self {
        Self {
            ansi: [
                0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2,
                0x6272a4, 0xff6e6e, 0x69ff94, 0xffffa5, 0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
            ],
            dim: [0x14151b, 0x992f2f, 0x2e9148, 0x8e9153, 0x6e5692, 0x994774, 0x518a97, 0x909090],
            fg: 0xf8f8f2,
            bg: 0x282a36,
            cursor: 0xf8f8f2,
            cursor_text: 0x282a36,
            selection: 0x44475a,
        }
    }

    /// Solarized Dark
    pub fn solarized_dark() -> Self {
        Self {
            ansi: [
                0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
                0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
            ],
            dim: [0x042029, 0x8a1e1c, 0x535c00, 0x6e5400, 0x175680, 0x82204f, 0x196360, 0x908c82],
            fg: 0x839496,
            bg: 0x002b36,
            cursor: 0x839496,
            cursor_text: 0x002b36,
            selection: 0x073642,
        }
    }

    /// One Dark (Atom)
    pub fn one_dark() -> Self {
        Self {
            ansi: [
                0x282c34, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf,
                0x545862, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xc8ccd4,
            ],
            dim: [0x1a1d23, 0x8a4248, 0x5d7849, 0x8d764c, 0x3c6c93, 0x7a4a88, 0x357078, 0x696e77],
            fg: 0xabb2bf,
            bg: 0x282c34,
            cursor: 0x528bff,
            cursor_text: 0x282c34,
            selection: 0x3e4452,
        }
    }

    /// Look up a built-in theme by name.
    pub fn by_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "catppuccin" | "catppuccin-mocha" | "catppuccin_mocha" => Self::catppuccin_mocha(),
            "dracula" => Self::dracula(),
            "solarized" | "solarized-dark" | "solarized_dark" => Self::solarized_dark(),
            "one-dark" | "one_dark" | "onedark" | "atom" => Self::one_dark(),
            _ => Self::tomorrow_night(),
        }
    }
}

// ─── Cell Metrics ───────────────────────────────────────────────

/// Cell dimensions measured from actual font metrics.
/// Created once via `measure_cell_metrics()` and cached.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct CellMetrics {
    /// Cell width in pixels (monospace advance of 'M')
    pub width: f32,
    /// Cell height in pixels (line height)
    pub height: f32,
    /// Font descent in pixels (for baseline calculation)
    pub descent: f32,
}

/// Measure cell dimensions from the actual monospace font.
/// Call once on first render and cache the result.
#[cfg(feature = "gpui")]
pub fn measure_cell_metrics(window: &mut Window, font_family: &str, font_size_f32: f32, line_height_mult: f32) -> CellMetrics {
    let text_system = window.text_system();
    let font_size = px(font_size_f32);
    let font = make_font(font_family, false);

    // Resolve font and get metrics
    let font_id = text_system.resolve_font(&font);
    let ascent = text_system.ascent(font_id, font_size);
    let descent = text_system.descent(font_id, font_size);

    // Measure cell width by shaping a long string and averaging.
    // A single char's shaped width can include trailing bearing, making cell_w
    // too large. Averaging over many chars gives the true advance per character.
    let sample = "0123456789abcdefghij";
    let run = gpui::TextRun {
        len: sample.len(),
        font,
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = text_system.shape_line(SharedString::from(sample), font_size, &[run], None);
    let cell_width = shaped.width().as_f32() / sample.len() as f32;

    CellMetrics {
        width: cell_width,
        height: (font_size_f32 * line_height_mult).ceil(),
        descent: descent.as_f32(),
    }
}

/// Construct a terminal Font with optional bold/italic.
#[cfg(feature = "gpui")]
fn make_font_styled(font_family: &str, bold: bool, italic: bool) -> Font {
    Font {
        family: SharedString::from(font_family.to_string()),
        weight: if bold { FontWeight::BOLD } else { FontWeight::NORMAL },
        style: if italic { FontStyle::Italic } else { FontStyle::Normal },
        ..Default::default()
    }
}

#[cfg(feature = "gpui")]
fn make_font(font_family: &str, bold: bool) -> Font {
    make_font_styled(font_family, bold, false)
}

// ─── Public Render API ──────────────────────────────────────────

/// Render a terminal using canvas-based pixel-perfect rendering.
///
/// Returns an element that fills its container. All text is shaped from
/// actual font metrics — no hardcoded cell width constants.
#[cfg(feature = "gpui")]
pub fn render_alacritty_terminal(
    term: &amux_platform::terminal::alacritty_view::AlacrittyTerminal,
    cursor_blink_on: bool,
    metrics: &CellMetrics,
    is_active_pane: bool,
    font_family: &str,
    font_size: f32,
    theme: &TerminalTheme,
) -> impl IntoElement {
    let mut data = collect_render_data(term, cursor_blink_on, theme);

    // Active pane: always show beam cursor.
    // TUI apps (Claude/Codex/OpenCode) hide the terminal cursor but track position
    // at the input field. We force-show a beam so the user always sees where they're typing.
    if is_active_pane && data.cursor_row < data.rows && data.cursor_col < data.cols {
        data.cursor_visible = true;
        data.cursor_shape = 1; // beam
    }
    let m = metrics.clone();
    let ff = font_family.to_string();
    let fs = font_size;

    let total_w = data.cols as f32 * metrics.width;
    let total_h = data.rows as f32 * metrics.height;

    canvas(
        move |bounds, window, _cx| prepaint_terminal(data, bounds, &m, &ff, fs, window),
        move |_bounds, prepaint, window, cx| paint_terminal(prepaint, window, cx),
    )
    .w(px(total_w))
    .h(px(total_h))
    .flex_1()
}

// ─── Internal Types ─────────────────────────────────────────────

#[cfg(feature = "gpui")]
struct RenderData {
    grid: Vec<Vec<RenderCell>>,
    rows: usize,
    cols: usize,
    cursor_row: usize,
    cursor_col: usize,
    cursor_visible: bool,
    /// 0=block, 1=beam, 2=underline
    cursor_shape: u8,
    cursor_color: Rgba,
    /// Selection: vec of (row, start_col, end_col) for highlighted cells
    selection_ranges: Vec<(usize, usize, usize)>,
    cursor_text_color: Rgba,
    default_bg: Rgba,
    selection_bg: Rgba,
    /// Scroll state: (display_offset, total_history, visible_rows)
    scroll_info: (usize, usize, usize),
}

#[cfg(feature = "gpui")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum UnderlineKind {
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

#[cfg(feature = "gpui")]
#[derive(Clone)]
struct RenderCell {
    ch: char,
    fg: Rgba,
    bg: Rgba,
    bold: bool,
    italic: bool,
    underline: UnderlineKind,
    strikethrough: bool,
    hidden: bool,
    wide_continuation: bool,
    /// OSC 8 hyperlink URL (if any)
    hyperlink_url: Option<String>,
}

#[cfg(feature = "gpui")]
impl UnderlineKind {
    fn as_u8(self) -> u8 {
        match self {
            UnderlineKind::None => 0,
            UnderlineKind::Single => 1,
            UnderlineKind::Double => 2,
            UnderlineKind::Curly => 3,
            UnderlineKind::Dotted => 4,
            UnderlineKind::Dashed => 5,
        }
    }
}

#[cfg(feature = "gpui")]
impl Default for RenderCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: rgb(0xcdd6f4),
            bg: rgb(0x1d1f21),
            bold: false,
            italic: false,
            underline: UnderlineKind::None,
            strikethrough: false,
            hidden: false,
            wide_continuation: false,
            hyperlink_url: None,
        }
    }
}

/// Intermediate data produced by prepaint, consumed by paint.
#[cfg(feature = "gpui")]
struct PrepaintData {
    /// Background rectangles (paint first)
    bg_rects: Vec<PaintRect>,
    /// Block/box drawing character rectangles
    special_rects: Vec<PaintRect>,
    /// Selection highlight rectangles
    selection_rects: Vec<PaintRect>,
    /// Shaped text lines with positions
    text_lines: Vec<PaintText>,
    /// Custom underline rectangles (rendered after text)
    underline_rects: Vec<PaintRect>,
    /// Scrollbar indicator (thin track on right edge)
    scrollbar_rects: Vec<PaintRect>,
    /// Cursor overlay rectangles (paint last)
    cursor_rects: Vec<PaintRect>,
    /// Line height for ShapedLine::paint
    line_height: Pixels,
}

#[cfg(feature = "gpui")]
struct PaintRect {
    origin: Point<Pixels>,
    size: Size<Pixels>,
    color: Rgba,
}

#[cfg(feature = "gpui")]
struct PaintText {
    origin: Point<Pixels>,
    shaped: gpui::ShapedLine,
}

// ─── Data Collection ────────────────────────────────────────────

/// Collect render data from the alacritty terminal.
#[cfg(feature = "gpui")]
fn collect_render_data(
    term: &amux_platform::terminal::alacritty_view::AlacrittyTerminal,
    cursor_blink_on: bool,
    theme: &TerminalTheme,
) -> RenderData {
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::term::cell::Flags as CellFlags;

    term.with_term(|t| {
        let content = t.renderable_content();
        let cols = t.columns();
        let rows = t.screen_lines();
        let cursor = content.cursor;
        let display_offset = t.grid().display_offset();

        let default_fg = rgb(theme.fg);
        let default_bg = rgb(theme.bg);
        let cursor_color = rgb(theme.cursor);
        let cursor_text_color = rgb(theme.cursor_text);

        let mut grid: Vec<Vec<RenderCell>> = vec![vec![RenderCell::default(); cols]; rows];

        for indexed in content.display_iter {
            let point = indexed.point;
            // Convert grid coordinates to viewport row.
            // Scrollback lines have negative line numbers (e.g. -1, -2, ...);
            // adding display_offset maps them to viewport rows 0, 1, ...
            let viewport_line = point.line.0 + display_offset as i32;
            if viewport_line < 0 {
                continue;
            }
            let row = viewport_line as usize;
            let col = point.column.0;
            if row < rows && col < cols {
                let cell = &indexed.cell;
                let flags = cell.flags;
                let is_dim = flags.contains(CellFlags::DIM);
                let mut fg =
                    convert_color(&cell.fg, &default_fg, true, is_dim, theme);
                let mut bg = convert_color(&cell.bg, &default_bg, false, false, theme);

                // REVERSE video: swap fg and bg
                if flags.contains(CellFlags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }

                // Underline kind
                let underline = if flags.contains(CellFlags::UNDERCURL) {
                    UnderlineKind::Curly
                } else if flags.contains(CellFlags::DOUBLE_UNDERLINE) {
                    UnderlineKind::Double
                } else if flags.contains(CellFlags::DOTTED_UNDERLINE) {
                    UnderlineKind::Dotted
                } else if flags.contains(CellFlags::DASHED_UNDERLINE) {
                    UnderlineKind::Dashed
                } else if flags.contains(CellFlags::UNDERLINE) {
                    UnderlineKind::Single
                } else {
                    UnderlineKind::None
                };

                let hyperlink_url = cell.hyperlink().map(|h| {
                    let vte_link: alacritty_terminal::vte::ansi::Hyperlink = h.into();
                    vte_link.uri.to_string()
                });

                // Hyperlinks: auto-underline and tint blue if no explicit style
                let (final_fg, final_underline) = if hyperlink_url.is_some() {
                    let link_fg = if matches!(cell.fg, alacritty_terminal::vte::ansi::Color::Named(
                        alacritty_terminal::vte::ansi::NamedColor::Foreground
                    )) {
                        // Default foreground — tint to link blue
                        Rgba { r: 0.478, g: 0.647, b: 0.855, a: 1.0 } // #7aa6da
                    } else {
                        fg
                    };
                    let ul = if underline == UnderlineKind::None {
                        UnderlineKind::Single
                    } else {
                        underline
                    };
                    (link_fg, ul)
                } else {
                    (fg, underline)
                };

                grid[row][col] = RenderCell {
                    ch: cell.c,
                    fg: final_fg,
                    bg,
                    bold: flags.contains(CellFlags::BOLD),
                    italic: flags.contains(CellFlags::ITALIC),
                    underline: final_underline,
                    strikethrough: flags.contains(CellFlags::STRIKEOUT),
                    hidden: flags.contains(CellFlags::HIDDEN),
                    wide_continuation: flags.contains(CellFlags::WIDE_CHAR_SPACER),
                    hyperlink_url,
                };
            }
        }

        let cursor_row = cursor.point.line.0.max(0) as usize;
        let cursor_col = cursor.point.column.0;
        let cursor_hidden = matches!(
            cursor.shape,
            alacritty_terminal::vte::ansi::CursorShape::Hidden
        );
        let cursor_visible = !cursor_hidden && cursor_blink_on && display_offset == 0;
        let cursor_shape = match cursor.shape {
            alacritty_terminal::vte::ansi::CursorShape::Block => 0u8,
            alacritty_terminal::vte::ansi::CursorShape::Beam => 1,
            alacritty_terminal::vte::ansi::CursorShape::Underline => 2,
            _ => 0,
        };

        // Extract selection ranges for highlighting
        let mut selection_ranges = Vec::new();
        if let Some(ref sel) = t.selection {
            if let Some(range) = sel.to_range(t) {
                use alacritty_terminal::index::Line;
                let sel_start = range.start;
                let sel_end = range.end;
                for line in sel_start.line.0..=sel_end.line.0 {
                    if line < 0 { continue; }
                    let r = line as usize;
                    if r >= rows { continue; }
                    let c_start = if line == sel_start.line.0 { sel_start.column.0 } else { 0 };
                    let c_end = if line == sel_end.line.0 { sel_end.column.0 } else { cols.saturating_sub(1) };
                    selection_ranges.push((r, c_start, c_end));
                }
            }
        }

        RenderData {
            grid,
            rows,
            cols,
            cursor_row,
            cursor_col,
            cursor_visible,
            cursor_shape,
            cursor_color,
            cursor_text_color,
            default_bg,
            selection_bg: rgb(theme.selection),
            scroll_info: {
                let offset = t.grid().display_offset();
                let history = t.grid().history_size();
                let visible = t.screen_lines();
                (offset, history, visible)
            },
            selection_ranges,
        }
    })
}

// ─── Prepaint Phase ─────────────────────────────────────────────

/// Shape text and collect paint operations.
/// Runs during GPUI's prepaint phase (CPU-only work).
#[cfg(feature = "gpui")]
/// Shape text prefix to get precise cursor X within a narrow run.
/// Returns the pixel offset from bounds origin.
#[cfg(feature = "gpui")]
fn shape_cursor_x(
    narrow_text: &str,
    text_idx: usize,
    narrow_start: usize,
    cell_w: f32,
    bold: bool,
    italic: bool,
    text_system: &std::sync::Arc<gpui::WindowTextSystem>,
    font_size: Pixels,
    font_family: &str,
) -> Pixels {
    if text_idx == 0 {
        px(narrow_start as f32 * cell_w)
    } else {
        let prefix: String = narrow_text.chars().take(text_idx).collect();
        let prun = gpui::TextRun {
            len: prefix.len(),
            font: make_font_styled(font_family, bold, italic),
            color: Hsla::default(),
            background_color: None, underline: None, strikethrough: None,
        };
        let ps = text_system.shape_line(SharedString::from(prefix), font_size, &[prun], None);
        px(narrow_start as f32 * cell_w) + ps.width()
    }
}

/// Shape a text run, using the glyph cache when possible.
#[cfg(feature = "gpui")]
fn shape_cached(
    text: &str,
    style_bits: u8,
    run: gpui::TextRun,
    font_size: Pixels,
    text_system: &std::sync::Arc<gpui::WindowTextSystem>,
) -> gpui::ShapedLine {
    if let Some(cached) = glyph_cache::get(text, style_bits) {
        return cached;
    }
    let shaped = text_system.shape_line(
        SharedString::from(text.to_string()), font_size, &[run], None,
    );
    glyph_cache::insert(text, style_bits, shaped.clone());
    shaped
}

fn prepaint_terminal(
    mut data: RenderData,
    bounds: Bounds<Pixels>,
    metrics: &CellMetrics,
    font_family: &str,
    font_size_f32: f32,
    window: &mut Window,
) -> PrepaintData {
    let text_system = window.text_system();
    let font_size = px(font_size_f32);
    let cell_w = metrics.width;
    let cell_h = metrics.height;
    let line_height = px(cell_h);
    // Apply left padding so content doesn't hug the pane edge
    let pad_x = px(TERMINAL_LEFT_PADDING);
    let content_origin_x = bounds.origin.x + pad_x;

    let mut bg_rects = Vec::with_capacity(data.rows * 4);
    let mut special_rects = Vec::with_capacity(64);
    let mut selection_rects = Vec::with_capacity(8);
    let mut text_lines = Vec::with_capacity(data.rows * 4);
    let mut underline_rects = Vec::with_capacity(32);
    let mut cursor_rects = Vec::with_capacity(2);

    // Build selection highlight rects
    let selection_bg = data.selection_bg;
    for &(row, c_start, c_end) in &data.selection_ranges {
        let x = content_origin_x + px(c_start as f32 * cell_w);
        let y = bounds.origin.y + px(row as f32 * cell_h);
        let w = ((c_end + 1).saturating_sub(c_start)) as f32 * cell_w;
        selection_rects.push(PaintRect {
            origin: point(x, y),
            size: size(px(w), px(cell_h)),
            color: selection_bg,
        });
    }

    // Block cursor colors are applied inline in the bg_rects loop below.

    // Cursor X is computed during Phase 2 text layout for the cursor row,
    // matching the exact run boundaries and shaped widths used for text rendering.
    // Fallback: grid-aligned position.
    let mut cursor_shaped_x = px(data.cursor_col as f32 * cell_w);
    let mut cursor_x_found = false;

    for row in 0..data.rows {
        let is_cursor_row = data.cursor_visible && row == data.cursor_row;
        let y = bounds.origin.y + px(row as f32 * cell_h);

        // ── Phase 1: Background quads ──
        // Group consecutive cells with same bg color into single quads
        let mut col = 0;
        while col < data.cols {
            let cell = &data.grid[row][col];
            let bg = cell.bg;
            let start_col = col;
            col += 1;
            while col < data.cols && data.grid[row][col].bg == bg {
                col += 1;
            }
            let x = content_origin_x + px(start_col as f32 * cell_w);
            let w = (col - start_col) as f32 * cell_w;
            // Block cursor: if this rect spans the cursor position, split it
            // to insert the cursor-colored cell.
            let has_block_cursor = data.cursor_visible
                && data.cursor_shape == 0
                && row == data.cursor_row
                && start_col <= data.cursor_col
                && col > data.cursor_col;

            if has_block_cursor {
                let cc = data.cursor_col;
                let cx = content_origin_x + cursor_shaped_x;
                // Part before cursor
                if cc > start_col {
                    let w_before = cx - x;
                    bg_rects.push(PaintRect {
                        origin: point(x, y),
                        size: size(w_before, px(cell_h)),
                        color: bg,
                    });
                }
                // Cursor cell — always 1 cell wide even on wide characters,
                // so the cursor doesn't look abnormally large on CJK text.
                bg_rects.push(PaintRect {
                    origin: point(cx, y),
                    size: size(px(cell_w), px(cell_h)),
                    color: data.cursor_color,
                });
                // Part after cursor
                let after_col = cc + 1;
                if after_col < col {
                    let x_after = cx + px(cell_w);
                    let w_after = content_origin_x + px(col as f32 * cell_w) - x_after;
                    bg_rects.push(PaintRect {
                        origin: point(x_after, y),
                        size: size(w_after, px(cell_h)),
                        color: bg,
                    });
                }
            } else {
                bg_rects.push(PaintRect {
                    origin: point(x, y),
                    size: size(px(w), px(cell_h)),
                    color: bg,
                });
            }
        }

        // ── Phase 2: Text runs + special chars ──
        col = 0;
        while col < data.cols {
            let cell = &data.grid[row][col];

            // Skip wide continuation cells
            if cell.wide_continuation {
                col += 1;
                continue;
            }

            // Handle block/box drawing characters as quads
            if is_special_render_char(cell.ch) {
                let x = content_origin_x + px(col as f32 * cell_w);
                // Check if this is a wide special char
                let char_cells =
                    if col + 1 < data.cols && data.grid[row][col + 1].wide_continuation {
                        2
                    } else {
                        1
                    };
                push_special_char(
                    cell.ch,
                    cell.fg,
                    cell.bg,
                    x,
                    y,
                    char_cells as f32 * cell_w,
                    cell_h,
                    &mut special_rects,
                );
                col += 1;
                continue;
            }

            // Build text runs, breaking at wide chars and style changes.
            // Each run has uniform (fg, bold, italic, underline, strikethrough, hidden).
            // Wide (CJK) chars are shaped individually at exact grid positions.
            let fg = cell.fg;
            let bold = cell.bold;
            let italic = cell.italic;
            let underline = cell.underline;
            let strikethrough = cell.strikethrough;
            let hidden = cell.hidden;
            let mut narrow_start = col;
            let mut narrow_text = String::new();
            let mut has_visible = false; // track if run has non-space chars

            // Helper: build TextRun with current style
            let build_run = |text_len: usize, fg: Rgba, bold: bool, italic: bool, _underline: UnderlineKind, strikethrough: bool, hidden: bool| -> gpui::TextRun {
                // Hidden text: render as invisible (fg = transparent)
                let fg_hsla = if hidden {
                    Hsla { h: 0.0, s: 0.0, l: 0.0, a: 0.0 }
                } else {
                    rgba_to_hsla(fg)
                };
                // Underlines are rendered as custom quads for full style support
                // (double, curly, dotted, dashed). Only strikethrough uses GPUI's built-in.
                gpui::TextRun {
                    len: text_len,
                    font: make_font_styled(font_family, bold, italic),
                    color: fg_hsla,
                    background_color: None,
                    underline: None,
                    strikethrough: if strikethrough {
                        Some(gpui::StrikethroughStyle { thickness: px(1.0), color: Some(fg_hsla) })
                    } else { None },
                }
            };

            while col < data.cols {
                let c = &data.grid[row][col];
                if c.wide_continuation {
                    col += 1;
                    continue;
                }
                if c.fg != fg || c.bold != bold || c.italic != italic
                    || c.underline != underline || c.strikethrough != strikethrough
                    || c.hidden != hidden
                {
                    break;
                }
                if is_special_render_char(c.ch) {
                    break;
                }

                let is_wide = col + 1 < data.cols
                    && data.grid[row][col + 1].wide_continuation;

                if is_wide {
                    // Cursor detection: check if cursor falls in the narrow run being flushed
                    if is_cursor_row && !cursor_x_found
                        && !narrow_text.is_empty()
                        && data.cursor_col >= narrow_start
                        && data.cursor_col <= narrow_start + narrow_text.len()
                    {
                        cursor_x_found = true;
                        cursor_shaped_x = shape_cursor_x(
                            &narrow_text, data.cursor_col - narrow_start, narrow_start,
                            cell_w, bold, italic, &text_system, font_size, font_family,
                        );
                    }

                    // Flush pending narrow run
                    if has_visible {
                        let sk = glyph_cache::style_key(bold, italic, underline.as_u8(), strikethrough);
                        let run = build_run(narrow_text.len(), fg, bold, italic, underline, strikethrough, hidden);
                        let shaped = shape_cached(&narrow_text, sk, run, font_size, &text_system);
                        let x = content_origin_x + px(narrow_start as f32 * cell_w);
                        text_lines.push(PaintText { origin: point(x, y), shaped });
                    }
                    narrow_text.clear();
                    has_visible = false;

                    // Cursor detection: cursor on this wide char
                    if is_cursor_row && !cursor_x_found && col == data.cursor_col {
                        cursor_x_found = true;
                        cursor_shaped_x = px(col as f32 * cell_w);
                    }

                    // Shape wide char individually at exact grid position
                    let ch = if c.ch == '\0' { ' ' } else { c.ch };
                    if ch != ' ' {
                        let ch_str = ch.to_string();
                        let sk = glyph_cache::style_key(bold, italic, underline.as_u8(), strikethrough);
                        let run = build_run(ch_str.len(), fg, bold, italic, underline, strikethrough, hidden);
                        let shaped = shape_cached(&ch_str, sk, run, font_size, &text_system);
                        let x = content_origin_x + px(col as f32 * cell_w);
                        text_lines.push(PaintText { origin: point(x, y), shaped });
                    }

                    col += 1;
                    narrow_start = col + 1;
                } else {
                    if narrow_text.is_empty() {
                        narrow_start = col;
                    }
                    let ch = if c.ch == '\0' { ' ' } else { c.ch };
                    if ch != ' ' { has_visible = true; }
                    narrow_text.push(ch);
                    col += 1;
                }
            }

            // Cursor detection: check if cursor falls in the remaining narrow run
            if is_cursor_row && !cursor_x_found
                && !narrow_text.is_empty()
                && data.cursor_col >= narrow_start
                && data.cursor_col <= narrow_start + narrow_text.len()
            {
                cursor_x_found = true;
                cursor_shaped_x = shape_cursor_x(
                    &narrow_text, data.cursor_col - narrow_start, narrow_start,
                    cell_w, bold, italic, &text_system, font_size, font_family,
                );
            }

            // Flush remaining narrow run
            if has_visible {
                let sk = glyph_cache::style_key(bold, italic, underline.as_u8(), strikethrough);
                let run = build_run(narrow_text.len(), fg, bold, italic, underline, strikethrough, hidden);
                let shaped = shape_cached(&narrow_text, sk, run, font_size, &text_system);
                let x = content_origin_x + px(narrow_start as f32 * cell_w);
                text_lines.push(PaintText { origin: point(x, y), shaped });
            }
        }

        // ── Phase 2.5: Underline spans ──
        // Scan row for consecutive cells with same underline style + color.
        {
            let mut ucol = 0;
            while ucol < data.cols {
                let cell = &data.grid[row][ucol];
                let ul = cell.underline;
                if ul == UnderlineKind::None || cell.wide_continuation {
                    ucol += 1;
                    continue;
                }
                let ul_color = cell.fg;
                let start = ucol;
                ucol += 1;
                while ucol < data.cols
                    && data.grid[row][ucol].underline == ul
                    && data.grid[row][ucol].fg == ul_color
                    && !data.grid[row][ucol].wide_continuation
                {
                    ucol += 1;
                }
                let x = content_origin_x + px(start as f32 * cell_w);
                let w = (ucol - start) as f32 * cell_w;
                let baseline_y = y + px(cell_h - metrics.descent.abs().max(2.0));
                push_underline(ul, ul_color, x, baseline_y, w, &mut underline_rects);
            }
        }
    }

    // ── Phase 3: Cursor overlay (beam/underline) ──
    // Reuse pre-computed cursor_shaped_x for precise positioning.
    if data.cursor_visible && data.cursor_shape > 0 {
        let cx = content_origin_x + cursor_shaped_x;
        let cy = bounds.origin.y + px(data.cursor_row as f32 * cell_h);

        // Wide char: underline spans 2 cells, beam stays at left edge
        let is_wide = data.cursor_row < data.rows
            && data.cursor_col + 1 < data.cols
            && data.grid[data.cursor_row][data.cursor_col + 1].wide_continuation;
        let cursor_w = if is_wide { cell_w * 2.0 } else { cell_w };
        match data.cursor_shape {
            1 => {
                // Beam cursor: 2px wide vertical line (always single-cell width)
                cursor_rects.push(PaintRect {
                    origin: point(cx, cy),
                    size: size(px(2.0), px(cell_h)),
                    color: data.cursor_color,
                });
            }
            2 => {
                // Underline cursor: spans full character width
                cursor_rects.push(PaintRect {
                    origin: point(cx, cy + px((cell_h - 2.0).max(0.0))),
                    size: size(px(cursor_w), px(2.0_f32.min(cell_h))),
                    color: data.cursor_color,
                });
            }
            _ => {}
        }
    }

    // ── Phase 4: Scrollbar (only visible when scrolled away from bottom) ──
    let mut scrollbar_rects = Vec::new();
    {
        let (offset, history, visible) = data.scroll_info;
        if history > 0 && offset > 0 {
            let total = history + visible;
            let track_h = data.rows as f32 * cell_h;
            let bar_w = 4.0_f32;
            let track_x = content_origin_x + px(data.cols as f32 * cell_w - bar_w);
            let track_y = bounds.origin.y;

            // Thumb: proportional to visible/total, position based on offset
            let thumb_ratio = (visible as f32 / total as f32).clamp(0.05, 1.0);
            let thumb_h = (track_h * thumb_ratio).max(8.0);
            // offset=0 means at bottom, offset=history means at top
            let scroll_frac = (offset as f32 / history as f32).clamp(0.0, 1.0);
            let thumb_y = track_y + px((track_h - thumb_h) * (1.0 - scroll_frac));

            // Track background
            scrollbar_rects.push(PaintRect {
                origin: point(track_x, track_y),
                size: size(px(bar_w), px(track_h)),
                color: Rgba { r: 1.0, g: 1.0, b: 1.0, a: 0.05 },
            });
            // Thumb
            scrollbar_rects.push(PaintRect {
                origin: point(track_x, thumb_y),
                size: size(px(bar_w), px(thumb_h)),
                color: Rgba { r: 1.0, g: 1.0, b: 1.0, a: 0.3 },
            });
        }
    }

    PrepaintData {
        bg_rects,
        special_rects,
        selection_rects,
        text_lines,
        underline_rects,
        scrollbar_rects,
        cursor_rects,
        line_height,
    }
}

// ─── Paint Phase ────────────────────────────────────────────────

/// Execute all paint operations.
/// Runs during GPUI's paint phase (GPU submission).
#[cfg(feature = "gpui")]
fn paint_terminal(data: PrepaintData, window: &mut Window, cx: &mut gpui::App) {
    // Layer 1: Backgrounds
    for rect in &data.bg_rects {
        paint_rect(rect, window);
    }

    // Layer 2: Selection highlight (under text, over bg)
    for rect in &data.selection_rects {
        paint_rect(rect, window);
    }

    // Layer 3: Block/box drawing characters
    for rect in &data.special_rects {
        paint_rect(rect, window);
    }

    // Layer 4: Text glyphs
    for line in &data.text_lines {
        let _ = line.shaped.paint(
            line.origin,
            data.line_height,
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
    }

    // Layer 4.5: Custom underlines (over text, under cursor)
    for rect in &data.underline_rects {
        paint_rect(rect, window);
    }

    // Layer 5: Scrollbar (over everything except cursor)
    for rect in &data.scrollbar_rects {
        paint_rect(rect, window);
    }

    // Layer 6: Cursor overlay (beam/underline)
    for rect in &data.cursor_rects {
        paint_rect(rect, window);
    }
}

/// Paint a single colored rectangle.
#[cfg(feature = "gpui")]
fn paint_rect(rect: &PaintRect, window: &mut Window) {
    window.paint_quad(gpui::PaintQuad {
        bounds: Bounds {
            origin: rect.origin,
            size: rect.size,
        },
        corner_radii: gpui::Corners::default(),
        background: rgba_to_hsla(rect.color).into(),
        border_widths: gpui::Edges::default(),
        border_color: Hsla::default(),
        border_style: gpui::BorderStyle::default(),
    });
}

// ─── Color Conversion ───────────────────────────────────────────

/// Convert Rgba to Hsla for GPUI APIs that require it.
#[cfg(feature = "gpui")]
fn rgba_to_hsla(c: Rgba) -> Hsla {
    c.into()
}

/// Convert alacritty color to Rgba using the active theme.
#[cfg(feature = "gpui")]
fn convert_color(
    color: &alacritty_terminal::vte::ansi::Color,
    default: &Rgba,
    is_fg: bool,
    dim: bool,
    theme: &TerminalTheme,
) -> Rgba {
    use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};

    let base = match color {
        AnsiColor::Named(name) => match name {
            NamedColor::Black => rgb(theme.ansi[0]),
            NamedColor::Red => rgb(theme.ansi[1]),
            NamedColor::Green => rgb(theme.ansi[2]),
            NamedColor::Yellow => rgb(theme.ansi[3]),
            NamedColor::Blue => rgb(theme.ansi[4]),
            NamedColor::Magenta => rgb(theme.ansi[5]),
            NamedColor::Cyan => rgb(theme.ansi[6]),
            NamedColor::White => rgb(theme.ansi[7]),
            NamedColor::BrightBlack => rgb(theme.ansi[8]),
            NamedColor::BrightRed => rgb(theme.ansi[9]),
            NamedColor::BrightGreen => rgb(theme.ansi[10]),
            NamedColor::BrightYellow => rgb(theme.ansi[11]),
            NamedColor::BrightBlue => rgb(theme.ansi[12]),
            NamedColor::BrightMagenta => rgb(theme.ansi[13]),
            NamedColor::BrightCyan => rgb(theme.ansi[14]),
            NamedColor::BrightWhite => rgb(theme.ansi[15]),
            NamedColor::Foreground => rgb(theme.fg),
            NamedColor::Background => rgb(theme.bg),
            NamedColor::Cursor => rgb(theme.cursor),
            NamedColor::BrightForeground => rgb(theme.ansi[15]),
            NamedColor::DimForeground => rgb(theme.dim[7]),
            NamedColor::DimBlack => rgb(theme.dim[0]),
            NamedColor::DimRed => rgb(theme.dim[1]),
            NamedColor::DimGreen => rgb(theme.dim[2]),
            NamedColor::DimYellow => rgb(theme.dim[3]),
            NamedColor::DimBlue => rgb(theme.dim[4]),
            NamedColor::DimMagenta => rgb(theme.dim[5]),
            NamedColor::DimCyan => rgb(theme.dim[6]),
            NamedColor::DimWhite => rgb(theme.dim[7]),
            _ => *default,
        },
        AnsiColor::Spec(rgb_color) => Rgba {
            r: rgb_color.r as f32 / 255.0,
            g: rgb_color.g as f32 / 255.0,
            b: rgb_color.b as f32 / 255.0,
            a: 1.0,
        },
        AnsiColor::Indexed(idx) => indexed_to_rgba(*idx, theme),
    };

    if dim && is_fg {
        // DIM attribute: reduce foreground brightness
        Rgba {
            r: base.r * 0.5,
            g: base.g * 0.5,
            b: base.b * 0.5,
            a: base.a,
        }
    } else if !is_fg && !matches!(color,
        AnsiColor::Named(NamedColor::Background | NamedColor::Foreground | NamedColor::Cursor)
    ) && base != *default {
        // Non-default ANSI background colors: dim to ~40% brightness so they
        // don't overpower foreground text. Common trigger: `ls` in WSL where
        // other-writable dirs get bright green/yellow backgrounds via LS_COLORS.
        let bg = rgb(theme.bg);
        let blend = 0.35_f32;
        Rgba {
            r: bg.r + (base.r - bg.r) * blend,
            g: bg.g + (base.g - bg.g) * blend,
            b: bg.b + (base.b - bg.b) * blend,
            a: base.a,
        }
    } else {
        base
    }
}

/// Convert 256-color index to Rgba using theme for indices 0-15.
#[cfg(feature = "gpui")]
fn indexed_to_rgba(idx: u8, theme: &TerminalTheme) -> Rgba {
    if idx < 16 {
        rgb(theme.ansi[idx as usize])
    } else if idx < 232 {
        let idx = idx - 16;
        let r = (idx / 36) % 6;
        let g = (idx / 6) % 6;
        let b = idx % 6;
        let to_val = |v: u8| if v == 0 { 0u8 } else { 55 + v * 40 };
        Rgba {
            r: to_val(r) as f32 / 255.0,
            g: to_val(g) as f32 / 255.0,
            b: to_val(b) as f32 / 255.0,
            a: 1.0,
        }
    } else {
        let v = 8 + (idx - 232) * 10;
        Rgba {
            r: v as f32 / 255.0,
            g: v as f32 / 255.0,
            b: v as f32 / 255.0,
            a: 1.0,
        }
    }
}

// ─── Special Character Rendering ────────────────────────────────

/// Push underline rectangles for a span of underlined text.
/// Renders Single, Double, Dotted, and Dashed as quads. Curly uses GPUI wavy fallback.
#[cfg(feature = "gpui")]
fn push_underline(
    kind: UnderlineKind,
    color: Rgba,
    x: Pixels,
    y: Pixels,
    w: f32,
    rects: &mut Vec<PaintRect>,
) {
    match kind {
        UnderlineKind::None => {}
        UnderlineKind::Single | UnderlineKind::Curly => {
            // Single line (curly would need wavy path — approximate as single for now)
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px(1.0)),
                color,
            });
        }
        UnderlineKind::Double => {
            // Two parallel lines, 2px apart
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px(1.0)),
                color,
            });
            rects.push(PaintRect {
                origin: point(x, y + px(3.0)),
                size: size(px(w), px(1.0)),
                color,
            });
        }
        UnderlineKind::Dotted => {
            // Dots: 1px on, 2px off
            let mut dx = 0.0_f32;
            while dx < w {
                let dot_w = 1.0_f32.min(w - dx);
                rects.push(PaintRect {
                    origin: point(x + px(dx), y),
                    size: size(px(dot_w), px(1.0)),
                    color,
                });
                dx += 3.0;
            }
        }
        UnderlineKind::Dashed => {
            // Dashes: 4px on, 2px off
            let mut dx = 0.0_f32;
            while dx < w {
                let dash_w = 4.0_f32.min(w - dx);
                rects.push(PaintRect {
                    origin: point(x + px(dx), y),
                    size: size(px(dash_w), px(1.0)),
                    color,
                });
                dx += 6.0;
            }
        }
    }
}

/// Check if a character should be rendered as geometric quads instead of text.
fn is_special_render_char(ch: char) -> bool {
    matches!(ch, '\u{2500}'..='\u{256C}' | '\u{2580}'..='\u{259F}')
}

/// Push paint rectangles for block drawing characters (U+2580–U+2593).
/// These are rendered as colored quads for pixel-perfect alignment.
#[cfg(feature = "gpui")]
fn push_special_char(
    ch: char,
    fg: Rgba,
    bg: Rgba,
    x: Pixels,
    y: Pixels,
    w: f32,
    h: f32,
    rects: &mut Vec<PaintRect>,
) {
    match ch {
        // Block characters (U+2580–U+2593)
        '\u{2588}' => {
            // █ Full block
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px(h)),
                color: fg,
            });
        }
        '\u{2580}' => {
            // ▀ Upper half
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px(h)),
                color: bg,
            });
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px((h / 2.0).ceil())),
                color: fg,
            });
        }
        '\u{2584}' => {
            // ▄ Lower half
            let half = (h / 2.0).ceil();
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px(h)),
                color: bg,
            });
            rects.push(PaintRect {
                origin: point(x, y + px(h - half)),
                size: size(px(w), px(half)),
                color: fg,
            });
        }
        '\u{258C}' => {
            // ▌ Left half
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px(h)),
                color: bg,
            });
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w * 0.5), px(h)),
                color: fg,
            });
        }
        '\u{2590}' => {
            // ▐ Right half
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px(h)),
                color: bg,
            });
            rects.push(PaintRect {
                origin: point(x + px(w * 0.5), y),
                size: size(px(w * 0.5), px(h)),
                color: fg,
            });
        }
        // Fractional blocks (lower)
        '\u{2581}' => push_lower_block(x, y, w, h, 0.125, fg, bg, rects),
        '\u{2582}' => push_lower_block(x, y, w, h, 0.25, fg, bg, rects),
        '\u{2583}' => push_lower_block(x, y, w, h, 0.375, fg, bg, rects),
        '\u{2585}' => push_lower_block(x, y, w, h, 0.625, fg, bg, rects),
        '\u{2586}' => push_lower_block(x, y, w, h, 0.75, fg, bg, rects),
        '\u{2587}' => push_lower_block(x, y, w, h, 0.875, fg, bg, rects),
        // Fractional blocks (left)
        '\u{2589}' => push_left_block(x, y, w, h, 0.875, fg, bg, rects),
        '\u{258A}' => push_left_block(x, y, w, h, 0.75, fg, bg, rects),
        '\u{258B}' => push_left_block(x, y, w, h, 0.625, fg, bg, rects),
        '\u{258D}' => push_left_block(x, y, w, h, 0.375, fg, bg, rects),
        '\u{258E}' => push_left_block(x, y, w, h, 0.25, fg, bg, rects),
        '\u{258F}' => push_left_block(x, y, w, h, 0.125, fg, bg, rects),
        // Shade characters
        '\u{2591}' => {
            let shade = blend_rgba(fg, bg, 0.25);
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px(h)),
                color: shade,
            });
        }
        '\u{2592}' => {
            let shade = blend_rgba(fg, bg, 0.5);
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px(h)),
                color: shade,
            });
        }
        '\u{2593}' => {
            let shade = blend_rgba(fg, bg, 0.75);
            rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px(h)),
                color: shade,
            });
        }
        // ▔ Upper one eighth block
        '\u{2594}' => {
            rects.push(PaintRect { origin: point(x, y), size: size(px(w), px(h)), color: bg });
            rects.push(PaintRect { origin: point(x, y), size: size(px(w), px(h * 0.125)), color: fg });
        }
        // ▕ Right one eighth block
        '\u{2595}' => {
            rects.push(PaintRect { origin: point(x, y), size: size(px(w), px(h)), color: bg });
            rects.push(PaintRect { origin: point(x + px(w * 0.875), y), size: size(px(w * 0.125), px(h)), color: fg });
        }
        // Quadrant blocks (U+2596–U+259F)
        // Each quadrant occupies one quarter of the cell: TL, TR, BL, BR
        ch @ '\u{2596}'..='\u{259F}' => {
            rects.push(PaintRect { origin: point(x, y), size: size(px(w), px(h)), color: bg });
            let hw = w * 0.5;
            let hh = (h * 0.5).ceil();
            // Bits: UL=0b0100, UR=0b1000, BL=0b0001, BR=0b0010
            let bits: u8 = match ch {
                '\u{2596}' => 0b0001, // ▖ Lower left
                '\u{2597}' => 0b0010, // ▗ Lower right
                '\u{2598}' => 0b0100, // ▘ Upper left
                '\u{2599}' => 0b0111, // ▙ UL + BL + BR
                '\u{259A}' => 0b0110, // ▚ UL + BR
                '\u{259B}' => 0b1101, // ▛ UL + UR + BL
                '\u{259C}' => 0b1110, // ▜ UL + UR + BR
                '\u{259D}' => 0b1000, // ▝ Upper right
                '\u{259E}' => 0b1001, // ▞ UR + BL
                '\u{259F}' => 0b1011, // ▟ UR + BL + BR
                _ => 0,
            };
            if bits & 0b0100 != 0 { // Upper left
                rects.push(PaintRect { origin: point(x, y), size: size(px(hw), px(hh)), color: fg });
            }
            if bits & 0b1000 != 0 { // Upper right
                rects.push(PaintRect { origin: point(x + px(hw), y), size: size(px(hw), px(hh)), color: fg });
            }
            if bits & 0b0001 != 0 { // Lower left
                rects.push(PaintRect { origin: point(x, y + px(h - hh)), size: size(px(hw), px(hh)), color: fg });
            }
            if bits & 0b0010 != 0 { // Lower right
                rects.push(PaintRect { origin: point(x + px(hw), y + px(h - hh)), size: size(px(hw), px(hh)), color: fg });
            }
        }
        // Box drawing characters (U+2500–U+256C)
        ch if ch >= '\u{2500}' && ch <= '\u{256C}' => {
            push_box_drawing(ch, fg, bg, x, y, w, h, rects);
        }
        _ => {}
    }
}

/// Push a lower fractional block (▁▂▃▅▆▇).
#[cfg(feature = "gpui")]
fn push_lower_block(
    x: Pixels,
    y: Pixels,
    w: f32,
    h: f32,
    frac: f32,
    fg: Rgba,
    bg: Rgba,
    rects: &mut Vec<PaintRect>,
) {
    rects.push(PaintRect {
        origin: point(x, y),
        size: size(px(w), px(h)),
        color: bg,
    });
    let block_h = h * frac;
    rects.push(PaintRect {
        origin: point(x, y + px(h - block_h)),
        size: size(px(w), px(block_h)),
        color: fg,
    });
}

/// Push a left fractional block (▉▊▋▍▎▏).
#[cfg(feature = "gpui")]
fn push_left_block(
    x: Pixels,
    y: Pixels,
    w: f32,
    h: f32,
    frac: f32,
    fg: Rgba,
    bg: Rgba,
    rects: &mut Vec<PaintRect>,
) {
    rects.push(PaintRect {
        origin: point(x, y),
        size: size(px(w), px(h)),
        color: bg,
    });
    rects.push(PaintRect {
        origin: point(x, y),
        size: size(px(w * frac), px(h)),
        color: fg,
    });
}

/// Blend two colors: result = fg * t + bg * (1 - t).
fn blend_rgba(fg: Rgba, bg: Rgba, t: f32) -> Rgba {
    Rgba {
        r: fg.r * t + bg.r * (1.0 - t),
        g: fg.g * t + bg.g * (1.0 - t),
        b: fg.b * t + bg.b * (1.0 - t),
        a: 1.0,
    }
}

// ─── Box Drawing ────────────────────────────────────────────────

/// Push paint rectangles for box-drawing characters (U+2500–U+256C).
/// Each box-drawing char is decomposed into horizontal and vertical line segments.
#[cfg(feature = "gpui")]
fn push_box_drawing(
    ch: char,
    fg: Rgba,
    _bg: Rgba,
    x: Pixels,
    y: Pixels,
    w: f32,
    h: f32,
    rects: &mut Vec<PaintRect>,
) {
    let thin = 1.0_f32;
    let thick = 2.0_f32;
    let cx = w / 2.0;
    let cy = h / 2.0;

    let (left, right, up, down) = box_segments(ch);
    let line_w = |heavy: bool| if heavy { thick } else { thin };

    // Horizontal segment
    if left || right {
        let lw = line_w(is_heavy_h(ch));
        let x_start = if left { 0.0 } else { cx };
        let x_end = if right { w } else { cx + lw };
        rects.push(PaintRect {
            origin: point(x + px(x_start), y + px(cy - lw / 2.0)),
            size: size(px(x_end - x_start), px(lw)),
            color: fg,
        });
    }

    // Vertical segment
    if up || down {
        let lw = line_w(is_heavy_v(ch));
        let y_start = if up { 0.0 } else { cy };
        let y_end = if down { h } else { cy + lw };
        rects.push(PaintRect {
            origin: point(x + px(cx - lw / 2.0), y + px(y_start)),
            size: size(px(lw), px(y_end - y_start)),
            color: fg,
        });
    }
}

/// Determine which segments a box-drawing character has (left, right, up, down).
fn box_segments(ch: char) -> (bool, bool, bool, bool) {
    match ch {
        '\u{2500}' | '\u{2501}' | '\u{2550}' => (true, true, false, false),
        '\u{2502}' | '\u{2503}' | '\u{2551}' => (false, false, true, true),
        '\u{250C}' | '\u{250D}' | '\u{250E}' | '\u{250F}' | '\u{2552}' | '\u{2553}'
        | '\u{2554}' => (false, true, false, true),
        '\u{2510}' | '\u{2511}' | '\u{2512}' | '\u{2513}' | '\u{2555}' | '\u{2556}'
        | '\u{2557}' => (true, false, false, true),
        '\u{2514}' | '\u{2515}' | '\u{2516}' | '\u{2517}' | '\u{2558}' | '\u{2559}'
        | '\u{255A}' => (false, true, true, false),
        '\u{2518}' | '\u{2519}' | '\u{251A}' | '\u{251B}' | '\u{255B}' | '\u{255C}'
        | '\u{255D}' => (true, false, true, false),
        '\u{251C}' | '\u{251D}' | '\u{251E}' | '\u{251F}' | '\u{2520}' | '\u{2521}'
        | '\u{2522}' | '\u{2523}' | '\u{255E}' | '\u{255F}' | '\u{2560}' => {
            (false, true, true, true)
        }
        '\u{2524}' | '\u{2525}' | '\u{2526}' | '\u{2527}' | '\u{2528}' | '\u{2529}'
        | '\u{252A}' | '\u{252B}' | '\u{2561}' | '\u{2562}' | '\u{2563}' => {
            (true, false, true, true)
        }
        '\u{252C}' | '\u{252D}' | '\u{252E}' | '\u{252F}' | '\u{2530}' | '\u{2531}'
        | '\u{2532}' | '\u{2533}' | '\u{2564}' | '\u{2565}' | '\u{2566}' => {
            (true, true, false, true)
        }
        '\u{2534}' | '\u{2535}' | '\u{2536}' | '\u{2537}' | '\u{2538}' | '\u{2539}'
        | '\u{253A}' | '\u{253B}' | '\u{2567}' | '\u{2568}' | '\u{2569}' => {
            (true, true, true, false)
        }
        '\u{253C}' | '\u{253D}' | '\u{253E}' | '\u{253F}' | '\u{2540}' | '\u{2541}'
        | '\u{2542}' | '\u{2543}' | '\u{2544}' | '\u{2545}' | '\u{2546}' | '\u{2547}'
        | '\u{2548}' | '\u{2549}' | '\u{254A}' | '\u{254B}' | '\u{256A}' | '\u{256B}'
        | '\u{256C}' => (true, true, true, true),
        _ => (false, false, false, false),
    }
}

/// Check if a box character uses heavy/thick horizontal lines.
fn is_heavy_h(ch: char) -> bool {
    matches!(
        ch,
        '\u{2501}'
            | '\u{2503}'
            | '\u{250D}'
            | '\u{250F}'
            | '\u{2511}'
            | '\u{2513}'
            | '\u{2515}'
            | '\u{2517}'
            | '\u{2519}'
            | '\u{251B}'
            | '\u{251D}'
            | '\u{2523}'
            | '\u{2525}'
            | '\u{252B}'
            | '\u{252F}'
            | '\u{2533}'
            | '\u{2537}'
            | '\u{253B}'
            | '\u{253F}'
            | '\u{254B}'
            | '\u{2550}'
    )
}

/// Check if a box character uses heavy/thick vertical lines.
fn is_heavy_v(ch: char) -> bool {
    matches!(
        ch,
        '\u{2503}'
            | '\u{250E}'
            | '\u{250F}'
            | '\u{2512}'
            | '\u{2513}'
            | '\u{2516}'
            | '\u{2517}'
            | '\u{251A}'
            | '\u{251B}'
            | '\u{251F}'
            | '\u{2520}'
            | '\u{2523}'
            | '\u{2526}'
            | '\u{2528}'
            | '\u{252B}'
            | '\u{2530}'
            | '\u{2531}'
            | '\u{2533}'
            | '\u{2538}'
            | '\u{253A}'
            | '\u{253B}'
            | '\u{2540}'
            | '\u{2541}'
            | '\u{2542}'
            | '\u{254B}'
            | '\u{2551}'
    )
}
