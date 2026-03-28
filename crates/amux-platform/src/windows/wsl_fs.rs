//! WSL Filesystem Operations
//!
//! Provides direct filesystem access to WSL distributions via WSL.exe commands.

use std::process::Command;

use crate::FsEntry;

/// WSL filesystem operation errors
#[derive(Debug)]
pub enum WslFsError {
    CommandFailed(String),
    ParseError(String),
    DistroNotFound(String),
    PathNotFound(String),
}

impl std::fmt::Display for WslFsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WslFsError::CommandFailed(msg) => write!(f, "WSL command failed: {}", msg),
            WslFsError::ParseError(msg) => write!(f, "Failed to parse WSL output: {}", msg),
            WslFsError::DistroNotFound(distro) => write!(f, "Distro not found: {}", distro),
            WslFsError::PathNotFound(path) => write!(f, "Path not found: {}", path),
        }
    }
}

impl std::error::Error for WslFsError {}

/// Read directory contents from a WSL distribution using `wsl ls`
/// 
/// This uses the WSL.exe command to directly access the Linux filesystem,
/// which is more reliable than UNC paths for certain operations.
pub fn wsl_read_dir(distro: &str, path: &str) -> Result<Vec<FsEntry>, WslFsError> {
    // Validate distro name (basic security check)
    if distro.is_empty() || distro.contains([' ', '\0', '\n']) {
        return Err(WslFsError::DistroNotFound(distro.to_string()));
    }
    
    // Sanitize path - remove leading slash for WSL command
    let clean_path = path.trim_start_matches('/');
    let display_path = if clean_path.is_empty() { "." } else { clean_path };
    
    // Use ls with long format to get file type info
    let output = Command::new("wsl")
        .args(["-d", distro])
        .args(["--", "ls", "-la", "--color=never", display_path])
        .output()
        .map_err(|e| WslFsError::CommandFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no such file or directory") {
            return Err(WslFsError::PathNotFound(path.to_string()));
        }
        return Err(WslFsError::CommandFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ls_output(&stdout, path)
}

/// Read a file from a WSL distribution
pub fn wsl_read_file(distro: &str, path: &str) -> Result<String, WslFsError> {
    let clean_path = path.trim_start_matches('/');
    
    let output = Command::new("wsl")
        .args(["-d", distro])
        .args(["--", "cat", clean_path])
        .output()
        .map_err(|e| WslFsError::CommandFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no such file or directory") || stderr.contains("Is a directory") {
            return Err(WslFsError::PathNotFound(path.to_string()));
        }
        return Err(WslFsError::CommandFailed(stderr.to_string()));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| WslFsError::ParseError(e.to_string()))
}

/// Write a file to a WSL distribution
pub fn wsl_write_file(distro: &str, path: &str, content: &str) -> Result<(), WslFsError> {
    // Use a temp file approach for writing since piping to cat is complex
    let clean_path = path.trim_start_matches('/');
    
    // Create a temporary file for content
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("amux_wsl_write_{}.tmp", std::process::id()));
    
    // Write content to temp file
    std::fs::write(&temp_file, content)
        .map_err(|e| WslFsError::CommandFailed(format!("Failed to create temp file: {}", e)))?;
    
    // Use cmd copy to transfer from Windows temp to WSL path
    let result = Command::new("cmd")
        .args(["/C", "copy", &temp_file.display().to_string(), &format!(r"\\wsl$\{}\{}", distro, clean_path)])
        .output();
    
    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);
    
    match result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(WslFsError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        )),
        Err(e) => Err(WslFsError::CommandFailed(e.to_string())),
    }
}

/// Check if a path exists in WSL
pub fn wsl_path_exists(distro: &str, path: &str) -> Result<bool, WslFsError> {
    let clean_path = path.trim_start_matches('/');
    let check_path = if clean_path.is_empty() { "." } else { clean_path };
    
    let output = Command::new("wsl")
        .args(["-d", distro])
        .args(["--", "test", "-e", check_path])
        .args(["&&", "echo", "exists"])
        .output()
        .map_err(|e| WslFsError::CommandFailed(e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.contains("exists"))
}

/// Get file/directory metadata from WSL
#[derive(Debug)]
pub struct WslMetadata {
    pub size: u64,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub modified: Option<String>,
    pub permissions: String,
}

pub fn wsl_stat(distro: &str, path: &str) -> Result<WslMetadata, WslFsError> {
    let clean_path = path.trim_start_matches('/');
    let stat_path = if clean_path.is_empty() { "." } else { clean_path };
    
    // Use stat with specific format
    let output = Command::new("wsl")
        .args(["-d", distro])
        .args(["--", "stat", "-c", "%s %F %L %Y %a", stat_path])
        .output()
        .map_err(|e| WslFsError::CommandFailed(e.to_string()))?;

    if !output.status.success() {
        return Err(WslFsError::PathNotFound(path.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_stat_output(&stdout)
}

/// Parse `ls -la` output into FsEntry structs
fn parse_ls_output(output: &str, base_path: &str) -> Result<Vec<FsEntry>, WslFsError> {
    let mut entries = Vec::new();
    let base = base_path.trim_end_matches('/');

    for line in output.lines() {
        let line = line.trim();
        
        // Skip the "total" line and empty lines
        if line.is_empty() || line.starts_with("total ") {
            continue;
        }

        // Parse ls -la format:
        // -rwxr-xr-x 1 user group   1234 Jan 15 10:30 filename
        // drwxr-xr-x 2 user group   4096 Jan 15 10:30 dirname
        // lrwxrwxrwx 1 user group      5 Jan 15 10:30 link -> target
        
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }

        let permissions = parts[0];
        let _size_or_blocks = parts[4];
        let name = parts[8..].join(" ").split(" -> ").next().unwrap_or("").trim().to_string();
        
        // Skip "." and ".."
        if name == "." || name == ".." {
            continue;
        }

        let is_dir = permissions.starts_with('d');
        let is_symlink = permissions.starts_with('l');
        
        // Build relative path
        let entry_relative = if base.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", base, name)
        };

        entries.push(FsEntry {
            name,
            relative_path: entry_relative,
            is_dir: is_dir || is_symlink, // Treat symlinks to dirs as dirs for simplicity
        });
    }

    Ok(entries)
}

/// Parse `stat -c` output into WslMetadata
fn parse_stat_output(output: &str) -> Result<WslMetadata, WslFsError> {
    let parts: Vec<&str> = output.trim().split_whitespace().collect();
    if parts.len() < 5 {
        return Err(WslFsError::ParseError("Invalid stat output".to_string()));
    }

    let size: u64 = parts[0].parse()
        .map_err(|_| WslFsError::ParseError("Invalid size".to_string()))?;
    
    let kind_str = parts[1];
    let is_dir = kind_str == "directory";
    let is_symlink = kind_str == "symlink";
    
    let modified = parts.get(3).map(|s| s.to_string());
    let permissions = parts.get(4).map(|s| s.to_string()).unwrap_or_default();

    Ok(WslMetadata {
        size,
        is_dir,
        is_symlink,
        modified,
        permissions,
    })
}

/// Join paths in Unix style
pub fn wsl_join_path(base: &str, relative: &str) -> String {
    let base = base.trim_end_matches('/');
    let relative = relative.trim_start_matches('/');
    if base.is_empty() {
        relative.to_string()
    } else {
        format!("{}/{}", base, relative)
    }
}

/// Get the parent directory path
pub fn wsl_parent_path(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 1 {
        None
    } else {
        Some(parts[..parts.len() - 1].join("/"))
    }
}

/// List root directory of a WSL distribution
pub fn wsl_list_root(distro: &str) -> Result<Vec<FsEntry>, WslFsError> {
    wsl_read_dir(distro, "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ls_output() {
        let ls_output = r#"total 64
drwxr-xr-x 20 root root 4096 Jan 15 10:30 .
drwxr-xr-x  1 root root 4096 Jan 15 10:30 ..
drwxr-xr-x  8 root root 4096 Jan 15 09:00 home
-rw-r--r--  1 root root 220 Jan 15 10:30 .bashrc
-rw-r--r--  1 root root 3107 Jan 15 10:30 README.md
drwxr-xr-x  2 root root 4096 Jan 15 10:30 projects"#;

        let entries = parse_ls_output(ls_output, "").unwrap();
        
        // Should have: .bashrc, README.md, home, projects (not . and ..)
        assert_eq!(entries.len(), 4);
        
        let readme = entries.iter().find(|e| e.name == "README.md").unwrap();
        assert!(!readme.is_dir);
        
        let home = entries.iter().find(|e| e.name == "home").unwrap();
        assert!(home.is_dir);
    }

    #[test]
    fn joins_paths() {
        assert_eq!(wsl_join_path("", "foo"), "foo");
        assert_eq!(wsl_join_path("bar", "foo"), "bar/foo");
        assert_eq!(wsl_join_path("bar/", "foo"), "bar/foo");
        assert_eq!(wsl_join_path("bar/baz", "../foo"), "bar/baz/../foo");
    }

    #[test]
    fn gets_parent_paths() {
        assert_eq!(wsl_parent_path("foo"), None);
        assert_eq!(wsl_parent_path("/"), None);
        assert_eq!(wsl_parent_path("foo/bar"), Some("foo".to_string()));
        assert_eq!(wsl_parent_path("/home/user"), Some("/home".to_string()));
    }
}
