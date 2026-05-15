//! Logging infrastructure for NevoFlux Agent.
//!
//! Provides centralized logging configuration with support for:
//! - Console and file logging
//! - Verbose mode with debug level output
//! - Environment variable override via RUST_LOG
//! - Startup and shutdown logging helpers

use std::path::PathBuf;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize logging with the given configuration.
///
/// # Arguments
///
/// * `verbose` - If true, sets log level to debug; otherwise uses RUST_LOG env or defaults to info
/// * `log_file` - Optional path to write logs to a file (in addition to console)
///
/// # Examples
///
/// ```ignore
/// // Console-only logging at info level
/// init_logging(false, None);
///
/// // Verbose console logging
/// init_logging(true, None);
///
/// // Console + file logging
/// init_logging(false, Some(PathBuf::from("/var/log/nevoflux.log")));
/// ```
pub fn init_logging(verbose: bool, log_file: Option<PathBuf>) {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    let subscriber = tracing_subscriber::registry().with(filter);

    if let Some(log_path) = log_file {
        // File logging
        let file = std::fs::File::create(&log_path).ok();
        if let Some(file) = file {
            let file_layer = fmt::layer().with_writer(file).with_ansi(false);
            subscriber.with(file_layer).with(fmt::layer()).init();
        } else {
            tracing::warn!("Failed to create log file at {:?}", log_path);
            subscriber.with(fmt::layer()).init();
        }
    } else {
        // Console only
        subscriber.with(fmt::layer()).init();
    }
}

/// Initialize logging for stderr output.
///
/// This is used for proxy and MCP modes where stdout is reserved for protocol messages.
///
/// # Arguments
///
/// * `verbose` - If true, sets log level to debug; otherwise uses RUST_LOG env or defaults to info
/// * `module_filter` - Optional module-specific filter (e.g., "nevoflux=debug")
pub fn init_stderr_logging(verbose: bool, module_filter: Option<&str>) {
    let base_filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    let filter = if let Some(directive) = module_filter {
        base_filter.add_directive(
            directive
                .parse()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        )
    } else {
        base_filter
    };

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();
}

/// Initialize logging to file only (no console/stderr output).
///
/// This is used for proxy mode where both stdout and stderr should be silent.
/// Logs are written to the specified file path.
///
/// # Arguments
///
/// * `log_file` - Path to write logs to
/// * `verbose` - If true, sets log level to debug; otherwise defaults to info
pub fn init_file_only_logging(log_file: PathBuf, verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
        let file_layer = fmt::layer()
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false);

        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .init();
    }
    // If file creation fails, logging is silently disabled
}

/// Log a startup message with version information.
///
/// # Arguments
///
/// * `version` - The version string to include in the log message
pub fn log_startup(version: &str) {
    tracing::info!(version = version, "NevoFlux Agent starting");
}

/// Log a shutdown message.
pub fn log_shutdown() {
    tracing::info!("NevoFlux Agent shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests are limited because tracing can only be initialized once per process.
    // We test the configuration building rather than actual initialization.

    #[test]
    fn test_env_filter_verbose_creates_filter() {
        // Verify that creating a debug filter doesn't panic
        let filter = EnvFilter::new("debug");
        // EnvFilter exists and can be used
        let _ = format!("{:?}", filter);
    }

    #[test]
    fn test_env_filter_default_creates_filter() {
        // Verify that creating an info filter doesn't panic
        let filter = EnvFilter::new("info");
        // EnvFilter exists and can be used
        let _ = format!("{:?}", filter);
    }

    #[test]
    fn test_env_filter_from_env_fallback() {
        // Should not panic and should return a valid filter
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let _ = format!("{:?}", filter);
    }

    #[test]
    fn test_log_file_path() {
        let path = PathBuf::from("/tmp/test.log");
        assert!(path.to_string_lossy().contains("test.log"));
    }
}
