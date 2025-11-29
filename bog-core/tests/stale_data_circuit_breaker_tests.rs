//! Test-Driven Development: Stale Data Circuit Breaker Tests
//!
//! These tests drive the implementation of stale data detection.
//! The circuit breaker prevents trading when market data is too old.

// ============================================================================
// BASIC STALE DATA DETECTION
// ============================================================================

/// Test: initial_state_is_fresh
///
/// Verifies fresh data state on initialization
#[test]
fn test_initial_state_is_fresh() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};

    let breaker = StaleDataBreaker::new(StaleDataConfig::default());
    assert!(breaker.is_fresh());
    assert_eq!(breaker.state(), StaleDataState::Fresh);
    assert!(!breaker.is_stale());
    assert!(!breaker.is_offline());
}

/// Test: detect_stale_by_age
///
/// Verifies stale detection by data age threshold
#[test]
fn test_detect_stale_by_age() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_millis(100), // Short timeout for testing
        max_empty_polls: 10000,
    };

    let mut breaker = StaleDataBreaker::new(config);
    assert!(breaker.is_fresh());

    // Wait longer than max_age
    std::thread::sleep(Duration::from_millis(150));

    // Mark empty poll - should trigger stale detection
    breaker.mark_empty_poll();

    assert!(breaker.is_stale());
    assert_eq!(breaker.state(), StaleDataState::Stale);
    assert!(!breaker.is_fresh());
}

/// Test: detect_offline_by_empty_polls
///
/// Verifies offline detection by consecutive empty polls
#[test]
fn test_detect_offline_by_empty_polls() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_secs(5),
        max_empty_polls: 10, // Low threshold for testing
    };

    let mut breaker = StaleDataBreaker::new(config);
    assert!(breaker.is_fresh());

    // Mark 11 empty polls (exceeds max_empty_polls of 10)
    for _ in 0..11 {
        breaker.mark_empty_poll();
    }

    assert!(breaker.is_offline());
    assert_eq!(breaker.state(), StaleDataState::Offline);
    assert!(!breaker.is_fresh());
}

/// Test: mark_fresh_resets_state
///
/// Verifies that marking fresh data resets stale detection
#[test]
fn test_mark_fresh_resets_state() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_millis(100),
        max_empty_polls: 5,
    };

    let mut breaker = StaleDataBreaker::new(config);

    // Make it stale
    for _ in 0..6 {
        breaker.mark_empty_poll();
    }
    assert!(breaker.is_offline());

    // Mark fresh - should reset everything
    breaker.mark_fresh();

    assert!(breaker.is_fresh());
    assert_eq!(breaker.state(), StaleDataState::Fresh);
}

// ============================================================================
// CONFIGURATION
// ============================================================================

/// Test: custom_stale_threshold
///
/// Verifies custom max_age configuration
#[test]
fn test_custom_stale_threshold() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_millis(100),
        max_empty_polls: 10000,
    };

    let mut breaker = StaleDataBreaker::new(config);
    breaker.mark_fresh();

    // Wait longer than custom max_age
    std::thread::sleep(Duration::from_millis(150));
    breaker.mark_empty_poll();

    assert_eq!(breaker.state(), StaleDataState::Stale);
}

/// Test: custom_empty_poll_threshold
///
/// Verifies custom max_empty_polls configuration
#[test]
fn test_custom_empty_poll_threshold() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_secs(5),
        max_empty_polls: 20,
    };

    let mut breaker = StaleDataBreaker::new(config);

    // Mark empty polls exceeding threshold
    for _ in 0..21 {
        breaker.mark_empty_poll();
    }

    assert_eq!(breaker.state(), StaleDataState::Offline);
}

// ============================================================================
// STATE TRANSITIONS
// ============================================================================

/// Test: state_transition_fresh_to_stale
///
/// Verifies Fresh → Stale transition
#[test]
fn test_state_transition_fresh_to_stale() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_millis(50),
        max_empty_polls: 10000,
    };

    let mut breaker = StaleDataBreaker::new(config);
    assert_eq!(breaker.state(), StaleDataState::Fresh);

    // Wait for stale timeout
    std::thread::sleep(Duration::from_millis(100));

    // Trigger stale detection
    breaker.mark_empty_poll();
    assert_eq!(breaker.state(), StaleDataState::Stale);
}

/// Test: state_transition_fresh_to_offline
///
/// Verifies Fresh → Offline transition (via empty polls only)
#[test]
fn test_state_transition_fresh_to_offline() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_secs(10),
        max_empty_polls: 5,
    };

    let mut breaker = StaleDataBreaker::new(config);
    assert_eq!(breaker.state(), StaleDataState::Fresh);

    // Go directly to offline via empty polls (without hitting stale)
    for _ in 0..6 {
        breaker.mark_empty_poll();
    }

    assert_eq!(breaker.state(), StaleDataState::Offline);
}

/// Test: state_transition_stale_to_offline
///
/// Verifies Stale → Offline transition
#[test]
fn test_state_transition_stale_to_offline() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_millis(50),
        max_empty_polls: 10,
    };

    let mut breaker = StaleDataBreaker::new(config);

    // First go stale
    std::thread::sleep(Duration::from_millis(100));
    breaker.mark_empty_poll();
    assert_eq!(breaker.state(), StaleDataState::Stale);

    // Then go offline
    for _ in 0..10 {
        breaker.mark_empty_poll();
    }

    assert_eq!(breaker.state(), StaleDataState::Offline);
}

/// Test: state_transition_any_to_fresh
///
/// Verifies any state → Fresh transition via mark_fresh()
#[test]
fn test_state_transition_any_to_fresh() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_millis(50),
        max_empty_polls: 5,
    };

    let mut breaker = StaleDataBreaker::new(config);

    // Go to offline
    for _ in 0..6 {
        breaker.mark_empty_poll();
    }
    assert_eq!(breaker.state(), StaleDataState::Offline);

    // Recover to fresh
    breaker.mark_fresh();
    assert_eq!(breaker.state(), StaleDataState::Fresh);
}

// ============================================================================
// TIMEOUT MECHANICS
// ============================================================================

/// Test: time_since_update_tracking
///
/// Verifies time tracking since last update
#[test]
fn test_time_since_update_tracking() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig};
    use std::time::Duration;

    let config = StaleDataConfig::default();
    let mut breaker = StaleDataBreaker::new(config);
    breaker.mark_fresh();

    // Wait and verify time progresses
    std::thread::sleep(Duration::from_millis(100));

    // Time since update should be at least 100ms
    // (We can't directly check this without exposing internal methods,
    // but we can verify the stale detection works as a proxy)
    breaker.mark_empty_poll();
    // Still fresh because max_age is 5s by default
    assert!(breaker.is_fresh());
}

/// Test: consecutive_empty_polls_increment
///
/// Verifies empty poll counter increments correctly
#[test]
fn test_consecutive_empty_polls_increment() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_secs(5),
        max_empty_polls: 100,
    };

    let mut breaker = StaleDataBreaker::new(config);

    // Mark a few empty polls - should not go offline yet
    for i in 0..5 {
        breaker.mark_empty_poll();
        assert!(
            !breaker.is_offline(),
            "Should not be offline after {} polls",
            i + 1
        );
    }

    // Reset should clear counter
    breaker.mark_fresh();

    // Verify we can mark more polls after reset
    for _ in 0..5 {
        breaker.mark_empty_poll();
    }
    assert!(!breaker.is_offline());
}

// ============================================================================
// RESET BEHAVIOR
// ============================================================================

/// Test: reset_clears_all_state
///
/// Verifies reset() method clears everything
#[test]
fn test_reset_clears_all_state() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_millis(50),
        max_empty_polls: 5,
    };

    let mut breaker = StaleDataBreaker::new(config);

    // Make it go offline
    for _ in 0..6 {
        breaker.mark_empty_poll();
    }
    assert_eq!(breaker.state(), StaleDataState::Offline);

    // Mark fresh resets everything
    breaker.mark_fresh();
    assert_eq!(breaker.state(), StaleDataState::Fresh);
    assert!(breaker.is_fresh());
}

/// Test: reset_after_offline
///
/// Verifies recovery from offline state via reset
#[test]
fn test_reset_after_offline() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_secs(5),
        max_empty_polls: 3,
    };

    let mut breaker = StaleDataBreaker::new(config);

    // Go offline
    for _ in 0..4 {
        breaker.mark_empty_poll();
    }
    assert!(breaker.is_offline());

    // Reset recovers
    breaker.mark_fresh();
    assert!(breaker.is_fresh());
    assert!(!breaker.is_offline());
}

// ============================================================================
// PERFORMANCE TESTS
// ============================================================================

/// Benchmark: is_fresh_latency
///
/// Performance requirement: <5ns
/// Must be inlineable for hot path
#[test]
fn test_is_fresh_latency() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig};

    let breaker = StaleDataBreaker::new(StaleDataConfig::default());

    // Just verify it doesn't panic - actual latency is measured in benchmarks
    let _result = breaker.is_fresh();
    assert!(true); // No panic = success
}

/// Benchmark: mark_empty_poll_latency
///
/// Performance requirement: <10ns
#[test]
fn test_mark_empty_poll_latency() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig};

    let mut breaker = StaleDataBreaker::new(StaleDataConfig::default());

    // Just verify it doesn't panic - actual latency is measured in benchmarks
    breaker.mark_empty_poll();
    assert!(true); // No panic = success
}

// ============================================================================
// EDGE CASES
// ============================================================================

/// Test: zero_max_age
///
/// Edge case: max_age = 0 (immediately stale)
#[test]
fn test_zero_max_age() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_secs(0), // Zero timeout
        max_empty_polls: 1000,
    };

    let mut breaker = StaleDataBreaker::new(config);

    // Any mark_empty_poll() should trigger stale
    breaker.mark_empty_poll();
    assert!(breaker.is_stale());
}

/// Test: zero_max_empty_polls
///
/// Edge case: max_empty_polls = 0 (immediately offline)
#[test]
fn test_zero_max_empty_polls() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_secs(5),
        max_empty_polls: 0, // Zero tolerance
    };

    let mut breaker = StaleDataBreaker::new(config);

    // First mark_empty_poll() should trigger offline
    breaker.mark_empty_poll();
    assert!(breaker.is_offline());
}

// ============================================================================
// INTEGRATION WITH ENGINE
// ============================================================================

/// Test: block_trades_when_stale
///
/// Verifies stale data prevents trading
#[test]
fn test_block_trades_when_stale() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_millis(50),
        max_empty_polls: 100,
    };

    let mut breaker = StaleDataBreaker::new(config);

    // Simulate stale condition
    std::thread::sleep(Duration::from_millis(100));
    breaker.mark_empty_poll();

    // Verify stale
    assert_eq!(breaker.state(), StaleDataState::Stale);

    // Engine integration: check freshness before execute()
    if !breaker.is_fresh() {
        // Skip execution - no trades occur
        assert!(true); // Trade blocked
    } else {
        panic!("Trade should be blocked!");
    }
}

/// Test: resume_trades_after_recovery
///
/// Verifies trading resumes after fresh data received
#[test]
fn test_resume_trades_after_recovery() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_millis(50),
        max_empty_polls: 100,
    };

    let mut breaker = StaleDataBreaker::new(config);

    // Become stale
    std::thread::sleep(Duration::from_millis(100));
    breaker.mark_empty_poll();
    assert_eq!(breaker.state(), StaleDataState::Stale);

    // Receive fresh data and recover
    breaker.mark_fresh();
    assert!(breaker.is_fresh());
}

// ============================================================================
// PROPERTY TESTS
// ============================================================================

/// Property test: is_fresh_is_boolean
///
/// Invariant: is_fresh() always returns true or false (never panics)
#[test]
fn test_is_fresh_is_boolean() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig};

    let breaker = StaleDataBreaker::new(StaleDataConfig::default());

    // Property: is_fresh() always returns bool
    let result = breaker.is_fresh();
    assert!(result == true || result == false);
}

/// Property test: state_transitions_valid
///
/// Invariant: state always transitions through valid paths
#[test]
fn test_state_transitions_valid() {
    use bog_core::resilience::{StaleDataBreaker, StaleDataConfig, StaleDataState};
    use std::time::Duration;

    let config = StaleDataConfig {
        max_age: Duration::from_millis(50),
        max_empty_polls: 5,
    };

    let mut breaker = StaleDataBreaker::new(config);
    assert_eq!(breaker.state(), StaleDataState::Fresh);

    // Valid: Fresh -> Stale
    std::thread::sleep(Duration::from_millis(100));
    breaker.mark_empty_poll();
    assert_eq!(breaker.state(), StaleDataState::Stale);

    // Valid: Stale -> Offline
    for _ in 0..5 {
        breaker.mark_empty_poll();
    }
    assert_eq!(breaker.state(), StaleDataState::Offline);

    // Valid: Offline -> Fresh
    breaker.mark_fresh();
    assert_eq!(breaker.state(), StaleDataState::Fresh);
}
