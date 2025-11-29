//! Thread-safe circuit breaker using three-state FSM
//!
//! This implements the three-state circuit breaker pattern (Closed → Open → HalfOpen)
//! with atomic operations for thread-safe access while using the typestate FSM internally.
//!
//! ## State Machine
//!
//! ```text
//!     CLOSED ──fail(N)──→ OPEN ──timeout──→ HALFOPEN
//!        ▲                                      │
//!        │               success(M)             │
//!        └──────────────────────────────────────┘
//!                             │
//!                            fail
//!                             ▼
//!                           OPEN
//! ```
//!
//! - **Closed**: Normal operation, requests allowed
//! - **Open**: Too many failures, requests rejected for `timeout` duration
//! - **HalfOpen**: Testing recovery, limited requests allowed
//!
//! ## Thread Safety
//!
//! Uses `parking_lot::Mutex` for interior mutability, allowing multiple threads
//! to safely record successes/failures and check state.

use crate::core::circuit_breaker_fsm::{
    ThreeStateBreakerState, ThreeStateClosed, ThreeStateHalfOrClosed, ThreeStateOpenOrHalf,
    ThreeStateResult,
};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Configuration for circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u64,
    /// Time window for counting failures (currently unused - immediate counting)
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

/// Thread-safe circuit breaker using typestate FSM
///
/// Wraps the three-state FSM with a Mutex for interior mutability.
#[derive(Clone)]
pub struct CircuitBreakerV2 {
    state: Arc<Mutex<ThreeStateBreakerState>>,
}

impl CircuitBreakerV2 {
    /// Create a new circuit breaker in Closed state
    pub fn new(config: CircuitBreakerConfig) -> Self {
        info!("Creating circuit breaker v2 with config: {:?}", config);
        let closed = ThreeStateClosed::new(
            config.failure_threshold,
            config.success_threshold,
            config.timeout,
        );
        Self {
            state: Arc::new(Mutex::new(ThreeStateBreakerState::Closed(closed))),
        }
    }

    /// Create with default configuration
    pub fn new_default() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    /// Check if operation is allowed to proceed
    ///
    /// Returns true if call is permitted, false otherwise.
    /// If in Open state and timeout expired, automatically transitions to HalfOpen.
    pub fn is_call_permitted(&self) -> bool {
        let mut state = self.state.lock();

        // Check current state
        if state.is_call_permitted() {
            // If Open and timeout expired, transition to HalfOpen
            if let ThreeStateBreakerState::Open(open) = std::mem::replace(
                &mut *state,
                ThreeStateBreakerState::Closed(ThreeStateClosed::new_default()),
            ) {
                match open.check_timeout() {
                    ThreeStateOpenOrHalf::HalfOpen(half_open) => {
                        debug!("Circuit breaker: OPEN → HALFOPEN (timeout expired)");
                        *state = ThreeStateBreakerState::HalfOpen(half_open);
                        true
                    }
                    ThreeStateOpenOrHalf::Open(still_open) => {
                        *state = ThreeStateBreakerState::Open(still_open);
                        false
                    }
                }
            } else {
                true // Closed or HalfOpen
            }
        } else {
            false
        }
    }

    /// Record a successful operation
    pub fn record_success(&self) {
        let mut state = self.state.lock();

        match std::mem::replace(
            &mut *state,
            ThreeStateBreakerState::Closed(ThreeStateClosed::new_default()),
        ) {
            ThreeStateBreakerState::Closed(closed) => {
                *state = ThreeStateBreakerState::Closed(closed.record_success());
                debug!("Circuit breaker: Success recorded in Closed state");
            }
            ThreeStateBreakerState::HalfOpen(half_open) => match half_open.record_success() {
                ThreeStateHalfOrClosed::Closed(closed) => {
                    info!("Circuit breaker: HALFOPEN → CLOSED (recovery successful)");
                    *state = ThreeStateBreakerState::Closed(closed);
                }
                ThreeStateHalfOrClosed::HalfOpen(still_half) => {
                    debug!(
                        "Circuit breaker: Success recorded in HalfOpen state ({}/{})",
                        still_half.data().success_count,
                        still_half.data().success_threshold
                    );
                    *state = ThreeStateBreakerState::HalfOpen(still_half);
                }
            },
            ThreeStateBreakerState::Open(open) => {
                // Restore Open state (successes ignored)
                *state = ThreeStateBreakerState::Open(open);
            }
        }
    }

    /// Record a failed operation
    pub fn record_failure(&self) {
        let mut state = self.state.lock();

        match std::mem::replace(
            &mut *state,
            ThreeStateBreakerState::Closed(ThreeStateClosed::new_default()),
        ) {
            ThreeStateBreakerState::Closed(closed) => match closed.record_failure() {
                ThreeStateResult::Open(open) => {
                    warn!("Circuit breaker: CLOSED → OPEN (failure threshold exceeded)");
                    *state = ThreeStateBreakerState::Open(open);
                }
                ThreeStateResult::Closed(still_closed) => {
                    debug!(
                        "Circuit breaker: Failure recorded in Closed state ({}/{})",
                        still_closed.data().failure_count,
                        still_closed.data().failure_threshold
                    );
                    *state = ThreeStateBreakerState::Closed(still_closed);
                }
            },
            ThreeStateBreakerState::HalfOpen(half_open) => {
                warn!("Circuit breaker: HALFOPEN → OPEN (failure during recovery)");
                *state = ThreeStateBreakerState::Open(half_open.record_failure());
            }
            ThreeStateBreakerState::Open(open) => {
                // Restore Open state (failures ignored)
                *state = ThreeStateBreakerState::Open(open);
            }
        }
    }

    /// Get current state name
    pub fn state_name(&self) -> String {
        self.state.lock().state_name().to_string()
    }

    /// Get current state (for compatibility)
    pub fn state(&self) -> CircuitState {
        match &*self.state.lock() {
            ThreeStateBreakerState::Closed(_) => CircuitState::Closed,
            ThreeStateBreakerState::Open(_) => CircuitState::Open,
            ThreeStateBreakerState::HalfOpen(_) => CircuitState::HalfOpen,
        }
    }

    /// Reset circuit breaker to Closed state (manual override)
    pub fn reset(&self) {
        let mut state = self.state.lock();
        info!("Circuit breaker manually reset to CLOSED");
        *state = ThreeStateBreakerState::Closed(ThreeStateClosed::new_default());
    }

    /// Force circuit breaker to Open state (manual override)
    pub fn force_open(&self) {
        warn!("Circuit breaker manually forced to OPEN");
        // Create a temporary closed state just to transition it to open
        let closed = ThreeStateClosed::new_default();
        match closed.record_failure() {
            ThreeStateResult::Open(open) => {
                *self.state.lock() = ThreeStateBreakerState::Open(open);
            }
            ThreeStateResult::Closed(_) => {
                // Shouldn't happen with threshold of 0, but be safe
            }
        }
    }
}

// Re-export CircuitState for compatibility
pub use crate::core::circuit_breaker_fsm::{
    ThreeStateClosed as CircuitClosed, ThreeStateHalfOpen as CircuitHalfOpen,
    ThreeStateOpen as CircuitOpen,
};

/// Circuit state enum (for compatibility with old API)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CircuitState {
    Closed = 0,
    Open = 1,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreakerV2::new_default();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.is_call_permitted());
    }

    #[test]
    fn test_circuit_breaker_opens_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreakerV2::new(config);

        // Record 2 failures (should stay closed)
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        // Record 3rd failure (should open)
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.is_call_permitted());
    }

    #[test]
    fn test_circuit_breaker_half_open_transition() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_millis(10),
            ..Default::default()
        };
        let cb = CircuitBreakerV2::new(config);

        // Open the circuit
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        // Wait for timeout
        thread::sleep(Duration::from_millis(15));

        // Next check should transition to HalfOpen
        assert!(cb.is_call_permitted());
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_millis(10),
            ..Default::default()
        };
        let cb = CircuitBreakerV2::new(config);

        // Open the circuit
        cb.record_failure();
        thread::sleep(Duration::from_millis(15));
        cb.is_call_permitted(); // Transition to HalfOpen

        // Record 2 successes (should close)
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_failure() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_millis(10),
            ..Default::default()
        };
        let cb = CircuitBreakerV2::new(config);

        // Open the circuit
        cb.record_failure();
        thread::sleep(Duration::from_millis(15));
        cb.is_call_permitted(); // Transition to HalfOpen

        // Failure in HalfOpen should reopen
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_manual_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let cb = CircuitBreakerV2::new(config);

        // Open the circuit
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        // Manual reset
        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.is_call_permitted());
    }

    #[test]
    fn test_concurrent_access() {
        let cb = CircuitBreakerV2::new_default();
        let cb_clone1 = cb.clone();
        let cb_clone2 = cb.clone();

        // Spawn threads that record failures
        let handle1 = std::thread::spawn(move || {
            for _ in 0..3 {
                cb_clone1.record_failure();
            }
        });

        let handle2 = std::thread::spawn(move || {
            for _ in 0..3 {
                cb_clone2.record_failure();
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();

        // Should be open after multiple failures
        assert_eq!(cb.state(), CircuitState::Open);
    }
}
