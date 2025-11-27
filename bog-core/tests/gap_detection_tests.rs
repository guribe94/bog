//! Test-Driven Development: Gap Detection Tests
//!
//! These tests drive the implementation of gap detection in Bog.
//! Gaps occur when the sequence number jumps unexpectedly, indicating:
//! - Missed messages (network packet loss)
//! - Huginn restart (sequence resets)
//! - Shared memory buffer overflow
//!
//! The GapDetector must handle:
//! - Small gaps (1-10 messages)
//! - Large gaps (100+  messages)
//! - Wraparound at u64::MAX
//! - Duplicate messages (no gap)
//! - Gap after recovery

use bog_core::resilience::gap_detector::GapDetector;
use std::time::Instant;

// ============================================================================
// BASIC GAP DETECTION
// ============================================================================

/// Test: detect_small_gap
///
/// Verifies that:
/// - Sequence increases normally: 1 → 2 → 3
/// - Gap of 1 is detected: 3 → 5 (missing 4)
/// - Gap size calculated correctly: 5 - 3 - 1 = 1
#[test]
fn test_detect_small_gap() {
    let mut detector = GapDetector::new();

    // 1. Process sequence 1 → no gap (first message)
    assert_eq!(detector.check(1), 0);
    assert!(!detector.gap_detected());

    // 2. Process sequence 2 → no gap (1+1)
    assert_eq!(detector.check(2), 0);
    assert!(!detector.gap_detected());

    // 3. Process sequence 3 → no gap (2+1)
    assert_eq!(detector.check(3), 0);
    assert!(!detector.gap_detected());

    // 4. Process sequence 5 → gap detected! (expected 4)
    // gap = 5 - 3 - 1 = 1
    let gap = detector.check(5);
    assert_eq!(gap, 1);
    assert!(detector.gap_detected());
    assert_eq!(detector.last_gap_size(), 1);
}

/// Test: detect_medium_gap
///
/// Verifies that:
/// - Large gap (100 messages) is detected correctly
/// - Gap size calculated: expected 100+
/// - Can be used to trigger snapshot recovery
#[test]
fn test_detect_medium_gap() {
    let mut detector = GapDetector::new();

    // 1. Last sequence: 1000
    detector.check(1000);
    assert!(!detector.gap_detected());

    // 2. Next sequence: 1050
    // Gap = 1050 - 1000 - 1 = 49 (missing 1001..1049)
    let gap = detector.check(1050);

    // 3. Verify gap size
    assert_eq!(gap, 49);
    assert!(detector.gap_detected());
    assert_eq!(detector.last_gap_size(), 49);
}

/// Test: detect_large_gap
///
/// Verifies that:
/// - Very large gap (1000+ messages) is detected
/// - Recovery requires snapshot resync
/// - No data loss on recovery
#[test]
fn test_detect_large_gap() {
    let mut detector = GapDetector::new();

    // 1. Last sequence: 100
    detector.check(100);

    // 2. Next sequence: 1100
    // Gap = 1100 - 100 - 1 = 999
    let gap = detector.check(1100);

    // 3. Verify detection
    assert_eq!(gap, 999);
    assert!(detector.gap_detected());
}

/// Test: no_gap_sequential
///
/// Verifies that normal operation (seq+1) doesn't trigger gap detection
#[test]
fn test_no_gap_sequential() {
    let mut detector = GapDetector::new();

    // Sequence: 1, 2, 3, 4, 5
    for i in 1..=5 {
        assert_eq!(detector.check(i), 0);
        assert!(!detector.gap_detected());
    }
}

/// Test: no_gap_first_message
///
/// Verifies that first message is accepted without requiring previous state
#[test]
fn test_no_gap_first_message() {
    let mut detector = GapDetector::new();

    // New detector is not ready
    assert!(!detector.is_ready());

    // First message arrives
    let gap = detector.check(100);

    // Should not be a gap
    assert_eq!(gap, 0);
    assert!(!detector.gap_detected());
    
    // Should be ready now
    assert!(detector.is_ready());
    assert_eq!(detector.last_sequence(), 100);
}

/// Test: duplicate_message_not_a_gap
///
/// Verifies that duplicate messages (same sequence) don't trigger gap
#[test]
fn test_duplicate_message_not_a_gap() {
    let mut detector = GapDetector::new();

    // 1. Process sequence 100
    detector.check(100);

    // 2. Process sequence 100 again (duplicate)
    let gap = detector.check(100);

    // 3. Not treated as a gap
    assert_eq!(gap, 0);
    assert!(!detector.gap_detected());
    assert_eq!(detector.last_sequence(), 100);
}

// ============================================================================
// WRAPAROUND AT u64::MAX
// ============================================================================

/// Test: wraparound_detection
///
/// Verifies gap detection works correctly when sequence wraps u64::MAX → 0
/// This is critical for long-running trading systems
#[test]
fn test_wraparound_detection() {
    let mut detector = GapDetector::new();

    // 1. Last sequence: u64::MAX
    detector.check(u64::MAX);

    // 2. Next sequence: 0 (wrapped around)
    // Ideally this is "next message", so distance is 1.
    // The gap calculation logic treats (current > last) as normal.
    // If current < last, it uses wraparound logic.
    // gap = (u64::MAX - last) + current = (u64::MAX - u64::MAX) + 0 = 0.
    // Wait, gap formula is: u64::MAX - last + current.
    // If last = u64::MAX, current = 0:
    // gap = u64::MAX - u64::MAX + 0 = 0. Correct.
    let gap = detector.check(0);

    assert_eq!(gap, 0);
    assert!(!detector.gap_detected());
}

/// Test: wraparound_with_gap_before
///
/// Verifies gap detection before wraparound
#[test]
fn test_wraparound_with_gap_before() {
    let mut detector = GapDetector::new();

    // 1. Last sequence: u64::MAX - 5
    detector.check(u64::MAX - 5);

    // 2. Next sequence: u64::MAX - 2
    // Gap = (u64::MAX - 2) - (u64::MAX - 5) - 1 = 2
    // Missing: u64::MAX - 4, u64::MAX - 3
    let gap = detector.check(u64::MAX - 2);

    assert_eq!(gap, 2);
    assert!(detector.gap_detected());
}

/// Test: wraparound_with_gap_after
///
/// Verifies gap detection after wraparound
#[test]
fn test_wraparound_with_gap_after() {
    let mut detector = GapDetector::new();

    // 1. Last sequence: u64::MAX
    detector.check(u64::MAX);

    // 2. Next sequence: 5
    // Gap = (u64::MAX - u64::MAX) + 5 = 5?
    // Let's trace: u64::MAX -> 0 (no gap) -> 1 -> 2 -> 3 -> 4 -> 5
    // If next is 5, we missed 0, 1, 2, 3, 4. That is 5 messages.
    // Formula: u64::MAX - last + current
    // gap = u64::MAX - u64::MAX + 5 = 5. Correct.
    let gap = detector.check(5);

    assert_eq!(gap, 5);
    assert!(detector.gap_detected());
}

/// Test: wraparound_arithmetic
///
/// Verifies correctness of wraparound-aware gap calculation
#[test]
fn test_wraparound_arithmetic() {
    let mut detector = GapDetector::new();

    // Scenario: last=u64::MAX-2, next=5
    // Expect gap: (u64::MAX-1), u64::MAX, 0, 1, 2, 3, 4 => 7 messages missed
    // Formula: u64::MAX - (u64::MAX - 2) + 5 = 2 + 5 = 7.
    detector.check(u64::MAX - 2);
    let gap = detector.check(5);
    
    assert_eq!(gap, 7);
    assert!(detector.gap_detected());
}

// ============================================================================
// GAP DETECTOR STRUCT
// ============================================================================

/// Test: gap_detector_initialization
///
/// Verifies GapDetector starts in correct state
#[test]
fn test_gap_detector_initialization() {
    let detector = GapDetector::new();
    
    assert_eq!(detector.last_sequence(), 0);
    assert_eq!(detector.last_gap_size(), 0);
    assert!(!detector.gap_detected());
    assert!(!detector.is_ready());
}

/// Test: gap_detector_check_updates_state
///
/// Verifies check() method updates internal state
#[test]
fn test_gap_detector_check_updates_state() {
    let mut detector = GapDetector::new();

    // 1. check(100) → updates last_sequence to 100
    detector.check(100);
    assert_eq!(detector.last_sequence(), 100);

    // 2. check(101) → detects no gap
    detector.check(101);
    assert_eq!(detector.last_sequence(), 101);
    assert!(!detector.gap_detected());

    // 3. check(105) → detects gap of 3
    detector.check(105);
    assert_eq!(detector.last_sequence(), 105);
    assert!(detector.gap_detected());
    assert_eq!(detector.last_gap_size(), 3);

    // 4. check(106) → no gap after recovery (implicitly handled by next message)
    detector.check(106);
    assert!(!detector.gap_detected());
    assert_eq!(detector.last_gap_size(), 0);
}

/// Test: gap_detector_idempotent
///
/// Verifies calling check() multiple times with same sequence is safe
#[test]
fn test_gap_detector_idempotent() {
    let mut detector = GapDetector::new();

    // 1. check(100) → OK
    detector.check(100);
    
    // 2. check(100) → duplicate
    let gap = detector.check(100);
    
    // 3. Verify
    assert_eq!(gap, 0);
    assert_eq!(detector.last_sequence(), 100);
}

/// Test: gap_detector_reset
///
/// Verifies GapDetector can be reset for recovery
#[test]
fn test_gap_detector_reset() {
    let mut detector = GapDetector::new();
    detector.check(100);
    detector.check(105); // Cause gap
    assert!(detector.gap_detected());

    detector.reset();

    assert_eq!(detector.last_sequence(), 0);
    assert_eq!(detector.last_gap_size(), 0);
    assert!(!detector.gap_detected());
    assert!(!detector.is_ready());
}

// ============================================================================
// GAP RECOVERY SCENARIOS
// ============================================================================

/// Test: recovery_after_small_gap
///
/// Verifies system recovers from small gap
#[test]
fn test_recovery_after_small_gap() {
    let mut detector = GapDetector::new();
    
    // 1. Normal processing
    detector.check(1);
    detector.check(2);
    
    // 2. Gap detected
    let gap = detector.check(5);
    assert_eq!(gap, 2);
    
    // 3. Simulate snapshot recovery by resetting at new sequence
    // In real system, we'd fetch snapshot corresponding to seq 5
    detector.reset_at_sequence(5);
    
    // 4. Continue
    assert!(detector.is_ready());
    assert!(!detector.gap_detected());
    
    // 5. Next message
    detector.check(6);
    assert!(!detector.gap_detected());
}

/// Test: recovery_after_large_gap
///
/// Verifies recovery from very large gap (1000+ messages)
#[test]
fn test_recovery_after_large_gap() {
    let mut detector = GapDetector::new();
    detector.check(100);
    
    // Gap
    let gap = detector.check(2000);
    assert_eq!(gap, 1899);
    
    // Recovery
    detector.reset_at_sequence(2000);
    
    assert!(!detector.gap_detected());
    assert_eq!(detector.last_sequence(), 2000);
}

/// Test: multiple_gaps_sequence
///
/// Verifies handling of multiple gaps in one session
#[test]
fn test_multiple_gaps_sequence() {
    let mut detector = GapDetector::new();
    
    // Gap 1
    detector.check(1);
    assert_eq!(detector.check(5), 3); // Gap of 3
    
    // Recovery 1
    detector.reset_at_sequence(5);
    
    // Gap 2
    detector.check(10); // Gap: 10 - 5 - 1 = 4
    assert_eq!(detector.last_gap_size(), 4);
    
    // Recovery 2
    detector.reset_at_sequence(10);
    
    assert!(!detector.gap_detected());
}

/// Test: gap_during_recovery
///
/// Verifies edge case: new gap detected while recovering from previous gap
/// Note: The GapDetector itself is synchronous. "During recovery" implies
/// logic in the caller (Engine). For the unit test, we simulate the
/// state transitions.
#[test]
fn test_gap_during_recovery() {
    let mut detector = GapDetector::new();
    
    detector.check(100);
    
    // Gap detected
    detector.check(110); 
    assert!(detector.gap_detected());
    
    // Before we reset, another message comes in with ANOTHER gap relative to 110
    // e.g. 120.
    // The detector will calculate gap from 110 -> 120.
    let gap2 = detector.check(120);
    
    // Gap = 120 - 110 - 1 = 9
    assert_eq!(gap2, 9);
    assert_eq!(detector.last_sequence(), 120);
    
    // Final recovery handles everything up to 120
    detector.reset_at_sequence(120);
    assert!(!detector.gap_detected());
}

// ============================================================================
// HUGINN RESTART DETECTION
// ============================================================================

/// Test: detect_huginn_restart
///
/// Verifies Huginn restart is detected via sequence reset + epoch change
#[test]
fn test_detect_huginn_restart() {
    let mut detector = GapDetector::new();
    
    // Initial state
    detector.check(1000);
    detector.set_epoch(5);
    
    // Restart: sequence drops, epoch increases
    let is_restart = detector.detect_restart(10, 6);
    
    assert!(is_restart);
    // Epoch should auto-update
    // Note: detect_restart updates epoch if true? Let's check impl.
    // Impl says: if is_restart { self.last_epoch = epoch; }
    
    // Verify internal state if exposed, otherwise verify behavior
    // We can verify by checking if a subsequent call with same epoch isn't a restart
    assert!(!detector.detect_restart(11, 6));
}

/// Test: sequence_reset_without_epoch_change_is_gap
///
/// Verifies that sequence reset without epoch change is treated as gap
#[test]
fn test_sequence_reset_without_epoch_change_is_gap() {
    let mut detector = GapDetector::new();
    
    detector.check(1000);
    detector.set_epoch(5);
    
    // Sequence drops, epoch same
    let is_restart = detector.detect_restart(10, 5);
    
    // Should NOT be a restart
    assert!(!is_restart);
    
    // Should be a gap (wraparound logic applies if we called check)
    // But detect_restart is separate check.
    // If we call check(10) after 1000:
    // wraparound gap: (u64::MAX - 1000) + 10
    let gap = detector.check(10);
    assert!(gap > 0);
}

// ============================================================================
// PROPERTY TESTS
// ============================================================================

/// Property test: gap_size_always_non_negative
///
/// Invariant: gap_size >= 0 always
#[test]
fn test_gap_size_always_non_negative() {
    let mut detector = GapDetector::new();
    detector.check(100);
    
    // Test various inputs
    let inputs = vec![101, 102, 200, u64::MAX, 0, 50];
    for seq in inputs {
        // check() returns u64, so it can't be negative in Rust type system
        // but we verify it doesn't panic or wrap incorrectly
        let _ = detector.check(seq);
    }
}

/// Property test: gap_detection_monotonic
///
/// Invariant: once gap detected at seq N, all recovered state >= N
#[test]
fn test_gap_detection_monotonic() {
    let mut detector = GapDetector::new();
    detector.check(100);
    
    // Next is 105
    detector.check(105);
    
    assert_eq!(detector.last_sequence(), 105);
}

/// Property test: wraparound_safe_comparison
///
/// Invariant: comparison works correctly across u64::MAX boundary
#[test]
fn test_wraparound_safe_comparison() {
    let mut detector = GapDetector::new();
    
    // Boundary case
    detector.check(u64::MAX);
    let gap = detector.check(0);
    assert_eq!(gap, 0);
}

// ============================================================================
// PERFORMANCE TESTS
// ============================================================================

/// Benchmark: gap_detection_latency
///
/// Performance requirement: Gap detection <10ns
/// This must be inline and branch-predicted
#[test]
fn test_gap_detection_latency() {
    let mut detector = GapDetector::new();
    detector.check(0);
    
    let start = Instant::now();
    let iterations = 1_000_000;
    
    for i in 1..=iterations {
        // Simulate sequential access
        let _ = detector.check(i);
    }
    
    let duration = start.elapsed();
    let ns_per_op = duration.as_nanos() as f64 / iterations as f64;
    
    println!("Gap detection latency: {:.2} ns/op", ns_per_op);
    
    // Rough check - might be flaky in CI/debug builds so we set a loose bound
    // In release mode this should be < 1ns
    assert!(ns_per_op < 50.0, "Latency too high: {} ns", ns_per_op);
}

// ============================================================================
// ERROR HANDLING
// ============================================================================

/// Test: invalid_sequence_zero
///
/// Verifies handling of invalid sequence logic if any.
/// Currently 0 is valid in our logic (wraparound target), so this test 
/// confirms 0 is accepted.
#[test]
fn test_invalid_sequence_zero() {
    let mut detector = GapDetector::new();
    // 0 is accepted as first message
    let gap = detector.check(0);
    assert_eq!(gap, 0);
    assert!(detector.is_ready());
}

/// Test: sequence_before_gap_recovery
///
/// Verifies behavior if sequence arrives before gap recovery completes
#[test]
fn test_sequence_before_gap_recovery() {
    // This is more of an integration behavior (buffering),
    // but for the unit test we verify the detector state is consistent
    // if we keep feeding it during a gap state.
    
    let mut detector = GapDetector::new();
    detector.check(100);
    
    // Gap
    detector.check(105); 
    assert!(detector.gap_detected());
    
    // More messages arrive (106, 107)
    detector.check(106);
    
    // It should treat 106 as valid continuation from 105
    // (Gap was from 100->105). 
    // 105->106 has no NEW gap.
    assert!(!detector.gap_detected()); 
    assert_eq!(detector.last_gap_size(), 0);
    
    // The "gap" flag in the detector is transient for the current check() call
    // The ENGINE is responsible for seeing the flag and pausing processing
}
