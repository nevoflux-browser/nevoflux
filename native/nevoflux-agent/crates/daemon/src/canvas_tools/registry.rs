//! Three-layer tool whitelist registry.
//!
//! Manages [`CanvasTool`] entries with a priority hierarchy:
//!
//!   **Builtin < User < Session**
//!
//! - [`ToolSource::Builtin`] tools are shipped with the daemon.
//! - [`ToolSource::User`] tools are loaded from the user's config directory
//!   and override builtins with the same name.
//! - [`ToolSource::Session`] tools are registered dynamically at runtime and
//!   take the highest priority; they survive [`load_from_disk`](ToolWhitelistRegistry::load_from_disk)
//!   reloads.

use std::path::{Path, PathBuf};

use dashmap::DashMap;
use tracing::debug;

use crate::canvas_tools::toml_parser::parse_tool_directory;
use crate::canvas_tools::types::{CanvasTool, ToolSource};

// ---------------------------------------------------------------------------
// ToolWhitelistRegistry
// ---------------------------------------------------------------------------

/// Concurrent, three-layer registry for whitelisted canvas tools.
///
/// Backed by a [`DashMap`] keyed on the tool name.  The priority model is
/// enforced at load/insert time: a higher-priority source always overwrites
/// a lower one, and [`load_from_disk`](Self::load_from_disk) preserves any
/// existing [`ToolSource::Session`] entries.
#[derive(Debug)]
pub struct ToolWhitelistRegistry {
    tools: DashMap<String, CanvasTool>,
    /// Builtin tools that have been overridden by a same-named User entry.
    /// Populated during `load_from_disk`; consulted by `list_all` to set
    /// `is_override` and by `remove_user_tool_with_restore` to revert.
    shadowed_builtins: DashMap<String, CanvasTool>,
    builtin_dir: Option<PathBuf>,
    user_dir: Option<PathBuf>,
}

impl Default for ToolWhitelistRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolWhitelistRegistry {
    // -----------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------

    /// Create an empty registry with no directory paths.
    pub fn new() -> Self {
        Self {
            tools: DashMap::new(),
            shadowed_builtins: DashMap::new(),
            builtin_dir: None,
            user_dir: None,
        }
    }

    /// Create a registry that knows where to load tool definitions from disk.
    pub fn with_dirs(builtin_dir: impl Into<PathBuf>, user_dir: impl Into<PathBuf>) -> Self {
        Self {
            tools: DashMap::new(),
            shadowed_builtins: DashMap::new(),
            builtin_dir: Some(builtin_dir.into()),
            user_dir: Some(user_dir.into()),
        }
    }

    // -----------------------------------------------------------------
    // Disk loading
    // -----------------------------------------------------------------

    /// (Re)load tool definitions from the builtin and user directories.
    ///
    /// 1. Snapshot and remove all non-Session entries.
    /// 2. Load from `builtin_dir` — each tool gets [`ToolSource::Builtin`].
    /// 3. Load from `user_dir`   — each tool gets [`ToolSource::User`] and
    ///    overwrites any builtin with the same name.
    /// 4. Existing [`ToolSource::Session`] entries are **preserved** and still
    ///    take priority over anything loaded from disk.
    pub async fn load_from_disk(&self) {
        // 1. Collect session tools so they survive the reload.
        let session_tools: Vec<CanvasTool> = self
            .tools
            .iter()
            .filter(|entry| entry.value().source == ToolSource::Session)
            .map(|entry| entry.value().clone())
            .collect();

        // 2. Clear the map entirely, then re-insert session tools.
        self.tools.clear();
        self.shadowed_builtins.clear();
        for tool in &session_tools {
            self.tools.insert(tool.name.clone(), tool.clone());
        }

        // 3. Load builtin tools.
        if let Some(dir) = &self.builtin_dir {
            self.load_dir(dir, ToolSource::Builtin).await;
        }

        // 4. Load user tools (overrides builtins but not session).
        if let Some(dir) = &self.user_dir {
            self.load_dir(dir, ToolSource::User).await;
        }
    }

    /// Load tools from a single directory, assigning the given source.
    ///
    /// Session-priority entries already in the map are never overwritten.
    async fn load_dir(&self, dir: &Path, source: ToolSource) {
        let mut tools = parse_tool_directory(dir).await;
        for tool in &mut tools {
            tool.source = source;

            if let Some(existing) = self.tools.get(&tool.name) {
                if existing.source == ToolSource::Session {
                    debug!(
                        name = %tool.name,
                        "Skipping disk tool — session override exists"
                    );
                    continue;
                }
                // If a User tool is about to evict a Builtin, preserve the
                // Builtin in the shadow map so Revert can restore it without
                // another disk reload.
                if source == ToolSource::User && existing.source == ToolSource::Builtin {
                    self.shadowed_builtins
                        .insert(tool.name.clone(), existing.value().clone());
                }
            }

            self.tools.insert(tool.name.clone(), tool.clone());
        }
    }

    // -----------------------------------------------------------------
    // Session tools
    // -----------------------------------------------------------------

    /// Register a tool with [`ToolSource::Session`] priority.
    ///
    /// This overwrites any existing entry regardless of its source.
    pub fn register_session_tool(&self, mut tool: CanvasTool) {
        tool.source = ToolSource::Session;
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Remove a tool **only** if its source is [`ToolSource::Session`].
    ///
    /// Returns `true` if the tool was present and removed.
    pub fn remove_session_tool(&self, name: &str) -> bool {
        // Use `remove_if` to atomically check + remove.
        self.tools
            .remove_if(name, |_, v| v.source == ToolSource::Session)
            .is_some()
    }

    // -----------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------

    /// Look up a tool by name, returning it only if it is **enabled**.
    pub fn get(&self, name: &str) -> Option<CanvasTool> {
        self.tools
            .get(name)
            .filter(|entry| entry.value().enabled)
            .map(|entry| entry.value().clone())
    }

    /// Look up a tool by name regardless of its `enabled` flag.
    pub fn get_any(&self, name: &str) -> Option<CanvasTool> {
        self.tools.get(name).map(|entry| entry.value().clone())
    }

    /// Peek at a Builtin that is being shadowed by a same-named User entry.
    /// Returns `None` when there is no shadow (no Builtin existed, or the
    /// live entry is itself a Builtin / Session).
    pub fn shadowed_builtin(&self, name: &str) -> Option<CanvasTool> {
        self.shadowed_builtins.get(name).map(|e| e.value().clone())
    }

    /// True iff the live entry for `name` is a User tool that shadows a Builtin.
    pub fn is_override(&self, name: &str) -> bool {
        self.shadowed_builtins.contains_key(name)
            && self
                .tools
                .get(name)
                .map(|e| e.value().source == ToolSource::User)
                .unwrap_or(false)
    }

    /// Return all **enabled** tools, sorted by name for deterministic output.
    pub fn list_enabled(&self) -> Vec<CanvasTool> {
        let mut out: Vec<CanvasTool> = self
            .tools
            .iter()
            .filter(|entry| entry.value().enabled)
            .map(|entry| entry.value().clone())
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Return **all** tools regardless of `enabled`, sorted by name.
    pub fn list_all(&self) -> Vec<CanvasTool> {
        let mut out: Vec<CanvasTool> = self
            .tools
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    // -----------------------------------------------------------------
    // Mutation helpers
    // -----------------------------------------------------------------

    /// Toggle the `enabled` flag on a tool.
    ///
    /// Returns `true` if the tool was found and updated.
    pub fn set_enabled(&self, name: &str, enabled: bool) -> bool {
        if let Some(mut entry) = self.tools.get_mut(name) {
            entry.value_mut().enabled = enabled;
            true
        } else {
            false
        }
    }

    /// Insert a tool directly (useful in tests).
    pub fn insert(&self, tool: CanvasTool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Remove the User entry `name` and, if a Builtin of the same name was
    /// shadowed, restore it. No-op when the live entry is not User.
    pub fn remove_user_tool_with_restore(&self, name: &str) -> DeleteOutcome {
        let removed = self
            .tools
            .remove_if(name, |_, v| v.source == ToolSource::User)
            .is_some();
        if !removed {
            return DeleteOutcome::default();
        }

        let restored = if let Some((_, builtin)) = self.shadowed_builtins.remove(name) {
            self.tools.insert(name.to_string(), builtin);
            true
        } else {
            false
        };

        DeleteOutcome {
            removed: true,
            restored_builtin: restored,
        }
    }

    /// Insert a tool as the User-source entry, moving any existing Builtin
    /// of the same name into the shadow map. Used by the save command after
    /// a successful write to disk.
    pub fn register_user_tool(&self, mut tool: CanvasTool) {
        tool.source = ToolSource::User;
        if let Some(existing) = self.tools.get(&tool.name) {
            if existing.source == ToolSource::Builtin {
                self.shadowed_builtins
                    .insert(tool.name.clone(), existing.value().clone());
            }
        }
        self.tools.insert(tool.name.clone(), tool);
    }

    // -----------------------------------------------------------------
    // Size helpers
    // -----------------------------------------------------------------

    /// Number of tools in the registry.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the registry contains no tools.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

// ---------------------------------------------------------------------------
// DeleteOutcome
// ---------------------------------------------------------------------------

/// Result of `remove_user_tool_with_restore`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeleteOutcome {
    pub removed: bool,
    pub restored_builtin: bool,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_tools::types::{ArgsMode, BackendKind, ExecutionConstraints};
    use std::collections::HashMap;

    /// Helper: build a minimal valid [`CanvasTool`] with the given name and source.
    fn make_tool(name: &str, source: ToolSource) -> CanvasTool {
        CanvasTool {
            name: name.into(),
            description: format!("{name} tool"),
            kind: BackendKind::Internal,
            binary: None,
            api: Some("builtin://test".into()),
            args_mode: ArgsMode::Template,
            args: vec![],
            allowed_subcommands: vec![],
            params: HashMap::new(),
            constraints: ExecutionConstraints::default(),
            enabled: true,
            source,
        }
    }

    // 1. Insert and get
    #[test]
    fn test_insert_and_get() {
        let reg = ToolWhitelistRegistry::new();
        let tool = make_tool("alpha", ToolSource::Builtin);
        reg.insert(tool.clone());

        let fetched = reg.get("alpha").expect("should find alpha");
        assert_eq!(fetched.name, "alpha");
        assert_eq!(fetched.source, ToolSource::Builtin);

        assert!(reg.get("nonexistent").is_none());
    }

    // 2. User overrides builtin
    #[test]
    fn test_user_overrides_builtin() {
        let reg = ToolWhitelistRegistry::new();

        let builtin = make_tool("dup", ToolSource::Builtin);
        reg.insert(builtin);

        let mut user = make_tool("dup", ToolSource::User);
        user.description = "User version".into();
        reg.insert(user);

        let fetched = reg.get("dup").unwrap();
        assert_eq!(fetched.source, ToolSource::User);
        assert_eq!(fetched.description, "User version");
    }

    // 3. Session overrides user
    #[test]
    fn test_session_overrides_user() {
        let reg = ToolWhitelistRegistry::new();

        let user = make_tool("dup", ToolSource::User);
        reg.insert(user);

        let mut session = make_tool("dup", ToolSource::Session);
        session.description = "Session version".into();
        reg.register_session_tool(session);

        let fetched = reg.get("dup").unwrap();
        assert_eq!(fetched.source, ToolSource::Session);
        assert_eq!(fetched.description, "Session version");
    }

    // 4. Remove session tool
    #[test]
    fn test_remove_session_tool() {
        let reg = ToolWhitelistRegistry::new();

        let tool = make_tool("temp", ToolSource::Session);
        reg.register_session_tool(tool);
        assert!(reg.get("temp").is_some());

        assert!(reg.remove_session_tool("temp"));
        assert!(reg.get("temp").is_none());

        // Removing again returns false.
        assert!(!reg.remove_session_tool("temp"));
    }

    // 5. Cannot remove non-session tool via remove_session_tool
    #[test]
    fn test_remove_session_tool_rejects_non_session() {
        let reg = ToolWhitelistRegistry::new();

        let tool = make_tool("stable", ToolSource::Builtin);
        reg.insert(tool);

        assert!(!reg.remove_session_tool("stable"));
        assert!(reg.get("stable").is_some(), "builtin should still exist");
    }

    // 6. Enabled filter
    #[test]
    fn test_enabled_filter() {
        let reg = ToolWhitelistRegistry::new();

        let mut enabled_tool = make_tool("on", ToolSource::Builtin);
        enabled_tool.enabled = true;
        reg.insert(enabled_tool);

        let mut disabled_tool = make_tool("off", ToolSource::Builtin);
        disabled_tool.enabled = false;
        reg.insert(disabled_tool);

        // get() respects enabled
        assert!(reg.get("on").is_some());
        assert!(reg.get("off").is_none());

        // get_any() ignores enabled
        assert!(reg.get_any("on").is_some());
        assert!(reg.get_any("off").is_some());

        // list_enabled vs list_all
        assert_eq!(reg.list_enabled().len(), 1);
        assert_eq!(reg.list_all().len(), 2);
    }

    // 7. set_enabled toggles the flag
    #[test]
    fn test_set_enabled() {
        let reg = ToolWhitelistRegistry::new();
        reg.insert(make_tool("toggle", ToolSource::Builtin));

        assert!(reg.get("toggle").is_some());

        assert!(reg.set_enabled("toggle", false));
        assert!(reg.get("toggle").is_none());
        assert!(reg.get_any("toggle").unwrap().enabled == false);

        assert!(reg.set_enabled("toggle", true));
        assert!(reg.get("toggle").is_some());

        // Non-existent tool returns false.
        assert!(!reg.set_enabled("missing", true));
    }

    // 8. load_from_disk preserves session tools
    #[tokio::test]
    async fn test_load_from_disk_with_session_preservation() {
        let reg = ToolWhitelistRegistry::new();

        // Pre-populate with a builtin and a session tool.
        reg.insert(make_tool("builtin_a", ToolSource::Builtin));
        reg.register_session_tool(make_tool("session_x", ToolSource::Session));

        assert_eq!(reg.len(), 2);

        // Reload from disk (no directories configured → nothing loaded).
        reg.load_from_disk().await;

        // Builtin should be gone (it came from a previous load, not from disk
        // this time), but session tool must survive.
        assert!(reg.get("builtin_a").is_none());
        assert!(reg.get("session_x").is_some());
        assert_eq!(reg.len(), 1);
    }

    // 9. load_from_disk reads TOML files from directories
    #[tokio::test]
    async fn test_load_from_disk_reads_toml_files() {
        let builtin_dir = tempfile::tempdir().unwrap();
        let user_dir = tempfile::tempdir().unwrap();

        // Builtin tool
        let builtin_toml = r#"
            name = "grep_tool"
            description = "Builtin grep"
            kind = "internal"
            api = "builtin://grep"
        "#;
        tokio::fs::write(builtin_dir.path().join("grep_tool.toml"), builtin_toml)
            .await
            .unwrap();

        // Another builtin
        let cat_toml = r#"
            name = "cat_tool"
            description = "Builtin cat"
            kind = "internal"
            api = "builtin://cat"
        "#;
        tokio::fs::write(builtin_dir.path().join("cat_tool.toml"), cat_toml)
            .await
            .unwrap();

        // User override for grep_tool
        let user_grep_toml = r#"
            name = "grep_tool"
            description = "User grep override"
            kind = "internal"
            api = "builtin://grep_custom"
        "#;
        tokio::fs::write(user_dir.path().join("grep_tool.toml"), user_grep_toml)
            .await
            .unwrap();

        let reg = ToolWhitelistRegistry::with_dirs(builtin_dir.path(), user_dir.path());

        // Pre-register a session tool with the same name as the user override.
        let mut session_grep = make_tool("grep_tool", ToolSource::Session);
        session_grep.description = "Session grep".into();
        reg.register_session_tool(session_grep);

        reg.load_from_disk().await;

        // cat_tool should be loaded as Builtin.
        let cat = reg.get("cat_tool").unwrap();
        assert_eq!(cat.source, ToolSource::Builtin);
        assert_eq!(cat.description, "Builtin cat");

        // grep_tool should still be Session (highest priority).
        let grep = reg.get("grep_tool").unwrap();
        assert_eq!(grep.source, ToolSource::Session);
        assert_eq!(grep.description, "Session grep");

        // cat_tool (Builtin) + grep_tool (Session) = 2.
        // The user-dir grep_tool was skipped because session takes priority.
        assert_eq!(reg.len(), 2);
    }

    #[tokio::test]
    async fn test_user_shadows_builtin_on_load() {
        let tmp = tempfile::tempdir().unwrap();
        let builtin_dir = tmp.path().join("builtin");
        let user_dir = tmp.path().join("user");
        std::fs::create_dir_all(&builtin_dir).unwrap();
        std::fs::create_dir_all(&user_dir).unwrap();

        let builtin_toml = r#"
name = "echo"
description = "BUILTIN echo"
kind = "internal"
api = "builtin://echo"
"#;
        let user_toml = r#"
name = "echo"
description = "USER echo"
kind = "internal"
api = "builtin://echo"
"#;
        std::fs::write(builtin_dir.join("echo.toml"), builtin_toml).unwrap();
        std::fs::write(user_dir.join("echo.toml"), user_toml).unwrap();

        let reg = ToolWhitelistRegistry::with_dirs(&builtin_dir, &user_dir);
        reg.load_from_disk().await;

        let live = reg.get("echo").unwrap();
        assert_eq!(live.description, "USER echo");
        assert_eq!(live.source, ToolSource::User);

        let shadowed = reg.shadowed_builtin("echo").unwrap();
        assert_eq!(shadowed.description, "BUILTIN echo");
        assert_eq!(shadowed.source, ToolSource::Builtin);
    }

    #[test]
    fn test_remove_user_tool_with_restore_reverts_builtin() {
        let reg = ToolWhitelistRegistry::new();
        let mut builtin = make_tool("echo", ToolSource::Builtin);
        builtin.description = "BUILTIN".into();
        let mut user = make_tool("echo", ToolSource::User);
        user.description = "USER".into();

        reg.tools.insert("echo".into(), user);
        reg.shadowed_builtins.insert("echo".into(), builtin);

        let outcome = reg.remove_user_tool_with_restore("echo");
        assert!(outcome.removed);
        assert!(outcome.restored_builtin);

        let live = reg.get_any("echo").unwrap();
        assert_eq!(live.description, "BUILTIN");
        assert_eq!(live.source, ToolSource::Builtin);
        assert!(reg.shadowed_builtin("echo").is_none());
    }

    #[test]
    fn test_remove_user_tool_without_shadow() {
        let reg = ToolWhitelistRegistry::new();
        reg.tools
            .insert("solo".into(), make_tool("solo", ToolSource::User));

        let outcome = reg.remove_user_tool_with_restore("solo");
        assert!(outcome.removed);
        assert!(!outcome.restored_builtin);
        assert!(reg.get_any("solo").is_none());
    }

    #[test]
    fn test_remove_user_tool_refuses_builtin() {
        let reg = ToolWhitelistRegistry::new();
        reg.tools
            .insert("b".into(), make_tool("b", ToolSource::Builtin));

        let outcome = reg.remove_user_tool_with_restore("b");
        assert!(!outcome.removed);
        assert!(!outcome.restored_builtin);
        assert!(reg.get_any("b").is_some());
    }
}
