//! GPUI Terminal Renderer — Alacritty backend
//!
//! Renders terminal content from alacritty_terminal::Term using GPUI elements.

#[cfg(feature = "gpui")]
use gpui::{
    rgb, px, div, AnyElement, IntoElement, ParentElement, Styled,
};
#[cfg(feature = "gpui")]
use gpui::Rgba;

/// Character cell dimensions (in pixels)
/// CELL_WIDTH should match the monospace font's actual glyph advance width.
/// For Cascadia Code / Consolas at text_sm (14px), ~8.0px is typical.
pub const CELL_WIDTH: f32 = 7.2;
pub const CELL_HEIGHT: f32 = 20.0;

/// Render a terminal from an AlacrittyTerminal
#[cfg(feature = "gpui")]
pub fn render_alacritty_terminal(
    term: &amux_platform::terminal::alacritty_view::AlacrittyTerminal,
    cursor_blink_on: bool,
) -> impl IntoElement {
    use alacritty_terminal::term::cell::Flags as CellFlags;
    use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
    use alacritty_terminal::grid::Dimensions;

    // Lock the term for reading
    let term_guard = term.with_term(|t| {
        let content = t.renderable_content();
        let cols = t.columns();
        let rows = t.screen_lines();
        let cursor = content.cursor;
        let display_offset = t.grid().display_offset();

        // Catppuccin Mocha colors
        let default_fg = rgb(0xcdd6f4);
        // Use the terminal's actual background (often black for TUI apps)
        let default_bg = rgb(0x1d1f21);
        let cursor_color = rgb(0xf5f5f5);
        let cursor_text_color = rgb(0x1d1f21);

        // Collect all cells into a grid of render data
        let mut grid: Vec<Vec<RenderCell>> = vec![vec![RenderCell::default(); cols]; rows];

        for indexed in content.display_iter {
            let point = indexed.point;
            let line_i32 = point.line.0;
            if line_i32 < 0 { continue; }
            let row = line_i32 as usize;
            let col = point.column.0;
            if row < rows && col < cols {
                let cell = &indexed.cell;
                let flags = cell.flags;

                let fg = convert_color(&cell.fg, &default_fg, true, flags.contains(CellFlags::DIM));
                let bg = convert_color(&cell.bg, &default_bg, false, false);

                grid[row][col] = RenderCell {
                    ch: cell.c,
                    fg,
                    bg,
                    bold: flags.contains(CellFlags::BOLD),
                    dim: flags.contains(CellFlags::DIM),
                    wide_continuation: flags.contains(CellFlags::WIDE_CHAR_SPACER),
                };
            }
        }

        // Cursor info
        let cursor_row = cursor.point.line.0 as usize;
        let cursor_col = cursor.point.column.0;
        let cursor_hidden = matches!(cursor.shape, alacritty_terminal::vte::ansi::CursorShape::Hidden);
        let cursor_visible = !cursor_hidden && cursor_blink_on && display_offset == 0;
        let cursor_shape = match cursor.shape {
            alacritty_terminal::vte::ansi::CursorShape::Block => 0u8,
            alacritty_terminal::vte::ansi::CursorShape::Beam => 1,
            alacritty_terminal::vte::ansi::CursorShape::Underline => 2,
            _ => 0,
        };

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
        }
    });

    render_grid(term_guard)
}

#[cfg(feature = "gpui")]
struct RenderData {
    grid: Vec<Vec<RenderCell>>,
    rows: usize,
    cols: usize,
    cursor_row: usize,
    cursor_col: usize,
    cursor_visible: bool,
    cursor_shape: u8,
    cursor_color: Rgba,
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
    dim: bool,
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
            dim: false,
            wide_continuation: false,
        }
    }
}

#[cfg(feature = "gpui")]
fn convert_color(color: &alacritty_terminal::vte::ansi::Color, default: &Rgba, is_fg: bool, dim: bool) -> Rgba {
    use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};

    let base = match color {
        AnsiColor::Named(name) => match name {
            // Alacritty default theme (Tomorrow Night)
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
        AnsiColor::Spec(rgb_color) => {
            Rgba { r: rgb_color.r as f32 / 255.0, g: rgb_color.g as f32 / 255.0, b: rgb_color.b as f32 / 255.0, a: 1.0 }
        }
        AnsiColor::Indexed(idx) => indexed_to_rgba(*idx),
    };

    if dim && is_fg {
        Rgba { r: base.r * 0.5, g: base.g * 0.5, b: base.b * 0.5, a: base.a }
    } else {
        base
    }
}

#[cfg(feature = "gpui")]
fn indexed_to_rgba(idx: u8) -> Rgba {
    if idx < 16 {
        // Alacritty default (Tomorrow Night)
        let colors: [u32; 16] = [
            0x1d1f21, 0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xc5c8c6,
            0x969896, 0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xffffff,
        ];
        rgb(colors[idx as usize])
    } else if idx < 232 {
        // 216 color cube
        let idx = idx - 16;
        let r = (idx / 36) % 6;
        let g = (idx / 6) % 6;
        let b = idx % 6;
        let to_val = |v: u8| if v == 0 { 0u8 } else { 55 + v * 40 };
        Rgba { r: to_val(r) as f32 / 255.0, g: to_val(g) as f32 / 255.0, b: to_val(b) as f32 / 255.0, a: 1.0 }
    } else {
        // Grayscale
        let v = 8 + (idx - 232) * 10;
        Rgba { r: v as f32 / 255.0, g: v as f32 / 255.0, b: v as f32 / 255.0, a: 1.0 }
    }
}

#[cfg(feature = "gpui")]
fn render_grid(data: RenderData) -> impl IntoElement {
    let total_h = data.rows as f32 * CELL_HEIGHT;
    let total_w = data.cols as f32 * CELL_WIDTH;
    div()
        .bg(data.default_bg)
        .w(px(total_w))
        .h(px(total_h))
        .flex_1()
        .overflow_hidden()
        .font_family("Cascadia Code, Consolas, DejaVu Sans Mono, monospace".to_string())
        .text_sm()
        .line_height(px(CELL_HEIGHT))
        .children((0..data.rows).map(|y| {
            let row = &data.grid[y];

            let mut spans: Vec<AnyElement> = Vec::new();
            let mut run_text = String::new();
            let mut run_char_count: usize = 0; // count in cell units (wide=2)
            let mut run_fg = data.default_bg;
            let mut run_bg = data.default_bg;
            let mut run_bold = false;
            let mut first = true;

            let flush = |spans: &mut Vec<AnyElement>, text: &str, char_count: usize, fg: Rgba, bg: Rgba, bold: bool| {
                if text.is_empty() { return; }
                let w = char_count as f32 * CELL_WIDTH;
                let mut d = div()
                    .w(px(w))
                    .flex_shrink_0()
                    .text_color(fg)
                    .bg(bg);
                if bold { d = d.font_weight(gpui::FontWeight::BOLD); }
                spans.push(d.child(text.to_string()).into_any_element());
            };

            for x in 0..data.cols {
                let cell = &row[x];
                if cell.wide_continuation {
                    continue;
                }

                let is_block_cursor = data.cursor_visible && data.cursor_shape == 0 && y == data.cursor_row && x == data.cursor_col;

                let fg = if is_block_cursor { data.cursor_text_color } else { cell.fg };
                let bg = if is_block_cursor { data.cursor_color } else { cell.bg };

                let cell_w = if x + 1 < data.cols && row[x + 1].wide_continuation { 2 } else { 1 };

                // Check if this is a block drawing character that should be rendered as a rectangle
                if let Some(block_el) = render_block_char(cell.ch, fg, bg, cell_w) {
                    // Flush any pending text run first
                    flush(&mut spans, &run_text, run_char_count, run_fg, run_bg, run_bold);
                    run_text.clear();
                    run_char_count = 0;
                    first = true;
                    spans.push(block_el);
                    continue;
                }

                if first || fg != run_fg || bg != run_bg || cell.bold != run_bold {
                    flush(&mut spans, &run_text, run_char_count, run_fg, run_bg, run_bold);
                    run_text.clear();
                    run_char_count = 0;
                    run_fg = fg;
                    run_bg = bg;
                    run_bold = cell.bold;
                    first = false;
                }
                run_text.push(cell.ch);
                run_char_count += cell_w;
            }

            // Flush last run
            flush(&mut spans, &run_text, run_char_count, run_fg, run_bg, run_bold);

            let row_y = y as f32 * CELL_HEIGHT;
            let mut row_div = div()
                .absolute()
                .left_0()
                .top(px(row_y))
                .flex()
                .flex_row()
                .h(px(CELL_HEIGHT))
                .w(px(data.cols as f32 * CELL_WIDTH))
                .bg(data.default_bg);

            // Beam/underline cursor overlay
            if data.cursor_visible && y == data.cursor_row && data.cursor_shape > 0 {
                let cx_px = data.cursor_col as f32 * CELL_WIDTH;
                if data.cursor_shape == 1 {
                    row_div = row_div.child(
                        div().absolute().left(px(cx_px)).top_0()
                            .w(px(2.0)).h(px(CELL_HEIGHT)).bg(data.cursor_color)
                    );
                } else if data.cursor_shape == 2 {
                    row_div = row_div.child(
                        div().absolute().left(px(cx_px)).bottom_0()
                            .w(px(CELL_WIDTH)).h(px(2.0)).bg(data.cursor_color)
                    );
                }
            }

            row_div.children(spans).into_any_element()
        }))
}

/// Render Unicode block drawing characters as colored rectangles instead of text glyphs.
/// Returns None for non-block characters.
#[cfg(feature = "gpui")]
fn render_block_char(ch: char, fg: Rgba, bg: Rgba, cell_w: usize) -> Option<AnyElement> {
    let w = cell_w as f32 * CELL_WIDTH;
    let h = CELL_HEIGHT;
    let half = (h / 2.0).ceil(); // Use ceil to prevent sub-pixel gap

    match ch {
        // █ Full block
        '\u{2588}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().bg(fg).into_any_element()),
        // ▀ Upper half block
        '\u{2580}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().bg(bg).child(
            div().w(px(w)).h(px(half)).bg(fg)
        ).into_any_element()),
        // ▄ Lower half block
        '\u{2584}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_col().justify_end().bg(bg).child(
            div().w(px(w)).h(px(half)).bg(fg)
        ).into_any_element()),
        // ▌ Left half block
        '\u{258C}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_row().bg(bg).child(
            div().w(px(w * 0.5)).h(px(h)).bg(fg)
        ).into_any_element()),
        // ▐ Right half block
        '\u{2590}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_row().justify_end().bg(bg).child(
            div().w(px(w * 0.5)).h(px(h)).bg(fg)
        ).into_any_element()),
        // ▁ Lower 1/8
        '\u{2581}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_col().justify_end().bg(bg).child(
            div().w(px(w)).h(px(h * 0.125)).bg(fg)
        ).into_any_element()),
        // ▂ Lower 1/4
        '\u{2582}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_col().justify_end().bg(bg).child(
            div().w(px(w)).h(px(h * 0.25)).bg(fg)
        ).into_any_element()),
        // ▃ Lower 3/8
        '\u{2583}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_col().justify_end().bg(bg).child(
            div().w(px(w)).h(px(h * 0.375)).bg(fg)
        ).into_any_element()),
        // ▅ Lower 5/8
        '\u{2585}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_col().justify_end().bg(bg).child(
            div().w(px(w)).h(px(h * 0.625)).bg(fg)
        ).into_any_element()),
        // ▆ Lower 3/4
        '\u{2586}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_col().justify_end().bg(bg).child(
            div().w(px(w)).h(px(h * 0.75)).bg(fg)
        ).into_any_element()),
        // ▇ Lower 7/8
        '\u{2587}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_col().justify_end().bg(bg).child(
            div().w(px(w)).h(px(h * 0.875)).bg(fg)
        ).into_any_element()),
        // ▉ Left 7/8
        '\u{2589}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_row().bg(bg).child(
            div().w(px(w * 0.875)).h(px(h)).bg(fg)
        ).into_any_element()),
        // ▊ Left 3/4
        '\u{258A}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_row().bg(bg).child(
            div().w(px(w * 0.75)).h(px(h)).bg(fg)
        ).into_any_element()),
        // ▋ Left 5/8
        '\u{258B}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_row().bg(bg).child(
            div().w(px(w * 0.625)).h(px(h)).bg(fg)
        ).into_any_element()),
        // ▍ Left 3/8
        '\u{258D}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_row().bg(bg).child(
            div().w(px(w * 0.375)).h(px(h)).bg(fg)
        ).into_any_element()),
        // ▎ Left 1/4
        '\u{258E}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_row().bg(bg).child(
            div().w(px(w * 0.25)).h(px(h)).bg(fg)
        ).into_any_element()),
        // ▏ Left 1/8
        '\u{258F}' => Some(div().w(px(w)).h(px(h)).flex_shrink_0().flex().flex_row().bg(bg).child(
            div().w(px(w * 0.125)).h(px(h)).bg(fg)
        ).into_any_element()),
        // ░ Light shade (25%)
        '\u{2591}' => {
            let shade = Rgba { r: fg.r * 0.25 + bg.r * 0.75, g: fg.g * 0.25 + bg.g * 0.75, b: fg.b * 0.25 + bg.b * 0.75, a: 1.0 };
            Some(div().w(px(w)).h(px(h)).flex_shrink_0().bg(shade).into_any_element())
        }
        // ▒ Medium shade (50%)
        '\u{2592}' => {
            let shade = Rgba { r: fg.r * 0.5 + bg.r * 0.5, g: fg.g * 0.5 + bg.g * 0.5, b: fg.b * 0.5 + bg.b * 0.5, a: 1.0 };
            Some(div().w(px(w)).h(px(h)).flex_shrink_0().bg(shade).into_any_element())
        }
        // ▓ Dark shade (75%)
        '\u{2593}' => {
            let shade = Rgba { r: fg.r * 0.75 + bg.r * 0.25, g: fg.g * 0.75 + bg.g * 0.25, b: fg.b * 0.75 + bg.b * 0.25, a: 1.0 };
            Some(div().w(px(w)).h(px(h)).flex_shrink_0().bg(shade).into_any_element())
        }
        // ═══ Box-drawing characters ═══
        // These need pixel-perfect rendering to form continuous lines.
        // Thin line thickness
        '\u{2500}' | '\u{2501}' | '\u{2502}' | '\u{2503}' |
        '\u{250C}' | '\u{250D}' | '\u{250E}' | '\u{250F}' |
        '\u{2510}' | '\u{2511}' | '\u{2512}' | '\u{2513}' |
        '\u{2514}' | '\u{2515}' | '\u{2516}' | '\u{2517}' |
        '\u{2518}' | '\u{2519}' | '\u{251A}' | '\u{251B}' |
        '\u{251C}' | '\u{251D}' | '\u{251E}' | '\u{251F}' |
        '\u{2520}' | '\u{2521}' | '\u{2522}' | '\u{2523}' |
        '\u{2524}' | '\u{2525}' | '\u{2526}' | '\u{2527}' |
        '\u{2528}' | '\u{2529}' | '\u{252A}' | '\u{252B}' |
        '\u{252C}' | '\u{252D}' | '\u{252E}' | '\u{252F}' |
        '\u{2530}' | '\u{2531}' | '\u{2532}' | '\u{2533}' |
        '\u{2534}' | '\u{2535}' | '\u{2536}' | '\u{2537}' |
        '\u{2538}' | '\u{2539}' | '\u{253A}' | '\u{253B}' |
        '\u{253C}' | '\u{253D}' | '\u{253E}' | '\u{253F}' |
        '\u{2540}' | '\u{2541}' | '\u{2542}' | '\u{2543}' |
        '\u{2544}' | '\u{2545}' | '\u{2546}' | '\u{2547}' |
        '\u{2548}' | '\u{2549}' | '\u{254A}' | '\u{254B}' |
        '\u{2550}' | '\u{2551}' |
        '\u{2552}' | '\u{2553}' | '\u{2554}' | '\u{2555}' | '\u{2556}' | '\u{2557}' |
        '\u{2558}' | '\u{2559}' | '\u{255A}' | '\u{255B}' | '\u{255C}' | '\u{255D}' |
        '\u{255E}' | '\u{255F}' | '\u{2560}' | '\u{2561}' | '\u{2562}' | '\u{2563}' |
        '\u{2564}' | '\u{2565}' | '\u{2566}' | '\u{2567}' | '\u{2568}' | '\u{2569}' |
        '\u{256A}' | '\u{256B}' | '\u{256C}' => {
            Some(render_box_drawing(ch, fg, bg, w, h))
        }
        _ => None,
    }
}

/// Render box-drawing character as pixel-perfect lines
#[cfg(feature = "gpui")]
fn render_box_drawing(ch: char, fg: Rgba, bg: Rgba, w: f32, h: f32) -> AnyElement {
    let thin = 1.0_f32;
    let thick = 2.0_f32;
    let cx = w / 2.0;
    let cy = h / 2.0;

    // Determine which segments to draw: left/right/up/down from center
    // and whether each segment is thin, thick, or double
    let (left, right, up, down) = box_segments(ch);

    let mut container = div()
        .w(px(w))
        .h(px(h))
        .flex_shrink_0()
        .bg(bg);

    let line_w = |heavy: bool| if heavy { thick } else { thin };

    // Horizontal line (left + right)
    if left || right {
        let lw = line_w(is_heavy_h(ch));
        let x_start = if left { 0.0 } else { cx };
        let x_end = if right { w } else { cx + lw };
        container = container.child(
            div()
                .absolute()
                .left(px(x_start))
                .top(px(cy - lw / 2.0))
                .w(px(x_end - x_start))
                .h(px(lw))
                .bg(fg)
        );
    }

    // Vertical line (up + down)
    if up || down {
        let lw = line_w(is_heavy_v(ch));
        let y_start = if up { 0.0 } else { cy };
        let y_end = if down { h } else { cy + lw };
        container = container.child(
            div()
                .absolute()
                .left(px(cx - lw / 2.0))
                .top(px(y_start))
                .w(px(lw))
                .h(px(y_end - y_start))
                .bg(fg)
        );
    }

    container.into_any_element()
}

/// Determine which segments a box-drawing character has (left, right, up, down)
fn box_segments(ch: char) -> (bool, bool, bool, bool) {
    match ch {
        // ─ horizontal lines
        '\u{2500}' | '\u{2501}' | '\u{2550}' => (true, true, false, false),
        // │ vertical lines
        '\u{2502}' | '\u{2503}' | '\u{2551}' => (false, false, true, true),
        // ┌ top-left corners
        '\u{250C}' | '\u{250D}' | '\u{250E}' | '\u{250F}' | '\u{2552}' | '\u{2553}' | '\u{2554}' => (false, true, false, true),
        // ┐ top-right corners
        '\u{2510}' | '\u{2511}' | '\u{2512}' | '\u{2513}' | '\u{2555}' | '\u{2556}' | '\u{2557}' => (true, false, false, true),
        // └ bottom-left corners
        '\u{2514}' | '\u{2515}' | '\u{2516}' | '\u{2517}' | '\u{2558}' | '\u{2559}' | '\u{255A}' => (false, true, true, false),
        // ┘ bottom-right corners
        '\u{2518}' | '\u{2519}' | '\u{251A}' | '\u{251B}' | '\u{255B}' | '\u{255C}' | '\u{255D}' => (true, false, true, false),
        // ├ left tee
        '\u{251C}' | '\u{251D}' | '\u{251E}' | '\u{251F}' | '\u{2520}' | '\u{2521}' | '\u{2522}' | '\u{2523}' |
        '\u{255E}' | '\u{255F}' | '\u{2560}' => (false, true, true, true),
        // ┤ right tee
        '\u{2524}' | '\u{2525}' | '\u{2526}' | '\u{2527}' | '\u{2528}' | '\u{2529}' | '\u{252A}' | '\u{252B}' |
        '\u{2561}' | '\u{2562}' | '\u{2563}' => (true, false, true, true),
        // ┬ top tee
        '\u{252C}' | '\u{252D}' | '\u{252E}' | '\u{252F}' | '\u{2530}' | '\u{2531}' | '\u{2532}' | '\u{2533}' |
        '\u{2564}' | '\u{2565}' | '\u{2566}' => (true, true, false, true),
        // ┴ bottom tee
        '\u{2534}' | '\u{2535}' | '\u{2536}' | '\u{2537}' | '\u{2538}' | '\u{2539}' | '\u{253A}' | '\u{253B}' |
        '\u{2567}' | '\u{2568}' | '\u{2569}' => (true, true, true, false),
        // ┼ cross
        '\u{253C}' | '\u{253D}' | '\u{253E}' | '\u{253F}' | '\u{2540}' | '\u{2541}' | '\u{2542}' | '\u{2543}' |
        '\u{2544}' | '\u{2545}' | '\u{2546}' | '\u{2547}' | '\u{2548}' | '\u{2549}' | '\u{254A}' | '\u{254B}' |
        '\u{256A}' | '\u{256B}' | '\u{256C}' => (true, true, true, true),
        _ => (false, false, false, false),
    }
}

/// Check if a box character uses heavy/thick horizontal lines
fn is_heavy_h(ch: char) -> bool {
    matches!(ch, '\u{2501}' | '\u{2503}' | '\u{250D}' | '\u{250F}' |
        '\u{2511}' | '\u{2513}' | '\u{2515}' | '\u{2517}' |
        '\u{2519}' | '\u{251B}' | '\u{251D}' | '\u{2523}' |
        '\u{2525}' | '\u{252B}' | '\u{252F}' | '\u{2533}' |
        '\u{2537}' | '\u{253B}' | '\u{253F}' | '\u{254B}' |
        '\u{2550}')
}

/// Check if a box character uses heavy/thick vertical lines
fn is_heavy_v(ch: char) -> bool {
    matches!(ch, '\u{2503}' | '\u{250E}' | '\u{250F}' |
        '\u{2512}' | '\u{2513}' | '\u{2516}' | '\u{2517}' |
        '\u{251A}' | '\u{251B}' | '\u{251F}' | '\u{2520}' | '\u{2523}' |
        '\u{2526}' | '\u{2528}' | '\u{252B}' |
        '\u{2530}' | '\u{2531}' | '\u{2533}' |
        '\u{2538}' | '\u{253A}' | '\u{253B}' |
        '\u{2540}' | '\u{2541}' | '\u{2542}' | '\u{254B}' |
        '\u{2551}')
}
