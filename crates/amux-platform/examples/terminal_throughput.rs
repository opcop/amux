//! Terminal emulator ingest throughput baseline.
//!
//! Feeds ~10 MiB of synthetic ANSI-flavored output through the same
//! parser+term stack the live app uses (`vte::ansi::Processor` +
//! `alacritty_terminal::Term`) and prints MiB/s. Serves as the first
//! regression baseline for amux performance work — no render, no
//! PTY, no child process. Run with:
//!
//! ```sh
//! cargo run --release --example terminal_throughput -p amux-platform
//! ```
//!
//! The payload is a repeating line mixing printable ASCII, UTF-8 and
//! two CSI SGR sequences. It doesn't exercise every parser branch
//! (no OSC, no DCS, no cursor moves) but it's a reasonable proxy for
//! the hot path — dominated by `input()` and SGR dispatch — which is
//! what tight inner loops in long builds / streaming agent output
//! look like.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config as TermConfig, Term};
use amux_platform::terminal::alacritty_view::AmuEventProxy;
use vte::ansi::Processor;

struct BenchSize {
    cols: usize,
    rows: usize,
}

impl Dimensions for BenchSize {
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

fn main() {
    let target_bytes: usize = std::env::var("AMUX_BENCH_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10 * 1024 * 1024);

    // One line ≈ 120 bytes including the two SGR escapes.
    let line: &str =
        "\x1b[32mhello world\x1b[0m this is a fairly typical line of terminal output — numbers 12345 symbols $@!#%\n";
    let mut payload: Vec<u8> = Vec::with_capacity(target_bytes + line.len());
    while payload.len() < target_bytes {
        payload.extend_from_slice(line.as_bytes());
    }
    let total_bytes = payload.len();

    let proxy = AmuEventProxy {
        title: Arc::new(Mutex::new(None)),
        bell: Arc::new(AtomicBool::new(false)),
        child_exited: Arc::new(AtomicBool::new(false)),
        dirty: Arc::new(AtomicBool::new(false)),
    };
    let size = BenchSize { cols: 120, rows: 40 };
    let mut config = TermConfig::default();
    config.scrolling_history = 10_000;
    let mut term = Term::new(config, &size, proxy);
    let mut parser: Processor = Processor::new();

    // Chunk the payload to simulate the ~4 KiB PTY reads the live
    // event loop actually performs. Parsing cost is per-byte so the
    // chunk size mostly affects cache behavior, but matching reality
    // keeps the number comparable to what amux sees at runtime.
    let chunk_size = 4096;
    let start = Instant::now();
    for chunk in payload.chunks(chunk_size) {
        parser.advance(&mut term, chunk);
    }
    let elapsed = start.elapsed();

    let mib = total_bytes as f64 / (1024.0 * 1024.0);
    let secs = elapsed.as_secs_f64();
    let throughput = mib / secs.max(1e-9);

    println!("amux terminal emulator throughput");
    println!("  bytes:      {} ({:.2} MiB)", total_bytes, mib);
    println!("  cols×rows:  {}×{}", size.cols, size.rows);
    println!("  chunk:      {} B", chunk_size);
    println!("  elapsed:    {:.3} s", secs);
    println!("  throughput: {:.1} MiB/s", throughput);
}
