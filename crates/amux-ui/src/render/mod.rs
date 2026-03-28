mod text;

#[cfg(feature = "gpui")]
mod gpui;

use crate::AppSnapshot;

pub use text::TextRenderer;

#[cfg(feature = "gpui")]
pub use gpui::{
    GpuiActiveSurfaceItem, GpuiAgentItem, GpuiFileItem, GpuiOpenFileItem, GpuiPaletteCommandItem,
    GpuiPaneItem, GpuiRenderer, GpuiSection, GpuiTabItem, GpuiWindowModel, GpuiWorkspaceItem,
};

pub trait AppRenderer {
    type Output;

    fn render(&self, app_name: &str, snapshot: &AppSnapshot) -> Self::Output;
}
