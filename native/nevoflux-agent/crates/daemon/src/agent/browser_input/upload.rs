// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Browser upload validation: workspace-path containment, sensitive-file
//! blocklist, file-size cap, and MIME detection via magic bytes.
//!
//! Token storage and the localhost HTTP server live on the AssetServer
//! (`crate::asset_server`); this module retains only the validation
//! business rules that are upload-specific.
//!
//! # Token lifecycle
//!
//! 1. Caller validates a file path with `validate_workspace_path` and
//!    checks its size with `check_file_size`.
//! 2. Caller hands the canonical path to
//!    `AssetServer::register_download`, which mints a short-lived UUID
//!    token in `download_tokens` and returns the URL the actor should
//!    fetch.
//! 3. The actor GETs the URL once; the AssetServer's eviction loop
//!    sweeps any unused tokens after their TTL.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Maximum file size allowed for upload (500 MiB).
pub const DEFAULT_MAX_SIZE: u64 = 500 * 1024 * 1024;

/// How long a token remains valid after insertion.
pub const TOKEN_TTL: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by upload validation helpers.
#[derive(Debug, Error)]
pub enum UploadError {
    #[error("File path not in allowed workspace: {path} (workspace: {workspace})")]
    PathNotAllowed { path: String, workspace: String },

    #[error("File too large: {size} bytes exceeds {max} byte limit")]
    FileTooLarge { size: u64, max: u64 },

    #[error("Sensitive file blocked: {path} ({reason})")]
    SensitiveFile { path: String, reason: &'static str },

    #[error("I/O error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

impl UploadError {
    /// Numeric error code matching the spec.
    ///
    /// - `1011` — path traversal / not-in-workspace
    /// - `1010` — file too large
    /// - `1012` — sensitive file blocked
    /// - `1005` — I/O error
    pub fn code(&self) -> u32 {
        match self {
            Self::PathNotAllowed { .. } => 1011,
            Self::FileTooLarge { .. } => 1010,
            Self::SensitiveFile { .. } => 1012,
            Self::Io { .. } => 1005,
        }
    }
}

// ---------------------------------------------------------------------------
// Token entry shape
// ---------------------------------------------------------------------------

/// Metadata stored alongside each upload token.  The actual store lives
/// on `AssetServer::state::download_tokens` (a generic `TokenStore<TokenEntry>`).
#[derive(Debug, Clone)]
pub struct TokenEntry {
    /// Canonical, validated file path.
    pub path: PathBuf,
    /// MIME type string (e.g. `"image/jpeg"`).
    pub mime_type: String,
    /// Original file name (last path component).
    pub file_name: String,
    /// File size in bytes.
    pub size: u64,
    /// Absolute instant at which the token expires.
    pub expires_at: Instant,
}

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

/// Ensure `file_path` is contained within `workspace_dir`.
///
/// Both paths are canonicalized before comparison to resolve `..` and
/// symlinks. Returns the canonical `file_path` on success.
///
/// # Errors
///
/// - [`UploadError::PathNotAllowed`] if the file is outside the workspace.
/// - [`UploadError::Io`] if either path cannot be canonicalized (e.g. the
///   file does not exist).
pub fn validate_workspace_path(
    file_path: &Path,
    workspace_dir: &Path,
) -> Result<PathBuf, UploadError> {
    let canonical_workspace = workspace_dir.canonicalize().map_err(|e| UploadError::Io {
        path: workspace_dir.display().to_string(),
        source: e,
    })?;

    let canonical_file = file_path.canonicalize().map_err(|e| UploadError::Io {
        path: file_path.display().to_string(),
        source: e,
    })?;

    if !canonical_file.starts_with(&canonical_workspace) {
        return Err(UploadError::PathNotAllowed {
            path: canonical_file.display().to_string(),
            workspace: canonical_workspace.display().to_string(),
        });
    }

    Ok(canonical_file)
}

// ---------------------------------------------------------------------------
// Sensitive file check
// ---------------------------------------------------------------------------

/// Directories and file patterns that must never be uploaded, even if
/// they fall inside the workspace. Prevents accidental credential /
/// key / config leakage when the user sets a broad workspace_dir
/// (e.g. their home directory).
///
/// Cross-platform: covers Linux, macOS, and Windows sensitive paths.
const SENSITIVE_DIRS: &[&str] = &[
    // Unix / cross-platform
    ".ssh",
    ".gnupg",
    ".gpg",
    ".aws",
    ".docker",
    ".kube",
    ".config", // covers .config/nevoflux/config.toml, .config/gcloud, etc.
    ".local",  // covers .local/share/keyrings, etc.
    // macOS
    "Keychains",    // ~/Library/Keychains
    "Cookies",      // ~/Library/Cookies
    "MobileDevice", // ~/Library/MobileDevice (iOS backups)
    // Windows (canonicalized paths use backslash, but component matching
    // works on individual directory names regardless of separator)
    "Vault",       // %LOCALAPPDATA%/Microsoft/Vault
    "Credentials", // %LOCALAPPDATA%/Microsoft/Credentials
    "Crypto",      // %APPDATA%/Microsoft/Crypto
    ".azure",      // Azure CLI credentials
    ".oci",        // Oracle Cloud credentials
];

const SENSITIVE_NAMES: &[&str] = &[
    // Unix shell config / history
    ".env",
    ".env.local",
    ".env.production",
    ".env.development",
    ".bashrc",
    ".zshrc",
    ".bash_profile",
    ".profile",
    ".bash_history",
    ".zsh_history",
    ".sh_history",
    // Git / package manager credentials
    ".gitconfig",
    ".git-credentials",
    ".npmrc",
    ".pypirc",
    ".netrc",
    "_netrc", // Windows equivalent of .netrc
    // Cloud / service credentials
    "credentials",
    "credentials.json",
    "service-account.json",
    "config.toml",
    // macOS
    "login.keychain",
    "login.keychain-db",
    // Windows
    "ntuser.dat",      // Windows registry hive
    "sam",             // Windows SAM database
    "system",          // Windows SYSTEM registry
    "security",        // Windows SECURITY registry
    "web credentials", // Windows Credential Manager export
    "desktop.ini",     // can reveal folder structure
];

const SENSITIVE_EXTENSIONS: &[&str] = &[
    // Keys and certificates
    "pem",
    "key",
    "p12",
    "pfx",
    "jks",
    "keystore",
    "cer",
    "crt",
    // macOS keychain
    "keychain",
    "keychain-db",
    // Windows DPAPI
    "rdp", // contains saved credentials
    // Password manager databases
    "kdbx",   // KeePass
    "1pux",   // 1Password export
    "psafe3", // Password Safe
];

/// Reject files that match known sensitive patterns.
///
/// Called after `validate_workspace_path` (so `path` is already
/// canonical). Checks:
/// 1. Any path component matches a sensitive directory name
/// 2. File name matches a sensitive name exactly
/// 3. File extension matches a sensitive extension
pub fn check_sensitive_path(path: &Path) -> Result<(), UploadError> {
    let path_str = path.display().to_string();

    // Check directory components
    for component in path.components() {
        let s = component.as_os_str().to_string_lossy();
        for dir in SENSITIVE_DIRS {
            if s == *dir {
                return Err(UploadError::SensitiveFile {
                    path: path_str,
                    reason: "path contains a sensitive directory (credentials/keys)",
                });
            }
        }
    }

    // Check file name
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let name_lower = name.to_lowercase();
        for sensitive in SENSITIVE_NAMES {
            if name_lower == *sensitive {
                return Err(UploadError::SensitiveFile {
                    path: path_str,
                    reason: "file name matches a known sensitive file pattern",
                });
            }
        }
        // Also catch patterns like "secret_key.txt", "*_credentials.json"
        if name_lower.contains("secret") || name_lower.contains("private_key") {
            return Err(UploadError::SensitiveFile {
                path: path_str,
                reason: "file name contains 'secret' or 'private_key'",
            });
        }
    }

    // Check extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        for sensitive_ext in SENSITIVE_EXTENSIONS {
            if ext_lower == *sensitive_ext {
                return Err(UploadError::SensitiveFile {
                    path: path_str,
                    reason: "file extension indicates a key/certificate file",
                });
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Size check
// ---------------------------------------------------------------------------

/// Stat `path` and reject it if the file size exceeds `max_size`.
///
/// Returns the file size in bytes on success.
///
/// # Errors
///
/// - [`UploadError::FileTooLarge`] if `size > max_size`.
/// - [`UploadError::Io`] on any I/O failure.
pub fn check_file_size(path: &Path, max_size: u64) -> Result<u64, UploadError> {
    let meta = std::fs::metadata(path).map_err(|e| UploadError::Io {
        path: path.display().to_string(),
        source: e,
    })?;

    let size = meta.len();
    if size > max_size {
        return Err(UploadError::FileTooLarge {
            size,
            max: max_size,
        });
    }

    Ok(size)
}

// ---------------------------------------------------------------------------
// MIME detection
// ---------------------------------------------------------------------------

/// Detect the MIME type of `path` by reading its first 12 bytes and
/// matching against known magic byte sequences.
///
/// Supported types:
///
/// | MIME | Magic |
/// |------|-------|
/// | `image/jpeg` | `FF D8 FF` |
/// | `image/png` | `89 50 4E 47` |
/// | `image/gif` | `47 49 46 38` |
/// | `image/webp` | `RIFF....WEBP` (bytes 0-3 + 8-11) |
/// | `video/mp4` | `ftyp` at bytes 4-7 |
/// | `application/pdf` | `25 50 44 46` |
/// | everything else | `application/octet-stream` |
///
/// # Errors
///
/// - [`UploadError::Io`] if the file cannot be opened or read.
pub fn detect_mime(path: &Path) -> Result<String, UploadError> {
    let mut f = std::fs::File::open(path).map_err(|e| UploadError::Io {
        path: path.display().to_string(),
        source: e,
    })?;

    let mut buf = [0u8; 12];
    // Read up to 12 bytes; fewer bytes for small files is fine — the
    // slice indexing below uses `get()` which returns None on short reads.
    let n = f.read(&mut buf).map_err(|e| UploadError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let buf = &buf[..n];

    let mime = if buf.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if buf.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if buf.starts_with(&[0x47, 0x49, 0x46, 0x38]) {
        "image/gif"
    } else if buf.get(0..4) == Some(b"RIFF") && buf.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else if buf.get(4..8) == Some(b"ftyp") {
        "video/mp4"
    } else if buf.starts_with(&[0x25, 0x50, 0x44, 0x46]) {
        "application/pdf"
    } else {
        "application/octet-stream"
    };

    Ok(mime.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // --- helpers ---

    fn make_file_with_bytes(bytes: &[u8]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
        f
    }

    // (TokenStore lives at `crate::asset_server::token_store`; its tests
    // live there. Upload-side tests focus on the validation business
    // rules below.)
    //
    // `TOKEN_TTL` and the `TokenEntry` struct stay in this module
    // because they are the wire shape `AssetServer::register_download`
    // accepts.

    // -----------------------------------------------------------------------
    // validate_workspace_path
    // -----------------------------------------------------------------------

    #[test]
    fn workspace_path_accepts_file_inside_workspace() {
        let workspace = tempfile::tempdir().unwrap();
        let file_path = workspace.path().join("document.txt");
        std::fs::write(&file_path, b"content").unwrap();

        let result = validate_workspace_path(&file_path, workspace.path());
        assert!(
            result.is_ok(),
            "should accept file inside workspace: {:?}",
            result
        );
    }

    #[test]
    fn workspace_path_rejects_file_outside_workspace() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let file_path = outside.path().join("secret.txt");
        std::fs::write(&file_path, b"secret").unwrap();

        let result = validate_workspace_path(&file_path, workspace.path());
        assert!(
            matches!(result, Err(UploadError::PathNotAllowed { .. })),
            "should reject file outside workspace: {:?}",
            result
        );
    }

    #[test]
    #[cfg(unix)]
    fn workspace_path_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let real_file = outside.path().join("real.txt");
        std::fs::write(&real_file, b"real").unwrap();

        // Create a symlink inside the workspace that points outside.
        let link_path = workspace.path().join("escape.txt");
        symlink(&real_file, &link_path).unwrap();

        let result = validate_workspace_path(&link_path, workspace.path());
        assert!(
            matches!(result, Err(UploadError::PathNotAllowed { .. })),
            "should reject symlink escape: {:?}",
            result
        );
    }

    // -----------------------------------------------------------------------
    // check_file_size
    // -----------------------------------------------------------------------

    #[test]
    fn file_size_accepts_small_file() {
        let f = make_file_with_bytes(&[0u8; 1024]);
        let size = check_file_size(f.path(), DEFAULT_MAX_SIZE).unwrap();
        assert_eq!(size, 1024);
    }

    #[test]
    fn file_size_rejects_oversized_file() {
        let f = make_file_with_bytes(&[0u8; 1024]);
        let result = check_file_size(f.path(), 512);
        assert!(
            matches!(
                result,
                Err(UploadError::FileTooLarge {
                    size: 1024,
                    max: 512
                })
            ),
            "unexpected result: {:?}",
            result
        );
    }

    // -----------------------------------------------------------------------
    // detect_mime
    // -----------------------------------------------------------------------

    #[test]
    fn mime_jpeg() {
        let f = make_file_with_bytes(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]);
        assert_eq!(detect_mime(f.path()).unwrap(), "image/jpeg");
    }

    #[test]
    fn mime_png() {
        let f = make_file_with_bytes(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(detect_mime(f.path()).unwrap(), "image/png");
    }

    #[test]
    fn mime_gif() {
        // GIF89a
        let f = make_file_with_bytes(&[0x47, 0x49, 0x46, 0x38, 0x39, 0x61]);
        assert_eq!(detect_mime(f.path()).unwrap(), "image/gif");
    }

    #[test]
    fn mime_webp() {
        // RIFF....WEBP
        let mut bytes = [0u8; 12];
        bytes[0..4].copy_from_slice(b"RIFF");
        bytes[4..8].copy_from_slice(&[0x24, 0x00, 0x00, 0x00]); // file size LE
        bytes[8..12].copy_from_slice(b"WEBP");
        let f = make_file_with_bytes(&bytes);
        assert_eq!(detect_mime(f.path()).unwrap(), "image/webp");
    }

    #[test]
    fn mime_mp4() {
        // ftyp box at offset 4
        let mut bytes = [0u8; 12];
        bytes[0..4].copy_from_slice(&[0x00, 0x00, 0x00, 0x20]); // box size
        bytes[4..8].copy_from_slice(b"ftyp");
        bytes[8..12].copy_from_slice(b"isom");
        let f = make_file_with_bytes(&bytes);
        assert_eq!(detect_mime(f.path()).unwrap(), "video/mp4");
    }

    #[test]
    fn mime_unknown_fallback() {
        let f = make_file_with_bytes(b"hello, world!");
        assert_eq!(detect_mime(f.path()).unwrap(), "application/octet-stream");
    }

    // -----------------------------------------------------------------------
    // Error codes
    // -----------------------------------------------------------------------

    #[test]
    fn error_codes_match_spec() {
        assert_eq!(
            UploadError::PathNotAllowed {
                path: "a".into(),
                workspace: "b".into()
            }
            .code(),
            1011
        );
        assert_eq!(UploadError::FileTooLarge { size: 1, max: 0 }.code(), 1010);
        assert_eq!(
            UploadError::Io {
                path: "x".into(),
                source: std::io::Error::new(std::io::ErrorKind::Other, "oops")
            }
            .code(),
            1005
        );
        assert_eq!(
            UploadError::SensitiveFile {
                path: "x".into(),
                reason: "test"
            }
            .code(),
            1012
        );
    }

    // -----------------------------------------------------------------------
    // Sensitive file check
    // -----------------------------------------------------------------------

    #[test]
    fn sensitive_ssh_key_blocked() {
        let path = Path::new("/home/user/.ssh/id_rsa");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_env_file_blocked() {
        let path = Path::new("/home/user/project/.env");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_pem_extension_blocked() {
        let path = Path::new("/home/user/certs/server.pem");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_key_extension_blocked() {
        let path = Path::new("/home/user/keys/deploy.key");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_gnupg_dir_blocked() {
        let path = Path::new("/home/user/.gnupg/secring.gpg");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_aws_credentials_blocked() {
        let path = Path::new("/home/user/.aws/credentials");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_secret_in_name_blocked() {
        let path = Path::new("/home/user/docs/my_secret_config.json");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_bashrc_blocked() {
        let path = Path::new("/home/user/.bashrc");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    // --- macOS sensitive paths ---

    #[test]
    fn sensitive_macos_keychain_blocked() {
        let path = Path::new("/Users/user/Library/Keychains/login.keychain-db");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_macos_keychain_ext_blocked() {
        let path = Path::new("/Users/user/backup/exported.keychain");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    // --- Windows sensitive paths ---

    #[test]
    fn sensitive_windows_vault_blocked() {
        let path = Path::new("C:/Users/user/AppData/Local/Microsoft/Vault/data.vcrd");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_windows_credentials_dir_blocked() {
        let path = Path::new("C:/Users/user/AppData/Local/Microsoft/Credentials/token");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_windows_rdp_blocked() {
        let path = Path::new("C:/Users/user/Desktop/server.rdp");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_keepass_db_blocked() {
        let path = Path::new("/home/user/passwords.kdbx");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_git_credentials_blocked() {
        let path = Path::new("/home/user/.git-credentials");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    #[test]
    fn sensitive_certificate_blocked() {
        let path = Path::new("/home/user/certs/server.crt");
        assert!(matches!(
            check_sensitive_path(path),
            Err(UploadError::SensitiveFile { .. })
        ));
    }

    // --- Positive cases (should pass through) ---

    #[test]
    fn normal_pdf_allowed() {
        let path = Path::new("/home/user/Documents/resume.pdf");
        assert!(check_sensitive_path(path).is_ok());
    }

    #[test]
    fn normal_image_allowed() {
        let path = Path::new("/home/user/photos/vacation.jpg");
        assert!(check_sensitive_path(path).is_ok());
    }

    #[test]
    fn normal_docx_allowed() {
        let path = Path::new("/home/user/work/report.docx");
        assert!(check_sensitive_path(path).is_ok());
    }
}
