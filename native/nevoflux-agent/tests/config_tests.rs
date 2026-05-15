//! Configuration system integration tests.
//!
//! These tests verify the configuration loading, saving, and default behavior
//! of the NevoFlux Agent configuration system.

use nevoflux_daemon::{
    AgentConfig, ConfigError, ContextConfig, DaemonConfig, LlmConfig, LoggingConfig, SessionConfig,
    StorageConfig,
};
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_config_defaults_are_sensible() {
    let config = AgentConfig::default();

    // Daemon defaults
    assert_eq!(config.daemon.port_range_start, 19500);
    assert_eq!(config.daemon.port_range_end, 19600);
    assert_eq!(config.daemon.max_concurrent_requests, 100);
    assert_eq!(config.daemon.idle_timeout_secs, 1800);
    assert_eq!(config.daemon.heartbeat_timeout_secs, 30);
    assert!(config.daemon.keep_alive_for_mcp);

    // Session defaults
    assert_eq!(config.daemon.session.max_sessions, 500);
    assert!(config.daemon.session.auto_create);

    // Storage defaults
    assert!(config.storage.wal_mode);
    assert_eq!(config.storage.max_size_mb, 1024);

    // Logging defaults
    assert_eq!(config.logging.level, "info");
    assert!(config.logging.stdout);
    assert!(!config.logging.json_format);

    // LLM defaults
    assert_eq!(config.llm.max_tokens, 4096);
    assert_eq!(config.llm.temperature, 0.7);
    assert_eq!(config.llm.max_retries, 3);
}

#[test]
fn test_config_missing_file_returns_defaults() {
    let config = AgentConfig::load_from_path(&PathBuf::from("/nonexistent/config.toml")).unwrap();
    assert_eq!(config.daemon.port_range_start, 19500);
    assert_eq!(config.daemon.port_range_end, 19600);
    assert_eq!(config.llm.max_tokens, 4096);
}

#[test]
fn test_config_save_and_load_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test_config.toml");

    let mut config = AgentConfig::default();
    config.daemon.max_concurrent_requests = 50;
    config.llm.temperature = 0.5;
    config.llm.max_tokens = 8192;
    config.logging.level = "debug".to_string();

    config.save_to_path(&path).unwrap();
    let loaded = AgentConfig::load_from_path(&path).unwrap();

    assert_eq!(loaded.daemon.max_concurrent_requests, 50);
    assert_eq!(loaded.llm.temperature, 0.5);
    assert_eq!(loaded.llm.max_tokens, 8192);
    assert_eq!(loaded.logging.level, "debug");
}

#[test]
fn test_config_partial_toml_missing_sections() {
    // When entire sections are missing, they get defaults
    let dir = tempdir().unwrap();
    let path = dir.path().join("partial_config.toml");

    let toml = r#"
[llm]
default_provider = "anthropic"
max_tokens = 8192
"#;

    std::fs::write(&path, toml).unwrap();

    let config = AgentConfig::load_from_path(&path).unwrap();

    // Custom LLM values should be loaded
    assert_eq!(config.llm.default_provider, Some("anthropic".to_string()));
    assert_eq!(config.llm.max_tokens, 8192);

    // Other sections should have defaults (entire sections missing)
    assert_eq!(config.daemon.port_range_start, 19500);
    assert_eq!(config.storage.max_size_mb, 1024);
    assert_eq!(config.logging.level, "info");
}

#[test]
fn test_config_all_sections_serialized() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("full_config.toml");

    let mut config = AgentConfig::default();
    config.llm.default_provider = Some("openai".to_string());
    config.save_to_path(&path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();

    // Verify all sections are present
    assert!(content.contains("[daemon]"));
    assert!(content.contains("[llm]"));
    assert!(content.contains("[storage]"));
    assert!(content.contains("[logging]"));
    assert!(content.contains("default_provider = \"openai\""));
}

#[test]
fn test_config_nested_structures() {
    // Test that saved config with nested structures loads correctly
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested_config.toml");

    // Create config with modified nested values
    let mut config = AgentConfig::default();
    config.daemon.port_range_start = 20000;
    config.daemon.port_range_end = 21000;
    config.daemon.session.max_sessions = 100;
    config.daemon.session.inactive_days = 30;
    config.daemon.context.system_prompt_reserve = 3000;
    config.daemon.context.include_memory = false;

    // Save and reload
    config.save_to_path(&path).unwrap();
    let loaded = AgentConfig::load_from_path(&path).unwrap();

    assert_eq!(loaded.daemon.port_range_start, 20000);
    assert_eq!(loaded.daemon.port_range_end, 21000);
    assert_eq!(loaded.daemon.session.max_sessions, 100);
    assert_eq!(loaded.daemon.session.inactive_days, 30);
    assert_eq!(loaded.daemon.context.system_prompt_reserve, 3000);
    assert!(!loaded.daemon.context.include_memory);
}

#[test]
fn test_config_merge_behavior() {
    let mut base = AgentConfig::default();
    let mut other = AgentConfig::default();

    // Set non-default values in other
    other.daemon.port_range_start = 21000;
    other.llm.default_provider = Some("anthropic".to_string());
    other.storage.data_dir = Some(PathBuf::from("/merged/path"));
    other.logging.level = "trace".to_string();

    base.merge(&other);

    // Merged values should be applied
    assert_eq!(base.daemon.port_range_start, 21000);
    assert_eq!(base.llm.default_provider, Some("anthropic".to_string()));
    assert_eq!(base.storage.data_dir, Some(PathBuf::from("/merged/path")));
    assert_eq!(base.logging.level, "trace");

    // Values not changed should keep defaults
    assert_eq!(base.daemon.idle_timeout_secs, 1800);
    assert_eq!(base.llm.max_tokens, 4096);
}

#[test]
fn test_config_creates_parent_directories() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested").join("dirs").join("config.toml");

    let config = AgentConfig::default();
    config.save_to_path(&path).unwrap();

    assert!(path.exists());
}

#[test]
fn test_config_invalid_toml_returns_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("invalid.toml");

    std::fs::write(&path, "this is not valid toml {{{{").unwrap();

    let result = AgentConfig::load_from_path(&path);
    assert!(result.is_err());

    match result {
        Err(ConfigError::ParseError(_)) => (),
        _ => panic!("Expected ParseError"),
    }
}

#[test]
fn test_daemon_config_builder_pattern() {
    let config = DaemonConfig::new()
        .with_idle_timeout(3600)
        .with_heartbeat_timeout(60)
        .with_keep_alive_for_mcp(false);

    assert_eq!(config.idle_timeout_secs, 3600);
    assert_eq!(config.heartbeat_timeout_secs, 60);
    assert!(!config.keep_alive_for_mcp);
}

#[test]
fn test_llm_config_defaults() {
    let config = LlmConfig::default();

    assert!(config.default_provider.is_none());
    assert!(config.default_model.is_none());
    assert_eq!(config.max_tokens, 4096);
    assert_eq!(config.temperature, 0.7);
    assert_eq!(config.timeout_secs, 120);
    assert_eq!(config.max_retries, 3);
}

#[test]
fn test_storage_config_defaults() {
    let config = StorageConfig::default();

    assert!(config.data_dir.is_none());
    assert_eq!(config.max_size_mb, 1024);
    assert!(config.wal_mode);
    assert!(!config.vacuum_on_startup);
}

#[test]
fn test_logging_config_defaults() {
    let config = LoggingConfig::default();

    assert_eq!(config.level, "info");
    assert!(config.file.is_none());
    assert!(config.stdout);
    assert!(!config.json_format);
}

#[test]
fn test_session_config_defaults() {
    let config = SessionConfig::default();

    assert_eq!(config.max_sessions, 500);
    assert_eq!(config.inactive_days, 90);
    assert_eq!(config.max_storage_mb, 500);
    assert!(config.auto_create);
}

#[test]
fn test_context_config_defaults() {
    let config = ContextConfig::default();

    assert_eq!(config.system_prompt_reserve, 2000);
    assert_eq!(config.safety_margin, 500);
    assert_eq!(config.max_history_messages, 50);
    assert!(config.include_memory);
    assert!(config.include_current_page);
}

#[test]
fn test_config_serialization_roundtrip() {
    let config = DaemonConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let decoded: DaemonConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(config.idle_timeout_secs, decoded.idle_timeout_secs);
    assert_eq!(config.port_range_start, decoded.port_range_start);
}
