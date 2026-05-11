//! Keyboard event → PTY input byte conversion.
//!
//! Self-contained encoder for keyboard input. Translates GPUI-style key
//! identifiers (e.g. "Enter", "ArrowUp", "F5") plus modifier flags into the
//! escape sequences a PTY child process expects, with optional support for
//! application cursor key mode.
//!
//! Lifted out of the legacy `terminal/view.rs` during the cross-platform
//! cleanup so the rest of that file (an unused in-house terminal view) can be
//! retired without taking this still-live encoder with it.

/// Convert keyboard event to PTY input bytes.
pub fn to_pty(key: &str, ctrl: bool, shift: bool, alt: bool) -> Vec<u8> {
    to_pty_with_mode(key, ctrl, shift, alt, false)
}

/// Convert keyboard event to PTY input bytes, with application cursor key mode support.
pub fn to_pty_with_mode(key: &str, ctrl: bool, shift: bool, alt: bool, app_cursor: bool) -> Vec<u8> {
    // Special keys
    match key {
        "Enter" => return if alt || shift { vec![0x0A] } else { vec![0x0D] },
        "Tab" => return vec![0x09],
        "Escape" => return vec![0x1B],
        "Backspace" => return vec![0x7F],
        "ArrowUp" | "ArrowDown" | "ArrowRight" | "ArrowLeft" => {
            let ch = match key {
                "ArrowUp" => "A",
                "ArrowDown" => "B",
                "ArrowRight" => "C",
                "ArrowLeft" => "D",
                _ => unreachable!(),
            };
            // Always use CSI encoding (ESC [ A/B/C/D) for arrow keys
            // regardless of DECCKM application cursor mode. The SS3
            // encoding (ESC O A) can cause ^[[A to appear on screen
            // when the mode read races with the application toggling
            // \x1b[?1h / \x1b[?1l. Modern shells and readline accept
            // CSI arrows in both modes; only niche legacy apps require
            // SS3, and those apps can use Home/End which still respect
            // app_cursor below.
            return escape_seq(ch, ctrl, shift, alt);
        }
        "Home" => return if app_cursor { vec![0x1B, b'O', b'H'] } else { escape_seq("H", ctrl, shift, alt) },
        "End" => return if app_cursor { vec![0x1B, b'O', b'F'] } else { escape_seq("F", ctrl, shift, alt) },
        "PageUp" => return escape_seq("5~", ctrl, shift, alt),
        "PageDown" => return escape_seq("6~", ctrl, shift, alt),
        "Insert" => return escape_seq("2~", ctrl, shift, alt),
        "Delete" => return escape_seq("3~", ctrl, shift, alt),
        "F1" => return vec![0x1B, 0x4F, 0x50],
        "F2" => return vec![0x1B, 0x4F, 0x51],
        "F3" => return vec![0x1B, 0x4F, 0x52],
        "F4" => return vec![0x1B, 0x4F, 0x53],
        "F5" => return vec![0x1B, 0x5B, 0x31, 0x35, 0x7E],
        "F6" => return vec![0x1B, 0x5B, 0x31, 0x37, 0x7E],
        "F7" => return vec![0x1B, 0x5B, 0x31, 0x38, 0x7E],
        "F8" => return vec![0x1B, 0x5B, 0x31, 0x39, 0x7E],
        "F9" => return vec![0x1B, 0x5B, 0x32, 0x30, 0x7E],
        "F10" => return vec![0x1B, 0x5B, 0x32, 0x31, 0x7E],
        "F11" => return vec![0x1B, 0x5B, 0x32, 0x33, 0x7E],
        "F12" => return vec![0x1B, 0x5B, 0x32, 0x34, 0x7E],
        _ => {}
    }

    // Control characters
    if ctrl && key.len() == 1 {
        if let Some(c) = key.chars().next() {
            if c.is_ascii_alphabetic() {
                let ctrl_char = (c.to_ascii_uppercase() as u8) - b'A' + 1;
                return vec![ctrl_char];
            }
            match c {
                '[' => return vec![0x1B],
                '\\' => return vec![0x1C],
                ']' => return vec![0x1D],
                '^' => return vec![0x1E],
                '_' => return vec![0x1F],
                _ => {}
            }
        }
    }

    // Alt modifier
    let mut result = Vec::new();
    if alt {
        result.push(0x1B);
    }

    // Handle shift for special characters
    if shift && key == "Space" {
        result.push(b' ');
        return result;
    }

    // Regular characters
    if key == "Space" {
        result.push(b' ');
        return result;
    }

    for c in key.chars() {
        if c == ' ' {
            result.push(b' ');
        } else if c.is_ascii() {
            let byte = if shift && c.is_ascii_lowercase() {
                c.to_ascii_uppercase() as u8
            } else {
                c as u8
            };
            result.push(byte);
        } else {
            // UTF-8
            let mut buf = [0u8; 4];
            let encoded = c.encode_utf8(&mut buf);
            result.extend_from_slice(encoded.as_bytes());
        }
    }

    result
}

/// Generate xterm-style escape sequence with modifier encoding.
///
/// Modifier parameter (CSI 1;Ps suffix):
///   2 = Shift, 3 = Alt, 4 = Shift+Alt, 5 = Ctrl, 6 = Ctrl+Shift,
///   7 = Ctrl+Alt, 8 = Ctrl+Shift+Alt
///
/// Without modifiers the sequence is just `ESC [ suffix`.
/// With modifiers it becomes `ESC [ 1 ; Ps suffix`.
fn escape_seq(suffix: &str, ctrl: bool, shift: bool, alt: bool) -> Vec<u8> {
    let mut result = Vec::new();

    // Compute the xterm modifier parameter. A value of 1 means "no
    // modifier" but is never emitted — the bare sequence is used
    // instead.
    let modifier = 1
        + if shift { 1 } else { 0 }
        + if alt   { 2 } else { 0 }
        + if ctrl  { 4 } else { 0 };

    if alt && modifier == 3 {
        // Alt-only without CSI modifier encoding: ESC ESC [ suffix
        // (some terminals prefer this for Alt+Arrow)
        result.push(0x1B);
    }

    result.push(0x1B);
    result.push(b'[');

    if modifier > 1 {
        result.push(b'1');
        result.push(b';');
        // modifier is 2..=8, fits in one ASCII digit
        result.push(b'0' + modifier as u8);
    }

    result.extend_from_slice(suffix.as_bytes());
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enter() {
        assert_eq!(to_pty("Enter", false, false, false), vec![0x0D]);
    }

    #[test]
    fn test_shift_enter_sends_lf() {
        assert_eq!(to_pty("Enter", false, true, false), vec![0x0A]);
    }

    #[test]
    fn test_alt_enter_sends_lf() {
        assert_eq!(to_pty("Enter", false, false, true), vec![0x0A]);
    }

    #[test]
    fn test_ctrl_c() {
        assert_eq!(to_pty("c", true, false, false), vec![0x03]);
    }

    #[test]
    fn test_arrow_up() {
        assert_eq!(to_pty("ArrowUp", false, false, false), vec![0x1B, 0x5B, 0x41]);
    }

    #[test]
    fn test_app_cursor_mode_arrow() {
        // Arrow keys always use CSI encoding regardless of app_cursor
        // to avoid ^[[A glitches from DECCKM mode race conditions.
        assert_eq!(
            to_pty_with_mode("ArrowUp", false, false, false, true),
            vec![0x1B, 0x5B, 0x41]
        );
    }
}
