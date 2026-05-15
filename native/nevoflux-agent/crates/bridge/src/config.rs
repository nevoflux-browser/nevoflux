//! Configuration types for the bridge.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Connection mode for the proxy-daemon link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionMode {
    /// Dev mode: connect to a manually-started daemon on fixed port 19500.
    Dev,
    /// Prod mode: auto-spawn daemon on ports 19501-19600, manage its lifecycle.
    Prod,
}

/// Bridge configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    /// TCP port range start.
    pub port_range_start: u16,
    /// TCP port range end.
    pub port_range_end: u16,
    /// Connection timeout.
    #[serde(with = "duration_serde")]
    pub connect_timeout: Duration,
    /// Heartbeat interval.
    #[serde(with = "duration_serde")]
    pub heartbeat_interval: Duration,
    /// Whether to auto-launch daemon if not running.
    pub auto_launch_daemon: bool,
    /// Data directory for port files, etc.
    pub data_dir: Option<PathBuf>,
    /// Connection mode (Dev or Prod).
    pub mode: ConnectionMode,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            port_range_start: 19501,
            port_range_end: 19600,
            connect_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(10),
            auto_launch_daemon: true,
            data_dir: None,
            mode: ConnectionMode::Prod,
        }
    }
}

impl BridgeConfig {
    /// Create a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the port range.
    pub fn with_port_range(mut self, start: u16, end: u16) -> Self {
        self.port_range_start = start;
        self.port_range_end = end;
        self
    }

    /// Set the connection timeout.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set the heartbeat interval.
    pub fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }

    /// Set auto-launch daemon.
    pub fn with_auto_launch(mut self, auto_launch: bool) -> Self {
        self.auto_launch_daemon = auto_launch;
        self
    }

    /// Set the connection mode and apply mode-specific defaults.
    ///
    /// - **Dev**: port 19500 only, auto-launch disabled
    /// - **Prod**: ports 19501-19600, auto-launch enabled
    pub fn with_mode(mut self, mode: ConnectionMode) -> Self {
        self.mode = mode;
        match mode {
            ConnectionMode::Dev => {
                self.port_range_start = 19500;
                self.port_range_end = 19500;
                self.auto_launch_daemon = false;
            }
            ConnectionMode::Prod => {
                self.port_range_start = 19501;
                self.port_range_end = 19600;
                self.auto_launch_daemon = true;
            }
        }
        self
    }

    /// Set the data directory.
    pub fn with_data_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.data_dir = Some(dir.into());
        self
    }

    /// Get the data directory, using platform default if not set.
    pub fn data_directory(&self) -> PathBuf {
        if let Some(ref dir) = self.data_dir {
            dir.clone()
        } else {
            default_data_dir()
        }
    }

    /// Get the port file path (mode-aware).
    ///
    /// - Dev: `daemon.port` (shared with manually-started daemon)
    /// - Prod: `daemon-managed.port` (isolated from dev daemon)
    pub fn port_file_path(&self) -> PathBuf {
        let name = match self.mode {
            ConnectionMode::Dev => "daemon.port",
            ConnectionMode::Prod => "daemon-managed.port",
        };
        self.data_directory().join(name)
    }

    /// Get the PID file path (mode-aware).
    pub fn pid_file_path(&self) -> PathBuf {
        let name = match self.mode {
            ConnectionMode::Dev => "daemon.pid",
            ConnectionMode::Prod => "daemon-managed.pid",
        };
        self.data_directory().join(name)
    }

    /// Get the lock file path (mode-aware).
    pub fn lock_file_path(&self) -> PathBuf {
        let name = match self.mode {
            ConnectionMode::Dev => "daemon.lock",
            ConnectionMode::Prod => "daemon-managed.lock",
        };
        self.data_directory().join(name)
    }
}

/// Get the default data directory for the current platform.
pub fn default_data_dir() -> PathBuf {
    if let Some(data_dir) = directories::ProjectDirs::from("com", "nevoflux", "nevoflux") {
        data_dir.data_dir().to_path_buf()
    } else {
        // Fallback
        PathBuf::from("~/.local/share/nevoflux")
    }
}

/// Serde helpers for Duration.
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_millis().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = BridgeConfig::default();
        assert_eq!(config.port_range_start, 19501);
        assert_eq!(config.port_range_end, 19600);
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.heartbeat_interval, Duration::from_secs(10));
        assert!(config.auto_launch_daemon);
        assert!(config.data_dir.is_none());
        assert_eq!(config.mode, ConnectionMode::Prod);
    }

    #[test]
    fn test_config_builder() {
        let config = BridgeConfig::new()
            .with_port_range(20000, 20100)
            .with_connect_timeout(Duration::from_secs(5))
            .with_heartbeat_interval(Duration::from_secs(30))
            .with_auto_launch(false)
            .with_data_dir("/tmp/nevoflux");

        assert_eq!(config.port_range_start, 20000);
        assert_eq!(config.port_range_end, 20100);
        assert_eq!(config.connect_timeout, Duration::from_secs(5));
        assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
        assert!(!config.auto_launch_daemon);
        assert_eq!(config.data_dir, Some(PathBuf::from("/tmp/nevoflux")));
    }

    #[test]
    fn test_config_data_directory_custom() {
        let config = BridgeConfig::new().with_data_dir("/custom/path");
        assert_eq!(config.data_directory(), PathBuf::from("/custom/path"));
    }

    #[test]
    fn test_config_file_paths_prod() {
        let config = BridgeConfig::new().with_data_dir("/test/dir");

        assert_eq!(
            config.port_file_path(),
            PathBuf::from("/test/dir/daemon-managed.port")
        );
        assert_eq!(
            config.pid_file_path(),
            PathBuf::from("/test/dir/daemon-managed.pid")
        );
        assert_eq!(
            config.lock_file_path(),
            PathBuf::from("/test/dir/daemon-managed.lock")
        );
    }

    #[test]
    fn test_config_file_paths_dev() {
        let config = BridgeConfig::new()
            .with_mode(ConnectionMode::Dev)
            .with_data_dir("/test/dir");

        assert_eq!(
            config.port_file_path(),
            PathBuf::from("/test/dir/daemon.port")
        );
        assert_eq!(
            config.pid_file_path(),
            PathBuf::from("/test/dir/daemon.pid")
        );
        assert_eq!(
            config.lock_file_path(),
            PathBuf::from("/test/dir/daemon.lock")
        );
    }

    #[test]
    fn test_config_json_serialization() {
        let config = BridgeConfig::new()
            .with_port_range(19501, 19600)
            .with_connect_timeout(Duration::from_millis(5000));

        let json = serde_json::to_string(&config).unwrap();
        let decoded: BridgeConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.port_range_start, 19501);
        assert_eq!(decoded.connect_timeout, Duration::from_millis(5000));
    }

    #[test]
    fn test_config_with_mode_dev() {
        let config = BridgeConfig::new().with_mode(ConnectionMode::Dev);
        assert_eq!(config.mode, ConnectionMode::Dev);
        assert_eq!(config.port_range_start, 19500);
        assert_eq!(config.port_range_end, 19500);
        assert!(!config.auto_launch_daemon);
    }

    #[test]
    fn test_config_with_mode_prod() {
        let config = BridgeConfig::new().with_mode(ConnectionMode::Prod);
        assert_eq!(config.mode, ConnectionMode::Prod);
        assert_eq!(config.port_range_start, 19501);
        assert_eq!(config.port_range_end, 19600);
        assert!(config.auto_launch_daemon);
    }

    #[test]
    fn test_connection_mode_eq() {
        assert_eq!(ConnectionMode::Dev, ConnectionMode::Dev);
        assert_eq!(ConnectionMode::Prod, ConnectionMode::Prod);
        assert_ne!(ConnectionMode::Dev, ConnectionMode::Prod);
    }

    #[test]
    fn test_default_data_dir_not_empty() {
        let dir = default_data_dir();
        assert!(!dir.as_os_str().is_empty());
    }
}
