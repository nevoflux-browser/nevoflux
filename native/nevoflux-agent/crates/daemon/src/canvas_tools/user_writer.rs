//! Atomic file I/O for user-layer Canvas Tool TOML files.
//!
//! Writes use a tmp-then-rename pattern so an interrupted write never
//! leaves a half-parsed file visible to the loader. Deletes are idempotent
//! (missing file is not an error).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Errors surfaced to the command handler. Converted to `CanvasToolError`
/// with `code = "io"` at the wire boundary.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("invalid tool name: {0}")]
    InvalidName(String),
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Absolute path where the named tool's TOML lives in the user dir.
pub fn user_tool_path(user_dir: &Path, name: &str) -> Result<PathBuf, WriteError> {
    if !is_safe_name(name) {
        return Err(WriteError::InvalidName(name.to_string()));
    }
    Ok(user_dir.join(format!("{name}.toml")))
}

/// Write `toml_text` to `<user_dir>/<name>.toml` atomically.
///
/// - Creates `user_dir` if missing.
/// - Writes to a tmp sibling and renames on success.
/// - Returns the final path.
pub fn write_user_tool_atomic(
    user_dir: &Path,
    name: &str,
    toml_text: &str,
) -> Result<PathBuf, WriteError> {
    let dest = user_tool_path(user_dir, name)?;

    fs::create_dir_all(user_dir).map_err(|e| WriteError::Io {
        path: user_dir.to_path_buf(),
        source: e,
    })?;

    let tmp = user_dir.join(format!(".{name}.toml.tmp.{}", std::process::id()));
    fs::write(&tmp, toml_text).map_err(|e| WriteError::Io {
        path: tmp.clone(),
        source: e,
    })?;

    fs::rename(&tmp, &dest).map_err(|e| {
        // Best-effort cleanup of the tmp so we don't leak it on failure.
        let _ = fs::remove_file(&tmp);
        WriteError::Io {
            path: dest.clone(),
            source: e,
        }
    })?;

    Ok(dest)
}

/// Delete `<user_dir>/<name>.toml`. Returns `true` if a file was removed,
/// `false` if it was already absent.
pub fn delete_user_tool_file(user_dir: &Path, name: &str) -> Result<bool, WriteError> {
    let path = user_tool_path(user_dir, name)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(WriteError::Io { path, source: e }),
    }
}

/// Conservative name validation — keep this in sync with
/// `canvas_tools::validator::validate_name`. Duplicated here because the
/// writer must refuse path-traversal characters even if the validator is
/// bypassed.
fn is_safe_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.starts_with('.') {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_user_tool_atomic_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let user_dir = tmp.path().join("canvas-tools");
        let path = write_user_tool_atomic(&user_dir, "demo", "name = \"demo\"\n").unwrap();
        assert!(path.exists());
        assert!(path.ends_with("demo.toml"));
        let body = fs::read_to_string(&path).unwrap();
        assert_eq!(body, "name = \"demo\"\n");
    }

    #[test]
    fn test_write_user_tool_atomic_no_tmp_leftovers() {
        let tmp = tempfile::tempdir().unwrap();
        let user_dir = tmp.path().join("canvas-tools");
        write_user_tool_atomic(&user_dir, "demo", "x = 1\n").unwrap();
        let leftovers: Vec<_> = fs::read_dir(&user_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "tmp file leaked: {:?}", leftovers);
    }

    #[test]
    fn test_write_rejects_dotfile_name() {
        let tmp = tempfile::tempdir().unwrap();
        let err = write_user_tool_atomic(tmp.path(), ".evil", "x = 1").unwrap_err();
        assert!(matches!(err, WriteError::InvalidName(_)));
    }

    #[test]
    fn test_write_rejects_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let err = write_user_tool_atomic(tmp.path(), "../boom", "x = 1").unwrap_err();
        assert!(matches!(err, WriteError::InvalidName(_)));
    }

    #[test]
    fn test_delete_user_tool_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        // First delete — file never existed.
        let gone = delete_user_tool_file(tmp.path(), "missing").unwrap();
        assert!(!gone);

        // Create, then delete, then delete again.
        fs::write(tmp.path().join("x.toml"), "name = \"x\"\n").unwrap();
        let gone1 = delete_user_tool_file(tmp.path(), "x").unwrap();
        let gone2 = delete_user_tool_file(tmp.path(), "x").unwrap();
        assert!(gone1);
        assert!(!gone2);
    }
}
