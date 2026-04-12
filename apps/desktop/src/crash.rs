//! Crash handling: panic hook, crash log writing, startup detection.
//!
//! The panic hook writes a timestamped file into
//! `~/.amux/logs/crash/crash-<unix_ms>.log` containing the panic
//! payload, source location, a full backtrace, platform info, and —
//! if available — the most recent serialized workspace layout
//! snapshot. The snapshot is published by the persistence layer
//! (`update_layout_snapshot`) after every successful save, so a crash
//! between saves still captures the last known-good layout.
//!
//! The previous panic hook is chained, so normal stderr output and
//! `RUST_BACKTRACE` behavior are preserved.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once, OnceLock};

static LAYOUT_SNAPSHOT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static HOOK_INSTALLED: Once = Once::new();

fn snapshot_cell() -> &'static Mutex<Option<String>> {
    LAYOUT_SNAPSHOT.get_or_init(|| Mutex::new(None))
}

/// Publish the most recent serialized layout JSON. Called by the
/// persistence layer after every successful save so a subsequent
/// panic can attach the last known-good layout to the crash log.
pub fn update_layout_snapshot(json: String) {
    if let Ok(mut guard) = snapshot_cell().lock() {
        *guard = Some(json);
    }
}

/// Canonical crash log directory: `~/.amux/logs/crash`.
pub fn crash_log_dir() -> PathBuf {
    amux_platform::amux_home_dir().join("logs").join("crash")
}

/// Install the panic hook. Idempotent.
pub fn install(log_dir: PathBuf) {
    HOOK_INSTALLED.call_once(move || {
        let _ = std::fs::create_dir_all(&log_dir);
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            prev(info);
            let _ = write_crash_log(&log_dir, info);
        }));
    });
}

fn write_crash_log(log_dir: &Path, info: &std::panic::PanicHookInfo<'_>) -> std::io::Result<()> {
    use std::io::Write;

    std::fs::create_dir_all(log_dir)?;
    let ts = unix_millis();
    let path = log_dir.join(format!("crash-{ts}.log"));
    let mut f = std::fs::File::create(&path)?;

    writeln!(f, "amux crash report")?;
    writeln!(f, "timestamp_ms: {ts}")?;
    writeln!(f, "version: {}", env!("CARGO_PKG_VERSION"))?;
    writeln!(f, "target_os: {}", std::env::consts::OS)?;
    writeln!(f, "target_arch: {}", std::env::consts::ARCH)?;
    if let Some(loc) = info.location() {
        writeln!(f, "location: {}:{}:{}", loc.file(), loc.line(), loc.column())?;
    }
    writeln!(f, "payload: {}", panic_payload(info))?;
    writeln!(f)?;
    writeln!(f, "-- backtrace --")?;
    writeln!(f, "{}", std::backtrace::Backtrace::force_capture())?;

    if let Some(json) = snapshot_cell().lock().ok().and_then(|g| g.clone()) {
        writeln!(f)?;
        writeln!(f, "-- last layout snapshot --")?;
        writeln!(f, "{json}")?;
    }
    f.sync_all()?;
    Ok(())
}

fn panic_payload(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

fn unix_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Scan the crash log directory for existing crash reports, newest
/// first. Used at startup to surface a banner in the status bar.
pub fn list_crashes(log_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".log"))
        })
        .collect();
    out.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_roundtrip() {
        update_layout_snapshot("hello".into());
        let got = snapshot_cell().lock().unwrap().clone();
        assert_eq!(got.as_deref(), Some("hello"));
    }

    #[test]
    fn list_crashes_filters_and_sorts() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("crash-100.log"), b"a").unwrap();
        std::fs::write(tmp.path().join("crash-200.log"), b"b").unwrap();
        std::fs::write(tmp.path().join("not-a-crash.txt"), b"x").unwrap();
        let got = list_crashes(tmp.path());
        assert_eq!(got.len(), 2);
        assert!(got[0].file_name().unwrap().to_string_lossy().contains("200"));
        assert!(got[1].file_name().unwrap().to_string_lossy().contains("100"));
    }
}
