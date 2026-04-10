//! Application configuration loaded from `~/.amux/config.toml`.
//!
//! All fields have sensible defaults. The config file is optional —
//! if missing or corrupted, defaults are used silently (corrupted files
//! print a warning to stderr).

use serde::Deserialize;

/// Application configuration.
///
/// Example `~/.amux/config.toml`:
/// ```toml
/// font_family = "JetBrains Mono"
/// font_size = 15.0
/// line_height = 1.5
/// theme = "catppuccin-mocha"
/// ```
///
/// Available themes: `tomorrow-night` (default), `catppuccin-mocha`, `dracula`,
/// `solarized-dark`, `one-dark`.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct AmuxConfig {
    /// Terminal font family name.
    pub font_family: String,
    /// Terminal font size in pixels.
    pub font_size: f32,
    /// Line height as a multiplier of font size (e.g. 1.4 = 140%).
    pub line_height: f32,
    /// Theme name.
    pub theme: String,
    /// Scrollback buffer size in lines.
    pub scrollback: usize,
}

impl Default for AmuxConfig {
    fn default() -> Self {
        Self {
            // Per-platform default monospace font. Must be a font that
            // is guaranteed to be installed on a stock OS — falling back
            // through the FontFallbacks chain produces slightly different
            // advance widths which makes the terminal cursor drift from
            // the text (each character is off by a fraction of a pixel,
            // accumulating visibly over a full line).
            font_family: if cfg!(target_os = "macos") {
                "Menlo".to_string()
            } else if cfg!(target_os = "windows") {
                "Cascadia Code".to_string()
            } else {
                "DejaVu Sans Mono".to_string()
            },
            font_size: 14.0,
            line_height: 1.4,
            theme: "tomorrow-night".to_string(),
            scrollback: 10000,
        }
    }
}

impl AmuxConfig {
    /// Load configuration from `~/.amux/config.toml`.
    ///
    /// - File missing → defaults (silent)
    /// - Parse error → defaults + stderr warning
    /// - Partial fields → missing fields filled from defaults via `#[serde(default)]`
    pub fn load() -> Self {
        let path = crate::gpui_workspace_persistence::amux_config_path();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };
        match toml::from_str::<AmuxConfig>(&content) {
            Ok(mut config) => {
                config.sanitize();
                config
            }
            Err(e) => {
                eprintln!("amux: failed to parse {}: {}", path.display(), e);
                Self::default()
            }
        }
    }

    /// Clamp values to safe ranges.
    fn sanitize(&mut self) {
        self.font_size = self.font_size.clamp(6.0, 72.0);
        self.line_height = self.line_height.clamp(1.0, 3.0);
        self.scrollback = self.scrollback.clamp(100, 100_000);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let config = AmuxConfig::default();
        assert_eq!(config.font_family, AmuxConfig::default().font_family);
        assert_eq!(config.font_size, 14.0);
        assert_eq!(config.line_height, 1.4);
        assert_eq!(config.theme, "tomorrow-night");
    }

    #[test]
    fn partial_toml_merges_with_defaults() {
        let toml_str = r#"font_size = 18.0"#;
        let config: AmuxConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.font_size, 18.0);
        assert_eq!(config.font_family, AmuxConfig::default().font_family); // default preserved
        assert_eq!(config.line_height, 1.4); // default preserved
    }

    #[test]
    fn empty_toml_gives_defaults() {
        let config: AmuxConfig = toml::from_str("").unwrap();
        assert_eq!(config.font_family, AmuxConfig::default().font_family);
        assert_eq!(config.font_size, 14.0);
    }

    #[test]
    fn invalid_toml_is_error() {
        let result = toml::from_str::<AmuxConfig>("not valid {{{{");
        assert!(result.is_err());
    }

    #[test]
    fn sanitize_clamps_values() {
        let toml_str = r#"
            font_size = 2.0
            line_height = 0.5
            scrollback = 50
        "#;
        let mut config: AmuxConfig = toml::from_str(toml_str).unwrap();
        config.sanitize();
        assert_eq!(config.font_size, 6.0);
        assert_eq!(config.line_height, 1.0);
        assert_eq!(config.scrollback, 100); // clamped to minimum
    }

    #[test]
    fn sanitize_clamps_scrollback_max() {
        let toml_str = r#"scrollback = 999999"#;
        let mut config: AmuxConfig = toml::from_str(toml_str).unwrap();
        config.sanitize();
        assert_eq!(config.scrollback, 100_000);
    }

    #[test]
    fn scrollback_default() {
        let config = AmuxConfig::default();
        assert_eq!(config.scrollback, 10000);
    }

    #[test]
    fn theme_default_is_tomorrow_night() {
        let config: AmuxConfig = toml::from_str("").unwrap();
        assert_eq!(config.theme, "tomorrow-night");
    }
}
