//! Regression tests for the terminal rendering grid invariant.
//!
//! The rule under test (see `docs/terminal-rendering-invariants.md`):
//! every character alacritty accepts lands in the grid at a specific
//! column, and that column is where the renderer must paint it —
//! independent of what any font has to say about the glyph.
//!
//! These tests drive an `alacritty_terminal::Term` directly via
//! `vte::ansi::Processor`, the same way the production event loop
//! does, and assert that the cells land where we expect. They do
//! **not** exercise the gpui rendering path (no `WindowTextSystem`
//! available in a headless test), but they lock down the cell
//! layout contract `prepaint_terminal` consumes — which is where
//! historical Nerd-Font / Powerline / wide-char bugs have rooted.
//!
//! If you're fixing a rendering bug, the first thing to do is add a
//! new `#[test]` here that captures the broken input sequence. Any
//! future "optimization" that re-breaks the invariant will fail this
//! file in CI.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config as TermConfig, Term};
use amux_platform::terminal::alacritty_view::AmuEventProxy;
use vte::ansi::Processor;

struct TestSize {
    cols: usize,
    rows: usize,
}

impl Dimensions for TestSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Build a headless `Term` fed with the given bytes. No PTY, no
/// window, no gpui — just the alacritty state machine in memory.
fn term_from_bytes(cols: usize, rows: usize, bytes: &[u8]) -> Term<AmuEventProxy> {
    // Headless test: the OSC event channel is never drained (no
    // PTY reader feeding it in this suite), so the rx end is
    // dropped immediately. Sender stays cheap to clone and never
    // actually transmits anything during grid rendering.
    let (osc_event_tx, _osc_event_rx) = std::sync::mpsc::channel();
    let proxy = AmuEventProxy {
        title: Arc::new(Mutex::new(None)),
        bell: Arc::new(AtomicBool::new(false)),
        child_exited: Arc::new(AtomicBool::new(false)),
        dirty: Arc::new(AtomicBool::new(false)),
        title_changed: Arc::new(AtomicBool::new(false)),
        osc_event_tx,
    };
    let size = TestSize { cols, rows };
    let config = TermConfig::default();
    let mut term = Term::new(config, &size, proxy);
    let mut parser: Processor = Processor::new();
    parser.advance(&mut term, bytes);
    term
}

/// Read the `char` at `(row, col)` from the alacritty grid, treating
/// `\0` and default-empty cells as space.
fn cell_char(term: &Term<AmuEventProxy>, row: usize, col: usize) -> char {
    let cell = &term.grid()[Point::new(Line(row as i32), Column(col))];
    let ch = cell.c;
    if ch == '\0' {
        ' '
    } else {
        ch
    }
}

fn cell_is_wide_spacer(term: &Term<AmuEventProxy>, row: usize, col: usize) -> bool {
    let cell = &term.grid()[Point::new(Line(row as i32), Column(col))];
    cell.flags.contains(Flags::WIDE_CHAR_SPACER)
}

fn row_string(term: &Term<AmuEventProxy>, row: usize, len: usize) -> String {
    (0..len).map(|c| cell_char(term, row, c)).collect()
}

/// Baseline sanity: plain ASCII lands cell-for-cell.
#[test]
fn ascii_lands_cell_for_cell() {
    let term = term_from_bytes(40, 5, b"hello world");
    assert_eq!(row_string(&term, 0, 11), "hello world");
}

/// SGR escapes don't occupy cells: they're parsed as terminal state,
/// not written to the grid. A run like `ESC[1;34m Brc20BatchMint ESC[0m`
/// must leave the 14 printable chars at cells 0..=13.
#[test]
fn sgr_escapes_do_not_consume_cells() {
    let bytes = b"\x1b[1;34mBrc20BatchMint\x1b[0m done";
    let term = term_from_bytes(80, 5, bytes);
    // 14 chars of "Brc20BatchMint", then space, then "done".
    assert_eq!(row_string(&term, 0, 14), "Brc20BatchMint");
    assert_eq!(cell_char(&term, 0, 14), ' ');
    assert_eq!(row_string(&term, 0, 19)[15..], *"done");
}

/// The exact shell-prompt shape that historically broke: a Powerline
/// branch icon (U+E0A0) surrounded by ASCII. Each cell must land at
/// its own column. The renderer's job then is to paint them there —
/// this test freezes the upstream half of that contract.
#[test]
fn powerline_prompt_lays_out_on_grid() {
    // `ESC[1;34mBrc20BatchMint ESC[0m ESC[1;35m\ue0a0 main ESC[0m `
    // — the literal bytes from a `print -P "$PROMPT"` dump of the
    // live reproduction case.
    let bytes = b"\x1b[1;34mBrc20BatchMint\x1b[0m \x1b[1;35m\xee\x82\xa0 main\x1b[0m ";
    let term = term_from_bytes(80, 5, bytes);

    // Cols 0..=13 = "Brc20BatchMint"
    assert_eq!(row_string(&term, 0, 14), "Brc20BatchMint");
    // Col 14 = space (after reset)
    assert_eq!(cell_char(&term, 0, 14), ' ');
    // Col 15 = U+E0A0 (Powerline branch icon). Alacritty classifies
    // PUA as narrow, so this must NOT be a wide spacer at 16.
    assert_eq!(cell_char(&term, 0, 15), '\u{e0a0}');
    assert!(
        !cell_is_wide_spacer(&term, 0, 16),
        "U+E0A0 must be a 1-cell narrow char; if alacritty starts classifying \
         it as wide, the renderer's wide-char branch must learn about it too"
    );
    // Col 16 = space (inside magenta segment)
    assert_eq!(cell_char(&term, 0, 16), ' ');
    // Cols 17..=20 = "main"
    assert_eq!(cell_char(&term, 0, 17), 'm');
    assert_eq!(cell_char(&term, 0, 18), 'a');
    assert_eq!(cell_char(&term, 0, 19), 'i');
    assert_eq!(cell_char(&term, 0, 20), 'n');
    // Col 21 = trailing space (after reset). The cursor will end up
    // at col 22 in the live app — that's `cursor_col * cell_w` and
    // it must not overlap any of the cells above.
    assert_eq!(cell_char(&term, 0, 21), ' ');
}

/// Nerd Font file icon from Private Use Area plane. Same as Powerline
/// case above but from the U+F000..U+F8FF range Nerd Fonts pack into.
#[test]
fn nerd_font_icon_stays_narrow() {
    // U+F418 = Nerd Font Octicons branch icon. UTF-8 = EF 90 98.
    let bytes = b"repo \xef\x90\x98 main";
    let term = term_from_bytes(40, 5, bytes);
    assert_eq!(cell_char(&term, 0, 0), 'r');
    assert_eq!(cell_char(&term, 0, 4), ' ');
    assert_eq!(cell_char(&term, 0, 5), '\u{f418}');
    assert!(!cell_is_wide_spacer(&term, 0, 6));
    assert_eq!(cell_char(&term, 0, 6), ' ');
    assert_eq!(cell_char(&term, 0, 7), 'm');
    assert_eq!(cell_char(&term, 0, 10), 'n');
}

/// Wide chars (CJK) occupy two cells: the head char at col N and a
/// `WIDE_CHAR_SPACER` at col N+1. The renderer reads this flag to
/// decide wide-vs-narrow handling, so any regression in alacritty's
/// classification would break Chinese/Japanese/Korean rendering.
#[test]
fn cjk_wide_char_occupies_two_cells() {
    // 你 (U+4F60) is a CJK unified ideograph, East Asian Width = Wide.
    let bytes = "a你b".as_bytes();
    let term = term_from_bytes(40, 5, bytes);
    assert_eq!(cell_char(&term, 0, 0), 'a');
    assert_eq!(cell_char(&term, 0, 1), '你');
    assert!(
        cell_is_wide_spacer(&term, 0, 2),
        "the cell after a wide char must be marked as WIDE_CHAR_SPACER \
         so the renderer knows to skip painting its own glyph"
    );
    assert_eq!(cell_char(&term, 0, 3), 'b');
}

/// Mixed PUA + wide + ASCII in one line — the stress case for the
/// narrow-run detection + wide-char break path + drift-tolerant
/// fallback path all interacting on the same row.
#[test]
fn mixed_pua_wide_ascii_lands_on_grid() {
    let bytes = "a\u{e0a0}你b".as_bytes();
    let term = term_from_bytes(40, 5, bytes);
    assert_eq!(cell_char(&term, 0, 0), 'a');
    assert_eq!(cell_char(&term, 0, 1), '\u{e0a0}');
    assert!(!cell_is_wide_spacer(&term, 0, 2));
    assert_eq!(cell_char(&term, 0, 2), '你');
    assert!(cell_is_wide_spacer(&term, 0, 3));
    assert_eq!(cell_char(&term, 0, 4), 'b');
}

/// Overwriting a cell via cursor movement must land on the right cell.
/// This is a sanity check that ESC[H (cursor home) + plain chars use
/// the same column arithmetic the renderer does.
#[test]
fn cursor_move_rewrites_correct_cell() {
    // Write "hello", move cursor to col 2 (1-based → Column(1)),
    // overwrite with 'X'. Expected result: "hXllo".
    let bytes = b"hello\x1b[1;2HX";
    let term = term_from_bytes(40, 5, bytes);
    assert_eq!(row_string(&term, 0, 5), "hXllo");
}
