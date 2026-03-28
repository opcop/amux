pub mod fs;
pub mod path_mapper;
pub mod process;
pub mod shell;
pub mod sys_metrics;
pub mod terminal;
pub mod terminal_output;
pub mod unix;
pub mod windows;

pub use fs::*;
pub use path_mapper::*;
pub use process::*;
pub use shell::*;
pub use sys_metrics::*;
pub use terminal::*;
pub use terminal_output::*;

// Re-export WSL detection types on Windows
#[cfg(target_os = "windows")]
pub use windows::wsl_detection::{
    detect_wsl_distributions, ensure_distro_running, get_default_distro, is_wsl_installed,
    WslDetectionCache, WslDetectionResult, WslDistroInfo, DistroState,
};

// Re-export WSL filesystem operations on Windows
#[cfg(target_os = "windows")]
pub use windows::wsl_fs::{
    wsl_read_dir, wsl_read_file, wsl_write_file, wsl_path_exists, wsl_stat,
    wsl_list_root, wsl_join_path, wsl_parent_path,
    WslFsError, WslMetadata,
};
