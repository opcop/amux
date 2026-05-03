//! Semantic theme tokens, Tomorrow Night palette.
//!
//! This is the single source of truth for colors and radii used by
//! **new** UI code. Every constant is a `u32` so callers keep using
//! the existing `gpui::rgb(theme::SURFACE)` idiom with zero runtime
//! overhead.
//!
//! ## Why tokens?
//!
//! 13+ `gpui_*.rs` files were each picking hex colors independently,
//! and three different palettes (Tomorrow Night, Catppuccin Mocha,
//! Tokyo Night) drifted into `gpui_status_bar.rs` alone. See the
//! notes under `docs/problem-ui/`. CLAUDE.md declares Tomorrow Night
//! as the canonical default; every color here is that palette.
//!
//! ## Migration policy
//!
//! This module is deliberately **not** applied retroactively to
//! pre-existing UI. New code and UI touched in active work should
//! reach for `theme::*` instead of raw hex. Legacy call sites get
//! migrated as they're edited — mass rewrites are out of scope.
//!
//! When you add a color, prefer a **semantic** name (`DANGER_BG`)
//! over a paint name (`RED_DIM`). Semantic names survive palette
//! swaps; paint names don't.

// A token library is a *vocabulary*. It deliberately declares
// colors and radii that aren't all used yet, so new UI code has a
// ready palette to reach for. The unused-code warning is noise
// against that purpose — we suppress it for this module only.
#![allow(dead_code)]

// ─── Raw Tomorrow Night palette ─────────────────────────────────
// Private on purpose — outside code should use the semantic
// aliases below so we can reshuffle the palette without a churn
// commit every time.

const TN_BG:         u32 = 0x1d1f21;
const TN_BG_SOFT:    u32 = 0x141516; // slightly darker — dimmer surface
const TN_BG_RAISED:  u32 = 0x282a2e; // slightly lighter — hover / raised
const TN_FG:         u32 = 0xc5c8c6;
const TN_FG_DIM:     u32 = 0x969896;
const TN_BORDER:     u32 = 0x373b41;
const TN_BORDER_DIM: u32 = 0x282a2e;

const TN_RED:        u32 = 0xcc6666;
const TN_RED_BRIGHT: u32 = 0xd54e53;
const TN_GREEN:      u32 = 0xb5bd68;
const TN_YELLOW:     u32 = 0xf0c674;
const TN_BLUE:       u32 = 0x81a2be;
const TN_PURPLE:     u32 = 0xb294bb;
const TN_AQUA:       u32 = 0x8abeb7;

// ─── Semantic tokens ────────────────────────────────────────────

// Surfaces
/// Default panel / overlay background.
pub const SURFACE:        u32 = TN_BG;
/// Dimmer background for input fields or nested surfaces.
pub const SURFACE_DIM:    u32 = TN_BG_SOFT;
/// Slightly raised background — hover states, badges.
pub const SURFACE_RAISED: u32 = TN_BG_RAISED;

// Borders
/// Default visible border.
pub const BORDER:     u32 = TN_BORDER;
/// Subtler border for nested surfaces or separators.
pub const BORDER_DIM: u32 = TN_BORDER_DIM;

// Text
/// Primary readable text.
pub const TEXT:          u32 = TN_FG;
/// Secondary/muted text — hints, labels.
pub const TEXT_DIM:      u32 = TN_FG_DIM;
/// Disabled menu items / unavailable actions. Dimmer than
/// `TEXT_DIM` so the eye registers "inactive" rather than
/// "secondary".
pub const TEXT_DISABLED: u32 = 0x4a4d4e;

// Semantic accents
/// Accent used for neutral information or links.
pub const ACCENT:  u32 = TN_BLUE;
/// Info tint — cyan-ish; debug HUD, neutral badges.
pub const INFO:    u32 = TN_AQUA;
/// Success / "all green" indicators.
pub const SUCCESS: u32 = TN_GREEN;
/// Warning — caution but not failure.
pub const WARNING: u32 = TN_YELLOW;
/// Danger — crashes, errors, destructive actions.
pub const DANGER:  u32 = TN_RED;
/// Brighter danger for emphasis (text on `DANGER_BG`).
pub const DANGER_BRIGHT: u32 = TN_RED_BRIGHT;
/// Dim red wash for danger surfaces (e.g. crash badge background).
pub const DANGER_BG: u32 = 0x3a1e1e;

// Search mode badge surfaces. Tinted variants of SURFACE_RAISED
// so the three modes read differently at a glance without
// screaming. These intentionally stay separate from the generic
// surface tokens above — they're a single-purpose UI affordance.
pub const MODE_LITERAL_BG: u32 = 0x3a3a4a;
pub const MODE_REGEX_BG:   u32 = 0x4a3a3a;
pub const MODE_FUZZY_BG:   u32 = 0x3a4a3a;

/// Background wash for *non-current* scrollback search matches.
/// Drawn **under** the primary selection color so the current
/// match (which is painted via the terminal's Selection) still
/// visibly pops. Dim mustard was chosen because it reads
/// differently from Tomorrow Night's blue selection
/// (`theme::ACCENT`) and from the red `DANGER` and green
/// `SUCCESS` semantic tints — zero risk of confusing a search
/// hit for an error or "success" marker.
pub const MATCH_HIGHLIGHT_BG: u32 = 0x5c4a20;

// ─── Radii ──────────────────────────────────────────────────────

pub const RADIUS_SM: f32 = 3.0;
pub const RADIUS_MD: f32 = 6.0;
pub const RADIUS_LG: f32 = 8.0;

// ─── Layout constants ────────────────────────────────────────────

/// Status bar height in pixels (26px content + 8px top padding)
pub const STATUS_BAR_H: f32 = 34.0;
/// macOS transparent titlebar inset height
pub const TITLEBAR_H: f32 = 28.0;
/// Tab strip height per pane
pub const TAB_STRIP_H: f32 = 28.0;
/// Split resize handle thickness
pub const SPLIT_HANDLE_W: f32 = 10.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// Prevents silent accidental drift away from Tomorrow Night.
    /// Anyone changing these values must consciously update this
    /// test, which makes palette edits reviewable in a diff.
    #[test]
    fn tomorrow_night_canonical_values() {
        assert_eq!(SURFACE, 0x1d1f21);
        assert_eq!(TEXT, 0xc5c8c6);
        assert_eq!(DANGER, 0xcc6666);
        assert_eq!(ACCENT, 0x81a2be);
    }

    #[test]
    fn mode_badges_are_distinct() {
        assert_ne!(MODE_LITERAL_BG, MODE_REGEX_BG);
        assert_ne!(MODE_REGEX_BG, MODE_FUZZY_BG);
        assert_ne!(MODE_LITERAL_BG, MODE_FUZZY_BG);
    }
}
