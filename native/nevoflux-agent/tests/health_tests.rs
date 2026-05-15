//! Health monitoring integration tests.
//!
//! These tests verify the health monitoring capabilities of the daemon,
//! including tracking metrics, shutdown handling, and serialization.

use nevoflux_daemon::{HealthMonitor, HealthStatus};
use std::sync::Arc;
use std::thread;

#[test]
fn test_health_monitor_lifecycle() {
    let monitor = HealthMonitor::new();

    // Initially healthy with no requests
    let status = monitor.status(0);
    assert!(status.healthy);
    assert_eq!(status.total_requests, 0);
    assert!(status.last_request_ago_secs.is_none());

    // Record some requests
    monitor.record_request();
    monitor.record_request();

    let status = monitor.status(5);
    assert_eq!(status.total_requests, 2);
    assert_eq!(status.active_sessions, 5);
    // Note: last_request_ago_secs may be None if the request happens within
    // the first second (the implementation checks if last_req > 0)

    // Request shutdown
    monitor.request_shutdown();
    let status = monitor.status(0);
    assert!(!status.healthy);
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
    assert!(json.contains("\"healthy\":true"));
    assert!(json.contains("\"uptime_secs\":3600"));
    assert!(json.contains("\"active_sessions\":10"));
    assert!(json.contains("\"total_requests\":1000"));
    assert!(json.contains("\"last_request_ago_secs\":5"));
}

#[test]
fn test_health_status_deserialization() {
    let json = r#"{
        "healthy": true,
        "uptime_secs": 3600,
        "active_sessions": 10,
        "total_requests": 1000,
        "last_request_ago_secs": 5
    }"#;

    let status: HealthStatus = serde_json::from_str(json).unwrap();

    assert!(status.healthy);
    assert_eq!(status.uptime_secs, 3600);
    assert_eq!(status.active_sessions, 10);
    assert_eq!(status.total_requests, 1000);
    assert_eq!(status.last_request_ago_secs, Some(5));
}

#[test]
fn test_health_status_with_no_last_request() {
    let status = HealthStatus {
        healthy: true,
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
fn test_health_monitor_default() {
    let monitor = HealthMonitor::default();

    assert_eq!(monitor.request_count(), 0);
    assert!(!monitor.is_shutdown_requested());
}

#[test]
fn test_health_monitor_request_counting() {
    let monitor = HealthMonitor::new();

    assert_eq!(monitor.request_count(), 0);

    monitor.record_request();
    assert_eq!(monitor.request_count(), 1);

    monitor.record_request();
    monitor.record_request();
    assert_eq!(monitor.request_count(), 3);
}

#[test]
fn test_health_monitor_uptime() {
    let monitor = HealthMonitor::new();

    let uptime = monitor.uptime();
    // Uptime should be very small (less than 1 second)
    assert!(uptime.as_secs() < 1);
}

#[test]
fn test_health_monitor_shutdown_request() {
    let monitor = HealthMonitor::new();

    // Initially not shutdown
    assert!(!monitor.is_shutdown_requested());

    let status = monitor.status(0);
    assert!(status.healthy);

    // Request shutdown
    monitor.request_shutdown();

    // Should now be in shutdown state
    assert!(monitor.is_shutdown_requested());

    let status = monitor.status(0);
    assert!(!status.healthy);
}

#[test]
fn test_health_monitor_thread_safety() {
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

#[test]
fn test_health_status_equality() {
    let status1 = HealthStatus {
        healthy: true,
        uptime_secs: 100,
        active_sessions: 5,
        total_requests: 50,
        last_request_ago_secs: Some(10),
    };

    let status2 = HealthStatus {
        healthy: true,
        uptime_secs: 100,
        active_sessions: 5,
        total_requests: 50,
        last_request_ago_secs: Some(10),
    };

    let status3 = HealthStatus {
        healthy: false,
        uptime_secs: 100,
        active_sessions: 5,
        total_requests: 50,
        last_request_ago_secs: Some(10),
    };

    assert_eq!(status1, status2);
    assert_ne!(status1, status3);
}

#[test]
fn test_health_status_clone() {
    let status = HealthStatus {
        healthy: true,
        uptime_secs: 3600,
        active_sessions: 10,
        total_requests: 1000,
        last_request_ago_secs: Some(5),
    };

    let cloned = status.clone();

    assert_eq!(status, cloned);
}

#[test]
fn test_health_monitor_session_count_in_status() {
    let monitor = HealthMonitor::new();

    // Check different session counts are reflected
    let status0 = monitor.status(0);
    let status10 = monitor.status(10);
    let status100 = monitor.status(100);

    assert_eq!(status0.active_sessions, 0);
    assert_eq!(status10.active_sessions, 10);
    assert_eq!(status100.active_sessions, 100);
}

#[test]
fn test_health_monitor_uptime_in_status() {
    let monitor = HealthMonitor::new();

    // Get immediate status
    let status = monitor.status(0);

    // Uptime should be very small but non-negative
    assert!(status.uptime_secs < 10);
}

#[test]
fn test_health_status_debug_format() {
    let status = HealthStatus {
        healthy: true,
        uptime_secs: 3600,
        active_sessions: 10,
        total_requests: 1000,
        last_request_ago_secs: Some(5),
    };

    let debug_str = format!("{:?}", status);

    assert!(debug_str.contains("healthy: true"));
    assert!(debug_str.contains("uptime_secs: 3600"));
}
