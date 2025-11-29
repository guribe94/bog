//! Edge case tests for snapshot validation to prevent crashes
//!
//! These tests verify that the validator handles malformed data gracefully
//! without panicking, even in edge cases that "should never happen".

use bog_core::data::{MarketSnapshot, SnapshotValidator, ValidationError};

/// Test that validator handles zero bid price without panicking
///
/// Bug: Line 294 in validator.rs divides by best_bid_price without checking for zero
/// This could happen if Huginn sends malformed data with bid=0
#[test]
fn test_validator_division_by_zero_in_spread_calculation() {
    let mut validator = SnapshotValidator::new();

    // Create a malformed snapshot with zero bid price
    // This could theoretically pass initial checks if ask is also zero
    let mut snapshot = MarketSnapshot::default();
    snapshot.sequence = 1;
    snapshot.best_bid_price = 0; // Zero bid
    snapshot.best_ask_price = 0; // Zero ask
    snapshot.best_bid_size = 1_000_000_000;
    snapshot.best_ask_size = 1_000_000_000;
    snapshot.exchange_timestamp_ns = 1_700_000_000_000_000_000;
    snapshot.local_recv_ns = 1_700_000_000_000_000_001;

    // This should NOT panic, should return an error
    let result = validator.validate(&mut snapshot);

    // We expect a validation error, not a panic
    assert!(result.is_err());
    match result {
        Err(ValidationError::ZeroBidPrice) => {
            // Expected error
        }
        Err(other) => {
            // Also acceptable if it's caught elsewhere
            println!("Got error: {:?}", other);
        }
        Ok(_) => {
            panic!("Should not accept snapshot with zero prices!");
        }
    }
}

/// Test validator with zero bid but non-zero ask
///
/// This tests a specific edge case where bid=0 but ask>0
/// The spread calculation would divide by zero
#[test]
fn test_validator_zero_bid_nonzero_ask() {
    let mut validator = SnapshotValidator::new();

    let mut snapshot = MarketSnapshot::default();
    snapshot.sequence = 1;
    snapshot.best_bid_price = 0; // Zero bid
    snapshot.best_ask_price = 50_000_000_000_000; // $50k ask
    snapshot.best_bid_size = 0; // Also zero size (market halted?)
    snapshot.best_ask_size = 1_000_000_000;
    snapshot.exchange_timestamp_ns = 1_700_000_000_000_000_000;

    // Should not panic
    let result = validator.validate(&mut snapshot);
    assert!(result.is_err());
}

/// Test that validator properly handles crossed orderbook with zeros
#[test]
fn test_validator_crossed_book_with_zeros() {
    let mut validator = SnapshotValidator::new();

    let mut snapshot = MarketSnapshot::default();
    snapshot.sequence = 1;
    // Both zero AND crossed (bid == ask)
    snapshot.best_bid_price = 0;
    snapshot.best_ask_price = 0;
    snapshot.best_bid_size = 1_000_000_000;
    snapshot.best_ask_size = 1_000_000_000;

    let result = validator.validate(&mut snapshot);
    assert!(result.is_err());

    // Should fail validation, not panic
    assert!(matches!(
        result,
        Err(ValidationError::ZeroBidPrice) | Err(ValidationError::ZeroAskPrice)
    ));
}

/// Test spread calculation with very small but non-zero prices
///
/// This tests precision edge cases
#[test]
fn test_validator_tiny_prices() {
    let mut validator = SnapshotValidator::new();

    let mut snapshot = MarketSnapshot::default();
    snapshot.sequence = 1;
    snapshot.best_bid_price = 1; // Smallest non-zero price
    snapshot.best_ask_price = 2; // Tiny spread
    snapshot.best_bid_size = 1_000_000_000;
    snapshot.best_ask_size = 1_000_000_000;
    snapshot.exchange_timestamp_ns = 1_700_000_000_000_000_000;

    // Should handle tiny prices without overflow/underflow
    let result = validator.validate(&mut snapshot);

    // This should actually be valid (100% spread is high but not invalid)
    // The validator should handle the arithmetic without panicking
    println!("Result for tiny prices: {:?}", result);
}

/// Test that spread percentage calculation doesn't overflow
#[test]
fn test_validator_spread_bps_overflow() {
    let mut validator = SnapshotValidator::new();

    let mut snapshot = MarketSnapshot::default();
    snapshot.sequence = 1;
    snapshot.best_bid_price = 1; // Tiny bid
    snapshot.best_ask_price = u64::MAX; // Huge ask
    snapshot.best_bid_size = 1_000_000_000;
    snapshot.best_ask_size = 1_000_000_000;

    // Spread calculation: (u64::MAX - 1) * 10_000 / 1
    // This would overflow without proper handling

    let result = validator.validate(&mut snapshot);

    // Should return error for unreasonable spread, not panic
    assert!(result.is_err());
}
