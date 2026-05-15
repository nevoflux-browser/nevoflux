//! Native file picker dialog support.
//!
//! On macOS, uses `osascript` (AppleScript) to show the native file dialog
//! because the daemon runs without an NSApplication run loop, which prevents
//! `rfd` from working correctly. On Linux, uses `rfd` with the GTK backend.

use nevoflux_protocol::{
    FileInfo, PickFilesError, PickFilesRequest, PickFilesResponse, PickerMode,
};
use std::path::Path;
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Lock to prevent concurrent file picker dialogs
static FILE_PICKER_LOCK: Mutex<()> = Mutex::const_new(());

/// Create FileInfo from a path by reading metadata
pub fn file_info_from_path(path: &Path) -> std::io::Result<FileInfo> {
    let metadata = std::fs::metadata(path)?;
    Ok(FileInfo {
        path: path.to_string_lossy().to_string(),
        is_directory: metadata.is_dir(),
        size: if metadata.is_file() {
            Some(metadata.len())
        } else {
            None
        },
        modified: metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
    })
}

/// Check if a graphical display is available (Linux only)
#[cfg(target_os = "linux")]
fn has_display() -> bool {
    std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok()
}

#[cfg(not(target_os = "linux"))]
fn has_display() -> bool {
    true
}

/// Pick files or directories using the native file dialog.
///
/// Only one dialog can be open at a time.
pub async fn pick_files(req: PickFilesRequest) -> Result<PickFilesResponse, PickFilesError> {
    // Check for display on Linux
    if !has_display() {
        return Err(PickFilesError::NoDisplay);
    }

    // Prevent concurrent dialogs
    let _guard = match FILE_PICKER_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(_) => return Err(PickFilesError::AlreadyPicking),
    };

    debug!(
        "Opening file picker: mode={:?}, multiple={}, title={:?}",
        req.mode, req.multiple, req.title
    );

    pick_files_impl(req).await
}

/// macOS implementation using osascript (AppleScript).
///
/// The daemon runs as a background tokio service without an NSApplication
/// run loop, so rfd's NSOpenPanel cannot display. osascript spawns its own
/// Cocoa event loop and returns POSIX paths.
#[cfg(target_os = "macos")]
async fn pick_files_impl(req: PickFilesRequest) -> Result<PickFilesResponse, PickFilesError> {
    let script = build_applescript(&req);
    debug!("Running osascript: {}", script);

    let output = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .await
        .map_err(|e| PickFilesError::DialogFailed(format!("Failed to run osascript: {}", e)))?;

    // Exit code 1 with stderr containing "User canceled" means the user cancelled
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("User canceled") || stderr.contains("cancelled") {
            debug!("File picker cancelled by user");
            return Ok(PickFilesResponse {
                files: vec![],
                cancelled: true,
            });
        }
        return Err(PickFilesError::DialogFailed(format!(
            "osascript failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let paths: Vec<&str> = stdout
        .trim()
        .split('\n')
        .filter(|s| !s.is_empty())
        .collect();

    let file_infos: Vec<FileInfo> = paths
        .into_iter()
        .filter_map(|p| match file_info_from_path(Path::new(p)) {
            Ok(info) => Some(info),
            Err(e) => {
                warn!("Failed to read metadata for {}: {}", p, e);
                None
            }
        })
        .collect();

    debug!("Selected {} files via osascript", file_infos.len());

    Ok(PickFilesResponse {
        files: file_infos,
        cancelled: false,
    })
}

/// Build an AppleScript command for the file picker.
#[cfg(target_os = "macos")]
fn build_applescript(req: &PickFilesRequest) -> String {
    let prompt = req.title.as_deref().unwrap_or("Select files");

    let default_location = req
        .default_path
        .as_deref()
        .filter(|p| Path::new(p).is_dir())
        .map(|p| format!(" default location POSIX file \"{}\"", p))
        .unwrap_or_default();

    match req.mode {
        PickerMode::Files | PickerMode::Both => {
            if req.multiple {
                // Multiple files: returns a list of aliases
                format!(
                    r#"set theFiles to choose file with prompt "{prompt}" with multiple selections allowed{default_location}
set posixPaths to ""
repeat with f in theFiles
    set posixPaths to posixPaths & POSIX path of f & "\n"
end repeat
return posixPaths"#
                )
            } else {
                // Single file
                format!(
                    r#"return POSIX path of (choose file with prompt "{prompt}"{default_location})"#
                )
            }
        }
        PickerMode::Directories => {
            if req.multiple {
                // Multiple folders: returns a list of aliases
                format!(
                    r#"set theFolders to choose folder with prompt "{prompt}" with multiple selections allowed{default_location}
set posixPaths to ""
repeat with f in theFolders
    set posixPaths to posixPaths & POSIX path of f & "\n"
end repeat
return posixPaths"#
                )
            } else {
                // Single folder
                format!(
                    r#"return POSIX path of (choose folder with prompt "{prompt}"{default_location})"#
                )
            }
        }
    }
}

/// Windows implementation using rfd.
///
/// Note: `Both` mode is handled at the server level by asking the sidebar
/// to choose between files or directories before reaching this function.
#[cfg(target_os = "windows")]
async fn pick_files_impl(req: PickFilesRequest) -> Result<PickFilesResponse, PickFilesError> {
    pick_files_rfd(req).await
}

/// Linux implementation using rfd.
///
/// Note: `Both` mode is handled at the server level by asking the sidebar
/// to choose between files or directories before reaching this function.
#[cfg(target_os = "linux")]
async fn pick_files_impl(req: PickFilesRequest) -> Result<PickFilesResponse, PickFilesError> {
    pick_files_rfd(req).await
}

/// Shared rfd-based file picker implementation for Linux and Windows (Files/Directories modes).
#[cfg(not(target_os = "macos"))]
async fn pick_files_rfd(req: PickFilesRequest) -> Result<PickFilesResponse, PickFilesError> {
    let mut dialog = rfd::AsyncFileDialog::new();

    if let Some(title) = &req.title {
        dialog = dialog.set_title(title);
    }

    if let Some(path) = &req.default_path {
        let p = Path::new(path);
        if p.exists() && p.is_dir() {
            dialog = dialog.set_directory(p);
        } else {
            warn!(
                "Default path does not exist or is not a directory: {}",
                path
            );
        }
    }

    // Pick based on mode
    let handles = match req.mode {
        PickerMode::Files | PickerMode::Both => {
            if req.multiple {
                dialog.pick_files().await
            } else {
                dialog.pick_file().await.map(|f| vec![f])
            }
        }
        PickerMode::Directories => {
            if req.multiple {
                dialog.pick_folders().await
            } else {
                dialog.pick_folder().await.map(|f| vec![f])
            }
        }
    };

    // Convert handles to FileInfo
    match handles {
        Some(files) => {
            let file_infos: Vec<FileInfo> = files
                .into_iter()
                .filter_map(|f| {
                    let path = f.path();
                    match file_info_from_path(path) {
                        Ok(info) => Some(info),
                        Err(e) => {
                            warn!("Failed to read metadata for {:?}: {}", path, e);
                            None
                        }
                    }
                })
                .collect();

            debug!("Selected {} files", file_infos.len());

            Ok(PickFilesResponse {
                files: file_infos,
                cancelled: false,
            })
        }
        None => {
            debug!("File picker cancelled");
            Ok(PickFilesResponse {
                files: vec![],
                cancelled: true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn test_file_info_from_path_file() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        File::create(&file_path).unwrap();

        let info = file_info_from_path(&file_path).unwrap();
        assert!(!info.is_directory);
        assert!(info.size.is_some());
        assert!(info.modified.is_some());
        assert!(info.path.ends_with("test.txt"));
    }

    #[test]
    fn test_file_info_from_path_directory() {
        let temp = TempDir::new().unwrap();

        let info = file_info_from_path(temp.path()).unwrap();
        assert!(info.is_directory);
        assert!(info.size.is_none());
        assert!(info.modified.is_some());
    }

    #[test]
    fn test_file_info_from_path_not_found() {
        let result = file_info_from_path(Path::new("/nonexistent/path/file.txt"));
        assert!(result.is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_has_display_checks_env() {
        // This test just verifies the function doesn't panic
        let _ = has_display();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_build_applescript_single_file() {
        let req = PickFilesRequest {
            mode: PickerMode::Files,
            multiple: false,
            title: Some("Pick a file".into()),
            default_path: None,
        };
        let script = build_applescript(&req);
        assert!(script.contains("choose file"));
        assert!(script.contains("Pick a file"));
        assert!(!script.contains("multiple selections"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_build_applescript_multiple_files() {
        let req = PickFilesRequest {
            mode: PickerMode::Files,
            multiple: true,
            title: None,
            default_path: None,
        };
        let script = build_applescript(&req);
        assert!(script.contains("choose file"));
        assert!(script.contains("multiple selections allowed"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_build_applescript_directory() {
        let req = PickFilesRequest {
            mode: PickerMode::Directories,
            multiple: false,
            title: Some("Select folder".into()),
            default_path: Some("/tmp".into()),
        };
        let script = build_applescript(&req);
        assert!(script.contains("choose folder"));
        assert!(script.contains("default location"));
        assert!(script.contains("/tmp"));
    }
}
