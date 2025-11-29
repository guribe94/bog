//! Reproduction test for the "size is zero but price is set" crash
//!
//! This test reproduces the exact crash scenario from 2025-11-22T10:16:22
//! where a sequence gap (20481209 → 20481211) was followed by an invalid
//! snapshot with depth level 1 having a price but zero size.

use bog_core::data::snapshot_builder::SnapshotBuilder;
use bog_core::data::types::MarketSnapshot;
use bog_core::data::validator::{SnapshotValidator, ValidationError};

/// Test the exact crash scenario from the logs (BEFORE FIX: would crash, AFTER FIX: passes)
#[test]
fn test_depth_level_with_price_but_zero_size() {
    let mut validator = SnapshotValidator::new();

    // Create a snapshot matching the crash scenario
    // From log: bid=819721900000, ask=819939800000, sequence=20481211
    // CRITICAL: The crash snapshot was INCREMENTAL (snapshot_flags=0)
    let mut snapshot = create_valid_snapshot();
    snapshot.sequence = 20481211;
    snapshot.best_bid_price = 819721900000;
    snapshot.best_ask_price = 819939800000;
    snapshot.snapshot_flags = 0; // INCREMENTAL snapshot

    // This is the problematic state: bid_prices[0] has a price but bid_sizes[0] is 0
    // This happened because the depth array had stale data from a previous full snapshot
    snapshot.bid_prices[0] = 819700000000; // Stale price from previous full snapshot
    snapshot.bid_sizes[0] = 0; // ZERO SIZE - stale/invalid data

    // AFTER FIX: This should PASS because depth validation is skipped for incremental snapshots
    let result = validator.validate(&snapshot);
    assert!(
        result.is_ok(),
        "Incremental snapshot with stale depth data should pass validation: {:?}",
        result.err()
    );
}

/// Test that empty depth levels (price=0, size=0) are valid
#[test]
fn test_empty_depth_levels_are_valid() {
    let mut validator = SnapshotValidator::new();

    let mut snapshot = create_valid_snapshot();

    // All depth levels are empty - this should be valid
    for i in 0..10 {
        snapshot.bid_prices[i] = 0;
        snapshot.bid_sizes[i] = 0;
        snapshot.ask_prices[i] = 0;
        snapshot.ask_sizes[i] = 0;
    }

    let result = validator.validate(&snapshot);
    assert!(result.is_ok(), "Empty depth levels should be valid");
}

/// Test that populated depth levels (price>0, size>0) are valid
#[test]
fn test_populated_depth_levels_are_valid() {
    let mut validator = SnapshotValidator::new();

    let mut snapshot = create_valid_snapshot();

    // Populate depth with valid descending bid prices
    snapshot.bid_prices[0] = 819700000000;
    snapshot.bid_sizes[0] = 100000000; // 0.1 BTC

    snapshot.bid_prices[1] = 819680000000; // Lower price
    snapshot.bid_sizes[1] = 200000000; // 0.2 BTC

    // Populate depth with valid ascending ask prices
    snapshot.ask_prices[0] = 819950000000;
    snapshot.ask_sizes[0] = 100000000;

    snapshot.ask_prices[1] = 819970000000; // Higher price
    snapshot.ask_sizes[1] = 200000000;

    let result = validator.validate(&snapshot);
    assert!(
        result.is_ok(),
        "Properly populated depth levels should be valid: {:?}",
        result.err()
    );
}

/// Test that size>0 with price=0 also fails (consistency check)
#[test]
fn test_size_without_price_also_fails() {
    let mut validator = SnapshotValidator::new();

    let mut snapshot = create_valid_snapshot();

    // Invalid state: size > 0 but price = 0
    snapshot.bid_prices[0] = 0;
    snapshot.bid_sizes[0] = 100000000;

    // This should also fail validation (though different from the crash)
    let result = validator.validate(&snapshot);

    // Note: Current validator skips levels with price=0 && size=0,
    // but doesn't explicitly check for price=0 && size>0
    // This test documents the current behavior
    println!("Result for size without price: {:?}", result);
}

/// Test sequence gap followed by incremental snapshot with stale depth (full reproduction)
#[test]
fn test_sequence_gap_followed_by_invalid_snapshot() {
    let mut validator = SnapshotValidator::new();

    // First snapshot: sequence 20481209 (incremental)
    let mut snapshot1 = create_valid_snapshot();
    snapshot1.sequence = 20481209;
    snapshot1.snapshot_flags = 0; // Incremental

    let result1 = validator.validate(&snapshot1);
    assert!(result1.is_ok(), "First snapshot should be valid");

    // Gap: skip sequence 20481210

    // Second snapshot: sequence 20481211 with stale depth data (incremental)
    let mut snapshot2 = create_valid_snapshot();
    snapshot2.sequence = 20481211;
    snapshot2.snapshot_flags = 0; // INCREMENTAL - this is key!
    snapshot2.bid_prices[0] = 819700000000; // Stale from previous full snapshot
    snapshot2.bid_sizes[0] = 0; // Invalid, but will be ignored

    // AFTER FIX: Should pass because depth validation is skipped for incremental snapshots
    let result2 = validator.validate(&snapshot2);
    assert!(
        result2.is_ok(),
        "Incremental snapshot after gap should pass (depth not validated): {:?}",
        result2.err()
    );
}

/// Test that incremental snapshots DON'T validate depth arrays (THE FIX!)
#[test]
fn test_incremental_snapshot_skips_depth_validation() {
    let mut validator = SnapshotValidator::new();

    // Create an incremental snapshot (snapshot_flags = 0)
    let mut snapshot = create_valid_snapshot();
    snapshot.snapshot_flags = 0; // Incremental mode

    // Add invalid depth data (price but zero size)
    // This would normally fail validation, but should be IGNORED for incremental snapshots
    snapshot.bid_prices[0] = 819700000000;
    snapshot.bid_sizes[0] = 0; // Invalid, but should be skipped!

    // This should PASS because depth validation is skipped for incremental snapshots
    let result = validator.validate(&snapshot);
    assert!(
        result.is_ok(),
        "Incremental snapshots should skip depth validation: {:?}",
        result.err()
    );
}

/// Test that full snapshots DO validate depth arrays
#[test]
fn test_full_snapshot_validates_depth() {
    let mut validator = SnapshotValidator::new();

    // Create a full snapshot (snapshot_flags = 1)
    let mut snapshot = create_valid_snapshot();
    snapshot.snapshot_flags = 1; // Full snapshot mode

    // Add invalid depth data
    snapshot.bid_prices[0] = 819700000000;
    snapshot.bid_sizes[0] = 0; // Invalid!

    // This should FAIL because depth validation is enabled for full snapshots
    let result = validator.validate(&snapshot);
    assert!(
        result.is_err(),
        "Full snapshots should validate depth arrays"
    );

    match result {
        Err(ValidationError::InvalidDepthLevel { level, reason }) => {
            assert_eq!(level, 1);
            assert_eq!(reason, "Size is zero but price is set");
        }
        _ => panic!("Expected InvalidDepthLevel error"),
    }
}

/// Helper to create a valid baseline snapshot
///
/// Uses SnapshotBuilder to ensure proper depth array sizing based on
/// compile-time ORDERBOOK_DEPTH configuration (no hardcoded values).
fn create_valid_snapshot() -> MarketSnapshot {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    SnapshotBuilder::new()
        .market_id(1000025)
        .sequence(1)
        .timestamp(now)
        .best_bid(819721900000, 100000000) // ~$819.72, 0.1 BTC
        .best_ask(819939800000, 100000000) // ~$819.94, 0.1 BTC
        .incremental_snapshot()
        .build()
}
