//! Integration tests for file picker functionality.
//!
//! Note: Tests that require a graphical display are skipped in CI.

use nevoflux_protocol::{FileInfo, PickFilesRequest, PickFilesResponse, PickerMode};
use std::path::Path;

#[test]
fn test_file_info_from_real_path() {
    // Test with /tmp which should exist on all Unix systems
    #[cfg(unix)]
    {
        use nevoflux_daemon::file_picker::file_info_from_path;

        let info = file_info_from_path(Path::new("/tmp")).unwrap();
        assert!(info.is_directory);
        assert_eq!(info.path, "/tmp");
    }
}

#[test]
fn test_pick_files_request_serialization() {
    let req = PickFilesRequest {
        mode: PickerMode::Files,
        multiple: true,
        title: Some("Select Files".into()),
        default_path: Some("/home".into()),
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"mode\":\"files\""));
    assert!(json.contains("\"multiple\":true"));

    let decoded: PickFilesRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req, decoded);
}

#[test]
fn test_pick_files_response_empty() {
    let resp = PickFilesResponse {
        files: vec![],
        cancelled: true,
    };

    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"cancelled\":true"));
    assert!(json.contains("\"files\":[]"));
}

#[test]
fn test_pick_files_response_with_files() {
    let resp = PickFilesResponse {
        files: vec![
            FileInfo {
                path: "/home/user/test.txt".into(),
                is_directory: false,
                size: Some(1024),
                modified: Some(1706600000),
            },
            FileInfo {
                path: "/home/user/docs".into(),
                is_directory: true,
                size: None,
                modified: Some(1706600000),
            },
        ],
        cancelled: false,
    };

    let json = serde_json::to_string(&resp).unwrap();
    let decoded: PickFilesResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.files.len(), 2);
    assert!(!decoded.files[0].is_directory);
    assert!(decoded.files[1].is_directory);
    assert!(!decoded.cancelled);
}

// Note: Actual dialog tests require manual testing as they need
// a graphical display and user interaction.
