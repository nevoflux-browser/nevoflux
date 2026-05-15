//! Port discovery for finding the daemon.
//!
//! The daemon writes its port to a file. This module handles reading that file
//! and checking if the daemon is running.

use crate::config::BridgeConfig;
use crate::error::{BridgeError, Result};
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use tokio::net::TcpStream;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Port discovery result.
#[derive(Debug, Clone)]
pub struct DaemonInfo {
    /// The port the daemon is listening on.
    pub port: u16,
    /// The PID of the daemon process.
    pub pid: Option<u32>,
}

/// Discover the daemon port.
///
/// Reads the port file and optionally the PID file to get daemon info.
pub async fn discover_daemon(config: &BridgeConfig) -> Result<DaemonInfo> {
    let port_file = config.port_file_path();
    let pid_file = config.pid_file_path();

    // Read port file
    let port = read_port_file(&port_file).await?;

    // Try to read PID file (optional)
    let pid = read_pid_file(&pid_file).await.ok();

    debug!("Discovered daemon at port {}, pid {:?}", port, pid);

    Ok(DaemonInfo { port, pid })
}

/// Read the port from the port file.
pub async fn read_port_file(path: &Path) -> Result<u16> {
    if !path.exists() {
        return Err(BridgeError::PortFileNotFound(path.display().to_string()));
    }

    let contents = fs::read_to_string(path).await.map_err(BridgeError::Io)?;

    let port: u16 = contents.trim().parse().map_err(|_| {
        BridgeError::InvalidPortFile(format!("Invalid port number: {}", contents.trim()))
    })?;

    // Validate port range
    if port < 1024 {
        return Err(BridgeError::InvalidPortFile(format!(
            "Port {} is in privileged range",
            port
        )));
    }

    Ok(port)
}

/// Read the PID from the PID file.
pub async fn read_pid_file(path: &Path) -> Result<u32> {
    if !path.exists() {
        return Err(BridgeError::PortFileNotFound(path.display().to_string()));
    }

    let contents = fs::read_to_string(path).await?;

    contents
        .trim()
        .parse()
        .map_err(|_| BridgeError::InvalidPortFile(format!("Invalid PID: {}", contents.trim())))
}

/// Write the port to the port file.
pub async fn write_port_file(path: &Path, port: u16) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    fs::write(path, port.to_string()).await?;
    Ok(())
}

/// Write the PID to the PID file.
pub async fn write_pid_file(path: &Path, pid: u32) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    fs::write(path, pid.to_string()).await?;
    Ok(())
}

/// Clean up port and PID files.
pub async fn cleanup_files(config: &BridgeConfig) -> Result<()> {
    let port_file = config.port_file_path();
    let pid_file = config.pid_file_path();

    if port_file.exists() {
        fs::remove_file(&port_file).await.ok();
    }

    if pid_file.exists() {
        fs::remove_file(&pid_file).await.ok();
    }

    Ok(())
}

/// Check if a process with the given PID is running.
#[cfg(unix)]
pub async fn is_process_running(pid: u32) -> bool {
    // Using /bin/kill is portable across Unix systems
    let output = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .await;

    matches!(output, Ok(o) if o.status.success())
}

#[cfg(windows)]
pub async fn is_process_running(pid: u32) -> bool {
    // On Windows, use tasklist
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/NH"])
        .output()
        .await;

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.contains(&pid.to_string())
        }
        Err(_) => false,
    }
}

/// Find an available port in the configured range.
pub async fn find_available_port(config: &BridgeConfig) -> Result<u16> {
    use tokio::net::TcpListener;

    for port in config.port_range_start..=config.port_range_end {
        let addr = format!("127.0.0.1:{}", port);
        match TcpListener::bind(&addr).await {
            Ok(_listener) => {
                debug!("Found available port: {}", port);
                return Ok(port);
            }
            Err(_) => continue,
        }
    }

    Err(BridgeError::ConnectionFailed(format!(
        "No available port in range {}-{}",
        config.port_range_start, config.port_range_end
    )))
}

/// Launch the daemon process.
pub async fn launch_daemon(executable: &Path, config: &BridgeConfig) -> Result<u32> {
    if !executable.exists() {
        return Err(BridgeError::DaemonLaunchFailed(format!(
            "Executable not found: {}",
            executable.display()
        )));
    }

    info!("Launching daemon: {}", executable.display());

    // On Windows, launch via powershell.exe so the daemon inherits the user's
    // PowerShell profile environment (e.g. API keys set in $PROFILE).
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;

        let powershell_cmd = format!(
            "& '{}' --daemon --port-start {} --port-end {} --managed",
            executable.display(),
            config.port_range_start,
            config.port_range_end,
        );

        let mut c = Command::new("powershell.exe");
        c.arg("-NoLogo").arg("-Command").arg(&powershell_cmd);

        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
        c
    };

    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new(executable);
        c.arg("--daemon");
        c.arg("--port-start")
            .arg(config.port_range_start.to_string());
        c.arg("--port-end").arg(config.port_range_end.to_string());
        c.arg("--managed");
        c
    };

    // CRITICAL: redirect stdout/stderr to null so daemon output does not
    // pollute the proxy's Native Messaging channel (stdout = protocol wire).
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    let child = cmd
        .spawn()
        .map_err(|e| BridgeError::DaemonLaunchFailed(e.to_string()))?;

    let pid = child
        .id()
        .ok_or_else(|| BridgeError::DaemonLaunchFailed("Failed to get daemon PID".into()))?;

    info!("Daemon launched with PID: {}", pid);

    // Wait for the daemon to start — it writes the port file early (right after
    // port discovery), but on cold starts with model downloads this can still
    // take several seconds.
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // Poll for port file
    let port_file = config.port_file_path();
    let mut attempts = 0;
    while !port_file.exists() && attempts < 50 {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        attempts += 1;
    }

    if !port_file.exists() {
        warn!("Port file not created after launching daemon");
    }

    Ok(pid)
}

/// Launch the daemon process with an explicit port (zero-file managed mode).
///
/// The proxy has already allocated the port, so the daemon binds to it directly
/// without writing any port/pid/lock files.
pub async fn launch_daemon_with_port(
    executable: &Path,
    config: &BridgeConfig,
    port: u16,
) -> Result<u32> {
    if !executable.exists() {
        return Err(BridgeError::DaemonLaunchFailed(format!(
            "Executable not found: {}",
            executable.display()
        )));
    }

    info!(
        "Launching daemon with explicit port {}: {}",
        port,
        executable.display()
    );

    // On Windows, launch via powershell.exe so the daemon inherits the user's
    // PowerShell profile environment (e.g. API keys set in $PROFILE).
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;

        let powershell_cmd = format!(
            "& '{}' --daemon --managed --port {} --port-start {} --port-end {}",
            executable.display(),
            port,
            config.port_range_start,
            config.port_range_end,
        );

        let mut c = Command::new("powershell.exe");
        c.arg("-NoLogo").arg("-Command").arg(&powershell_cmd);

        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
        c
    };

    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new(executable);
        c.arg("--daemon")
            .arg("--managed")
            .arg("--port")
            .arg(port.to_string());
        c.arg("--port-start")
            .arg(config.port_range_start.to_string());
        c.arg("--port-end").arg(config.port_range_end.to_string());
        c
    };

    // Redirect stdout/stderr to null so daemon output does not
    // pollute the proxy's Native Messaging channel.
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    let child = cmd
        .spawn()
        .map_err(|e| BridgeError::DaemonLaunchFailed(e.to_string()))?;

    let pid = child
        .id()
        .ok_or_else(|| BridgeError::DaemonLaunchFailed("Failed to get daemon PID".into()))?;

    info!("Daemon launched with PID {} on port {}", pid, port);
    Ok(pid)
}

/// Wait for a daemon to become ready by polling TCP connectivity.
///
/// Replaces file-polling in zero-file managed mode. Returns `Ok(())` once a
/// TCP connection to the given port succeeds, or an error if the timeout is
/// exceeded.
pub async fn wait_for_daemon_ready(port: u16, timeout: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    let addr = format!("127.0.0.1:{}", port);

    debug!(
        "Waiting for daemon on port {} (timeout {:?})",
        port, timeout
    );

    loop {
        if tokio::time::Instant::now() > deadline {
            return Err(BridgeError::ConnectionFailed(format!(
                "Daemon did not start in time on port {}",
                port
            )));
        }
        match TcpStream::connect(&addr).await {
            Ok(_) => {
                debug!("Daemon ready on port {}", port);
                return Ok(());
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(dir: &Path) -> BridgeConfig {
        BridgeConfig::new().with_data_dir(dir)
    }

    #[tokio::test]
    async fn test_read_port_file() {
        let temp = TempDir::new().unwrap();
        let port_file = temp.path().join("daemon.port");

        fs::write(&port_file, "19500").await.unwrap();

        let port = read_port_file(&port_file).await.unwrap();
        assert_eq!(port, 19500);
    }

    #[tokio::test]
    async fn test_read_port_file_not_found() {
        let temp = TempDir::new().unwrap();
        let port_file = temp.path().join("daemon.port");

        let result = read_port_file(&port_file).await;
        assert!(matches!(result, Err(BridgeError::PortFileNotFound(_))));
    }

    #[tokio::test]
    async fn test_read_port_file_invalid() {
        let temp = TempDir::new().unwrap();
        let port_file = temp.path().join("daemon.port");

        fs::write(&port_file, "not-a-number").await.unwrap();

        let result = read_port_file(&port_file).await;
        assert!(matches!(result, Err(BridgeError::InvalidPortFile(_))));
    }

    #[tokio::test]
    async fn test_read_port_file_privileged() {
        let temp = TempDir::new().unwrap();
        let port_file = temp.path().join("daemon.port");

        fs::write(&port_file, "80").await.unwrap();

        let result = read_port_file(&port_file).await;
        assert!(matches!(result, Err(BridgeError::InvalidPortFile(_))));
    }

    #[tokio::test]
    async fn test_read_port_file_with_whitespace() {
        let temp = TempDir::new().unwrap();
        let port_file = temp.path().join("daemon.port");

        fs::write(&port_file, "  19500\n").await.unwrap();

        let port = read_port_file(&port_file).await.unwrap();
        assert_eq!(port, 19500);
    }

    #[tokio::test]
    async fn test_read_pid_file() {
        let temp = TempDir::new().unwrap();
        let pid_file = temp.path().join("daemon.pid");

        fs::write(&pid_file, "12345").await.unwrap();

        let pid = read_pid_file(&pid_file).await.unwrap();
        assert_eq!(pid, 12345);
    }

    #[tokio::test]
    async fn test_read_pid_file_not_found() {
        let temp = TempDir::new().unwrap();
        let pid_file = temp.path().join("daemon.pid");

        let result = read_pid_file(&pid_file).await;
        assert!(matches!(result, Err(BridgeError::PortFileNotFound(_))));
    }

    #[tokio::test]
    async fn test_write_port_file() {
        let temp = TempDir::new().unwrap();
        let port_file = temp.path().join("subdir/daemon.port");

        write_port_file(&port_file, 19550).await.unwrap();

        let contents = fs::read_to_string(&port_file).await.unwrap();
        assert_eq!(contents, "19550");
    }

    #[tokio::test]
    async fn test_write_pid_file() {
        let temp = TempDir::new().unwrap();
        let pid_file = temp.path().join("daemon.pid");

        write_pid_file(&pid_file, 99999).await.unwrap();

        let contents = fs::read_to_string(&pid_file).await.unwrap();
        assert_eq!(contents, "99999");
    }

    #[tokio::test]
    async fn test_discover_daemon() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());

        // Write port and PID files
        fs::write(config.port_file_path(), "19500").await.unwrap();
        fs::write(config.pid_file_path(), "12345").await.unwrap();

        let info = discover_daemon(&config).await.unwrap();
        assert_eq!(info.port, 19500);
        assert_eq!(info.pid, Some(12345));
    }

    #[tokio::test]
    async fn test_discover_daemon_no_pid_file() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());

        // Write only port file
        fs::write(config.port_file_path(), "19500").await.unwrap();

        let info = discover_daemon(&config).await.unwrap();
        assert_eq!(info.port, 19500);
        assert!(info.pid.is_none());
    }

    #[tokio::test]
    async fn test_discover_daemon_no_port_file() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());

        let result = discover_daemon(&config).await;
        assert!(matches!(result, Err(BridgeError::PortFileNotFound(_))));
    }

    #[tokio::test]
    async fn test_cleanup_files() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());

        // Create files
        fs::write(config.port_file_path(), "19500").await.unwrap();
        fs::write(config.pid_file_path(), "12345").await.unwrap();

        // Cleanup
        cleanup_files(&config).await.unwrap();

        assert!(!config.port_file_path().exists());
        assert!(!config.pid_file_path().exists());
    }

    #[tokio::test]
    async fn test_cleanup_files_not_exist() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());

        // Should not error if files don't exist
        cleanup_files(&config).await.unwrap();
    }

    #[tokio::test]
    async fn test_find_available_port() {
        let config = BridgeConfig::new().with_port_range(49152, 49200);

        let port = find_available_port(&config).await.unwrap();
        assert!((49152..=49200).contains(&port));
    }

    #[tokio::test]
    async fn test_daemon_info_debug() {
        let info = DaemonInfo {
            port: 19500,
            pid: Some(12345),
        };

        let debug_str = format!("{:?}", info);
        assert!(debug_str.contains("19500"));
        assert!(debug_str.contains("12345"));
    }

    #[tokio::test]
    async fn test_launch_daemon_not_found() {
        let config = BridgeConfig::new();
        let result = launch_daemon(Path::new("/nonexistent/binary"), &config).await;
        assert!(matches!(result, Err(BridgeError::DaemonLaunchFailed(_))));
    }

    #[tokio::test]
    async fn test_launch_daemon_with_port_not_found() {
        let config = BridgeConfig::new();
        let result =
            launch_daemon_with_port(Path::new("/nonexistent/binary"), &config, 19523).await;
        assert!(matches!(result, Err(BridgeError::DaemonLaunchFailed(_))));
    }

    #[tokio::test]
    async fn test_wait_for_daemon_ready_timeout() {
        // Use a port that nothing is listening on
        let result = wait_for_daemon_ready(19999, std::time::Duration::from_millis(200)).await;
        assert!(matches!(result, Err(BridgeError::ConnectionFailed(_))));
    }

    #[tokio::test]
    async fn test_wait_for_daemon_ready_success() {
        // Start a TCP listener, then wait_for_daemon_ready should succeed
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let result = wait_for_daemon_ready(port, std::time::Duration::from_secs(2)).await;
        assert!(result.is_ok());
    }
}
