//! Integration tests for retry functionality.
//!
//! These tests verify the retry infrastructure from the daemon crate,
//! including RetryConfig, the Retryable trait, and the with_retry function.

use nevoflux_daemon::{with_retry, RetryConfig, Retryable};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// Test Error Type
// ============================================================================

/// A test error type that implements Retryable.
#[derive(Debug, Clone)]
struct TestError {
    message: String,
    retryable: bool,
}

impl TestError {
    fn retryable(message: &str) -> Self {
        Self {
            message: message.to_string(),
            retryable: true,
        }
    }

    fn permanent(message: &str) -> Self {
        Self {
            message: message.to_string(),
            retryable: false,
        }
    }
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (retryable: {})", self.message, self.retryable)
    }
}

impl std::error::Error for TestError {}

impl Retryable for TestError {
    fn is_retryable(&self) -> bool {
        self.retryable
    }
}

// ============================================================================
// RetryConfig Tests
// ============================================================================

#[test]
fn test_retry_config_defaults() {
    let config = RetryConfig::default();

    assert_eq!(config.max_retries, 3);
    assert_eq!(config.initial_delay, Duration::from_millis(100));
    assert_eq!(config.max_delay, Duration::from_secs(10));
    assert!((config.multiplier - 2.0).abs() < f64::EPSILON);
    assert!(config.jitter);
}

#[test]
fn test_retry_config_new() {
    let config = RetryConfig::new();
    let default = RetryConfig::default();

    assert_eq!(config.max_retries, default.max_retries);
    assert_eq!(config.initial_delay, default.initial_delay);
    assert_eq!(config.max_delay, default.max_delay);
    assert!((config.multiplier - default.multiplier).abs() < f64::EPSILON);
    assert_eq!(config.jitter, default.jitter);
}

#[test]
fn test_retry_config_builder_pattern() {
    let config = RetryConfig::new()
        .with_max_retries(5)
        .with_initial_delay(Duration::from_millis(50))
        .with_max_delay(Duration::from_secs(30))
        .with_multiplier(3.0)
        .with_jitter(false);

    assert_eq!(config.max_retries, 5);
    assert_eq!(config.initial_delay, Duration::from_millis(50));
    assert_eq!(config.max_delay, Duration::from_secs(30));
    assert!((config.multiplier - 3.0).abs() < f64::EPSILON);
    assert!(!config.jitter);
}

#[test]
fn test_delay_calculation_exponential_backoff() {
    let config = RetryConfig::new()
        .with_initial_delay(Duration::from_millis(100))
        .with_multiplier(2.0)
        .with_jitter(false);

    // Verify exponential growth: 100 * 2^n
    assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100)); // 100 * 2^0 = 100
    assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200)); // 100 * 2^1 = 200
    assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400)); // 100 * 2^2 = 400
    assert_eq!(config.delay_for_attempt(3), Duration::from_millis(800)); // 100 * 2^3 = 800
    assert_eq!(config.delay_for_attempt(4), Duration::from_millis(1600)); // 100 * 2^4 = 1600
}

#[test]
fn test_delay_caps_at_max_delay() {
    let config = RetryConfig::new()
        .with_initial_delay(Duration::from_millis(100))
        .with_max_delay(Duration::from_millis(500))
        .with_multiplier(2.0)
        .with_jitter(false);

    // Delay should be capped at max_delay
    assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
    assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));
    assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));
    assert_eq!(config.delay_for_attempt(3), Duration::from_millis(500)); // Capped at 500, not 800
    assert_eq!(config.delay_for_attempt(10), Duration::from_millis(500)); // Still capped
}

#[test]
fn test_delay_with_jitter_is_bounded() {
    let config = RetryConfig::new()
        .with_initial_delay(Duration::from_millis(100))
        .with_multiplier(1.0) // Keep base constant for testing
        .with_jitter(true);

    // Run multiple times to verify jitter is within bounds
    for _ in 0..100 {
        let delay = config.delay_for_attempt(0);
        // With jitter factor 0.5-1.5, delay should be 50-150ms
        assert!(
            delay >= Duration::from_millis(50),
            "Delay {:?} is below minimum",
            delay
        );
        assert!(
            delay <= Duration::from_millis(150),
            "Delay {:?} is above maximum",
            delay
        );
    }
}

#[test]
fn test_delay_with_different_multipliers() {
    // Test with multiplier 1.5
    let config = RetryConfig::new()
        .with_initial_delay(Duration::from_millis(100))
        .with_multiplier(1.5)
        .with_jitter(false);

    assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100)); // 100 * 1.5^0 = 100
    assert_eq!(config.delay_for_attempt(1), Duration::from_millis(150)); // 100 * 1.5^1 = 150
    assert_eq!(config.delay_for_attempt(2), Duration::from_millis(225)); // 100 * 1.5^2 = 225

    // Test with multiplier 3.0
    let config = RetryConfig::new()
        .with_initial_delay(Duration::from_millis(100))
        .with_multiplier(3.0)
        .with_jitter(false);

    assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100)); // 100 * 3^0 = 100
    assert_eq!(config.delay_for_attempt(1), Duration::from_millis(300)); // 100 * 3^1 = 300
    assert_eq!(config.delay_for_attempt(2), Duration::from_millis(900)); // 100 * 3^2 = 900
}

// ============================================================================
// with_retry Tests - Success Scenarios
// ============================================================================

#[tokio::test]
async fn test_retry_succeeds_on_first_attempt() {
    let config = RetryConfig::new().with_jitter(false);
    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    let result = with_retry(&config, || {
        let count = count.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            Ok::<_, TestError>(42)
        }
    })
    .await;

    assert_eq!(result.unwrap(), 42);
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_retry_succeeds_after_failures() {
    let config = RetryConfig::new()
        .with_max_retries(5)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    // Fail twice, then succeed
    let result = with_retry(&config, || {
        let count = count.clone();
        async move {
            let current = count.fetch_add(1, Ordering::SeqCst);
            if current < 2 {
                Err(TestError::retryable("transient failure"))
            } else {
                Ok("success")
            }
        }
    })
    .await;

    assert_eq!(result.unwrap(), "success");
    // Initial attempt (fail) + 1st retry (fail) + 2nd retry (success) = 3 calls
    assert_eq!(call_count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_retry_succeeds_on_last_attempt() {
    let config = RetryConfig::new()
        .with_max_retries(3)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    // Fail 3 times, succeed on 4th (last allowed)
    let result = with_retry(&config, || {
        let count = count.clone();
        async move {
            let current = count.fetch_add(1, Ordering::SeqCst);
            if current < 3 {
                Err(TestError::retryable("still failing"))
            } else {
                Ok("finally!")
            }
        }
    })
    .await;

    assert_eq!(result.unwrap(), "finally!");
    // Initial + 3 retries = 4 calls
    assert_eq!(call_count.load(Ordering::SeqCst), 4);
}

// ============================================================================
// with_retry Tests - Failure Scenarios
// ============================================================================

#[tokio::test]
async fn test_retry_fails_on_non_retryable_error() {
    let config = RetryConfig::new()
        .with_max_retries(5)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    let result = with_retry(&config, || {
        let count = count.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            Err::<i32, _>(TestError::permanent("permanent failure"))
        }
    })
    .await;

    // Should fail immediately without retrying
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.message, "permanent failure");
    assert!(!err.is_retryable());

    // Only called once - no retries for non-retryable errors
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_retry_exhausts_attempts() {
    let config = RetryConfig::new()
        .with_max_retries(3)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    let result = with_retry(&config, || {
        let count = count.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            Err::<i32, _>(TestError::retryable("keeps failing"))
        }
    })
    .await;

    // Should fail after exhausting all retries
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.message, "keeps failing");
    assert!(err.is_retryable()); // Error is retryable, but we ran out of attempts

    // Initial attempt + 3 retries = 4 calls
    assert_eq!(call_count.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn test_retry_zero_retries_means_no_retry() {
    let config = RetryConfig::new()
        .with_max_retries(0)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    let result = with_retry(&config, || {
        let count = count.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            Err::<i32, _>(TestError::retryable("failing"))
        }
    })
    .await;

    assert!(result.is_err());
    // Only the initial attempt, no retries when max_retries is 0
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

// ============================================================================
// with_retry Tests - Timing and Behavior
// ============================================================================

#[tokio::test]
async fn test_retry_respects_delays() {
    let config = RetryConfig::new()
        .with_max_retries(2)
        .with_initial_delay(Duration::from_millis(50))
        .with_multiplier(2.0)
        .with_jitter(false);

    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    let start = Instant::now();

    let _result = with_retry(&config, || {
        let count = count.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            Err::<i32, _>(TestError::retryable("failing"))
        }
    })
    .await;

    let elapsed = start.elapsed();

    // Should have waited at least 50ms + 100ms = 150ms
    // (first delay is 50ms * 2^0 = 50ms, second delay is 50ms * 2^1 = 100ms)
    assert!(
        elapsed >= Duration::from_millis(140), // Allow small timing variance
        "Expected at least 140ms delay, got {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_retry_stops_immediately_on_non_retryable_mid_sequence() {
    let config = RetryConfig::new()
        .with_max_retries(5)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    // Fail with retryable error twice, then fail with non-retryable
    let result: Result<i32, TestError> = with_retry(&config, || {
        let count = count.clone();
        async move {
            let current = count.fetch_add(1, Ordering::SeqCst);
            if current < 2 {
                Err(TestError::retryable("transient"))
            } else {
                Err(TestError::permanent("fatal error"))
            }
        }
    })
    .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.message, "fatal error");
    assert!(!err.is_retryable());

    // Should stop as soon as non-retryable error is encountered
    assert_eq!(call_count.load(Ordering::SeqCst), 3);
}

// ============================================================================
// with_retry Tests - Complex Scenarios
// ============================================================================

#[tokio::test]
async fn test_retry_with_complex_operation() {
    let config = RetryConfig::new()
        .with_max_retries(3)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    // Simulate a complex operation that returns a complex result
    #[derive(Debug, Clone, PartialEq)]
    struct ComplexResult {
        value: i32,
        message: String,
    }

    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    let result = with_retry(&config, || {
        let count = count.clone();
        async move {
            let current = count.fetch_add(1, Ordering::SeqCst);
            if current < 1 {
                Err(TestError::retryable("network error"))
            } else {
                Ok(ComplexResult {
                    value: 42,
                    message: "success".to_string(),
                })
            }
        }
    })
    .await;

    let result = result.unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(result.message, "success");
    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_retry_preserves_error_context() {
    let config = RetryConfig::new()
        .with_max_retries(2)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    // Each call returns a different error message
    let result: Result<i32, TestError> = with_retry(&config, || {
        let count = count.clone();
        async move {
            let current = count.fetch_add(1, Ordering::SeqCst);
            Err(TestError::retryable(&format!("error #{}", current)))
        }
    })
    .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    // The last error should be preserved
    assert_eq!(err.message, "error #2"); // 0-indexed: attempts 0, 1, 2
}

// ============================================================================
// Retryable Trait Tests
// ============================================================================

#[test]
fn test_retryable_trait_implementation() {
    let retryable_err = TestError::retryable("can retry");
    assert!(retryable_err.is_retryable());

    let permanent_err = TestError::permanent("cannot retry");
    assert!(!permanent_err.is_retryable());
}

#[test]
fn test_error_display() {
    let err = TestError::retryable("connection timeout");
    let display = format!("{}", err);
    assert!(display.contains("connection timeout"));
    assert!(display.contains("retryable: true"));

    let err = TestError::permanent("invalid credentials");
    let display = format!("{}", err);
    assert!(display.contains("invalid credentials"));
    assert!(display.contains("retryable: false"));
}

#[test]
fn test_error_debug() {
    let err = TestError::retryable("test error");
    let debug = format!("{:?}", err);
    assert!(debug.contains("TestError"));
    assert!(debug.contains("test error"));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_retry_with_very_large_max_retries() {
    // Verify the system handles large retry counts gracefully
    let config = RetryConfig::new()
        .with_max_retries(1000)
        .with_initial_delay(Duration::from_millis(1))
        .with_jitter(false);

    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    // Succeed on the 5th attempt
    let result = with_retry(&config, || {
        let count = count.clone();
        async move {
            let current = count.fetch_add(1, Ordering::SeqCst);
            if current < 4 {
                Err(TestError::retryable("keep trying"))
            } else {
                Ok("done")
            }
        }
    })
    .await;

    assert_eq!(result.unwrap(), "done");
    assert_eq!(call_count.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn test_retry_with_very_short_initial_delay() {
    let config = RetryConfig::new()
        .with_max_retries(10)
        .with_initial_delay(Duration::from_nanos(1))
        .with_jitter(false);

    let call_count = Arc::new(AtomicU32::new(0));
    let count = call_count.clone();

    let result = with_retry(&config, || {
        let count = count.clone();
        async move {
            let current = count.fetch_add(1, Ordering::SeqCst);
            if current < 5 {
                Err(TestError::retryable("tiny delay"))
            } else {
                Ok(current)
            }
        }
    })
    .await;

    assert_eq!(result.unwrap(), 5);
    assert_eq!(call_count.load(Ordering::SeqCst), 6);
}
