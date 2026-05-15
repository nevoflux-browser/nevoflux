//! Input validation utilities for security.

use std::path::Path;

/// Validation error types.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Invalid port number: {0}")]
    InvalidPort(u16),

    #[error("Path contains invalid characters: {0}")]
    InvalidPath(String),

    #[error("Path traversal detected: {0}")]
    PathTraversal(String),

    #[error("Value too long: max {max}, got {actual}")]
    TooLong { max: usize, actual: usize },

    #[error("Invalid extension ID format")]
    InvalidExtensionId,

    #[error("Empty value not allowed")]
    Empty,
}

/// Validate a port number is in acceptable range (1024-65535).
///
/// Ports below 1024 are reserved for privileged services and require
/// root/administrator access, so we only allow user ports.
///
/// # Examples
///
/// ```
/// use nevoflux_daemon::validation::validate_port;
///
/// assert!(validate_port(8080).is_ok());
/// assert!(validate_port(80).is_err()); // Privileged port
/// assert!(validate_port(0).is_err());  // Invalid
/// ```
pub fn validate_port(port: u16) -> Result<(), ValidationError> {
    if port >= 1024 {
        Ok(())
    } else {
        Err(ValidationError::InvalidPort(port))
    }
}

/// Validate a file path doesn't contain traversal (no "..").
///
/// This helps prevent path traversal attacks where an attacker
/// tries to access files outside the intended directory.
///
/// # Examples
///
/// ```
/// use nevoflux_daemon::validation::validate_path;
///
/// assert!(validate_path("/home/user/file.txt").is_ok());
/// assert!(validate_path("../../../etc/passwd").is_err());
/// assert!(validate_path("/home/../etc/passwd").is_err());
/// ```
pub fn validate_path(path: &str) -> Result<(), ValidationError> {
    if path.is_empty() {
        return Err(ValidationError::Empty);
    }

    // Check for path traversal sequences
    let path_obj = Path::new(path);
    for component in path_obj.components() {
        if let std::path::Component::ParentDir = component {
            return Err(ValidationError::PathTraversal(path.to_string()));
        }
    }

    // Also check for encoded or obfuscated traversal attempts
    if path.contains("..") {
        return Err(ValidationError::PathTraversal(path.to_string()));
    }

    // Check for null bytes which could be used for path truncation attacks
    if path.contains('\0') {
        return Err(ValidationError::InvalidPath(path.to_string()));
    }

    Ok(())
}

/// Validate a Chrome extension ID format (32 lowercase letters).
///
/// Chrome extension IDs are exactly 32 characters consisting only of
/// lowercase letters a-p (base-16 encoded using a-p instead of 0-9a-f).
///
/// # Examples
///
/// ```
/// use nevoflux_daemon::validation::validate_extension_id;
///
/// assert!(validate_extension_id("abcdefghijklmnopabcdefghijklmnop").is_ok());
/// assert!(validate_extension_id("ABCDEFGHIJKLMNOP").is_err()); // Uppercase
/// assert!(validate_extension_id("abc123").is_err()); // Too short, has digits
/// ```
pub fn validate_extension_id(id: &str) -> Result<(), ValidationError> {
    if id.is_empty() {
        return Err(ValidationError::Empty);
    }

    // Chrome extension IDs are exactly 32 lowercase letters (a-p only)
    if id.len() != 32 {
        return Err(ValidationError::InvalidExtensionId);
    }

    // Check all characters are lowercase letters a-p
    for c in id.chars() {
        if !c.is_ascii_lowercase() || c > 'p' {
            return Err(ValidationError::InvalidExtensionId);
        }
    }

    Ok(())
}

/// Validate string length.
///
/// Ensures the given string doesn't exceed the maximum allowed length.
/// Also rejects empty strings.
///
/// # Examples
///
/// ```
/// use nevoflux_daemon::validation::validate_length;
///
/// assert!(validate_length("hello", 10).is_ok());
/// assert!(validate_length("hello world", 5).is_err()); // Too long
/// assert!(validate_length("", 10).is_err()); // Empty
/// ```
pub fn validate_length(value: &str, max: usize) -> Result<(), ValidationError> {
    if value.is_empty() {
        return Err(ValidationError::Empty);
    }

    if value.len() > max {
        return Err(ValidationError::TooLong {
            max,
            actual: value.len(),
        });
    }

    Ok(())
}

/// Validate session ID format (alphanumeric, hyphens, underscores).
///
/// Session IDs should only contain safe characters that won't cause
/// issues when used in file paths, URLs, or database queries.
///
/// # Examples
///
/// ```
/// use nevoflux_daemon::validation::validate_session_id;
///
/// assert!(validate_session_id("session-123_abc").is_ok());
/// assert!(validate_session_id("session/../../etc").is_err()); // Path traversal
/// assert!(validate_session_id("session<script>").is_err()); // Invalid chars
/// ```
pub fn validate_session_id(id: &str) -> Result<(), ValidationError> {
    if id.is_empty() {
        return Err(ValidationError::Empty);
    }

    // Check all characters are alphanumeric, hyphens, or underscores
    for c in id.chars() {
        if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
            return Err(ValidationError::InvalidPath(id.to_string()));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_port_valid() {
        // Valid user ports
        assert!(validate_port(1024).is_ok());
        assert!(validate_port(8080).is_ok());
        assert!(validate_port(3000).is_ok());
        assert!(validate_port(65535).is_ok());
        assert!(validate_port(49152).is_ok()); // Dynamic/private ports start
    }

    #[test]
    fn test_validate_port_invalid() {
        // Privileged ports (require root)
        assert!(validate_port(0).is_err());
        assert!(validate_port(80).is_err());
        assert!(validate_port(443).is_err());
        assert!(validate_port(22).is_err());
        assert!(validate_port(1023).is_err());

        // Check error message
        let err = validate_port(80).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidPort(80)));
    }

    #[test]
    fn test_validate_path_valid() {
        assert!(validate_path("/home/user/file.txt").is_ok());
        assert!(validate_path("relative/path/file.txt").is_ok());
        assert!(validate_path("/var/log/app.log").is_ok());
        assert!(validate_path("file.txt").is_ok());
        assert!(validate_path("/").is_ok());
    }

    #[test]
    fn test_validate_path_traversal() {
        // Path traversal attempts
        assert!(validate_path("../../../etc/passwd").is_err());
        assert!(validate_path("/home/../etc/passwd").is_err());
        assert!(validate_path("foo/../../bar").is_err());
        assert!(validate_path("..").is_err());

        // Empty path
        assert!(validate_path("").is_err());

        // Check error type
        let err = validate_path("../secret").unwrap_err();
        assert!(matches!(err, ValidationError::PathTraversal(_)));
    }

    #[test]
    fn test_validate_path_null_byte() {
        // Null byte injection
        let path_with_null = "file.txt\0.jpg";
        assert!(validate_path(path_with_null).is_err());

        let err = validate_path(path_with_null).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidPath(_)));
    }

    #[test]
    fn test_validate_extension_id_valid() {
        // Valid Chrome extension IDs (32 lowercase letters a-p)
        assert!(validate_extension_id("abcdefghijklmnopabcdefghijklmnop").is_ok());
        assert!(validate_extension_id("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaan").is_ok());
        assert!(validate_extension_id("pppppppppppppppppppppppppppppppp").is_ok());
    }

    #[test]
    fn test_validate_extension_id_invalid() {
        // Too short
        assert!(validate_extension_id("abc").is_err());

        // Too long
        assert!(validate_extension_id("abcdefghijklmnopabcdefghijklmnopx").is_err());

        // Contains uppercase
        assert!(validate_extension_id("ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP").is_err());

        // Contains numbers
        assert!(validate_extension_id("abcdefghijklmnop1234567890123456").is_err());

        // Contains letters beyond 'p'
        assert!(validate_extension_id("qrstuvwxyzabcdefghijklmnopabcdef").is_err());

        // Empty
        assert!(validate_extension_id("").is_err());

        // Contains special characters
        assert!(validate_extension_id("abcdefghijklmnop!@#$%^&*()abcde").is_err());
    }

    #[test]
    fn test_validate_length_valid() {
        assert!(validate_length("hello", 10).is_ok());
        assert!(validate_length("hello", 5).is_ok()); // Exactly at limit
        assert!(validate_length("a", 1).is_ok());
        assert!(validate_length("test string", 100).is_ok());
    }

    #[test]
    fn test_validate_length_invalid() {
        // Too long
        let err = validate_length("hello world", 5).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::TooLong { max: 5, actual: 11 }
        ));

        // Empty
        let err = validate_length("", 10).unwrap_err();
        assert!(matches!(err, ValidationError::Empty));
    }

    #[test]
    fn test_validate_session_id_valid() {
        assert!(validate_session_id("session-123").is_ok());
        assert!(validate_session_id("my_session_id").is_ok());
        assert!(validate_session_id("Session-123_ABC").is_ok());
        assert!(validate_session_id("a").is_ok());
        assert!(validate_session_id("123").is_ok());
        assert!(validate_session_id("abc-def_ghi").is_ok());
    }

    #[test]
    fn test_validate_session_id_invalid() {
        // Empty
        assert!(validate_session_id("").is_err());

        // Contains path separators
        assert!(validate_session_id("session/id").is_err());
        assert!(validate_session_id("session\\id").is_err());

        // Contains special characters
        assert!(validate_session_id("session<script>").is_err());
        assert!(validate_session_id("session!@#$").is_err());

        // Contains spaces
        assert!(validate_session_id("session id").is_err());

        // Contains dots (could be used for path manipulation)
        assert!(validate_session_id("session..id").is_err());
    }
}
