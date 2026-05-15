//! Validation integration tests.

use nevoflux_daemon::{
    validate_extension_id, validate_length, validate_path, validate_port, validate_session_id,
};

#[test]
fn test_port_validation_range() {
    // Valid ports
    assert!(validate_port(8080).is_ok());
    assert!(validate_port(19530).is_ok());
    assert!(validate_port(65535).is_ok());

    // Invalid ports
    assert!(validate_port(0).is_err());
    assert!(validate_port(80).is_err());
    assert!(validate_port(443).is_err());
}

#[test]
fn test_path_security() {
    // Safe paths
    assert!(validate_path("/home/user/file.txt").is_ok());
    assert!(validate_path("relative/path").is_ok());

    // Dangerous paths
    assert!(validate_path("../../../etc/passwd").is_err());
    assert!(validate_path("/path/with/../traversal").is_err());
    assert!(validate_path("path\0with\0nulls").is_err());
}

#[test]
fn test_extension_id_format() {
    // Valid IDs (32 lowercase letters a-p)
    assert!(validate_extension_id("abcdefghijklmnopabcdefghijklmnop").is_ok());

    // Invalid IDs
    assert!(validate_extension_id("short").is_err());
    assert!(validate_extension_id("ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEF").is_err());
    assert!(validate_extension_id("abcdefghijklmnopqrstuvwxyz123456").is_err());
}

#[test]
fn test_session_id_format() {
    // Valid session IDs
    assert!(validate_session_id("session-123").is_ok());
    assert!(validate_session_id("test_session").is_ok());

    // Invalid session IDs
    assert!(validate_session_id("").is_err());
}

#[test]
fn test_length_validation() {
    assert!(validate_length("short", 10).is_ok());
    assert!(validate_length("this is too long", 5).is_err());
}
