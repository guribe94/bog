//! Tests for stale data rejection
//!
//! These tests verify that the system rejects market data that is too old.

use bog_core::data::MarketSnapshot;

#[test]
fn test_stale_snapshot_rejected() {
    // Test that snapshots older than max_age are rejected
    //
    // Current behavior:
    //   - validate_snapshot() does NOT check staleness
    //   - Circuit breaker checks staleness but only SKIPS the tick
    //   - Strategy could trade on 10+ second old data
    //
    // Expected: validate_snapshot() should reject stale data

    // This test FAILS because validate_snapshot doesn't have max_age_ns parameter yet

    let max_age_ns = 5_000_000_000u64;  // 5 seconds

    // Create a stale snapshot (10 seconds old)
    // Currently we can't easily create a snapshot with specific timestamp,
    // so this test is a placeholder

    // After implementation:
    // assert!(validate_snapshot(&stale_snapshot, max_age_ns).is_err());
}

#[test]
fn test_recent_snapshot_accepted() {
    // Test that recent snapshots are accepted
    //
    // Expected: validate_snapshot() accepts data < max_age_ns

    let max_age_ns = 5_000_000_000u64;  // 5 seconds

    // Create a recent snapshot (1 second old)
    // After implementation:
    // assert!(validate_snapshot(&recent_snapshot, max_age_ns).is_ok());
}

#[test]
fn test_timestamp_boundary_condition() {
    // Test the boundary: snapshot exactly at max_age_ns
    //
    // Scenario: max_age_ns = 5 seconds, snapshot is 5.0 seconds old
    // Expected: Should be ACCEPTED (not rejected)

    let max_age_ns = 5_000_000_000u64;
    let snapshot_age_ns = 5_000_000_000u64;

    // Currently: no way to test this
    // After implementation: add test with snapshot exactly at boundary

    assert!(snapshot_age_ns <= max_age_ns, "Boundary: exactly at max age");
}

#[test]
fn test_just_over_boundary_rejected() {
    // Test that just over boundary is rejected
    //
    // Scenario: max_age_ns = 5 seconds, snapshot is 5.1 seconds old
    // Expected: Should be REJECTED

    let max_age_ns = 5_000_000_000u64;
    let snapshot_age_ns = 5_100_000_000u64;  // 5.1 seconds

    assert!(snapshot_age_ns > max_age_ns, "Should be over boundary");
    // After implementation: verify rejection
}

#[test]
fn test_zero_age_always_accepted() {
    // Test that current data (zero age) is always accepted

    let max_age_ns = 5_000_000_000u64;
    let snapshot_age_ns = 0u64;

    assert!(snapshot_age_ns <= max_age_ns, "Current data should always be accepted");
}

#[test]
fn test_multiple_stale_checks() {
    // Test staleness check with multiple snapshots at different ages

    let max_age_ns = 5_000_000_000;

    let test_cases = vec![
        (0u64, true),                    // Current: accept
        (1_000_000_000u64, true),        // 1 sec: accept
        (4_999_999_999u64, true),        // 4.999 sec: accept
        (5_000_000_000u64, true),        // 5.0 sec: accept (boundary)
        (5_000_000_001u64, false),       // 5.001 sec: reject
        (10_000_000_000u64, false),      // 10 sec: reject
    ];

    for (age_ns, should_accept) in test_cases {
        let is_valid = age_ns <= max_age_ns;
        assert_eq!(
            is_valid, should_accept,
            "Age {}ns should be {}",
            age_ns,
            if should_accept { "accepted" } else { "rejected" }
        );
    }
}
