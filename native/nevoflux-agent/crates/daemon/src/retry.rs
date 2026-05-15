//! Retry infrastructure with exponential backoff
//!
//! This module provides retry logic for transient failures with configurable
//! exponential backoff and optional jitter.
//!
//! # Example
//!
//! ```rust,ignore
//! use nevoflux_daemon::retry::{RetryConfig, with_retry};
//!
//! let config = RetryConfig::new()
//!     .with_max_retries(3)
//!     .with_initial_delay(Duration::from_millis(100));
//!
//! let result = with_retry(&config, || async {
//!     // Operation that might fail transiently
//!     perform_network_call().await
//! }).await;
//! ```

use std::future::Future;
use std::time::Duration;

use rand::Rng;

/// Configuration for retry behavior with exponential backoff.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not including the initial attempt).
    pub max_retries: u32,
    /// Initial delay before the first retry.
    pub initial_delay: Duration,
    /// Maximum delay between retries (caps the exponential growth).
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (delay = initial_delay * multiplier^attempt).
    pub multiplier: f64,
    /// Whether to add random jitter to delays to prevent thundering herd.
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Creates a new RetryConfig with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum number of retry attempts.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Sets the initial delay before the first retry.
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Sets the maximum delay between retries.
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Sets the multiplier for exponential backoff.
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    /// Enables or disables jitter.
    pub fn with_jitter(mut self, jitter: bool) -> Self {
        self.jitter = jitter;
        self
    }

    /// Calculates the delay for a given attempt number (0-indexed).
    ///
    /// The delay follows exponential backoff: `initial_delay * multiplier^attempt`
    /// capped at `max_delay`. If jitter is enabled, a random factor between
    /// 0.5 and 1.5 is applied.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        // Calculate base delay with exponential backoff
        let base_delay_ms =
            self.initial_delay.as_millis() as f64 * self.multiplier.powi(attempt as i32);

        // Cap at max_delay
        let capped_delay_ms = base_delay_ms.min(self.max_delay.as_millis() as f64);

        // Apply jitter if enabled
        let final_delay_ms = if self.jitter {
            let mut rng = rand::thread_rng();
            let jitter_factor = rng.gen_range(0.5..1.5);
            capped_delay_ms * jitter_factor
        } else {
            capped_delay_ms
        };

        Duration::from_millis(final_delay_ms as u64)
    }
}

/// Trait for errors that can be retried.
///
/// Implement this trait for your error types to indicate which errors
/// should trigger a retry attempt.
pub trait Retryable {
    /// Returns true if this error is transient and the operation should be retried.
    fn is_retryable(&self) -> bool;
}

/// Executes an async operation with retry logic.
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation` - An async closure that returns a Result
///
/// # Returns
///
/// The result of the operation, or the last error if all retries are exhausted.
///
/// # Example
///
/// ```rust,ignore
/// use nevoflux_daemon::retry::{RetryConfig, Retryable, with_retry};
///
/// #[derive(Debug)]
/// struct MyError { retryable: bool }
///
/// impl Retryable for MyError {
///     fn is_retryable(&self) -> bool { self.retryable }
/// }
///
/// let config = RetryConfig::new();
/// let result = with_retry(&config, || async {
///     // Your operation here
///     Ok::<_, MyError>(42)
/// }).await;
/// ```
pub async fn with_retry<F, Fut, T, E>(config: &RetryConfig, mut operation: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Retryable + std::fmt::Debug,
{
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                if !err.is_retryable() || attempt >= config.max_retries {
                    tracing::debug!(
                        attempt = attempt,
                        max_retries = config.max_retries,
                        retryable = err.is_retryable(),
                        "Operation failed, not retrying: {:?}",
                        err
                    );
                    return Err(err);
                }

                let delay = config.delay_for_attempt(attempt);
                tracing::debug!(
                    attempt = attempt,
                    delay_ms = delay.as_millis(),
                    "Retrying after transient error: {:?}",
                    err
                );

                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_config_default_values() {
        let config = RetryConfig::default();

        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(10));
        assert!((config.multiplier - 2.0).abs() < f64::EPSILON);
        assert!(config.jitter);
    }

    #[test]
    fn test_retry_config_new_returns_default() {
        let config = RetryConfig::new();
        let default = RetryConfig::default();

        assert_eq!(config.max_retries, default.max_retries);
        assert_eq!(config.initial_delay, default.initial_delay);
        assert_eq!(config.max_delay, default.max_delay);
    }

    #[test]
    fn test_retry_config_builder_methods() {
        let config = RetryConfig::new()
            .with_max_retries(5)
            .with_initial_delay(Duration::from_millis(200))
            .with_max_delay(Duration::from_secs(30))
            .with_multiplier(3.0)
            .with_jitter(false);

        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(200));
        assert_eq!(config.max_delay, Duration::from_secs(30));
        assert!((config.multiplier - 3.0).abs() < f64::EPSILON);
        assert!(!config.jitter);
    }

    #[test]
    fn test_delay_calculation_without_jitter() {
        let config = RetryConfig::new()
            .with_initial_delay(Duration::from_millis(100))
            .with_multiplier(2.0)
            .with_jitter(false);

        // Attempt 0: 100 * 2^0 = 100ms
        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));

        // Attempt 1: 100 * 2^1 = 200ms
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));

        // Attempt 2: 100 * 2^2 = 400ms
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));

        // Attempt 3: 100 * 2^3 = 800ms
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(800));
    }

    #[test]
    fn test_delay_caps_at_max_delay() {
        let config = RetryConfig::new()
            .with_initial_delay(Duration::from_millis(100))
            .with_max_delay(Duration::from_millis(500))
            .with_multiplier(2.0)
            .with_jitter(false);

        // Attempt 0: 100ms (within max)
        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));

        // Attempt 1: 200ms (within max)
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));

        // Attempt 2: 400ms (within max)
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));

        // Attempt 3: Would be 800ms, but capped at 500ms
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(500));

        // Attempt 10: Would be huge, still capped at 500ms
        assert_eq!(config.delay_for_attempt(10), Duration::from_millis(500));
    }

    #[test]
    fn test_delay_with_jitter_is_within_bounds() {
        let config = RetryConfig::new()
            .with_initial_delay(Duration::from_millis(100))
            .with_multiplier(2.0)
            .with_jitter(true);

        // Run multiple times to test jitter variation
        for _ in 0..100 {
            let delay = config.delay_for_attempt(0);
            // With jitter factor 0.5-1.5, delay should be 50-150ms
            assert!(delay >= Duration::from_millis(50));
            assert!(delay <= Duration::from_millis(150));
        }
    }

    // Test error type for testing
    #[derive(Debug, Clone)]
    struct TestError {
        retryable: bool,
        #[allow(dead_code)]
        message: String,
    }

    impl Retryable for TestError {
        fn is_retryable(&self) -> bool {
            self.retryable
        }
    }

    #[tokio::test]
    async fn test_with_retry_succeeds_on_first_attempt() {
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
    async fn test_with_retry_retries_on_retryable_error() {
        let config = RetryConfig::new()
            .with_max_retries(3)
            .with_initial_delay(Duration::from_millis(1))
            .with_jitter(false);

        let call_count = Arc::new(AtomicU32::new(0));
        let count = call_count.clone();

        let result = with_retry(&config, || {
            let count = count.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current < 2 {
                    Err(TestError {
                        retryable: true,
                        message: "transient error".to_string(),
                    })
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        // Initial attempt + 2 retries = 3 calls
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_does_not_retry_non_retryable_error() {
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
                Err::<i32, _>(TestError {
                    retryable: false,
                    message: "permanent error".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        // Should only be called once - no retries for non-retryable errors
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_exhausts_all_retries() {
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
                Err::<i32, _>(TestError {
                    retryable: true,
                    message: "always fails".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        // Initial attempt + 3 retries = 4 calls
        assert_eq!(call_count.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn test_with_retry_zero_retries_means_no_retry() {
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
                Err::<i32, _>(TestError {
                    retryable: true,
                    message: "fails".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        // Only the initial attempt, no retries
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }
}
