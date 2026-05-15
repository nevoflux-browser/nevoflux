//! Circuit breaker for context compression.
//!
//! Prevents infinite retries when the summarization LLM is unavailable.
//! Opens after `max_failures` consecutive failures, transitions to half-open
//! after `cooldown` elapses, and resets on success.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// State of the compression circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation, compression allowed.
    Closed,
    /// Too many consecutive failures, compression blocked.
    Open,
    /// Cooldown elapsed, one probe attempt allowed.
    HalfOpen,
}

/// Circuit breaker that protects the context compression LLM call.
pub struct CompressionCircuitBreaker {
    consecutive_failures: AtomicU32,
    max_failures: u32,
    cooldown: Duration,
    last_open_at: Mutex<Option<Instant>>,
}

impl CompressionCircuitBreaker {
    /// Create a new circuit breaker.
    pub fn new(max_failures: u32, cooldown: Duration) -> Self {
        Self {
            consecutive_failures: AtomicU32::new(0),
            max_failures,
            cooldown,
            last_open_at: Mutex::new(None),
        }
    }

    /// Query the current state.
    pub fn state(&self) -> CircuitState {
        let failures = self.consecutive_failures.load(Ordering::Relaxed);
        if failures < self.max_failures {
            return CircuitState::Closed;
        }
        // failures >= max_failures → check cooldown
        let guard = self.last_open_at.lock().unwrap();
        match *guard {
            Some(opened) if opened.elapsed() >= self.cooldown => CircuitState::HalfOpen,
            _ => CircuitState::Open,
        }
    }

    /// Record a successful compression. Resets the breaker to Closed.
    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        *self.last_open_at.lock().unwrap() = None;
    }

    /// Record a failed compression. May transition to Open.
    pub fn record_failure(&self) {
        let prev = self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        if prev + 1 >= self.max_failures {
            *self.last_open_at.lock().unwrap() = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_closed_by_default() {
        let cb = CompressionCircuitBreaker::new(3, Duration::from_secs(300));
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_stays_closed_under_max() {
        let cb = CompressionCircuitBreaker::new(3, Duration::from_secs(300));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens_after_max_failures() {
        let cb = CompressionCircuitBreaker::new(3, Duration::from_secs(300));
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_resets_on_success() {
        let cb = CompressionCircuitBreaker::new(3, Duration::from_secs(300));
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
        // Verify counter is actually zero — needs 3 more failures to open
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_after_cooldown() {
        let cb = CompressionCircuitBreaker::new(3, Duration::from_millis(50));
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        // Wait for cooldown
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_reopen_on_half_open_failure() {
        let cb = CompressionCircuitBreaker::new(3, Duration::from_millis(50));
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();

        // Wait for cooldown → HalfOpen
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // Fail again → should re-open and reset timer
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        // Should NOT be half-open immediately (timer was reset)
        assert_eq!(cb.state(), CircuitState::Open);
    }
}
