use amux_core::SessionState;

pub trait SessionCodec {
    fn encode(&self, session: &SessionState) -> Result<String, String>;
    fn decode(&self, raw: &str) -> Result<SessionState, String>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JsonSessionCodec;

impl SessionCodec for JsonSessionCodec {
    fn encode(&self, session: &SessionState) -> Result<String, String> {
        serde_json::to_string_pretty(session).map_err(|err| err.to_string())
    }

    fn decode(&self, raw: &str) -> Result<SessionState, String> {
        let session: SessionState = serde_json::from_str(raw).map_err(|err| err.to_string())?;
        Ok(amux_core::normalize_session(session))
    }
}

#[cfg(test)]
mod tests {
    use amux_core::SessionState;

    use super::{JsonSessionCodec, SessionCodec};

    #[test]
    fn codec_round_trips_session() {
        let codec = JsonSessionCodec;
        let session = SessionState::default();

        let encoded = codec.encode(&session).expect("session should encode");
        let decoded = codec.decode(&encoded).expect("session should decode");

        assert_eq!(decoded.version, 2);
        assert_eq!(decoded.workspaces.len(), 0);
    }
}
