use crate::SurfaceId;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EditorSurfaceState {
    pub surface_id: SurfaceId,
    pub relative_path: String,
    pub language: Option<String>,
    pub dirty: bool,
    pub readonly: bool,
}
