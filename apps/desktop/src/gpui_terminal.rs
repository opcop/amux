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

// ─── Font Configuration ─────────────────────────────────────────

/// Font family for terminal rendering
pub const FONT_FAMILY: &str = "Cascadia Code";
/// Font size in pixels
pub const FONT_SIZE: f32 = 14.0;

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
pub fn measure_cell_metrics(window: &mut Window) -> CellMetrics {
    let text_system = window.text_system();
    let font_size = px(FONT_SIZE);
    let font = make_font(false);

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
        // Line height = font_size × 1.4, ceil to avoid sub-pixel gaps
        height: (FONT_SIZE * 1.4).ceil(),
        descent: descent.as_f32(),
    }
}

/// Construct a terminal Font with optional bold/italic.
#[cfg(feature = "gpui")]
fn make_font_styled(bold: bool, italic: bool) -> Font {
    Font {
        family: SharedString::from(FONT_FAMILY),
        weight: if bold { FontWeight::BOLD } else { FontWeight::NORMAL },
        style: if italic { FontStyle::Italic } else { FontStyle::Normal },
        ..Default::default()
    }
}

#[cfg(feature = "gpui")]
fn make_font(bold: bool) -> Font {
    make_font_styled(bold, false)
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
) -> impl IntoElement {
    let mut data = collect_render_data(term, cursor_blink_on);
    // Only override cursor shape for plain shell (block → beam for active pane).
    // TUI apps (Claude Code, vim, etc.) set their own cursor shape via CSI —
    // if the app set beam or underline, respect it; only override the default block.
    if data.cursor_visible && data.cursor_shape == 0 {
        // Default block cursor → beam for active pane, keep block for inactive
        if is_active_pane {
            data.cursor_shape = 1; // beam
        }
    }
    let m = metrics.clone();

    let total_w = data.cols as f32 * metrics.width;
    let total_h = data.rows as f32 * metrics.height;

    canvas(
        move |bounds, window, _cx| prepaint_terminal(data, bounds, &m, window),
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
}

#[cfg(feature = "gpui")]
#[derive(Clone)]
struct RenderCell {
    ch: char,
    fg: Rgba,
    bg: Rgba,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    wide_continuation: bool,
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
            underline: false,
            strikethrough: false,
            wide_continuation: false,
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
) -> RenderData {
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::term::cell::Flags as CellFlags;

    term.with_term(|t| {
        let content = t.renderable_content();
        let cols = t.columns();
        let rows = t.screen_lines();
        let cursor = content.cursor;
        let display_offset = t.grid().display_offset();

        let default_fg = rgb(0xc5c8c6);
        let default_bg = rgb(0x1d1f21);
        let cursor_color = rgb(0xf5f5f5);
        let cursor_text_color = rgb(0x1d1f21);

        let mut grid: Vec<Vec<RenderCell>> = vec![vec![RenderCell::default(); cols]; rows];

        for indexed in content.display_iter {
            let point = indexed.point;
            let line_i32 = point.line.0;
            if line_i32 < 0 {
                continue;
            }
            let row = line_i32 as usize;
            let col = point.column.0;
            if row < rows && col < cols {
                let cell = &indexed.cell;
                let flags = cell.flags;
                let fg =
                    convert_color(&cell.fg, &default_fg, true, flags.contains(CellFlags::DIM));
                let bg = convert_color(&cell.bg, &default_bg, false, false);

                grid[row][col] = RenderCell {
                    ch: cell.c,
                    fg,
                    bg,
                    bold: flags.contains(CellFlags::BOLD),
                    italic: flags.contains(CellFlags::ITALIC),
                    underline: flags.intersects(
                        CellFlags::UNDERLINE | CellFlags::DOUBLE_UNDERLINE
                        | CellFlags::UNDERCURL | CellFlags::DOTTED_UNDERLINE
                        | CellFlags::DASHED_UNDERLINE
                    ),
                    strikethrough: flags.contains(CellFlags::STRIKEOUT),
                    wide_continuation: flags.contains(CellFlags::WIDE_CHAR_SPACER),
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
            selection_ranges,
        }
    })
}

// ─── Prepaint Phase ─────────────────────────────────────────────

/// Shape text and collect paint operations.
/// Runs during GPUI's prepaint phase (CPU-only work).
#[cfg(feature = "gpui")]
fn prepaint_terminal(
    mut data: RenderData,
    bounds: Bounds<Pixels>,
    metrics: &CellMetrics,
    window: &mut Window,
) -> PrepaintData {
    let text_system = window.text_system();
    let font_size = px(FONT_SIZE);
    let cell_w = metrics.width;
    let cell_h = metrics.height;
    let line_height = px(cell_h);

    let mut bg_rects = Vec::with_capacity(data.rows * 4);
    let mut special_rects = Vec::with_capacity(64);
    let mut selection_rects = Vec::new();
    let mut text_lines = Vec::with_capacity(data.rows * 8);
    let mut cursor_rects = Vec::new();

    // Build selection highlight rects
    let selection_bg = Rgba { r: 0.2, g: 0.35, b: 0.6, a: 1.0 }; // blue highlight
    for &(row, c_start, c_end) in &data.selection_ranges {
        let x = bounds.origin.x + px(c_start as f32 * cell_w);
        let y = bounds.origin.y + px(row as f32 * cell_h);
        let w = ((c_end + 1).saturating_sub(c_start)) as f32 * cell_w;
        selection_rects.push(PaintRect {
            origin: point(x, y),
            size: size(px(w), px(cell_h)),
            color: selection_bg,
        });
    }

    // Apply block cursor colors directly to the grid cell.
    // For wide (CJK) characters, also color the continuation cell so the
    // cursor background spans the full 2-cell width of the character.
    if data.cursor_visible
        && data.cursor_shape == 0
        && data.cursor_row < data.rows
        && data.cursor_col < data.cols
    {
        let cell = &mut data.grid[data.cursor_row][data.cursor_col];
        cell.fg = data.cursor_text_color;
        cell.bg = data.cursor_color;
        // If this is a wide char, extend cursor bg to the continuation cell
        if data.cursor_col + 1 < data.cols
            && data.grid[data.cursor_row][data.cursor_col + 1].wide_continuation
        {
            data.grid[data.cursor_row][data.cursor_col + 1].bg = data.cursor_color;
        }
    }

    for row in 0..data.rows {
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
            let x = bounds.origin.x + px(start_col as f32 * cell_w);
            let w = (col - start_col) as f32 * cell_w;
            bg_rects.push(PaintRect {
                origin: point(x, y),
                size: size(px(w), px(cell_h)),
                color: bg,
            });
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
                let x = bounds.origin.x + px(col as f32 * cell_w);
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
            // Each run has uniform (fg, bold, italic, underline, strikethrough).
            // Wide (CJK) chars are shaped individually at exact grid positions.
            let fg = cell.fg;
            let bold = cell.bold;
            let italic = cell.italic;
            let underline = cell.underline;
            let strikethrough = cell.strikethrough;
            let mut narrow_start = col;
            let mut narrow_text = String::new();
            let mut has_visible = false; // track if run has non-space chars

            // Helper: build TextRun with current style
            let build_run = |text_len: usize, fg: Rgba, bold: bool, italic: bool, underline: bool, strikethrough: bool| -> gpui::TextRun {
                let fg_hsla = rgba_to_hsla(fg);
                gpui::TextRun {
                    len: text_len,
                    font: make_font_styled(bold, italic),
                    color: fg_hsla,
                    background_color: None,
                    underline: if underline {
                        Some(gpui::UnderlineStyle { thickness: px(1.0), color: Some(fg_hsla), wavy: false })
                    } else { None },
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
                {
                    break;
                }
                if is_special_render_char(c.ch) {
                    break;
                }

                let is_wide = col + 1 < data.cols
                    && data.grid[row][col + 1].wide_continuation;

                if is_wide {
                    // Flush pending narrow run
                    if has_visible {
                        let run = build_run(narrow_text.len(), fg, bold, italic, underline, strikethrough);
                        let shaped = text_system.shape_line(
                            SharedString::from(narrow_text.clone()), font_size, &[run], None,
                        );
                        let x = bounds.origin.x + px(narrow_start as f32 * cell_w);
                        text_lines.push(PaintText { origin: point(x, y), shaped });
                    }
                    narrow_text.clear();
                    has_visible = false;

                    // Shape wide char individually at exact grid position
                    let ch = if c.ch == '\0' { ' ' } else { c.ch };
                    if ch != ' ' {
                        let ch_str = ch.to_string();
                        let run = build_run(ch_str.len(), fg, bold, italic, underline, strikethrough);
                        let shaped = text_system.shape_line(
                            SharedString::from(ch_str), font_size, &[run], None,
                        );
                        let x = bounds.origin.x + px(col as f32 * cell_w);
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

            // Flush remaining narrow run
            if has_visible {
                let run = build_run(narrow_text.len(), fg, bold, italic, underline, strikethrough);
                let shaped = text_system.shape_line(
                    SharedString::from(narrow_text), font_size, &[run], None,
                );
                let x = bounds.origin.x + px(narrow_start as f32 * cell_w);
                text_lines.push(PaintText { origin: point(x, y), shaped });
            }
        }
    }

    // ── Phase 3: Cursor overlay (beam/underline) ──
    if data.cursor_visible && data.cursor_shape > 0 {
        let cx = bounds.origin.x + px(data.cursor_col as f32 * cell_w);
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

    PrepaintData {
        bg_rects,
        special_rects,
        selection_rects,
        text_lines,
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

    // Layer 4: Cursor overlay
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

/// Convert alacritty color to Rgba. Tomorrow Night color scheme.
#[cfg(feature = "gpui")]
fn convert_color(
    color: &alacritty_terminal::vte::ansi::Color,
    default: &Rgba,
    is_fg: bool,
    dim: bool,
) -> Rgba {
    use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};

    let base = match color {
        AnsiColor::Named(name) => match name {
            NamedColor::Black => rgb(0x1d1f21),
            NamedColor::Red => rgb(0xcc6666),
            NamedColor::Green => rgb(0xb5bd68),
            NamedColor::Yellow => rgb(0xf0c674),
            NamedColor::Blue => rgb(0x81a2be),
            NamedColor::Magenta => rgb(0xb294bb),
            NamedColor::Cyan => rgb(0x8abeb7),
            NamedColor::White => rgb(0xc5c8c6),
            NamedColor::BrightBlack => rgb(0x969896),
            NamedColor::BrightRed => rgb(0xcc6666),
            NamedColor::BrightGreen => rgb(0xb5bd68),
            NamedColor::BrightYellow => rgb(0xf0c674),
            NamedColor::BrightBlue => rgb(0x81a2be),
            NamedColor::BrightMagenta => rgb(0xb294bb),
            NamedColor::BrightCyan => rgb(0x8abeb7),
            NamedColor::BrightWhite => rgb(0xffffff),
            NamedColor::Foreground => rgb(0xc5c8c6),
            NamedColor::Background => rgb(0x1d1f21),
            NamedColor::Cursor => rgb(0xc5c8c6),
            NamedColor::BrightForeground => rgb(0xeaeaea),
            NamedColor::DimForeground => rgb(0x828482),
            NamedColor::DimBlack => rgb(0x131515),
            NamedColor::DimRed => rgb(0x864343),
            NamedColor::DimGreen => rgb(0x777e45),
            NamedColor::DimYellow => rgb(0x9f834d),
            NamedColor::DimBlue => rgb(0x556b7e),
            NamedColor::DimMagenta => rgb(0x75627c),
            NamedColor::DimCyan => rgb(0x5c7e7a),
            NamedColor::DimWhite => rgb(0x828482),
            _ => *default,
        },
        AnsiColor::Spec(rgb_color) => Rgba {
            r: rgb_color.r as f32 / 255.0,
            g: rgb_color.g as f32 / 255.0,
            b: rgb_color.b as f32 / 255.0,
            a: 1.0,
        },
        AnsiColor::Indexed(idx) => indexed_to_rgba(*idx),
    };

    if dim && is_fg {
        Rgba {
            r: base.r * 0.5,
            g: base.g * 0.5,
            b: base.b * 0.5,
            a: base.a,
        }
    } else {
        base
    }
}

/// Convert 256-color index to Rgba.
#[cfg(feature = "gpui")]
fn indexed_to_rgba(idx: u8) -> Rgba {
    if idx < 16 {
        let colors: [u32; 16] = [
            0x1d1f21, 0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xc5c8c6,
            0x969896, 0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xffffff,
        ];
        rgb(colors[idx as usize])
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

/// Check if a character should be rendered as geometric quads instead of text.
fn is_special_render_char(ch: char) -> bool {
    matches!(ch, '\u{2500}'..='\u{256C}' | '\u{2580}'..='\u{2593}')
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
