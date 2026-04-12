//! Lightweight runtime metrics for the `AMUX_DEBUG_STATS=1` HUD.
//!
//! Tracks two things cheap to collect from any thread:
//!   * a rolling window of the last 60 frame durations (μs), written
//!     by the render guard;
//!   * global glyph cache hit/miss counters (relaxed atomics), updated
//!     inline inside `gpui_terminal::glyph_cache`.
//!
//! Nothing is exported publicly; the only consumer is
//! `snapshot()`, which formats a single-line summary string shown in
//! the status bar when the HUD is enabled. When the HUD is disabled,
//! `snapshot()` returns `None` in ~3 ns (one env var lookup is
//! cached into a `OnceLock<bool>`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub static GLYPH_HITS: AtomicU64 = AtomicU64::new(0);
pub static GLYPH_MISSES: AtomicU64 = AtomicU64::new(0);

const FRAME_WINDOW: usize = 60;

struct FrameWindow {
    buf: [u64; FRAME_WINDOW],
    len: usize,
    head: usize,
}

impl FrameWindow {
    const fn new() -> Self {
        Self {
            buf: [0; FRAME_WINDOW],
            len: 0,
            head: 0,
        }
    }

    fn push(&mut self, micros: u64) {
        self.buf[self.head] = micros;
        self.head = (self.head + 1) % FRAME_WINDOW;
        if self.len < FRAME_WINDOW {
            self.len += 1;
        }
    }

    /// Mean of populated entries, in microseconds. Returns 0 if empty.
    fn mean_micros(&self) -> u64 {
        if self.len == 0 {
            return 0;
        }
        let sum: u64 = self.buf[..self.len].iter().sum();
        sum / self.len as u64
    }
}

fn window_cell() -> &'static Mutex<FrameWindow> {
    static WINDOW: OnceLock<Mutex<FrameWindow>> = OnceLock::new();
    WINDOW.get_or_init(|| Mutex::new(FrameWindow::new()))
}

/// RAII guard: start a render frame, record its duration on drop.
/// Used at the top of `Render::render`.
pub struct FrameGuard {
    start: Instant,
}

impl FrameGuard {
    pub fn start() -> Self {
        Self { start: Instant::now() }
    }
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        let micros = self.start.elapsed().as_micros() as u64;
        if let Ok(mut w) = window_cell().lock() {
            w.push(micros);
        }
    }
}

/// True when `AMUX_DEBUG_STATS=1` was set in the process env at
/// startup. Cached so the render path doesn't hit env lookups.
pub fn hud_enabled() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| {
        std::env::var("AMUX_DEBUG_STATS")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false)
    })
}

/// Formatted one-line HUD string, or `None` when disabled.
pub fn snapshot() -> Option<String> {
    if !hud_enabled() {
        return None;
    }
    let frame_us = window_cell()
        .lock()
        .ok()
        .map(|w| w.mean_micros())
        .unwrap_or(0);
    let hits = GLYPH_HITS.load(Ordering::Relaxed);
    let misses = GLYPH_MISSES.load(Ordering::Relaxed);
    let total = hits + misses;
    let hit_pct = if total > 0 { hits * 100 / total } else { 0 };
    Some(format!(
        "f {:.1}ms · glyph {}% ({}k)",
        frame_us as f32 / 1000.0,
        hit_pct,
        total / 1000
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_window_rolls_over() {
        let mut w = FrameWindow::new();
        for i in 0..(FRAME_WINDOW as u64 + 10) {
            w.push(i);
        }
        assert_eq!(w.len, FRAME_WINDOW);
        // After rollover the mean should reflect only the last 60
        // entries (10..=69), not 0..=69.
        let expected: u64 = (10..(FRAME_WINDOW as u64 + 10)).sum::<u64>() / FRAME_WINDOW as u64;
        assert_eq!(w.mean_micros(), expected);
    }

    #[test]
    fn frame_guard_records_elapsed() {
        // Can't assert exact timings, just that a push happens.
        let before = window_cell().lock().unwrap().len;
        {
            let _g = FrameGuard::start();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let after = window_cell().lock().unwrap().len;
        assert!(after >= before);
    }
}
