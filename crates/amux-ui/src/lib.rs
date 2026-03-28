pub mod commands;
pub mod components;
pub mod controller;
pub mod panels;
pub mod render;
pub mod root;
pub mod state;
pub mod surfaces;

pub use commands::{PaletteCategory, PaletteCommand, UiAction};
pub use controller::{AutoSaveConfig, AutoSaveState};
pub use render::AppRenderer;
#[cfg(feature = "gpui")]
pub use render::GpuiRenderer;
#[cfg(feature = "gpui")]
pub use render::{
    GpuiActiveSurfaceItem, GpuiAgentItem, GpuiFileItem, GpuiOpenFileItem, GpuiPaletteCommandItem,
    GpuiPaneItem, GpuiTabItem, GpuiWindowModel, GpuiWorkspaceItem,
};
pub use render::TextRenderer;
pub use root::DesktopApp;
pub use state::{
    ActiveSurfaceItem, AgentListItem, AppSnapshot, FileListItem, LayoutSnapshot, OpenFileItem,
    PaneSnapshot, RecentWorkspaceItem, SaveStatus, SplitSnapshot, TabSnapshot, UiState,
    WorkspaceListItem, WorkspaceSnapshot,
};
