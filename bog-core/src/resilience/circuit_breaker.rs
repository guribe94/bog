//! Circuit breaker pattern for preventing cascade failures
//!
//! Monitors operation success/failure rates and automatically trips to prevent
//! overwhelming failing services. Implements the three-state circuit breaker:
//! Closed (normal) → Open (tripped) → HalfOpen (testing recovery)

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CircuitState {
    /// Normal operation, requests pass through
    Closed = 0,
    /// Circuit tripped, requests fail fast
    Open = 1,
    /// Testing if service recovered
    HalfOpen = 2,
}

impl From<u8> for CircuitState {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Closed,
            1 => Self::Open,
            2 => Self::HalfOpen,
            _ => Self::Closed,
        }
    }
}

/// Configuration for circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u64,
    /// Time window for counting failures
    pub failure_window: Duration,
    /// How long to wait in Open state before trying HalfOpen
    pub timeout: Duration,
    /// Number of successful requests in HalfOpen to close circuit
    pub success_threshold: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            failure_window: Duration::from_secs(60),
            timeout: Duration::from_secs(30),
            success_threshold: 2,
        }
    }
}

impl CircuitBreakerConfig {
    /// Aggressive configuration (for testing)
    pub fn aggressive() -> Self {
        Self {
            failure_threshold: 3,
            failure_window: Duration::from_secs(10),
            timeout: Duration::from_secs(5),
            success_threshold: 2,
        }
    }

    /// Conservative configuration (for production)
    pub fn conservative() -> Self {
        Self {
            failure_threshold: 10,
            failure_window: Duration::from_secs(120),
            timeout: Duration::from_secs(60),
            success_threshold: 5,
        }
    }
}

/// Circuit breaker implementation
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<AtomicU8>,
    failure_count: Arc<AtomicU64>,
    success_count: Arc<AtomicU64>,
    last_failure_time: Arc<parking_lot::Mutex<Option<Instant>>>,
    last_state_change: Arc<parking_lot::Mutex<Instant>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        info!("Creating circuit breaker with config: {:?}", config);
        Self {
            config,
            state: Arc::new(AtomicU8::new(CircuitState::Closed as u8)),
            failure_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            last_failure_time: Arc::new(parking_lot::Mutex::new(None)),
            last_state_change: Arc::new(parking_lot::Mutex::new(Instant::now())),
        }
    }

    /// Check if operation is allowed to proceed
    pub fn is_call_permitted(&self) -> bool {
        let state: CircuitState = self.state.load(Ordering::Acquire).into();

        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout expired, transition to HalfOpen
                let last_change = *self.last_state_change.lock();
                if last_change.elapsed() >= self.config.timeout {
                    self.transition_to_half_open();
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record successful operation
    pub fn record_success(&self) {
        let state: CircuitState = self.state.load(Ordering::Acquire).into();

        match state {
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Release);
            }
            CircuitState::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::AcqRel) + 1;
                if successes >= self.config.success_threshold {
                    self.transition_to_closed();
                }
            }
            CircuitState::Open => {
                // Ignore successes in Open state
            }
        }
    }

    /// Record failed operation
    pub fn record_failure(&self) {
        let state: CircuitState = self.state.load(Ordering::Acquire).into();

        // Update last failure time
        *self.last_failure_time.lock() = Some(Instant::now());

        match state {
            CircuitState::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::AcqRel) + 1;

                // Check if we should trip the circuit
                if failures >= self.config.failure_threshold {
                    // Verify failures are within time window
                    if let Some(last_failure) = *self.last_failure_time.lock() {
                        if last_failure.elapsed() <= self.config.failure_window {
                            self.transition_to_open();
                        }
                    }
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in HalfOpen immediately opens circuit
                self.transition_to_open();
            }
            CircuitState::Open => {
                // Ignore failures in Open state
            }
        }
    }

    /// Transition to Closed state
    fn transition_to_closed(&self) {
        info!("Circuit breaker transitioning to CLOSED");
        self.state.store(CircuitState::Closed as u8, Ordering::Release);
        self.failure_count.store(0, Ordering::Release);
        self.success_count.store(0, Ordering::Release);
        *self.last_state_change.lock() = Instant::now();
    }

    /// Transition to Open state
    fn transition_to_open(&self) {
        warn!("Circuit breaker TRIPPED - transitioning to OPEN");
        self.state.store(CircuitState::Open as u8, Ordering::Release);
        self.success_count.store(0, Ordering::Release);
        *self.last_state_change.lock() = Instant::now();
    }

    /// Transition to HalfOpen state
    fn transition_to_half_open(&self) {
        debug!("Circuit breaker transitioning to HALF-OPEN (testing recovery)");
        self.state.store(CircuitState::HalfOpen as u8, Ordering::Release);
        self.failure_count.store(0, Ordering::Release);
        self.success_count.store(0, Ordering::Release);
        *self.last_state_change.lock() = Instant::now();
    }

    /// Get current state
    pub fn state(&self) -> CircuitState {
        self.state.load(Ordering::Acquire).into()
    }

    /// Get failure count
    pub fn failure_count(&self) -> u64 {
        self.failure_count.load(Ordering::Acquire)
    }

    /// Get success count (in HalfOpen state)
    pub fn success_count(&self) -> u64 {
        self.success_count.load(Ordering::Acquire)
    }

    /// Reset circuit breaker to Closed state
    pub fn reset(&self) {
        info!("Circuit breaker manually reset to CLOSED");
        self.transition_to_closed();
    }

    /// Force circuit breaker to Open state
    pub fn force_open(&self) {
        warn!("Circuit breaker manually forced to OPEN");
        self.transition_to_open();
    }
}

impl Clone for CircuitBreaker {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            state: Arc::clone(&self.state),
            failure_count: Arc::clone(&self.failure_count),
            success_count: Arc::clone(&self.success_count),
            last_failure_time: Arc::clone(&self.last_failure_time),
            last_state_change: Arc::clone(&self.last_state_change),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.is_call_permitted());
    }

    #[test]
    fn test_circuit_breaker_opens_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Record failures
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.is_call_permitted());
    }

    #[test]
    fn test_circuit_breaker_half_open_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(10),
            success_threshold: 2,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Trip circuit
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        // Wait for timeout
        thread::sleep(Duration::from_millis(15));

        // Should transition to HalfOpen on next call
        assert!(cb.is_call_permitted());
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // Record successes to close circuit
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_failure_reopens() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(10),
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Trip circuit
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        // Wait for timeout
        thread::sleep(Duration::from_millis(15));
        assert!(cb.is_call_permitted()); // Transition to HalfOpen

        // Failure in HalfOpen immediately reopens
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_success_resets_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.failure_count(), 2);

        cb.record_success();
        assert_eq!(cb.failure_count(), 0);
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_manual_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Trip circuit
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        // Manual reset
        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.is_call_permitted());
    }

    #[test]
    fn test_circuit_breaker_force_open() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.force_open();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.is_call_permitted());
    }

    #[test]
    fn test_circuit_breaker_clone() {
        let cb1 = CircuitBreaker::new(CircuitBreakerConfig::default());
        let cb2 = cb1.clone();

        cb1.record_failure();
        assert_eq!(cb2.failure_count(), 1);
    }

    #[test]
    fn test_aggressive_config() {
        let config = CircuitBreakerConfig::aggressive();
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.timeout, Duration::from_secs(5));
    }

    #[test]
    fn test_conservative_config() {
        let config = CircuitBreakerConfig::conservative();
        assert_eq!(config.failure_threshold, 10);
        assert_eq!(config.timeout, Duration::from_secs(60));
    }
}
