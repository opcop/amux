//! Application configuration loaded from `~/.amux/config.toml`.
//!
//! All fields have sensible defaults. The config file is optional —
//! if missing or corrupted, defaults are used silently (corrupted files
//! print a warning to stderr).

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Built-in AI provider preset. Ships with the app — users only need
/// to supply an API key to activate one. The `env` map is a template:
/// values containing `{api_key}` are interpolated at runtime.
#[derive(Clone, Debug)]
pub(crate) struct AiPreset {
    pub name: &'static str,
    pub api_key_hint: &'static str,
    pub env: fn(api_key: &str) -> HashMap<String, String>,
}

/// Built-in presets. Order matters — shown in the picker in this order.
pub(crate) fn builtin_presets() -> Vec<AiPreset> {
    vec![
        AiPreset {
            name: "DeepSeek V4",
            api_key_hint: "sk-...",
            env: |key| {
                let mut m = HashMap::new();
                m.insert("ANTHROPIC_BASE_URL".into(), "https://api.deepseek.com/anthropic".into());
                m.insert("ANTHROPIC_AUTH_TOKEN".into(), key.into());
                m.insert("ANTHROPIC_MODEL".into(), "deepseek-v4-pro[1m]".into());
                m.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), "deepseek-v4-pro".into());
                m.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), "deepseek-v4-pro".into());
                m.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), "deepseek-v4-flash".into());
                m.insert("CLAUDE_CODE_SUBAGENT_MODEL".into(), "deepseek-v4-pro".into());
                m.insert("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(), "1".into());
                m
            },
        },
        AiPreset {
            name: "MiMo",
            api_key_hint: "tp-...",
            env: |key| {
                let mut m = HashMap::new();
                m.insert("ANTHROPIC_BASE_URL".into(), "https://token-plan-cn.xiaomimimo.com/anthropic".into());
                m.insert("ANTHROPIC_AUTH_TOKEN".into(), key.into());
                m.insert("ANTHROPIC_MODEL".into(), "mimo-v2.5-pro".into());
                m.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), "mimo-v2.5-pro".into());
                m.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), "mimo-v2.5-pro".into());
                m.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), "mimo-v2.5-pro".into());
                m
            },
        },
        AiPreset {
            name: "OpenRouter (Ling)",
            api_key_hint: "sk-or-...",
            env: |key| {
                let mut m = HashMap::new();
                m.insert("ANTHROPIC_BASE_URL".into(), "https://openrouter.ai/api".into());
                m.insert("ANTHROPIC_AUTH_TOKEN".into(), key.into());
                m.insert("ANTHROPIC_API_KEY".into(), "".into());
                m.insert("ANTHROPIC_MODEL".into(), "inclusionai/ling-2.6-1t:free".into());
                m.insert("ANTHROPIC_SMALL_FAST_MODEL".into(), "inclusionai/ling-2.6-1t:free".into());
                m.insert("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(), "1".into());
                m
            },
        },
    ]
}

/// Custom user-defined profile (from config.toml `[[ai_profiles]]`).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct AiProfile {
    pub name: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

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
    /// AI model provider profiles for Claude Code integration.
    /// Each profile defines env vars injected into new terminals.
    #[serde(default)]
    pub ai_profiles: Vec<AiProfile>,
    /// API keys for built-in presets. Maps preset name → API key.
    /// Stored here so users don't need to edit config.toml manually.
    #[serde(default)]
    pub ai_keys: HashMap<String, String>,
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
            ai_profiles: Vec::new(),
            ai_keys: HashMap::new(),
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

    /// Save ai_keys back to config.toml, preserving all other fields.
    pub fn save_ai_keys(&self) {
        let path = crate::gpui_workspace_persistence::amux_config_path();
        // Read existing config to preserve fields we don't touch.
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let mut doc: toml::Value = existing.parse().unwrap_or(toml::Value::Table(toml::map::Map::new()));
        // Update the [ai_keys] section.
        if let Some(table) = doc.as_table_mut() {
            let keys_table: toml::value::Table = self.ai_keys.iter()
                .map(|(k, v)| (k.clone(), toml::Value::String(v.clone())))
                .collect();
            table.insert("ai_keys".to_string(), toml::Value::Table(keys_table));
        }
        match toml::to_string_pretty(&doc) {
            Ok(output) => { let _ = std::fs::write(&path, output); }
            Err(e) => eprintln!("amux: failed to serialize config: {}", e),
        }
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
