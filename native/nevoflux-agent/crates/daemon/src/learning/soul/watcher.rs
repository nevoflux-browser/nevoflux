//! File watcher for the soul directory that detects external edits.
//!
//! Uses `notify::RecommendedWatcher` to monitor the five soul documents
//! (IDENTITY.md, SOUL.md, USER.md, TOOLS.md, AGENTS.md) for changes.
//! Only `Modify` and `Create` events on recognised filenames are forwarded
//! through an async channel so that the consumer (e.g. a reload loop) can
//! validate, reload, and log the change.

use std::path::{Path, PathBuf};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::error::{DaemonError, Result};

/// Filenames that the watcher considers "soul documents".
const WATCHED_FILES: [&str; 5] = ["IDENTITY.md", "SOUL.md", "USER.md", "TOOLS.md", "AGENTS.md"];

/// Watches the soul directory for external modifications to soul documents.
///
/// Internally holds a [`RecommendedWatcher`] that is kept alive as long as
/// the `SoulWatcher` value exists. Changed file paths are delivered via an
/// async channel and can be consumed with [`next_change`](Self::next_change).
pub struct SoulWatcher {
    /// The directory being watched.
    soul_dir: PathBuf,
    /// Internal watcher handle. Dropping this stops watching.
    _watcher: RecommendedWatcher,
    /// Channel receiver for change events.
    rx: mpsc::Receiver<PathBuf>,
}

impl SoulWatcher {
    /// Start watching the soul directory for changes.
    ///
    /// Returns a `SoulWatcher` that yields changed file paths via
    /// [`next_change`](Self::next_change). Only changes to the five
    /// recognised soul documents are reported; all other files in the
    /// directory are silently ignored.
    pub fn start(soul_dir: &Path) -> Result<Self> {
        let soul_dir = soul_dir.to_path_buf();
        let (tx, rx) = mpsc::channel(32);

        let soul_dir_clone = soul_dir.clone();

        let mut watcher =
            notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            for path in &event.paths {
                                if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                                    if WATCHED_FILES.contains(&filename)
                                        && path.parent() == Some(soul_dir_clone.as_path())
                                    {
                                        let _ = tx.blocking_send(path.clone());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            })
            .map_err(|e| {
                DaemonError::InternalError(format!("Failed to create file watcher: {}", e))
            })?;

        watcher
            .watch(&soul_dir, RecursiveMode::NonRecursive)
            .map_err(|e| {
                DaemonError::InternalError(format!("Failed to watch soul directory: {}", e))
            })?;

        Ok(Self {
            soul_dir,
            _watcher: watcher,
            rx,
        })
    }

    /// Wait for the next file change event.
    ///
    /// Returns `Some(path)` when a recognised soul document is modified or
    /// created, or `None` if the watcher channel is closed (which happens
    /// when the watcher is dropped from another context).
    pub async fn next_change(&mut self) -> Option<PathBuf> {
        self.rx.recv().await
    }

    /// Returns the soul directory being watched.
    pub fn soul_dir(&self) -> &Path {
        &self.soul_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    #[tokio::test]
    async fn watcher_detects_file_creation() {
        let tmp = TempDir::new().unwrap();
        let soul_dir = tmp.path().to_path_buf();

        // Create initial file
        std::fs::write(soul_dir.join("TOOLS.md"), "# initial").unwrap();

        let mut watcher = SoulWatcher::start(&soul_dir).unwrap();

        // Modify the file after a brief delay so the watcher is ready
        tokio::time::sleep(Duration::from_millis(100)).await;
        std::fs::write(soul_dir.join("TOOLS.md"), "# modified").unwrap();

        // Wait for the event with timeout
        let result = tokio::time::timeout(Duration::from_secs(5), watcher.next_change()).await;
        assert!(result.is_ok(), "Should receive change event within timeout");

        let changed_path = result.unwrap().unwrap();
        assert!(changed_path.ends_with("TOOLS.md"));
    }

    #[tokio::test]
    async fn watcher_ignores_non_soul_files() {
        let tmp = TempDir::new().unwrap();
        let soul_dir = tmp.path().to_path_buf();

        let mut watcher = SoulWatcher::start(&soul_dir).unwrap();

        // Create a non-soul file
        tokio::time::sleep(Duration::from_millis(100)).await;
        std::fs::write(soul_dir.join("random.txt"), "not a soul file").unwrap();

        // Should NOT get an event (use short timeout)
        let result = tokio::time::timeout(Duration::from_millis(500), watcher.next_change()).await;
        assert!(
            result.is_err(),
            "Should NOT receive event for non-soul files"
        );
    }

    #[test]
    fn watcher_soul_dir_accessor() {
        let tmp = TempDir::new().unwrap();
        let soul_dir = tmp.path().to_path_buf();
        let watcher = SoulWatcher::start(&soul_dir).unwrap();
        assert_eq!(watcher.soul_dir(), soul_dir);
    }

    #[tokio::test]
    async fn watcher_detects_user_md_change() {
        let tmp = TempDir::new().unwrap();
        let soul_dir = tmp.path().to_path_buf();

        // Create initial USER.md
        std::fs::write(soul_dir.join("USER.md"), "# user").unwrap();

        let mut watcher = SoulWatcher::start(&soul_dir).unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;
        std::fs::write(soul_dir.join("USER.md"), "# updated user").unwrap();

        let result = tokio::time::timeout(Duration::from_secs(5), watcher.next_change()).await;
        assert!(result.is_ok(), "Should detect USER.md change");
    }
}
