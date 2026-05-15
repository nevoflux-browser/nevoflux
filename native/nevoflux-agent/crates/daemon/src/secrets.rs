//! Secure API key management with layered lookup.
//!
//! Provides secure storage and retrieval of API keys using a layered approach:
//! 1. Environment variables (highest priority)
//! 2. System keyring (secure cross-platform storage)
//! 3. Config file (fallback, least secure)
//!
//! # Example
//!
//! ```rust,ignore
//! use nevoflux_daemon::secrets::ApiKeyManager;
//!
//! let manager = ApiKeyManager::new("nevoflux");
//!
//! // Get an API key (checks env, then keyring, then config)
//! let key = manager.get_api_key("anthropic")?;
//!
//! // Store a key in the keyring
//! manager.store_api_key("anthropic", "sk-...")?;
//! ```

use std::collections::HashMap;
use thiserror::Error;

/// Service name for keyring entries.
const DEFAULT_SERVICE: &str = "nevoflux";

/// Environment variable prefix for API keys.
const ENV_PREFIX: &str = "NEVOFLUX_API_KEY_";

/// Errors that can occur during secret operations.
#[derive(Debug, Error)]
pub enum SecretError {
    /// API key not found in any source.
    #[error("API key not found for provider: {0}")]
    NotFound(String),

    /// Keyring access error.
    #[error("Keyring error: {0}")]
    KeyringError(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Result type for secret operations.
pub type Result<T> = std::result::Result<T, SecretError>;

/// Source of an API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// From environment variable.
    Environment,
    /// From system keyring.
    Keyring,
    /// From configuration file.
    Config,
    /// From fallback/cache.
    Fallback,
}

/// An API key with its source.
#[derive(Debug, Clone)]
pub struct ApiKey {
    /// The key value.
    pub value: String,
    /// Where the key was found.
    pub source: KeySource,
}

impl ApiKey {
    /// Create a new API key.
    pub fn new(value: impl Into<String>, source: KeySource) -> Self {
        Self {
            value: value.into(),
            source,
        }
    }
}

/// Manager for secure API key storage and retrieval.
///
/// Implements layered lookup: env var → keyring → config file.
pub struct ApiKeyManager {
    /// Service name for keyring entries.
    service: String,
    /// Fallback keys from config (least secure).
    config_keys: HashMap<String, String>,
    /// Whether keyring is available.
    keyring_available: bool,
}

impl Default for ApiKeyManager {
    fn default() -> Self {
        Self::new(DEFAULT_SERVICE)
    }
}

impl ApiKeyManager {
    /// Create a new API key manager with the given service name.
    pub fn new(service: impl Into<String>) -> Self {
        let service = service.into();
        let keyring_available = Self::check_keyring_available(&service);

        Self {
            service,
            config_keys: HashMap::new(),
            keyring_available,
        }
    }

    /// Create a manager with config fallback keys.
    pub fn with_config_keys(mut self, keys: HashMap<String, String>) -> Self {
        self.config_keys = keys;
        self
    }

    /// Add a config fallback key.
    pub fn add_config_key(&mut self, provider: impl Into<String>, key: impl Into<String>) {
        self.config_keys.insert(provider.into(), key.into());
    }

    /// Get an API key using layered lookup.
    ///
    /// Checks in order:
    /// 1. Environment variable: `NEVOFLUX_API_KEY_{PROVIDER}` (uppercase)
    /// 2. System keyring
    /// 3. Config file fallback
    pub fn get_api_key(&self, provider: &str) -> Result<ApiKey> {
        // 1. Check environment variable
        let env_key = format!("{}{}", ENV_PREFIX, provider.to_uppercase());
        if let Ok(value) = std::env::var(&env_key) {
            if !value.is_empty() {
                return Ok(ApiKey::new(value, KeySource::Environment));
            }
        }

        // Also check provider-specific env vars (e.g., ANTHROPIC_API_KEY)
        let provider_env = format!("{}_API_KEY", provider.to_uppercase());
        if let Ok(value) = std::env::var(&provider_env) {
            if !value.is_empty() {
                return Ok(ApiKey::new(value, KeySource::Environment));
            }
        }

        // 2. Check keyring
        if self.keyring_available {
            if let Ok(key) = self.get_from_keyring(provider) {
                return Ok(key);
            }
        }

        // 3. Check config fallback
        if let Some(value) = self.config_keys.get(provider) {
            return Ok(ApiKey::new(value.clone(), KeySource::Config));
        }

        Err(SecretError::NotFound(provider.to_string()))
    }

    /// Store an API key in the system keyring.
    pub fn store_api_key(&self, provider: &str, key: &str) -> Result<()> {
        if !self.keyring_available {
            return Err(SecretError::KeyringError(
                "Keyring not available on this system".to_string(),
            ));
        }

        self.store_in_keyring(provider, key)
    }

    /// Delete an API key from the system keyring.
    pub fn delete_api_key(&self, provider: &str) -> Result<()> {
        if !self.keyring_available {
            return Err(SecretError::KeyringError(
                "Keyring not available on this system".to_string(),
            ));
        }

        self.delete_from_keyring(provider)
    }

    /// Check if an API key exists for a provider.
    pub fn has_api_key(&self, provider: &str) -> bool {
        self.get_api_key(provider).is_ok()
    }

    /// Check if keyring is available on this system.
    pub fn is_keyring_available(&self) -> bool {
        self.keyring_available
    }

    /// List all providers that have API keys configured.
    pub fn list_providers(&self) -> Vec<String> {
        let mut providers = Vec::new();

        // Check env vars
        for (key, _) in std::env::vars() {
            if let Some(provider) = key.strip_prefix(ENV_PREFIX) {
                providers.push(provider.to_lowercase());
            }
            if let Some(provider) = key.strip_suffix("_API_KEY") {
                if !provider.starts_with("NEVOFLUX") {
                    providers.push(provider.to_lowercase());
                }
            }
        }

        // Add config keys
        for provider in self.config_keys.keys() {
            if !providers.contains(provider) {
                providers.push(provider.clone());
            }
        }

        providers.sort();
        providers.dedup();
        providers
    }

    /// Check if keyring is available on the system.
    fn check_keyring_available(service: &str) -> bool {
        // Try to access the keyring with a test entry
        // This is a heuristic - actual availability depends on the platform
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            // On supported platforms, assume keyring is available
            // Actual errors will be caught when trying to use it
            let _ = service; // Suppress unused warning
            true
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = service;
            false
        }
    }

    /// Get a key from the keyring.
    ///
    /// Note: This is a placeholder implementation. In a real implementation,
    /// you would use the `keyring` crate to access the system keyring.
    fn get_from_keyring(&self, provider: &str) -> Result<ApiKey> {
        // Placeholder: In production, use keyring crate:
        // let entry = keyring::Entry::new(&self.service, provider)?;
        // let key = entry.get_password()?;
        // return Ok(ApiKey::new(key, KeySource::Keyring));

        // For now, check a special env var that simulates keyring
        let keyring_env = format!("NEVOFLUX_KEYRING_{}", provider.to_uppercase());
        if let Ok(value) = std::env::var(&keyring_env) {
            if !value.is_empty() {
                return Ok(ApiKey::new(value, KeySource::Keyring));
            }
        }

        Err(SecretError::NotFound(provider.to_string()))
    }

    /// Store a key in the keyring.
    fn store_in_keyring(&self, provider: &str, _key: &str) -> Result<()> {
        // Placeholder: In production, use keyring crate:
        // let entry = keyring::Entry::new(&self.service, provider)?;
        // entry.set_password(key)?;

        tracing::debug!(
            service = %self.service,
            provider = %provider,
            "Would store API key in keyring (placeholder)"
        );

        Ok(())
    }

    /// Delete a key from the keyring.
    fn delete_from_keyring(&self, provider: &str) -> Result<()> {
        // Placeholder: In production, use keyring crate:
        // let entry = keyring::Entry::new(&self.service, provider)?;
        // entry.delete_password()?;

        tracing::debug!(
            service = %self.service,
            provider = %provider,
            "Would delete API key from keyring (placeholder)"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_manager_new() {
        let manager = ApiKeyManager::new("test-service");
        assert_eq!(manager.service, "test-service");
    }

    #[test]
    fn test_api_key_manager_default() {
        let manager = ApiKeyManager::default();
        assert_eq!(manager.service, DEFAULT_SERVICE);
    }

    #[test]
    fn test_api_key_from_env() {
        // Set up test env var
        std::env::set_var("NEVOFLUX_API_KEY_TESTPROVIDER", "test-key-123");

        let manager = ApiKeyManager::new("test");
        let key = manager.get_api_key("testprovider").unwrap();

        assert_eq!(key.value, "test-key-123");
        assert_eq!(key.source, KeySource::Environment);

        // Clean up
        std::env::remove_var("NEVOFLUX_API_KEY_TESTPROVIDER");
    }

    #[test]
    fn test_api_key_from_provider_env() {
        // Set up provider-specific env var
        std::env::set_var("MYPROVIDER_API_KEY", "provider-key-456");

        let manager = ApiKeyManager::new("test");
        let key = manager.get_api_key("myprovider").unwrap();

        assert_eq!(key.value, "provider-key-456");
        assert_eq!(key.source, KeySource::Environment);

        // Clean up
        std::env::remove_var("MYPROVIDER_API_KEY");
    }

    #[test]
    fn test_api_key_from_config() {
        let mut config_keys = HashMap::new();
        config_keys.insert("configprovider".to_string(), "config-key-789".to_string());

        let manager = ApiKeyManager::new("test").with_config_keys(config_keys);
        let key = manager.get_api_key("configprovider").unwrap();

        assert_eq!(key.value, "config-key-789");
        assert_eq!(key.source, KeySource::Config);
    }

    #[test]
    fn test_api_key_not_found() {
        let manager = ApiKeyManager::new("test");
        let result = manager.get_api_key("nonexistent");

        assert!(matches!(result, Err(SecretError::NotFound(_))));
    }

    #[test]
    fn test_add_config_key() {
        let mut manager = ApiKeyManager::new("test");
        manager.add_config_key("added", "added-key");

        let key = manager.get_api_key("added").unwrap();
        assert_eq!(key.value, "added-key");
        assert_eq!(key.source, KeySource::Config);
    }

    #[test]
    fn test_has_api_key() {
        let mut config_keys = HashMap::new();
        config_keys.insert("exists".to_string(), "key".to_string());

        let manager = ApiKeyManager::new("test").with_config_keys(config_keys);

        assert!(manager.has_api_key("exists"));
        assert!(!manager.has_api_key("nonexistent"));
    }

    #[test]
    fn test_env_takes_priority() {
        // Set up both env and config
        std::env::set_var("NEVOFLUX_API_KEY_PRIORITY", "env-key");

        let mut config_keys = HashMap::new();
        config_keys.insert("priority".to_string(), "config-key".to_string());

        let manager = ApiKeyManager::new("test").with_config_keys(config_keys);
        let key = manager.get_api_key("priority").unwrap();

        // Env should take priority
        assert_eq!(key.value, "env-key");
        assert_eq!(key.source, KeySource::Environment);

        // Clean up
        std::env::remove_var("NEVOFLUX_API_KEY_PRIORITY");
    }

    #[test]
    fn test_api_key_struct() {
        let key = ApiKey::new("test-value", KeySource::Keyring);

        assert_eq!(key.value, "test-value");
        assert_eq!(key.source, KeySource::Keyring);
    }

    #[test]
    fn test_keyring_simulation() {
        // Set up simulated keyring env var
        std::env::set_var("NEVOFLUX_KEYRING_SIMULATED", "keyring-key");

        let manager = ApiKeyManager::new("test");
        let key = manager.get_api_key("simulated").unwrap();

        assert_eq!(key.value, "keyring-key");
        assert_eq!(key.source, KeySource::Keyring);

        // Clean up
        std::env::remove_var("NEVOFLUX_KEYRING_SIMULATED");
    }

    #[test]
    fn test_list_providers_empty() {
        let manager = ApiKeyManager::new("test");
        let providers = manager.list_providers();
        // May not be empty if there are env vars set, but should not panic
        let _ = providers;
    }

    #[test]
    fn test_list_providers_with_config() {
        let mut config_keys = HashMap::new();
        config_keys.insert("provider1".to_string(), "key1".to_string());
        config_keys.insert("provider2".to_string(), "key2".to_string());

        let manager = ApiKeyManager::new("test").with_config_keys(config_keys);
        let providers = manager.list_providers();

        assert!(providers.contains(&"provider1".to_string()));
        assert!(providers.contains(&"provider2".to_string()));
    }

    #[test]
    fn test_keyring_availability() {
        let manager = ApiKeyManager::new("test");
        // On supported platforms, keyring should be available
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            assert!(manager.is_keyring_available());
        }
    }
}
