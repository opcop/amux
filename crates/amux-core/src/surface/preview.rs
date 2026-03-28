use crate::SurfaceId;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PreviewSurfaceState {
    pub surface_id: SurfaceId,
    pub source_relative_path: String,
    pub kind: PreviewKind,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PreviewKind {
    Markdown,
    PlainText,
}
