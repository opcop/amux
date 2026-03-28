//! WSL Distribution Detection
//!
//! Detects installed WSL distributions on Windows.

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Represents a detected WSL distribution
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslDistroInfo {
    pub name: String,
    pub version: u32,
    pub default_user: Option<String>,
    pub state: DistroState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DistroState {
    Running,
    Stopped,
    Installing,
}

/// Result of WSL distribution detection
#[derive(Clone, Debug)]
pub struct WslDetectionResult {
    pub distros: Vec<WslDistroInfo>,
    pub default_distro: Option<String>,
    pub wsl_available: bool,
}

impl Default for WslDetectionResult {
    fn default() -> Self {
        Self {
            distros: Vec::new(),
            default_distro: None,
            wsl_available: false,
        }
    }
}

/// Cached WSL detection with automatic refresh
pub struct WslDetectionCache {
    result: WslDetectionResult,
    last_refresh: Instant,
    cache_ttl: Duration,
}

impl WslDetectionCache {
    pub fn new() -> Self {
        Self {
            result: WslDetectionResult::default(),
            last_refresh: Instant::now() - Duration::from_secs(3600), // Force refresh on first call
            cache_ttl: Duration::from_secs(30), // Refresh every 30 seconds
        }
    }

    /// Get cached detection result, refreshing if stale
    pub fn get(&mut self) -> &WslDetectionResult {
        if self.last_refresh.elapsed() > self.cache_ttl {
            self.refresh();
        }
        &self.result
    }

    /// Force refresh the detection
    pub fn refresh(&mut self) {
        self.result = detect_wsl_distributions();
        self.last_refresh = Instant::now();
    }

    /// Check if WSL is available
    pub fn is_wsl_available(&self) -> bool {
        self.result.wsl_available
    }

    /// Get list of distribution names
    pub fn distro_names(&self) -> Vec<String> {
        self.result.distros.iter().map(|d| d.name.clone()).collect()
    }
}

impl Default for WslDetectionCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect all installed WSL distributions
pub fn detect_wsl_distributions() -> WslDetectionResult {
    let output = Command::new("wsl")
        .args(["--list", "--verbose"])
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            parse_wsl_list_output(&stdout)
        }
        Err(_) => WslDetectionResult::default(),
    }
}

fn parse_wsl_list_output(output: &str) -> WslDetectionResult {
    let mut result = WslDetectionResult {
        wsl_available: true,
        ..Default::default()
    };

    // Skip header line ("NAME STATE VERSION")
    for line in output.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse line format: "* Ubuntu              Running         2" or "  docker-desktop      Stopped         2"
        // Split on multiple spaces
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        // Check if first part is just "*" (default distro marker)
        let (name, is_default) = if parts[0] == "*" {
            (parts[1].to_string(), true)
        } else {
            (parts[0].to_string(), false)
        };
        
        let state = match parts.get(if is_default { 2 } else { 1 }).map(|s| *s) {
            Some("Running") => DistroState::Running,
            Some("Stopped") => DistroState::Stopped,
            Some("Installing") => DistroState::Installing,
            _ => DistroState::Stopped,
        };
        
        let version_index = if is_default { 3 } else { 2 };
        let version: u32 = parts.get(version_index).and_then(|v| v.parse().ok()).unwrap_or(1);

        // Check if this is the default distro
        if is_default {
            result.default_distro = Some(name.clone());
        }

        result.distros.push(WslDistroInfo {
            name,
            version,
            default_user: None, // Would need separate query to get default user
            state,
        });
    }

    result
}

/// Check if WSL is installed on the system
pub fn is_wsl_installed() -> bool {
    Command::new("wsl")
        .args(["--status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get the default WSL distribution
pub fn get_default_distro() -> Option<String> {
    detect_wsl_distributions().default_distro
}

/// Launch a WSL distribution if it's not running
pub fn ensure_distro_running(distro: &str) -> Result<(), String> {
    let output = Command::new("wsl")
        .args(["-d", distro])
        .args(["-e", "true"])
        .output()
        .map_err(|e| format!("Failed to start WSL: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Thread-safe WSL detection (for use across multiple modules)
pub type SharedWslCache = Arc<std::sync::Mutex<WslDetectionCache>>;

pub fn create_shared_cache() -> SharedWslCache {
    Arc::new(std::sync::Mutex::new(WslDetectionCache::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wsl_list_output() {
        let output = "NAME                   STATE           VERSION\n* Ubuntu              Running         2\n  docker-desktop      Stopped         2\n  docker-desktop-data Stopped         2";
        let result = parse_wsl_list_output(output);
        
        assert!(result.wsl_available);
        assert_eq!(result.default_distro, Some("Ubuntu".to_string()));
        assert_eq!(result.distros.len(), 3);
        assert_eq!(result.distros[0].name, "Ubuntu");
        assert_eq!(result.distros[0].state, DistroState::Running);
        assert_eq!(result.distros[0].version, 2);
    }
}
