//! Shared `MetricsService` implementation backed by `SystemMetricsCollector`.
//!
//! Used by all three platform adapters (Windows / macOS / Linux). Previously
//! each adapter contained an identical copy of this struct + impl.

use std::sync::Mutex;

use crate::{MetricsService, SystemMetrics, SystemMetricsCollector};

#[derive(Default)]
pub struct CollectorMetricsService {
    collector: Mutex<SystemMetricsCollector>,
}

impl std::fmt::Debug for CollectorMetricsService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectorMetricsService").finish()
    }
}

impl MetricsService for CollectorMetricsService {
    fn current_metrics(&self) -> Result<SystemMetrics, String> {
        let mut collector = self
            .collector
            .lock()
            .map_err(|_| "system metrics mutex poisoned".to_string())?;
        Ok(collector.get_metrics())
    }
}
