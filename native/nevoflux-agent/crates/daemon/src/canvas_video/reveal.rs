//! OS-native "play" / "reveal in folder" shell-out.
//!
//! Called by the sidebar via `canvas_video_reveal_path` TCP bridge. The
//! daemon runs on the user's machine so it can safely invoke xdg-open /
//! open / explorer. We reject paths outside a known-safe allowlist
//! (Videos/NevoFlux, Downloads/NevoFlux, ~/.cache/nevoflux/render) to
//! prevent the sidebar from asking us to open arbitrary files.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{DaemonError, Result};
use nevoflux_protocol::canvas_video::{RevealAction, RevealPathRequest, RevealPathResponse};

/// Execute a reveal/play action on a whitelisted path.
pub fn reveal_path(req: RevealPathRequest) -> Result<RevealPathResponse> {
    let path = PathBuf::from(&req.path);
    // Canonicalize to resolve symlinks / "..". If the path doesn't exist,
    // that's a hard error for both actions.
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Ok(RevealPathResponse {
                success: false,
                error: Some(format!("path not found: {} ({})", req.path, e)),
            });
        }
    };

    // Allowlist: canonical path must live under one of these roots.
    if !is_whitelisted(&canonical) {
        tracing::warn!(
            path = %canonical.display(),
            "reveal_path rejected: not in allowlist"
        );
        return Ok(RevealPathResponse {
            success: false,
            error: Some("path is outside the allowed roots".into()),
        });
    }

    match req.action {
        RevealAction::Play => open_default(&canonical),
        RevealAction::Reveal => reveal_in_folder(&canonical),
    }
}

/// Roots that `reveal_path` is willing to operate on. Any of:
/// - UserDirs::video_dir()/NevoFlux
/// - UserDirs::download_dir()/NevoFlux
/// - $HOME/.cache/nevoflux/render
fn whitelisted_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(dirs) = directories::UserDirs::new() {
        if let Some(v) = dirs.video_dir() {
            roots.push(v.join("NevoFlux"));
        }
        if let Some(d) = dirs.download_dir() {
            roots.push(d.join("NevoFlux"));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(home).join(".cache/nevoflux/render"));
    }
    roots
}

fn is_whitelisted(canonical_path: &Path) -> bool {
    let roots = whitelisted_roots();
    for root in roots {
        // Canonicalize the root lazily; if the root doesn't exist, skip.
        if let Ok(root_canonical) = root.canonicalize() {
            if canonical_path.starts_with(&root_canonical) {
                return true;
            }
        }
    }
    false
}

fn open_default(path: &Path) -> Result<RevealPathResponse> {
    let (cmd, args): (&str, Vec<String>) = if cfg!(target_os = "linux") {
        ("xdg-open", vec![path.to_string_lossy().into_owned()])
    } else if cfg!(target_os = "macos") {
        ("open", vec![path.to_string_lossy().into_owned()])
    } else if cfg!(target_os = "windows") {
        (
            "cmd",
            vec![
                "/c".into(),
                "start".into(),
                "".into(),
                path.to_string_lossy().into_owned(),
            ],
        )
    } else {
        return Ok(RevealPathResponse {
            success: false,
            error: Some("open not supported on this OS".into()),
        });
    };
    spawn_detached(cmd, &args)
}

fn reveal_in_folder(path: &Path) -> Result<RevealPathResponse> {
    if cfg!(target_os = "macos") {
        // macOS has native "show in Finder" via `open -R`.
        return spawn_detached("open", &["-R".into(), path.to_string_lossy().into_owned()]);
    }
    if cfg!(target_os = "windows") {
        // Windows: explorer /select,<path>
        return spawn_detached("explorer", &[format!("/select,{}", path.to_string_lossy())]);
    }
    // Linux: no universal "select file" — fall back to opening the parent dir.
    let parent = path.parent().ok_or_else(|| {
        DaemonError::InternalError(format!("path has no parent: {}", path.display()))
    })?;
    spawn_detached("xdg-open", &[parent.to_string_lossy().into_owned()])
}

fn spawn_detached(cmd: &str, args: &[String]) -> Result<RevealPathResponse> {
    match Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_child) => {
            // Fire-and-forget; don't wait for exit (the command detaches a GUI).
            Ok(RevealPathResponse {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(RevealPathResponse {
            success: false,
            error: Some(format!("spawn {} failed: {}", cmd, e)),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whitelisted_roots_include_cache() {
        let roots = whitelisted_roots();
        let has_cache = roots.iter().any(|r| r.ends_with(".cache/nevoflux/render"));
        // At least the cache root should be present when HOME is set.
        if std::env::var("HOME").is_ok() {
            assert!(
                has_cache,
                "whitelisted roots should include ~/.cache/nevoflux/render, got {:?}",
                roots
            );
        }
    }

    #[test]
    fn test_rejects_missing_path() {
        let resp = reveal_path(RevealPathRequest {
            path: "/nonexistent/file-that-cannot-exist-abc123.mp4".into(),
            action: RevealAction::Play,
        })
        .unwrap();
        assert!(!resp.success);
        assert!(resp.error.unwrap().contains("path not found"));
    }

    #[test]
    fn test_rejects_non_whitelisted_path() {
        // Create a real file outside the whitelist roots.
        let tmp = std::env::temp_dir().join("nevoflux-reveal-test.txt");
        std::fs::write(&tmp, "hi").unwrap();
        let resp = reveal_path(RevealPathRequest {
            path: tmp.to_string_lossy().into_owned(),
            action: RevealAction::Play,
        })
        .unwrap();
        let _ = std::fs::remove_file(&tmp);
        assert!(!resp.success);
        let err = resp.error.unwrap();
        assert!(
            err.contains("outside"),
            "expected allowlist rejection, got: {err}"
        );
    }
}
