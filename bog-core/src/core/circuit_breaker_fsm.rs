//! Circuit Breaker State Machines - Typestate Pattern
//!
//! This module implements compile-time verified state machines for circuit breakers.
//! Two distinct patterns are provided:
//!
//! 1. **Binary Circuit Breaker**: Simple Normal ↔ Halted (for risk management)
//! 2. **Three-State Circuit Breaker**: Closed → Open → HalfOpen (for resilience)
//!
//! # Binary Circuit Breaker (Risk Management)
//!
//! ```text
//!     NORMAL ←─────────→ HALTED
//!        │      trip()      │
//!        │    ←──────────   │
//!        │      reset()     │
//!        └──────────────────┘
//! ```
//!
//! Used for: Flash crash detection, market anomaly protection
//! - Trips on excessive spread, price movement, low liquidity, stale data
//! - Requires manual reset
//! - Cannot accidentally re-enable trading (compile-time enforced)
//!
//! # Three-State Circuit Breaker (Resilience)
//!
//! ```text
//!     CLOSED ──fail──→ OPEN ──timeout──→ HALFOPEN
//!        ▲                                    │
//!        │               success              │
//!        └────────────────────────────────────┘
//!                           │
//!                          fail
//!                           ▼
//!                         OPEN
//! ```
//!
//! Used for: Connection failures, API errors, preventing cascade failures
//! - Opens after N failures
//! - Automatically attempts recovery after timeout
//! - Closes after M consecutive successes
//!
//! # Usage
//!
//! ```
//! use bog_core::core::circuit_breaker_fsm::*;
//!
//! // Binary breaker (risk management)
//! let breaker = BinaryNormal::new();
//! let breaker = breaker.trip(HaltReason::Manual("Flash crash detected".to_string()));
//! // Now in Halted state - cannot trade!
//! // breaker.check_market(); // COMPILE ERROR - Halted has no check_market()
//! let breaker = breaker.reset(); // Manual reset required
//!
//! // Three-state breaker (resilience)
//! let breaker = ThreeStateClosed::new_default();
//! let breaker = breaker.record_failure(); // After N failures → Open
//! // In Open state, automatically transitions to HalfOpen after timeout
//! ```

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant, SystemTime};

// ============================================================================
// BINARY CIRCUIT BREAKER (Risk Management)
// ============================================================================

/// Reason for circuit breaker trip
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HaltReason {
    /// Spread exceeded maximum (flash crash)
    ExcessiveSpread { spread_bps: u64, max_bps: u64 },
    /// Price changed too much in one tick
    ExcessivePriceMove { change_pct: u64, max_pct: u64 },
    /// Insufficient liquidity on both sides
    InsufficientLiquidity {
        min_size: u64,
        actual_bid: u64,
        actual_ask: u64,
    },
    /// Market data is stale
    StaleData { age_ms: i64, max_age_ms: i64 },
    /// Manual halt by operator
    Manual(String),
    /// Other custom reason
    Custom(String),
}

impl std::fmt::Display for HaltReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HaltReason::ExcessiveSpread {
                spread_bps,
                max_bps,
            } => {
                write!(
                    f,
                    "Excessive spread: {}bps (max: {}bps)",
                    spread_bps, max_bps
                )
            }
            HaltReason::ExcessivePriceMove {
                change_pct,
                max_pct,
            } => {
                write!(
                    f,
                    "Excessive price move: {}% (max: {}%)",
                    change_pct, max_pct
                )
            }
            HaltReason::InsufficientLiquidity {
                min_size,
                actual_bid,
                actual_ask,
            } => {
                write!(
                    f,
                    "Insufficient liquidity: bid={}, ask={} (min: {})",
                    actual_bid, actual_ask, min_size
                )
            }
            HaltReason::StaleData { age_ms, max_age_ms } => {
                write!(f, "Stale data: {}ms old (max: {}ms)", age_ms, max_age_ms)
            }
            HaltReason::Manual(msg) => write!(f, "Manual halt: {}", msg),
            HaltReason::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

/// Data tracked by binary circuit breaker
#[derive(Debug, Clone)]
pub struct BinaryBreakerData {
    /// Total times tripped
    pub total_trips: u64,
    /// Last trip reason (if any)
    pub last_trip_reason: Option<HaltReason>,
    /// Timestamp of last trip
    pub last_trip_time: Option<SystemTime>,
    /// Timestamp of last reset
    pub last_reset_time: Option<SystemTime>,
}

impl BinaryBreakerData {
    pub fn new() -> Self {
        Self {
            total_trips: 0,
            last_trip_reason: None,
            last_trip_time: None,
            last_reset_time: None,
        }
    }
}

impl Default for BinaryBreakerData {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// State: BinaryNormal
// ============================================================================

/// Binary circuit breaker in Normal state
///
/// Trading is allowed in this state.
///
/// **Valid Transitions:**
/// - `trip(reason)` → BinaryHalted
#[derive(Debug, Clone)]
pub struct BinaryNormal {
    data: BinaryBreakerData,
}

impl BinaryNormal {
    /// Create a new binary circuit breaker in Normal state
    pub fn new() -> Self {
        Self {
            data: BinaryBreakerData::new(),
        }
    }

    /// Access the underlying data
    pub fn data(&self) -> &BinaryBreakerData {
        &self.data
    }

    /// Transition: Normal → Halted (circuit breaker trips)
    pub fn trip(mut self, reason: HaltReason) -> BinaryHalted {
        self.data.total_trips += 1;
        self.data.last_trip_reason = Some(reason.clone());
        self.data.last_trip_time = Some(SystemTime::now());
        BinaryHalted {
            data: self.data,
            reason,
        }
    }

    /// Check if currently operational (always true for Normal)
    pub fn is_operational(&self) -> bool {
        true
    }
}

impl Default for BinaryNormal {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// State: BinaryHalted
// ============================================================================

/// Binary circuit breaker in Halted state (terminal until reset)
///
/// Trading is NOT allowed in this state.
///
/// **Valid Transitions:**
/// - `reset()` → BinaryNormal
#[derive(Debug, Clone)]
pub struct BinaryHalted {
    data: BinaryBreakerData,
    /// The reason for this halt
    reason: HaltReason,
}

impl BinaryHalted {
    /// Access the underlying data
    pub fn data(&self) -> &BinaryBreakerData {
        &self.data
    }

    /// Get the halt reason
    pub fn reason(&self) -> &HaltReason {
        &self.reason
    }

    /// Transition: Halted → Normal (manual reset)
    pub fn reset(mut self) -> BinaryNormal {
        self.data.last_reset_time = Some(SystemTime::now());
        BinaryNormal { data: self.data }
    }

    /// Check if currently operational (always false for Halted)
    pub fn is_operational(&self) -> bool {
        false
    }
}

// ============================================================================
// Enum wrapper for Binary Circuit Breaker
// ============================================================================

/// Type-erased binary circuit breaker state
#[derive(Debug, Clone)]
pub enum BinaryBreakerState {
    Normal(BinaryNormal),
    Halted(BinaryHalted),
}

impl BinaryBreakerState {
    /// Check if operational
    pub fn is_operational(&self) -> bool {
        match self {
            BinaryBreakerState::Normal(_) => true,
            BinaryBreakerState::Halted(_) => false,
        }
    }

    /// Get halt reason if halted
    pub fn halt_reason(&self) -> Option<&HaltReason> {
        match self {
            BinaryBreakerState::Normal(_) => None,
            BinaryBreakerState::Halted(h) => Some(h.reason()),
        }
    }
}

impl From<BinaryNormal> for BinaryBreakerState {
    fn from(n: BinaryNormal) -> Self {
        BinaryBreakerState::Normal(n)
    }
}

impl From<BinaryHalted> for BinaryBreakerState {
    fn from(h: BinaryHalted) -> Self {
        BinaryBreakerState::Halted(h)
    }
}

// ============================================================================
// THREE-STATE CIRCUIT BREAKER (Resilience)
// ============================================================================

/// Data tracked by three-state circuit breaker
#[derive(Debug, Clone)]
pub struct ThreeStateBreakerData {
    /// Configuration
    pub failure_threshold: u64,
    pub success_threshold: u64,
    pub timeout_duration: Duration,
    /// Current failure count
    pub failure_count: u64,
    /// Current success count (in HalfOpen)
    pub success_count: u64,
    /// Total times opened
    pub total_opens: u64,
    /// Last state change time
    pub last_state_change: Instant,
    /// Time when circuit opened (for timeout calculation)
    pub opened_at: Option<Instant>,
}

impl ThreeStateBreakerData {
    pub fn new(failure_threshold: u64, success_threshold: u64, timeout: Duration) -> Self {
        Self {
            failure_threshold,
            success_threshold,
            timeout_duration: timeout,
            failure_count: 0,
            success_count: 0,
            total_opens: 0,
            last_state_change: Instant::now(),
            opened_at: None,
        }
    }

    /// Check if timeout has expired (for Open → HalfOpen transition)
    pub fn is_timeout_expired(&self) -> bool {
        if let Some(opened) = self.opened_at {
            opened.elapsed() >= self.timeout_duration
        } else {
            false
        }
    }
}

// ============================================================================
// State: ThreeStateClosed
// ============================================================================

/// Three-state circuit breaker in Closed state (normal operation)
///
/// **Valid Transitions:**
/// - `record_failure()` → ThreeStateClosed | ThreeStateOpen
/// - `record_success()` → ThreeStateClosed
#[derive(Debug, Clone)]
pub struct ThreeStateClosed {
    data: ThreeStateBreakerData,
}

impl ThreeStateClosed {
    /// Create a new three-state circuit breaker in Closed state
    pub fn new(failure_threshold: u64, success_threshold: u64, timeout: Duration) -> Self {
        Self {
            data: ThreeStateBreakerData::new(failure_threshold, success_threshold, timeout),
        }
    }

    /// Create with default configuration
    pub fn new_default() -> Self {
        Self::new(5, 2, Duration::from_secs(30))
    }

    /// Access the underlying data
    pub fn data(&self) -> &ThreeStateBreakerData {
        &self.data
    }

    /// Record a successful operation (resets failure count)
    pub fn record_success(mut self) -> Self {
        self.data.failure_count = 0;
        self
    }

    /// Record a failed operation
    ///
    /// Returns ThreeStateOpen if failure threshold exceeded, otherwise stays Closed
    pub fn record_failure(mut self) -> ThreeStateResult {
        self.data.failure_count += 1;

        if self.data.failure_count >= self.data.failure_threshold {
            // Transition to Open
            self.data.total_opens += 1;
            self.data.opened_at = Some(Instant::now());
            self.data.last_state_change = Instant::now();
            ThreeStateResult::Open(ThreeStateOpen { data: self.data })
        } else {
            // Stay Closed
            ThreeStateResult::Closed(self)
        }
    }

    /// Check if operation is allowed (always true for Closed)
    pub fn is_call_permitted(&self) -> bool {
        true
    }
}

// ============================================================================
// State: ThreeStateOpen
// ============================================================================

/// Three-state circuit breaker in Open state (tripped)
///
/// **Valid Transitions:**
/// - `check_timeout()` → ThreeStateOpen | ThreeStateHalfOpen
#[derive(Debug, Clone)]
pub struct ThreeStateOpen {
    data: ThreeStateBreakerData,
}

impl ThreeStateOpen {
    /// Access the underlying data
    pub fn data(&self) -> &ThreeStateBreakerData {
        &self.data
    }

    /// Check if timeout expired and transition to HalfOpen if so
    pub fn check_timeout(mut self) -> ThreeStateOpenOrHalf {
        if self.data.is_timeout_expired() {
            // Transition to HalfOpen
            self.data.success_count = 0;
            self.data.last_state_change = Instant::now();
            ThreeStateOpenOrHalf::HalfOpen(ThreeStateHalfOpen { data: self.data })
        } else {
            // Stay Open
            ThreeStateOpenOrHalf::Open(self)
        }
    }

    /// Check if operation is allowed (always false for Open unless timeout expired)
    pub fn is_call_permitted(&self) -> bool {
        self.data.is_timeout_expired()
    }
}

// ============================================================================
// State: ThreeStateHalfOpen
// ============================================================================

/// Three-state circuit breaker in HalfOpen state (testing recovery)
///
/// **Valid Transitions:**
/// - `record_success()` → ThreeStateHalfOpen | ThreeStateClosed
/// - `record_failure()` → ThreeStateOpen
#[derive(Debug, Clone)]
pub struct ThreeStateHalfOpen {
    data: ThreeStateBreakerData,
}

impl ThreeStateHalfOpen {
    /// Access the underlying data
    pub fn data(&self) -> &ThreeStateBreakerData {
        &self.data
    }

    /// Record a successful operation
    ///
    /// Returns Closed if success threshold reached, otherwise stays HalfOpen
    pub fn record_success(mut self) -> ThreeStateHalfOrClosed {
        self.data.success_count += 1;

        if self.data.success_count >= self.data.success_threshold {
            // Transition to Closed
            self.data.failure_count = 0;
            self.data.success_count = 0;
            self.data.opened_at = None;
            self.data.last_state_change = Instant::now();
            ThreeStateHalfOrClosed::Closed(ThreeStateClosed { data: self.data })
        } else {
            // Stay HalfOpen
            ThreeStateHalfOrClosed::HalfOpen(self)
        }
    }

    /// Record a failed operation (immediately transitions back to Open)
    pub fn record_failure(mut self) -> ThreeStateOpen {
        self.data.total_opens += 1;
        self.data.opened_at = Some(Instant::now());
        self.data.last_state_change = Instant::now();
        self.data.success_count = 0;
        ThreeStateOpen { data: self.data }
    }

    /// Check if operation is allowed (always true for HalfOpen - we're testing!)
    pub fn is_call_permitted(&self) -> bool {
        true
    }
}

// ============================================================================
// Result types for state transitions
// ============================================================================

/// Result of record_failure() on Closed state
pub enum ThreeStateResult {
    Closed(ThreeStateClosed),
    Open(ThreeStateOpen),
}

impl ThreeStateResult {
    pub fn is_open(&self) -> bool {
        matches!(self, ThreeStateResult::Open(_))
    }
}

/// Result of check_timeout() on Open state
pub enum ThreeStateOpenOrHalf {
    Open(ThreeStateOpen),
    HalfOpen(ThreeStateHalfOpen),
}

/// Result of record_success() on HalfOpen state
pub enum ThreeStateHalfOrClosed {
    HalfOpen(ThreeStateHalfOpen),
    Closed(ThreeStateClosed),
}

impl ThreeStateHalfOrClosed {
    pub fn is_closed(&self) -> bool {
        matches!(self, ThreeStateHalfOrClosed::Closed(_))
    }
}

// ============================================================================
// Enum wrapper for Three-State Circuit Breaker
// ============================================================================

/// Type-erased three-state circuit breaker state
#[derive(Debug, Clone)]
pub enum ThreeStateBreakerState {
    Closed(ThreeStateClosed),
    Open(ThreeStateOpen),
    HalfOpen(ThreeStateHalfOpen),
}

impl ThreeStateBreakerState {
    /// Check if operation is allowed
    pub fn is_call_permitted(&self) -> bool {
        match self {
            ThreeStateBreakerState::Closed(c) => c.is_call_permitted(),
            ThreeStateBreakerState::Open(o) => o.is_call_permitted(),
            ThreeStateBreakerState::HalfOpen(h) => h.is_call_permitted(),
        }
    }

    /// Get current state name
    pub fn state_name(&self) -> &'static str {
        match self {
            ThreeStateBreakerState::Closed(_) => "Closed",
            ThreeStateBreakerState::Open(_) => "Open",
            ThreeStateBreakerState::HalfOpen(_) => "HalfOpen",
        }
    }
}

impl From<ThreeStateClosed> for ThreeStateBreakerState {
    fn from(c: ThreeStateClosed) -> Self {
        ThreeStateBreakerState::Closed(c)
    }
}

impl From<ThreeStateOpen> for ThreeStateBreakerState {
    fn from(o: ThreeStateOpen) -> Self {
        ThreeStateBreakerState::Open(o)
    }
}

impl From<ThreeStateHalfOpen> for ThreeStateBreakerState {
    fn from(h: ThreeStateHalfOpen) -> Self {
        ThreeStateBreakerState::HalfOpen(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // ========================================================================
    // Binary Circuit Breaker Tests
    // ========================================================================

    #[test]
    fn test_binary_normal_to_halted() {
        let breaker = BinaryNormal::new();
        assert!(breaker.is_operational());
        assert_eq!(breaker.data().total_trips, 0);

        let reason = HaltReason::ExcessiveSpread {
            spread_bps: 150,
            max_bps: 100,
        };

        let breaker = breaker.trip(reason.clone());
        assert!(!breaker.is_operational());
        assert_eq!(breaker.reason(), &reason);
        assert_eq!(breaker.data().total_trips, 1);
    }

    #[test]
    fn test_binary_halted_to_normal() {
        let breaker = BinaryNormal::new();
        let breaker = breaker.trip(HaltReason::Manual("Test".to_string()));

        let breaker = breaker.reset();
        assert!(breaker.is_operational());
        assert!(breaker.data().last_reset_time.is_some());
    }

    #[test]
    fn test_binary_multiple_trips() {
        let breaker = BinaryNormal::new();
        let breaker = breaker.trip(HaltReason::Manual("Trip 1".to_string()));
        let breaker = breaker.reset();
        let breaker = breaker.trip(HaltReason::Manual("Trip 2".to_string()));

        assert_eq!(breaker.data().total_trips, 2);
    }

    // ========================================================================
    // Three-State Circuit Breaker Tests
    // ========================================================================

    #[test]
    fn test_three_state_closed_to_open() {
        let mut breaker = ThreeStateClosed::new(3, 2, Duration::from_secs(1));

        // Record 2 failures (should stay closed)
        breaker = match breaker.record_failure() {
            ThreeStateResult::Closed(c) => c,
            ThreeStateResult::Open(_) => panic!("Should not open yet"),
        };

        breaker = match breaker.record_failure() {
            ThreeStateResult::Closed(c) => c,
            ThreeStateResult::Open(_) => panic!("Should not open yet"),
        };

        // Record 3rd failure (should open)
        match breaker.record_failure() {
            ThreeStateResult::Open(o) => {
                assert!(!o.is_call_permitted() || o.data().is_timeout_expired());
                assert_eq!(o.data().total_opens, 1);
            }
            ThreeStateResult::Closed(_) => panic!("Should be open"),
        }
    }

    #[test]
    fn test_three_state_open_to_half_open() {
        let breaker = ThreeStateClosed::new(1, 2, Duration::from_millis(10));

        // Open the circuit
        let breaker = match breaker.record_failure() {
            ThreeStateResult::Open(o) => o,
            _ => panic!("Should be open"),
        };

        // Wait for timeout
        thread::sleep(Duration::from_millis(15));

        // Check timeout (should transition to HalfOpen)
        match breaker.check_timeout() {
            ThreeStateOpenOrHalf::HalfOpen(h) => {
                assert!(h.is_call_permitted());
            }
            ThreeStateOpenOrHalf::Open(_) => panic!("Should be half-open after timeout"),
        }
    }

    #[test]
    fn test_three_state_half_open_to_closed() {
        let breaker = ThreeStateClosed::new(1, 2, Duration::from_millis(10));
        let breaker = match breaker.record_failure() {
            ThreeStateResult::Open(o) => o,
            _ => panic!(),
        };

        thread::sleep(Duration::from_millis(15));

        let mut breaker = match breaker.check_timeout() {
            ThreeStateOpenOrHalf::HalfOpen(h) => h,
            _ => panic!(),
        };

        // Record first success (should stay half-open)
        breaker = match breaker.record_success() {
            ThreeStateHalfOrClosed::HalfOpen(h) => h,
            ThreeStateHalfOrClosed::Closed(_) => panic!("Should stay half-open"),
        };

        // Record second success (should close)
        match breaker.record_success() {
            ThreeStateHalfOrClosed::Closed(c) => {
                assert!(c.is_call_permitted());
                assert_eq!(c.data().failure_count, 0);
            }
            ThreeStateHalfOrClosed::HalfOpen(_) => panic!("Should be closed"),
        }
    }

    #[test]
    fn test_three_state_half_open_failure_reopens() {
        let breaker = ThreeStateClosed::new(1, 2, Duration::from_millis(10));
        let breaker = match breaker.record_failure() {
            ThreeStateResult::Open(o) => o,
            _ => panic!(),
        };

        thread::sleep(Duration::from_millis(15));

        let breaker = match breaker.check_timeout() {
            ThreeStateOpenOrHalf::HalfOpen(h) => h,
            _ => panic!(),
        };

        // Failure in HalfOpen should reopen
        let breaker = breaker.record_failure();
        assert!(!breaker.is_call_permitted() || breaker.data().is_timeout_expired());
        assert_eq!(breaker.data().total_opens, 2); // Opened twice now
    }

    #[test]
    fn test_three_state_success_resets_failures_in_closed() {
        let mut breaker = ThreeStateClosed::new(3, 2, Duration::from_secs(1));

        // Record 2 failures
        breaker = match breaker.record_failure() {
            ThreeStateResult::Closed(c) => c,
            _ => panic!(),
        };
        breaker = match breaker.record_failure() {
            ThreeStateResult::Closed(c) => c,
            _ => panic!(),
        };

        assert_eq!(breaker.data().failure_count, 2);

        // Record success (should reset failure count)
        breaker = breaker.record_success();
        assert_eq!(breaker.data().failure_count, 0);
    }
}
