use serde::{Serialize, Deserialize};

/// A structured message for inter-pane communication.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BridgeMessage {
    pub workspace: String,
    pub pane_id: String,
    pub agent: String,  // agent kind or "user"
    pub text: String,
}

const ENVELOPE_PREFIX: &str = "[amux-bridge ";

impl BridgeMessage {
    /// Format as the envelope string for terminal delivery.
    pub fn format(&self) -> String {
        format!("[amux-bridge workspace:{} pane:{} agent:{}] {}",
            self.workspace, self.pane_id, self.agent, self.text)
    }

    /// Parse an envelope string back into a BridgeMessage.
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if !line.starts_with(ENVELOPE_PREFIX) { return None; }
        let bracket_end = line.find(']')?;
        let header = &line[ENVELOPE_PREFIX.len()..bracket_end];
        let text = line[bracket_end + 1..].trim().to_string();

        let mut workspace = String::new();
        let mut pane_id = String::new();
        let mut agent = String::new();

        for part in header.split_whitespace() {
            if let Some(val) = part.strip_prefix("workspace:") {
                workspace = val.to_string();
            } else if let Some(val) = part.strip_prefix("pane:") {
                pane_id = val.to_string();
            } else if let Some(val) = part.strip_prefix("agent:") {
                agent = val.to_string();
            }
        }

        if workspace.is_empty() || pane_id.is_empty() { return None; }

        Some(BridgeMessage { workspace, pane_id, agent, text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let msg = BridgeMessage {
            workspace: "ws1".into(),
            pane_id: "p1".into(),
            agent: "claude".into(),
            text: "hello world".into(),
        };
        let formatted = msg.format();
        assert_eq!(formatted, "[amux-bridge workspace:ws1 pane:p1 agent:claude] hello world");
        let parsed = BridgeMessage::parse(&formatted).expect("should parse");
        assert_eq!(parsed.workspace, "ws1");
        assert_eq!(parsed.pane_id, "p1");
        assert_eq!(parsed.agent, "claude");
        assert_eq!(parsed.text, "hello world");
    }

    #[test]
    fn parse_rejects_invalid() {
        assert!(BridgeMessage::parse("not a bridge message").is_none());
        assert!(BridgeMessage::parse("[amux-bridge ] text").is_none());
    }
}
