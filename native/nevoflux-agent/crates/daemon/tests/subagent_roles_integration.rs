//! Integration tests for the subagent role system.
//!
//! These tests verify the end-to-end behavior of the role registry, spawn
//! configuration, nesting prevention, and tool filtering components.

use std::path::PathBuf;
use std::sync::Arc;

use nevoflux_builtin_wasm::host::HostFunctions;
use nevoflux_daemon::agent::roles::AgentRoleRegistry;
use nevoflux_daemon::agent_host::DaemonHostFunctions;
use nevoflux_daemon::AgentConfig;
use nevoflux_protocol::subagent::{
    is_tool_allowed, matches_tool_pattern, SpawnSubagentConfig, ToolsConfig,
};

/// Path to the built-in role definition files from the daemon crate's perspective.
fn builtin_agents_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../builtin-wasm/prompts/agents")
}

// =============================================================================
// Test 1: Registry with builtin roles
// =============================================================================

#[test]
fn test_role_registry_with_builtin_roles() {
    let user_dir = tempfile::TempDir::new().unwrap();
    let builtin_dir = builtin_agents_dir();

    let mut registry = AgentRoleRegistry::new(user_dir.path().to_path_buf(), builtin_dir);
    let count = registry.scan().unwrap();

    // We expect at least 4 built-in roles: explorer, researcher, worker, reader
    assert!(
        count >= 4,
        "Expected at least 4 builtin roles, got {}",
        count
    );

    let summaries = registry.list();
    let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"explorer"), "Missing 'explorer' role");
    assert!(names.contains(&"researcher"), "Missing 'researcher' role");
    assert!(names.contains(&"worker"), "Missing 'worker' role");
    assert!(names.contains(&"reader"), "Missing 'reader' role");

    // Verify explorer definition details
    let explorer = registry.get("explorer").unwrap();
    assert_eq!(explorer.mode, "browser");
    assert_eq!(explorer.max_iterations, 10);
    // Explorer should have an Allow tools_config with specific browser tools
    match &explorer.tools_config {
        Some(ToolsConfig::Allow(tools)) => {
            assert!(
                tools.contains(&"browser_navigate".to_string()),
                "Explorer should allow browser_navigate"
            );
            assert!(
                tools.contains(&"browser_get_markdown".to_string()),
                "Explorer should allow browser_get_markdown"
            );
            assert!(
                tools.contains(&"web_search".to_string()),
                "Explorer should allow web_search"
            );
        }
        other => panic!("Expected ToolsConfig::Allow for explorer, got {:?}", other),
    }
}

// =============================================================================
// Test 2: Spawn config role resolution
// =============================================================================

#[test]
fn test_spawn_config_role_resolution() {
    let user_dir = tempfile::TempDir::new().unwrap();
    let builtin_dir = builtin_agents_dir();

    let mut registry = AgentRoleRegistry::new(user_dir.path().to_path_buf(), builtin_dir);
    registry.scan().unwrap();

    // Create a SpawnSubagentConfig with role: "reader"
    let config = SpawnSubagentConfig {
        prompt: "Analyze the codebase".to_string(),
        role: Some("reader".to_string()),
        ..Default::default()
    };

    // Look up the role
    let role_def = registry.get(config.role.as_ref().unwrap()).unwrap();

    // Verify role defaults
    assert_eq!(role_def.mode, "agent");
    assert_eq!(role_def.max_iterations, 10);

    // Verify tools_config from role
    match &role_def.tools_config {
        Some(ToolsConfig::Allow(tools)) => {
            assert!(
                tools.contains(&"read".to_string()),
                "Reader should allow 'read'"
            );
            assert!(
                tools.contains(&"glob".to_string()),
                "Reader should allow 'glob'"
            );
            assert!(
                tools.contains(&"grep".to_string()),
                "Reader should allow 'grep'"
            );
            assert_eq!(tools.len(), 3, "Reader should have exactly 3 tools");
        }
        other => panic!("Expected ToolsConfig::Allow for reader, got {:?}", other),
    }

    // Simulate config merging (defaults <- role <- spawn params):
    // Since config has no overrides, role values should be used
    let final_mode = config.mode.unwrap_or_else(|| role_def.mode.clone());
    let final_tools = config.tools.or(role_def.tools_config.clone());
    let final_max_iterations = config.max_iterations.unwrap_or(role_def.max_iterations);

    assert_eq!(final_mode, "agent");
    assert_eq!(final_max_iterations, 10);
    assert_eq!(
        final_tools,
        Some(ToolsConfig::Allow(vec![
            "read".to_string(),
            "glob".to_string(),
            "grep".to_string(),
        ]))
    );
}

// =============================================================================
// Test 3: Spawn config inline overrides
// =============================================================================

#[test]
fn test_spawn_config_inline_overrides() {
    let user_dir = tempfile::TempDir::new().unwrap();
    let builtin_dir = builtin_agents_dir();

    let mut registry = AgentRoleRegistry::new(user_dir.path().to_path_buf(), builtin_dir);
    registry.scan().unwrap();

    // Create config with role: "explorer" + max_iterations override
    let config = SpawnSubagentConfig {
        prompt: "Browse the web".to_string(),
        role: Some("explorer".to_string()),
        max_iterations: Some(5), // Override the role's default of 10
        ..Default::default()
    };

    let role_def = registry.get(config.role.as_ref().unwrap()).unwrap();

    // Role's default is 10
    assert_eq!(role_def.max_iterations, 10);

    // But config override should win
    let final_max_iterations = config.max_iterations.unwrap_or(role_def.max_iterations);
    assert_eq!(
        final_max_iterations, 5,
        "Config override should win over role default"
    );

    // Mode from role (no override in config)
    let final_mode = config.mode.unwrap_or_else(|| role_def.mode.clone());
    assert_eq!(final_mode, "browser", "Should inherit mode from role");

    // Tools from role (no override in config)
    let final_tools = config.tools.or(role_def.tools_config.clone());
    assert!(
        matches!(final_tools, Some(ToolsConfig::Allow(_))),
        "Should inherit tools from role"
    );
}

// =============================================================================
// Test 4: Spawn config tools none
// =============================================================================

#[test]
fn test_spawn_config_tools_none() {
    let config = SpawnSubagentConfig {
        prompt: "Just analyze this text".to_string(),
        tools: Some(ToolsConfig::None),
        ..Default::default()
    };

    assert_eq!(config.tools, Some(ToolsConfig::None));

    // Verify serialization roundtrip
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: SpawnSubagentConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.tools, Some(ToolsConfig::None));
}

// =============================================================================
// Test 5: Spawn config model override
// =============================================================================

#[test]
fn test_spawn_config_model_override() {
    let config = SpawnSubagentConfig {
        prompt: "Do something with GPT-4o".to_string(),
        provider: Some("openai".to_string()),
        model: Some("gpt-4o".to_string()),
        ..Default::default()
    };

    assert_eq!(config.provider, Some("openai".to_string()));
    assert_eq!(config.model, Some("gpt-4o".to_string()));

    // Verify serialization roundtrip
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: SpawnSubagentConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.provider, Some("openai".to_string()));
    assert_eq!(deserialized.model, Some("gpt-4o".to_string()));
}

// =============================================================================
// Test 6: Backward compatibility with old-style JSON
// =============================================================================

#[test]
fn test_spawn_config_backward_compat() {
    // Old-style JSON with only prompt and mode
    let json = r#"{"prompt":"do something","mode":"browser"}"#;
    let config: SpawnSubagentConfig = serde_json::from_str(json).unwrap();

    assert_eq!(config.prompt, "do something");
    assert_eq!(config.mode, Some("browser".to_string()));

    // All new fields should be None/default
    assert_eq!(config.role, None);
    assert_eq!(config.system_prompt, None);
    assert_eq!(config.provider, None);
    assert_eq!(config.model, None);
    assert_eq!(config.tools, None);
    assert_eq!(config.max_iterations, None);
    assert_eq!(config.tab_id, None);
}

// =============================================================================
// Test 7: List agents returns builtin roles
// =============================================================================

#[test]
fn test_list_agents_returns_builtin_roles() {
    let user_dir = tempfile::TempDir::new().unwrap();
    let builtin_dir = builtin_agents_dir();

    let mut registry = AgentRoleRegistry::new(user_dir.path().to_path_buf(), builtin_dir);
    registry.scan().unwrap();

    let summaries = registry.list();

    // Verify count matches expected built-in roles
    assert!(
        summaries.len() >= 4,
        "Expected at least 4 builtin roles, got {}",
        summaries.len()
    );

    // Verify all expected names are present
    let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
    let expected = ["explorer", "researcher", "worker", "reader"];
    for name in &expected {
        assert!(names.contains(name), "Missing expected role: {}", name);
    }

    // Verify descriptions are non-empty
    for summary in &summaries {
        assert!(
            !summary.description.is_empty(),
            "Role '{}' has empty description",
            summary.name
        );
    }
}

// =============================================================================
// Test 8: Nesting prevention
// =============================================================================

#[test]
fn test_nesting_prevention() {
    let config = Arc::new(AgentConfig::default());
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = DaemonHostFunctions::new(config, rt.handle().clone()).with_is_subagent(true);

    // subagent_spawn should return 403
    let result = host.subagent_spawn("test task", "agent", None);
    assert!(
        result.is_err(),
        "Subagent spawn should fail for nested subagent"
    );
    let err = result.unwrap_err();
    assert_eq!(err.code, 403, "Expected 403 error code, got {}", err.code);
    assert!(
        err.message.contains("cannot spawn"),
        "Error message should mention cannot spawn, got: {}",
        err.message
    );

    // list_agents should also return 403
    let result = host.list_agents();
    assert!(
        result.is_err(),
        "list_agents should fail for nested subagent"
    );
    assert_eq!(result.unwrap_err().code, 403);

    // subagent_list should return 403
    let result = host.subagent_list();
    assert!(
        result.is_err(),
        "subagent_list should fail for nested subagent"
    );
    assert_eq!(result.unwrap_err().code, 403);

    // subagent_wait should return 403
    let result = host.subagent_wait(1);
    assert!(
        result.is_err(),
        "subagent_wait should fail for nested subagent"
    );
    assert_eq!(result.unwrap_err().code, 403);

    // subagent_wait_all should return 403
    let result = host.subagent_wait_all(&[1, 2]);
    assert!(
        result.is_err(),
        "subagent_wait_all should fail for nested subagent"
    );
    assert_eq!(result.unwrap_err().code, 403);

    // subagent_kill should return 403
    let result = host.subagent_kill(1);
    assert!(
        result.is_err(),
        "subagent_kill should fail for nested subagent"
    );
    assert_eq!(result.unwrap_err().code, 403);
}

// =============================================================================
// Test 9: Tool filter integration
// =============================================================================

/// Tests the tool filtering logic that powers Agent::filter_tools.
///
/// Since `filter_tools` is private to the Agent struct and `MockHostFunctions`
/// is `#[cfg(test)]`-only in the builtin-wasm crate, we test the underlying
/// `is_tool_allowed` / `matches_tool_pattern` functions from the protocol crate,
/// which are the building blocks of the three-layer filtering.
#[test]
fn test_tool_filter_integration() {
    // Simulate the tool set an agent mode would provide
    let all_tools = vec![
        "browser_navigate",
        "browser_click",
        "browser_get_markdown",
        "browser_screenshot",
        "browser_scroll",
        "browser_go_back",
        "browser_get_content",
        "read",
        "write",
        "bash",
        "glob",
        "grep",
        "web_search",
        "web_fetch",
        "memory_search",
        "memory_store",
    ];

    // Apply Allow(["browser_*"]) filter
    let allowlist = vec!["browser_*".to_string()];
    let filtered: Vec<&&str> = all_tools
        .iter()
        .filter(|name| is_tool_allowed(&allowlist, name))
        .collect();

    // Only browser tools should remain
    assert_eq!(
        filtered.len(),
        7,
        "Expected 7 browser_* tools, got {}",
        filtered.len()
    );
    for tool in &filtered {
        assert!(
            tool.starts_with("browser_"),
            "Non-browser tool passed filter: {}",
            tool
        );
    }

    // Verify specific tools are excluded
    assert!(!is_tool_allowed(&allowlist, "read"));
    assert!(!is_tool_allowed(&allowlist, "write"));
    assert!(!is_tool_allowed(&allowlist, "bash"));
    assert!(!is_tool_allowed(&allowlist, "web_search"));

    // Test multi-pattern allowlist (like researcher role: browser_*, web_*, memory_*)
    let multi_allowlist = vec![
        "browser_*".to_string(),
        "web_*".to_string(),
        "memory_*".to_string(),
    ];
    let multi_filtered: Vec<&&str> = all_tools
        .iter()
        .filter(|name| is_tool_allowed(&multi_allowlist, name))
        .collect();

    // browser_* (7) + web_* (2) + memory_* (2) = 11
    assert_eq!(
        multi_filtered.len(),
        11,
        "Expected 11 tools for researcher-like filter, got {}",
        multi_filtered.len()
    );

    // read, write, bash, glob, grep should be excluded
    assert!(!is_tool_allowed(&multi_allowlist, "read"));
    assert!(!is_tool_allowed(&multi_allowlist, "write"));
    assert!(!is_tool_allowed(&multi_allowlist, "bash"));

    // Test exact match patterns (like reader role: read, glob, grep)
    let exact_allowlist = vec!["read".to_string(), "glob".to_string(), "grep".to_string()];
    let exact_filtered: Vec<&&str> = all_tools
        .iter()
        .filter(|name| is_tool_allowed(&exact_allowlist, name))
        .collect();

    assert_eq!(
        exact_filtered.len(),
        3,
        "Expected 3 tools for reader-like filter, got {}",
        exact_filtered.len()
    );
    assert!(is_tool_allowed(&exact_allowlist, "read"));
    assert!(is_tool_allowed(&exact_allowlist, "glob"));
    assert!(is_tool_allowed(&exact_allowlist, "grep"));
    assert!(!is_tool_allowed(&exact_allowlist, "browser_navigate"));
}

// =============================================================================
// Test 10: ToolsConfig::None returns empty
// =============================================================================

/// Tests that ToolsConfig::None semantics correctly filter out all tools.
///
/// Since the actual `filter_tools` method is private, we verify the semantics
/// that the method implements: when tools_config is ToolsConfig::None,
/// no tools should pass filtering.
#[test]
fn test_tools_config_none_returns_empty() {
    let tools_config = Some(ToolsConfig::None);

    let all_tools = vec![
        "browser_navigate",
        "browser_click",
        "read",
        "write",
        "bash",
        "web_search",
    ];

    // Replicate the filter_tools logic:
    // - None (Option::None) = inherit full set
    // - Some(ToolsConfig::None) = empty vec
    // - Some(ToolsConfig::Allow(list)) = filter by allowlist
    let filtered: Vec<&&str> = match &tools_config {
        None => all_tools.iter().collect(),
        Some(ToolsConfig::None) => Vec::new(),
        Some(ToolsConfig::Allow(allowlist)) => all_tools
            .iter()
            .filter(|name| is_tool_allowed(allowlist, name))
            .collect(),
    };

    assert!(
        filtered.is_empty(),
        "ToolsConfig::None should result in empty tool set"
    );

    // Also verify that Option::None (inherit) returns all tools
    let inherit_config: Option<ToolsConfig> = None;
    let inherit_filtered: Vec<&&str> = match &inherit_config {
        None => all_tools.iter().collect(),
        Some(ToolsConfig::None) => Vec::new(),
        Some(ToolsConfig::Allow(allowlist)) => all_tools
            .iter()
            .filter(|name| is_tool_allowed(allowlist, name))
            .collect(),
    };

    assert_eq!(
        inherit_filtered.len(),
        all_tools.len(),
        "None (inherit) should return all tools"
    );
}

// =============================================================================
// Additional integration tests
// =============================================================================

/// Verify that role definitions loaded from registry have valid system prompts.
#[test]
fn test_role_definitions_have_system_prompts() {
    let user_dir = tempfile::TempDir::new().unwrap();
    let builtin_dir = builtin_agents_dir();

    let mut registry = AgentRoleRegistry::new(user_dir.path().to_path_buf(), builtin_dir);
    registry.scan().unwrap();

    let expected_roles = ["explorer", "researcher", "worker", "reader"];
    for name in &expected_roles {
        let def = registry.get(name).unwrap();
        assert!(
            !def.system_prompt.is_empty(),
            "Role '{}' should have a non-empty system prompt",
            name
        );
        assert!(
            !def.description.is_empty(),
            "Role '{}' should have a non-empty description",
            name
        );
    }
}

/// Verify wildcard pattern matching used by tool filtering.
#[test]
fn test_wildcard_pattern_matching() {
    // Star matches everything
    assert!(matches_tool_pattern("*", "anything"));
    assert!(matches_tool_pattern("*", ""));

    // Prefix wildcard
    assert!(matches_tool_pattern("browser_*", "browser_navigate"));
    assert!(matches_tool_pattern("browser_*", "browser_click"));
    assert!(!matches_tool_pattern("browser_*", "web_search"));
    assert!(!matches_tool_pattern("browser_*", "browser")); // no underscore

    // Exact match
    assert!(matches_tool_pattern("read", "read"));
    assert!(!matches_tool_pattern("read", "read_file"));
    assert!(!matches_tool_pattern("read", "rea"));
}

/// Verify that user roles override builtin roles.
#[test]
fn test_user_role_overrides_builtin() {
    let user_dir = tempfile::TempDir::new().unwrap();
    let builtin_dir = builtin_agents_dir();

    // Create a user role file that overrides the builtin "explorer"
    std::fs::write(
        user_dir.path().join("explorer.md"),
        r#"---
name: explorer
description: "Custom explorer with extra tools"
mode: agent
max_iterations: 25
---

You are a custom explorer agent with additional capabilities.
"#,
    )
    .unwrap();

    let mut registry = AgentRoleRegistry::new(user_dir.path().to_path_buf(), builtin_dir);
    registry.scan().unwrap();

    // The user override should win
    let summaries = registry.list();
    let explorer_summary = summaries.iter().find(|s| s.name == "explorer").unwrap();
    assert_eq!(
        explorer_summary.description,
        "Custom explorer with extra tools"
    );

    // Full definition should also reflect user version
    let def = registry.get("explorer").unwrap();
    assert_eq!(def.mode, "agent"); // changed from browser to agent
    assert_eq!(def.max_iterations, 25); // changed from 10 to 25
    assert!(def.system_prompt.contains("custom explorer"));
}

/// Verify spawn_with_config-style merging: role provides defaults, config overrides.
#[test]
fn test_full_config_merge_chain() {
    let user_dir = tempfile::TempDir::new().unwrap();
    let builtin_dir = builtin_agents_dir();

    let mut registry = AgentRoleRegistry::new(user_dir.path().to_path_buf(), builtin_dir);
    registry.scan().unwrap();

    // Config with role + some overrides
    let config = SpawnSubagentConfig {
        prompt: "Search for Rust articles".to_string(),
        role: Some("researcher".to_string()),
        provider: Some("openai".to_string()),
        model: Some("gpt-4o".to_string()),
        max_iterations: Some(30),
        mode: None,          // inherit from role -> "browser"
        tools: None,         // inherit from role -> Allow(["browser_*", "web_*", "memory_*"])
        system_prompt: None, // inherit from role
        tab_id: None,
    };

    let role_def = registry.get(config.role.as_ref().unwrap()).unwrap();

    // Merge: config overrides role defaults
    let final_mode = config.mode.clone().unwrap_or_else(|| role_def.mode.clone());
    let final_provider = config.provider.clone().or(role_def.provider.clone());
    let final_model = config.model.clone().or(role_def.model.clone());
    let final_tools = config.tools.clone().or(role_def.tools_config.clone());
    let final_max_iterations = config.max_iterations.unwrap_or(role_def.max_iterations);

    assert_eq!(final_mode, "browser", "Mode should come from role");
    assert_eq!(
        final_provider,
        Some("openai".to_string()),
        "Provider should come from config override"
    );
    assert_eq!(
        final_model,
        Some("gpt-4o".to_string()),
        "Model should come from config override"
    );
    assert_eq!(
        final_max_iterations, 30,
        "max_iterations should come from config override"
    );
    assert!(
        matches!(final_tools, Some(ToolsConfig::Allow(_))),
        "Tools should come from role"
    );
}

/// Verify that non-subagent hosts can call list_agents without 403.
#[test]
fn test_non_subagent_can_list() {
    let config = Arc::new(AgentConfig::default());
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = DaemonHostFunctions::new(config, rt.handle().clone());

    // Should not get 403 (may fail for other reasons like no services)
    let result = host.list_agents();
    match result {
        Ok(ref json_str) => {
            // Empty list is fine (no services/registry configured)
            let parsed: Vec<serde_json::Value> = serde_json::from_str(json_str).unwrap();
            assert!(parsed.is_empty(), "Should be empty without services");
        }
        Err(ref e) => {
            assert_ne!(
                e.code, 403,
                "Non-subagent should not get 403, got: {}",
                e.message
            );
        }
    }
}

/// Verify SpawnSubagentConfig with tools override beats role tools.
#[test]
fn test_config_tools_override_beats_role() {
    let user_dir = tempfile::TempDir::new().unwrap();
    let builtin_dir = builtin_agents_dir();

    let mut registry = AgentRoleRegistry::new(user_dir.path().to_path_buf(), builtin_dir);
    registry.scan().unwrap();

    // Explorer role has Allow([browser tools...])
    // But config explicitly sets tools to None
    let config = SpawnSubagentConfig {
        prompt: "Just think about this".to_string(),
        role: Some("explorer".to_string()),
        tools: Some(ToolsConfig::None),
        ..Default::default()
    };

    let role_def = registry.get(config.role.as_ref().unwrap()).unwrap();

    // Config tools should override role tools
    let final_tools = config.tools.or(role_def.tools_config);
    assert_eq!(
        final_tools,
        Some(ToolsConfig::None),
        "Config tools override should win over role tools"
    );
}
