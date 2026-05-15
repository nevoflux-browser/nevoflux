//! Health monitoring for daemon.
//!
//! This module provides health monitoring capabilities for the daemon,
//! tracking metrics like uptime, request counts, and session activity.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Health status of the daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthStatus {
    /// Whether the daemon is healthy.
    pub healthy: bool,
    /// Uptime in seconds.
    pub uptime_secs: u64,
    /// Active session count.
    pub active_sessions: usize,
    /// Total requests processed.
    pub total_requests: u64,
    /// Seconds since last request.
    pub last_request_ago_secs: Option<u64>,
}

/// Health monitor tracks daemon metrics.
///
/// This struct is thread-safe and can be shared across tasks using `Arc`.
///
/// # Example
///
/// ```
/// use nevoflux_daemon::health::HealthMonitor;
///
/// let monitor = HealthMonitor::new();
///
/// // Record incoming requests
/// monitor.record_request();
/// monitor.record_request();
///
/// // Get current health status
/// let status = monitor.status(5); // 5 active sessions
/// assert!(status.healthy);
/// assert_eq!(status.total_requests, 2);
/// assert_eq!(status.active_sessions, 5);
/// ```
#[derive(Debug)]
pub struct HealthMonitor {
    start_time: Instant,
    request_count: AtomicU64,
    last_request_time: AtomicU64,
    shutdown_requested: AtomicBool,
}

impl HealthMonitor {
    /// Create a new health monitor.
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            request_count: AtomicU64::new(0),
            last_request_time: AtomicU64::new(0),
            shutdown_requested: AtomicBool::new(false),
        }
    }

    /// Record a request.
    ///
    /// This increments the request counter and updates the last request time.
    pub fn record_request(&self) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        self.last_request_time
            .store(self.start_time.elapsed().as_secs(), Ordering::Relaxed);
    }

    /// Get health status.
    ///
    /// # Arguments
    ///
    /// * `session_count` - The current number of active sessions.
    ///
    /// # Returns
    ///
    /// A `HealthStatus` struct containing the current health metrics.
    pub fn status(&self, session_count: usize) -> HealthStatus {
        let uptime = self.start_time.elapsed().as_secs();
        let last_req = self.last_request_time.load(Ordering::Relaxed);

        HealthStatus {
            healthy: !self.is_shutdown_requested(),
            uptime_secs: uptime,
            active_sessions: session_count,
            total_requests: self.request_count.load(Ordering::Relaxed),
            last_request_ago_secs: if last_req > 0 {
                Some(uptime.saturating_sub(last_req))
            } else {
                None
            },
        }
    }

    /// Request shutdown.
    ///
    /// This marks the daemon as unhealthy and signals that shutdown has been requested.
    pub fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Relaxed);
    }

    /// Check if shutdown was requested.
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::Relaxed)
    }

    /// Get uptime.
    ///
    /// # Returns
    ///
    /// The duration since the monitor was created.
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get total request count.
    pub fn request_count(&self) -> u64 {
        self.request_count.load(Ordering::Relaxed)
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_monitor_new() {
        let monitor = HealthMonitor::new();
        assert_eq!(monitor.request_count(), 0);
        assert!(!monitor.is_shutdown_requested());
    }

    #[test]
    fn test_health_monitor_default() {
        let monitor = HealthMonitor::default();
        assert_eq!(monitor.request_count(), 0);
        assert!(!monitor.is_shutdown_requested());
    }

    #[test]
    fn test_health_monitor_record_request() {
        let monitor = HealthMonitor::new();

        monitor.record_request();
        assert_eq!(monitor.request_count(), 1);

        monitor.record_request();
        monitor.record_request();
        assert_eq!(monitor.request_count(), 3);
    }

    #[test]
    fn test_health_monitor_status() {
        let monitor = HealthMonitor::new();
        monitor.record_request();

        let status = monitor.status(5);

        assert!(status.healthy);
        assert_eq!(status.active_sessions, 5);
        assert_eq!(status.total_requests, 1);
        // Uptime should be very small but >= 0
        assert!(status.uptime_secs < 10);
    }

    #[test]
    fn test_health_monitor_status_no_requests() {
        let monitor = HealthMonitor::new();

        let status = monitor.status(0);

        assert!(status.healthy);
        assert_eq!(status.active_sessions, 0);
        assert_eq!(status.total_requests, 0);
        assert!(status.last_request_ago_secs.is_none());
    }

    #[test]
    fn test_health_monitor_shutdown() {
        let monitor = HealthMonitor::new();

        assert!(!monitor.is_shutdown_requested());

        let status = monitor.status(0);
        assert!(status.healthy);

        monitor.request_shutdown();

        assert!(monitor.is_shutdown_requested());

        let status = monitor.status(0);
        assert!(!status.healthy);
    }

    #[test]
    fn test_health_monitor_uptime() {
        let monitor = HealthMonitor::new();

        let uptime = monitor.uptime();
        // Uptime should be very small (less than 1 second)
        assert!(uptime.as_secs() < 1);
    }

    #[test]
    fn test_health_status_serialization() {
        let status = HealthStatus {
            healthy: true,
            uptime_secs: 3600,
            active_sessions: 10,
            total_requests: 1000,
            last_request_ago_secs: Some(5),
        };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: HealthStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_health_status_serialization_no_last_request() {
        let status = HealthStatus {
            healthy: false,
            uptime_secs: 0,
            active_sessions: 0,
            total_requests: 0,
            last_request_ago_secs: None,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"last_request_ago_secs\":null"));

        let deserialized: HealthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_health_monitor_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let monitor = Arc::new(HealthMonitor::new());
        let mut handles = vec![];

        // Spawn multiple threads to record requests concurrently
        for _ in 0..10 {
            let monitor_clone = Arc::clone(&monitor);
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    monitor_clone.record_request();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // All requests should be counted
        assert_eq!(monitor.request_count(), 1000);
    }
}
