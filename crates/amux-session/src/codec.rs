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
    use amux_core::{SessionState, WorkspaceTarget};

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

    #[test]
    fn codec_decodes_legacy_windows_workspace_session() {
        let codec = JsonSessionCodec;
        let raw = r#"
        {
          "version": 1,
          "active_workspace_id": "workspace-1",
          "workspaces": [
            {
              "id": "workspace-1",
              "name": "amux",
              "target": {
                "WindowsPath": {
                  "path": "D:/repo/amux"
                }
              },
              "layout": {
                "Pane": {
                  "pane_id": "pane-workspace-1-1",
                  "tabs": [
                    {
                      "id": "tab-workspace-1-1",
                      "title": "Welcome",
                      "pinned": false,
                      "surface": {
                        "Welcome": {
                          "surface_id": "surface-workspace-1-1",
                          "title": "Welcome"
                        }
                      }
                    }
                  ],
                  "active_tab_id": "tab-workspace-1-1"
                }
              },
              "active_pane_id": "pane-workspace-1-1",
              "env_profile_id": null,
              "default_agent_provider_id": null,
              "recent_files": []
            }
          ],
          "recent_workspaces": [],
          "ui_preferences": {
            "sidebar_collapsed": false,
            "sidebar_width": 240,
            "font_size": 14,
            "theme": "system"
          },
          "last_saved": null
        }
        "#;

        let decoded = codec.decode(raw).expect("legacy session should decode");

        assert_eq!(decoded.version, 2);
        assert_eq!(decoded.recent_workspaces.len(), 1);
        assert!(matches!(
            decoded.workspaces[0].target,
            WorkspaceTarget::WindowsPath { .. }
        ));
    }
}
