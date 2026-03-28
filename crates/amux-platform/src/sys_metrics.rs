//! System Metrics Module
//!
//! Provides system resource monitoring (CPU, memory) using the sysinfo crate.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

/// System metrics snapshot
#[derive(Clone, Debug, PartialEq)]
pub struct SystemMetrics {
    /// CPU usage as percentage (0.0 - 100.0)
    pub cpu_usage: f32,
    /// Memory used in bytes
    pub memory_used: u64,
    /// Total memory in bytes
    pub memory_total: u64,
    /// Memory usage as percentage (0.0 - 100.0)
    pub memory_usage_percent: f32,
    /// Number of CPU cores
    pub cpu_count: usize,
    /// System uptime in seconds
    pub uptime_secs: u64,
}

/// CPU metrics for individual cores
#[derive(Clone, Debug)]
pub struct CpuCoreMetrics {
    pub core_index: usize,
    pub usage: f32,
}

/// Thread-safe system metrics collector
pub struct SystemMetricsCollector {
    system: Arc<Mutex<System>>,
    last_refresh: Instant,
    refresh_interval: Duration,
}

impl SystemMetricsCollector {
    /// Create a new collector with default settings
    pub fn new() -> Self {
        let mut system = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        
        // Initial refresh
        system.refresh_all();
        
        Self {
            system: Arc::new(Mutex::new(system)),
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(1), // Refresh at most once per second
        }
    }
    
    /// Create a collector with custom refresh interval
    pub fn with_interval(refresh_interval: Duration) -> Self {
        let mut collector = Self::new();
        collector.refresh_interval = refresh_interval;
        collector
    }
    
    /// Get current system metrics
    pub fn get_metrics(&mut self) -> SystemMetrics {
        self.refresh_if_needed();
        
        let system = self.system.lock().unwrap();
        
        let cpu_usage = system.global_cpu_usage();
        let memory_used = system.used_memory();
        let memory_total = system.total_memory();
        let memory_usage_percent = if memory_total > 0 {
            (memory_used as f32 / memory_total as f32) * 100.0
        } else {
            0.0
        };
        let cpu_count = system.cpus().len();
        let uptime_secs = System::uptime();
        
        SystemMetrics {
            cpu_usage,
            memory_used,
            memory_total,
            memory_usage_percent,
            cpu_count,
            uptime_secs,
        }
    }
    
    /// Get per-core CPU metrics
    pub fn get_cpu_cores(&mut self) -> Vec<CpuCoreMetrics> {
        self.refresh_if_needed();
        
        let system = self.system.lock().unwrap();
        
        system.cpus()
            .iter()
            .enumerate()
            .map(|(index, cpu)| CpuCoreMetrics {
                core_index: index,
                usage: cpu.cpu_usage(),
            })
            .collect()
    }
    
    /// Force a refresh of system metrics
    pub fn refresh(&mut self) {
        let mut system = self.system.lock().unwrap();
        system.refresh_all();
        self.last_refresh = Instant::now();
    }
    
    fn refresh_if_needed(&mut self) {
        if self.last_refresh.elapsed() >= self.refresh_interval {
            let mut system = self.system.lock().unwrap();
            system.refresh_cpu_all();
            system.refresh_memory();
            self.last_refresh = Instant::now();
        }
    }
}

impl Default for SystemMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Format bytes as human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format uptime as human-readable string
pub fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    
    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

/// Format CPU usage as string with optional color indicator
pub fn format_cpu_usage(usage: f32) -> String {
    format!("{:.0}%", usage)
}

/// Format memory usage as string
pub fn format_memory(used: u64, total: u64) -> String {
    format!("{}/{}", format_bytes(used), format_bytes(total))
}

/// Check if system is under high load (CPU > 80% or Memory > 90%)
pub fn is_high_load(metrics: &SystemMetrics) -> bool {
    metrics.cpu_usage > 80.0 || metrics.memory_usage_percent > 90.0
}

/// Get load status color indicator
/// Returns: "green", "yellow", or "red"
pub fn get_load_status(metrics: &SystemMetrics) -> &'static str {
    if metrics.cpu_usage > 90.0 || metrics.memory_usage_percent > 95.0 {
        "red"
    } else if metrics.cpu_usage > 70.0 || metrics.memory_usage_percent > 80.0 {
        "yellow"
    } else {
        "green"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes_correctly() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.0 GB");
    }

    #[test]
    fn formats_uptime_correctly() {
        assert_eq!(format_uptime(30), "0m");
        assert_eq!(format_uptime(90), "1m");
        assert_eq!(format_uptime(3600), "1h 0m");
        assert_eq!(format_uptime(9000), "2h 30m");
        assert_eq!(format_uptime(90000), "1d 1h");
    }

    #[test]
    fn identifies_high_load() {
        let normal = SystemMetrics {
            cpu_usage: 30.0,
            memory_used: 4_000_000_000,
            memory_total: 16_000_000_000,
            memory_usage_percent: 25.0,
            cpu_count: 8,
            uptime_secs: 3600,
        };
        assert!(!is_high_load(&normal));

        let high_cpu = SystemMetrics {
            cpu_usage: 85.0,
            ..normal.clone()
        };
        assert!(is_high_load(&high_cpu));

        let high_mem = SystemMetrics {
            memory_usage_percent: 92.0,
            ..normal
        };
        assert!(is_high_load(&high_mem));
    }

    #[test]
    fn gets_load_status() {
        let low = SystemMetrics {
            cpu_usage: 30.0,
            memory_used: 4_000_000_000,
            memory_total: 16_000_000_000,
            memory_usage_percent: 25.0,
            cpu_count: 8,
            uptime_secs: 3600,
        };
        assert_eq!(get_load_status(&low), "green");

        let medium = SystemMetrics {
            cpu_usage: 75.0,
            ..low.clone()
        };
        assert_eq!(get_load_status(&medium), "yellow");

        let high = SystemMetrics {
            cpu_usage: 95.0,
            ..low
        };
        assert_eq!(get_load_status(&high), "red");
    }
}
