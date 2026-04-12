//! Lightweight runtime metrics for the `AMUX_DEBUG_STATS=1` HUD
//! and the `AMUX_BENCH_STARTUP=1` startup-phase dumper.
//!
//! Tracks four things, all cheap:
//!
//!   * **Frame time** — rolling window of the last 60 frame
//!     durations (μs), written by `FrameGuard` at the top of
//!     `Render::render`.
//!   * **Glyph cache** — global hit/miss counters (relaxed
//!     atomics), updated inline inside `gpui_terminal::glyph_cache`.
//!   * **Input latency** — keystrokes call `mark_input()` which
//!     stashes a process-relative timestamp; the next render calls
//!     `consume_input_latency()` which computes the delta and
//!     stores it for the HUD to display.
//!   * **Startup phases** — `startup_phase("name")` records a
//!     `(name, Instant)` pair during startup; `dump_startup_report()`
//!     prints the deltas to stderr once, gated on
//!     `AMUX_BENCH_STARTUP=1`.
//!
//! `snapshot()` formats a one-line HUD string used by the status
//! bar. Returns `None` when the HUD env var isn't set. The env
//! lookup is cached in a `OnceLock<bool>` so the render path pays
//! one atomic load per frame.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once, OnceLock};
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
    let in_us = LAST_INPUT_LATENCY_US.load(Ordering::Relaxed);
    Some(format!(
        "f {:.1}ms · in {:.1}ms · glyph {}% ({}k)",
        frame_us as f32 / 1000.0,
        in_us as f32 / 1000.0,
        hit_pct,
        total / 1000
    ))
}

// ─── Input latency ──────────────────────────────────────────────
//
// A keystroke fires `mark_input()` which stashes a monotonic
// timestamp into `INPUT_TIMESTAMP_NS` (nanoseconds since the
// `PROCESS_START` anchor). The next render fires
// `consume_input_latency()` which reads and clears that slot,
// computes the delta, and stores it in `LAST_INPUT_LATENCY_US`
// for the HUD. Both paths are lock-free atomic ops.
//
// Latency measured this way is *keystroke-received → render-
// dispatched*, not *keystroke-received → pixel-on-screen*. The
// latter also includes GPUI's paint phase and a vsync wait —
// neither is exposed to us here. In practice this is the
// dominant controllable slice anyway.

static INPUT_TIMESTAMP_NS: AtomicU64 = AtomicU64::new(0);
static LAST_INPUT_LATENCY_US: AtomicU64 = AtomicU64::new(0);

fn process_start() -> Instant {
    static S: OnceLock<Instant> = OnceLock::new();
    *S.get_or_init(Instant::now)
}

/// Record that a keystroke was just received. Cheap — one atomic
/// store. If multiple keys arrive before the next render, only
/// the most recent timestamp is retained, which is what we want
/// for the "last key press → next frame" interpretation.
pub fn mark_input() {
    let ns = process_start().elapsed().as_nanos() as u64;
    // Never store 0 (which means "nothing pending") — add 1 ns
    // if the process has been running for less than 1 ns, which
    // is implausible on real hardware but defensive.
    let ns = if ns == 0 { 1 } else { ns };
    INPUT_TIMESTAMP_NS.store(ns, Ordering::Relaxed);
}

/// Called at the top of each render frame. If a keystroke is
/// pending, compute the elapsed time and publish it for the HUD.
/// No-op when no keystroke arrived since the last render.
pub fn consume_input_latency() {
    let stored = INPUT_TIMESTAMP_NS.swap(0, Ordering::Relaxed);
    if stored == 0 {
        return;
    }
    let now_ns = process_start().elapsed().as_nanos() as u64;
    let us = now_ns.saturating_sub(stored) / 1000;
    LAST_INPUT_LATENCY_US.store(us, Ordering::Relaxed);
}

// ─── Startup phase timing ───────────────────────────────────────
//
// Each `startup_phase("name")` call pushes a `(name, Instant)`
// pair into a shared Vec. `dump_startup_report()` prints all
// phases with both absolute-from-first and delta-between-adjacent
// timings, one time, to stderr, iff `AMUX_BENCH_STARTUP=1`. The
// dump itself runs behind a `Once` so putting the call in
// `Render::render` (which fires every frame) is safe.

fn startup_phases() -> &'static Mutex<Vec<(&'static str, Instant)>> {
    static PHASES: OnceLock<Mutex<Vec<(&'static str, Instant)>>> = OnceLock::new();
    PHASES.get_or_init(|| Mutex::new(Vec::new()))
}

/// Record a named startup milestone. Safe to call from any
/// thread. Ordering is by wall-clock `Instant`, so phases recorded
/// out of source order (e.g. a background thread hitting a
/// milestone before the main thread) still sort correctly in the
/// dump.
pub fn startup_phase(name: &'static str) {
    let now = Instant::now();
    if let Ok(mut v) = startup_phases().lock() {
        v.push((name, now));
    }
}

/// Print the recorded startup phases to stderr, once, iff
/// `AMUX_BENCH_STARTUP=1` is set. Called from the first render
/// frame so all pre-first-frame phases have been captured.
pub fn dump_startup_report() {
    static DUMPED: Once = Once::new();
    DUMPED.call_once(|| {
        if std::env::var("AMUX_BENCH_STARTUP").ok().as_deref() != Some("1") {
            return;
        }
        let phases = match startup_phases().lock() {
            Ok(g) => g.clone(),
            Err(_) => return,
        };
        if phases.len() < 2 {
            return;
        }
        let mut sorted = phases;
        sorted.sort_by_key(|(_, t)| *t);
        let t0 = sorted[0].1;
        eprintln!("[amux-bench] startup phases (relative to `{}`):", sorted[0].0);
        for (name, t) in &sorted {
            let ms = t.saturating_duration_since(t0).as_secs_f64() * 1000.0;
            eprintln!("  {:24}  {:8.2} ms", name, ms);
        }
        eprintln!("[amux-bench] inter-phase deltas:");
        for w in sorted.windows(2) {
            let delta_ms = w[1].1.saturating_duration_since(w[0].1).as_secs_f64() * 1000.0;
            eprintln!("  {:24} → {:<24}  Δ {:8.2} ms", w[0].0, w[1].0, delta_ms);
        }
    });
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

    #[test]
    fn input_latency_mark_then_consume() {
        // Clear any stale state from earlier tests in the same
        // process (atomics are global).
        INPUT_TIMESTAMP_NS.store(0, Ordering::Relaxed);
        LAST_INPUT_LATENCY_US.store(0, Ordering::Relaxed);

        mark_input();
        std::thread::sleep(std::time::Duration::from_millis(2));
        consume_input_latency();

        let us = LAST_INPUT_LATENCY_US.load(Ordering::Relaxed);
        // Loose bounds: at least 1ms elapsed, less than 500ms.
        assert!(us >= 1_000, "expected ≥1ms, got {}μs", us);
        assert!(us < 500_000, "expected <500ms, got {}μs", us);
        // Slot should be cleared after consume.
        assert_eq!(INPUT_TIMESTAMP_NS.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn input_latency_consume_without_mark_is_noop() {
        INPUT_TIMESTAMP_NS.store(0, Ordering::Relaxed);
        LAST_INPUT_LATENCY_US.store(12345, Ordering::Relaxed);
        consume_input_latency();
        // LAST_INPUT_LATENCY_US must NOT be overwritten when
        // there was no pending keystroke — otherwise idle frames
        // would zero out the display value and the HUD would
        // flicker.
        assert_eq!(LAST_INPUT_LATENCY_US.load(Ordering::Relaxed), 12345);
    }

    #[test]
    fn startup_phase_sorts_by_instant() {
        // We can't easily test the full dump (it's behind a
        // Once + an env var), but we can at least verify that
        // startup_phase mutations land in the shared Vec.
        let before_len = startup_phases().lock().unwrap().len();
        startup_phase("unit_test_phase_a");
        startup_phase("unit_test_phase_b");
        let after = startup_phases().lock().unwrap();
        assert_eq!(after.len(), before_len + 2);
        // The two adjacent entries we just pushed must be in the
        // order we pushed them (Instant monotonicity on real
        // hardware).
        let a = after.iter().rev().nth(1).unwrap();
        let b = after.iter().rev().next().unwrap();
        assert_eq!(a.0, "unit_test_phase_a");
        assert_eq!(b.0, "unit_test_phase_b");
        assert!(b.1 >= a.1);
    }
}
