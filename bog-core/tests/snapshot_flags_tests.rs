//! Snapshot Flags Tests
//!
//! Tests for handling IS_FULL_SNAPSHOT flag in MarketSnapshot.
//! The flag distinguishes between:
//! - Full snapshots: Complete orderbook (all 10 bid/ask levels valid)
//! - Incremental updates: Only top-of-book may have changed
//!
//! This affects orderbook state machine behavior.

use anyhow::Result;
use bog_core::data::{MarketSnapshot, SnapshotBuilder, ORDERBOOK_DEPTH};
use bog_core::orderbook::l2_book::L2OrderBook;

/// Helper to create a test snapshot with specific parameters
///
/// Uses SnapshotBuilder to ensure proper depth array sizing (no hardcoded values).
fn create_test_snapshot(sequence: u64, is_full: bool) -> MarketSnapshot {
    let mut builder = SnapshotBuilder::new()
        .market_id(1)
        .sequence(sequence)
        .timestamp(1_000_000_000_000)
        .best_bid(50_000_000_000_000, 1_000_000_000) // $50,000, 1.0 BTC
        .best_ask(50_010_000_000_000, 1_000_000_000); // $50,010, 1.0 BTC

    if is_full {
        // Populate all ORDERBOOK_DEPTH levels
        let mut bid_prices = Vec::with_capacity(ORDERBOOK_DEPTH);
        let mut bid_sizes = Vec::with_capacity(ORDERBOOK_DEPTH);
        let mut ask_prices = Vec::with_capacity(ORDERBOOK_DEPTH);
        let mut ask_sizes = Vec::with_capacity(ORDERBOOK_DEPTH);

        for i in 0..ORDERBOOK_DEPTH {
            bid_prices.push(50_000_000_000_000 - (i as u64 + 1) * 10_000_000_000); // $10 decrements
            bid_sizes.push((1_000_000_000 * (ORDERBOOK_DEPTH - i) as u64) / ORDERBOOK_DEPTH as u64);
            ask_prices.push(50_010_000_000_000 + (i as u64) * 10_000_000_000); // $10 increments
            ask_sizes.push((1_000_000_000 * (ORDERBOOK_DEPTH - i) as u64) / ORDERBOOK_DEPTH as u64);
        }

        builder.with_depth(&bid_prices, &bid_sizes, &ask_prices, &ask_sizes)
    } else {
        builder.incremental_snapshot().build()
    }
}

/// Test: snapshot_flag_full_set
///
/// Verifies that:
/// - Full snapshots have IS_FULL_SNAPSHOT flag = 1
/// - is_full_snapshot() method returns true
/// - Flag value is exactly 0x01
#[test]
fn test_snapshot_flag_full_set() {
    // Create a full snapshot
    let full_snapshot = create_test_snapshot(100, true);

    // Verify the flag is set correctly
    assert_eq!(
        full_snapshot.snapshot_flags & 0x01,
        0x01,
        "IS_FULL_SNAPSHOT flag not set in snapshot_flags"
    );

    // Verify the helper method works
    assert!(
        full_snapshot.is_full_snapshot(),
        "is_full_snapshot() should return true for full snapshot"
    );

    // Verify all levels are populated in a full snapshot
    for i in 0..10 {
        assert!(
            full_snapshot.bid_prices[i] > 0,
            "Bid price {} should be non-zero",
            i
        );
        assert!(
            full_snapshot.ask_prices[i] > 0,
            "Ask price {} should be non-zero",
            i
        );
    }
}

/// Test: snapshot_flag_incremental_clear
///
/// Verifies that:
/// - Incremental updates have IS_FULL_SNAPSHOT flag = 0
/// - is_full_snapshot() method returns false
/// - is_incremental() method returns true (if implemented)
#[test]
fn test_snapshot_flag_incremental_clear() {
    // Create an incremental snapshot
    let incremental_snapshot = create_test_snapshot(101, false);

    // Verify the flag is NOT set
    assert_eq!(
        incremental_snapshot.snapshot_flags & 0x01,
        0x00,
        "IS_FULL_SNAPSHOT flag should not be set for incremental"
    );

    // Verify the helper method works
    assert!(
        !incremental_snapshot.is_full_snapshot(),
        "is_full_snapshot() should return false for incremental"
    );

    // Verify we have an is_incremental() helper
    assert!(
        incremental_snapshot.is_incremental(),
        "is_incremental() should return true for incremental"
    );

    // Incremental updates typically only have top-of-book
    assert!(incremental_snapshot.best_bid_price > 0);
    assert!(incremental_snapshot.best_ask_price > 0);
}

/// Test: full_snapshot_triggers_rebuild
///
/// Verifies L2OrderBook behavior:
/// - Receives full snapshot
/// - L2OrderBook::sync_from_snapshot() detects IS_FULL_SNAPSHOT=1
/// - Performs full rebuild (clears state, loads all 10 levels)
#[test]
fn test_full_snapshot_triggers_rebuild() {
    let mut orderbook = L2OrderBook::new(1);

    // Put some initial state in the orderbook
    orderbook.bid_prices[0] = 1000;
    orderbook.bid_prices[9] = 999;
    orderbook.ask_prices[0] = 2000;
    orderbook.ask_prices[9] = 2999;

    // Create a full snapshot with different values
    let full_snapshot = create_test_snapshot(100, true);

    // Sync with full snapshot - should completely replace state
    orderbook.sync_from_snapshot(&full_snapshot);

    // Verify all 10 bid levels are from the snapshot (not old values)
    for i in 0..10 {
        assert_eq!(
            orderbook.bid_prices[i], full_snapshot.bid_prices[i],
            "Bid price {} not updated from full snapshot",
            i
        );
        assert_eq!(
            orderbook.bid_sizes[i], full_snapshot.bid_sizes[i],
            "Bid size {} not updated from full snapshot",
            i
        );
    }

    // Verify all 10 ask levels are from the snapshot
    for i in 0..10 {
        assert_eq!(
            orderbook.ask_prices[i], full_snapshot.ask_prices[i],
            "Ask price {} not updated from full snapshot",
            i
        );
        assert_eq!(
            orderbook.ask_sizes[i], full_snapshot.ask_sizes[i],
            "Ask size {} not updated from full snapshot",
            i
        );
    }

    // Verify sequence and timestamp updated
    assert_eq!(orderbook.last_sequence, full_snapshot.sequence);
    assert_eq!(
        orderbook.last_update_ns,
        full_snapshot.exchange_timestamp_ns
    );

    // Verify no stale data remains (old values 1000, 999, 2000, 2999 are gone)
    assert_ne!(orderbook.bid_prices[0], 1000);
    assert_ne!(orderbook.bid_prices[9], 999);
    assert_ne!(orderbook.ask_prices[0], 2000);
    assert_ne!(orderbook.ask_prices[9], 2999);
}

/// Test: incremental_update_preserves_depth
///
/// Verifies L2OrderBook behavior:
/// - Receives incremental update
/// - L2OrderBook::sync_from_snapshot() detects IS_FULL_SNAPSHOT=0
/// - Performs incremental update (only updates top-of-book)
/// - Preserves depth levels 2-10
#[test]
fn test_incremental_update_preserves_depth() {
    let mut orderbook = L2OrderBook::new(1);

    // First, populate with a full snapshot
    let full_snapshot = create_test_snapshot(100, true);
    orderbook.sync_from_snapshot(&full_snapshot);

    // Save the deep levels (2-9) for later comparison
    let saved_bid_levels: Vec<_> = (1..10)
        .map(|i| (orderbook.bid_prices[i], orderbook.bid_sizes[i]))
        .collect();
    let saved_ask_levels: Vec<_> = (1..10)
        .map(|i| (orderbook.ask_prices[i], orderbook.ask_sizes[i]))
        .collect();

    // Create an incremental update with different top-of-book
    let mut incremental = create_test_snapshot(101, false);
    incremental.best_bid_price = 50_001_000_000_000; // Changed
    incremental.best_bid_size = 2_000_000_000; // Changed
    incremental.best_ask_price = 50_009_000_000_000; // Changed
    incremental.best_ask_size = 3_000_000_000; // Changed

    // Apply incremental update
    orderbook.sync_from_snapshot(&incremental);

    // Verify top-of-book (level 0) was updated
    assert_eq!(
        orderbook.bid_prices[0], incremental.best_bid_price,
        "Best bid price not updated"
    );
    assert_eq!(
        orderbook.bid_sizes[0], incremental.best_bid_size,
        "Best bid size not updated"
    );
    assert_eq!(
        orderbook.ask_prices[0], incremental.best_ask_price,
        "Best ask price not updated"
    );
    assert_eq!(
        orderbook.ask_sizes[0], incremental.best_ask_size,
        "Best ask size not updated"
    );

    // Verify deeper levels (1-9) were preserved
    for i in 1..10 {
        assert_eq!(
            (orderbook.bid_prices[i], orderbook.bid_sizes[i]),
            saved_bid_levels[i - 1],
            "Bid level {} changed during incremental update",
            i
        );
        assert_eq!(
            (orderbook.ask_prices[i], orderbook.ask_sizes[i]),
            saved_ask_levels[i - 1],
            "Ask level {} changed during incremental update",
            i
        );
    }

    // Verify sequence updated
    assert_eq!(orderbook.last_sequence, incremental.sequence);
}

/// Test: full_snapshot_after_incremental
///
/// Verifies state machine transitions:
/// - Start with empty orderbook
/// - Receive incremental update (should not crash despite empty state)
/// - Receive full snapshot
/// - Verify correct rebuild
#[test]
fn test_full_snapshot_after_incremental() {
    let mut orderbook = L2OrderBook::new(1);

    // Verify orderbook starts empty
    assert_eq!(orderbook.bid_prices[0], 0);
    assert_eq!(orderbook.ask_prices[0], 0);

    // Apply incremental update to empty orderbook (should not crash)
    let incremental = create_test_snapshot(50, false);
    orderbook.sync_from_snapshot(&incremental);

    // Verify top-of-book was updated even on empty orderbook
    assert_eq!(orderbook.bid_prices[0], incremental.best_bid_price);
    assert_eq!(orderbook.ask_prices[0], incremental.best_ask_price);

    // Deeper levels should still be empty
    assert_eq!(orderbook.bid_prices[9], 0);
    assert_eq!(orderbook.ask_prices[9], 0);

    // Now receive a full snapshot
    let full_snapshot = create_test_snapshot(100, true);
    orderbook.sync_from_snapshot(&full_snapshot);

    // Verify complete rebuild with all levels
    for i in 0..10 {
        assert_eq!(orderbook.bid_prices[i], full_snapshot.bid_prices[i]);
        assert_eq!(orderbook.ask_prices[i], full_snapshot.ask_prices[i]);
        assert!(
            orderbook.bid_prices[i] > 0,
            "Level {} should be populated",
            i
        );
        assert!(
            orderbook.ask_prices[i] > 0,
            "Level {} should be populated",
            i
        );
    }
}

/// Test: incremental_after_full
///
/// Verifies normal flow:
/// - Receive full snapshot
/// - Receive incremental updates
/// - Verify prices update while deeper levels remain stable
#[test]
fn test_incremental_after_full() {
    let mut orderbook = L2OrderBook::new(1);

    // Start with full snapshot (normal initialization)
    let full_snapshot = create_test_snapshot(100, true);
    orderbook.sync_from_snapshot(&full_snapshot);

    // Verify all levels populated
    for i in 0..10 {
        assert!(orderbook.bid_prices[i] > 0);
        assert!(orderbook.ask_prices[i] > 0);
    }

    // Save deep level 9 for comparison
    let saved_bid_9 = orderbook.bid_prices[9];
    let saved_ask_9 = orderbook.ask_prices[9];

    // Apply a series of incremental updates
    for seq in 101..110 {
        let mut incremental = create_test_snapshot(seq, false);
        // Vary the top-of-book prices
        incremental.best_bid_price = 50_000_000_000_000 + seq * 1_000_000_000;
        incremental.best_ask_price = 50_010_000_000_000 + seq * 1_000_000_000;

        orderbook.sync_from_snapshot(&incremental);

        // Verify top-of-book updated
        assert_eq!(orderbook.bid_prices[0], incremental.best_bid_price);
        assert_eq!(orderbook.ask_prices[0], incremental.best_ask_price);

        // Verify deep levels unchanged
        assert_eq!(
            orderbook.bid_prices[9], saved_bid_9,
            "Deep bid changed at seq {}",
            seq
        );
        assert_eq!(
            orderbook.ask_prices[9], saved_ask_9,
            "Deep ask changed at seq {}",
            seq
        );
    }
}

/// Test: multiple_incremental_updates
///
/// Verifies handling of multiple incremental updates:
/// - No degradation in performance
/// - State remains consistent
/// - Deeper levels eventually update (after getting new full snapshot)
#[test]
fn test_multiple_incremental_updates() {
    let mut orderbook = L2OrderBook::new(1);

    // Initialize with full snapshot
    let full_snapshot = create_test_snapshot(1000, true);
    orderbook.sync_from_snapshot(&full_snapshot);

    let initial_deep_bid = orderbook.bid_prices[5];
    let initial_deep_ask = orderbook.ask_prices[5];

    // Apply many incremental updates
    for seq in 1001..1200 {
        let mut incremental = create_test_snapshot(seq, false);

        // Simulate realistic price movement
        let price_offset = ((seq - 1000) as i64 - 100) * 100_000_000; // Oscillate around mid
        incremental.best_bid_price = (50_000_000_000_000 as i64 + price_offset) as u64;
        incremental.best_ask_price = (50_010_000_000_000 as i64 + price_offset) as u64;

        orderbook.sync_from_snapshot(&incremental);

        // Verify orderbook remains valid (no crossed book)
        assert!(
            orderbook.bid_prices[0] < orderbook.ask_prices[0],
            "Crossed orderbook at seq {}",
            seq
        );

        // Verify sequence is updating
        assert_eq!(orderbook.last_sequence, seq);
    }

    // After many incrementals, deep levels should still be from original full snapshot
    assert_eq!(
        orderbook.bid_prices[5], initial_deep_bid,
        "Deep bid changed without full snapshot"
    );
    assert_eq!(
        orderbook.ask_prices[5], initial_deep_ask,
        "Deep ask changed without full snapshot"
    );

    // Now receive a new full snapshot - deep levels should update
    let mut new_full_snapshot = create_test_snapshot(2000, true);

    // Change deep levels to ensure we can detect the update
    new_full_snapshot.bid_prices[5] -= 1_000_000_000;
    new_full_snapshot.ask_prices[5] += 1_000_000_000;

    orderbook.sync_from_snapshot(&new_full_snapshot);

    // Verify deep levels are now different
    assert_ne!(
        orderbook.bid_prices[5], initial_deep_bid,
        "Deep bid not updated by new full snapshot"
    );
    assert_ne!(
        orderbook.ask_prices[5], initial_deep_ask,
        "Deep ask not updated by new full snapshot"
    );

    // All levels should match the new snapshot
    for i in 0..10 {
        assert_eq!(orderbook.bid_prices[i], new_full_snapshot.bid_prices[i]);
        assert_eq!(orderbook.ask_prices[i], new_full_snapshot.ask_prices[i]);
    }
}

/// Test: orderbook_state_after_rebuild
///
/// Verifies orderbook state after various operations:
/// - State is consistent
/// - Methods like mid_price(), spread_bps() work correctly
/// - No invalid states after transitions
#[test]
fn test_orderbook_state_after_rebuild() {
    let mut orderbook = L2OrderBook::new(1);

    // Initialize with full snapshot
    let full_snapshot = create_test_snapshot(100, true);
    orderbook.sync_from_snapshot(&full_snapshot);

    // Check state is valid
    let mid_price = orderbook.mid_price();
    assert!(mid_price > 0, "Mid price should be positive");

    let spread_bps = orderbook.spread_bps();
    assert!(
        spread_bps > 0 && spread_bps < 1000,
        "Spread should be reasonable"
    );

    let imbalance = orderbook.imbalance();
    assert!(
        imbalance >= -100 && imbalance <= 100,
        "Imbalance should be in range"
    );

    // Apply incremental update
    let incremental = create_test_snapshot(101, false);
    orderbook.sync_from_snapshot(&incremental);

    // State should still be valid
    let new_mid_price = orderbook.mid_price();
    assert!(
        new_mid_price > 0,
        "Mid price should remain positive after incremental"
    );

    // Apply another full snapshot
    let new_full = create_test_snapshot(200, true);
    orderbook.sync_from_snapshot(&new_full);

    // State should be completely replaced and valid
    assert_eq!(orderbook.last_sequence, 200);
    assert!(orderbook.mid_price() > 0);
    assert!(orderbook.spread_bps() > 0);
}

/// Test: sync_from_snapshot_idempotent
///
/// Verifies that applying the same snapshot twice has no ill effects
#[test]
fn test_sync_from_snapshot_idempotent() {
    let mut orderbook = L2OrderBook::new(1);

    let full_snapshot = create_test_snapshot(100, true);

    // Apply snapshot once
    orderbook.sync_from_snapshot(&full_snapshot);
    let state_after_first = orderbook.clone();

    // Apply same snapshot again
    orderbook.sync_from_snapshot(&full_snapshot);

    // State should be identical
    assert_eq!(orderbook.bid_prices, state_after_first.bid_prices);
    assert_eq!(orderbook.ask_prices, state_after_first.ask_prices);
    assert_eq!(orderbook.last_sequence, state_after_first.last_sequence);
}

/// Property test: full_snapshot_has_all_levels
///
/// Any full snapshot must have all 10 levels populated
#[test]
fn test_full_snapshot_has_all_levels() {
    // Test with various sequences
    for seq in [0, 1, 100, 1000, u64::MAX - 1, u64::MAX] {
        let full_snapshot = create_test_snapshot(seq, true);

        // All levels must be populated
        for i in 0..10 {
            assert!(
                full_snapshot.bid_prices[i] > 0,
                "Full snapshot seq {} missing bid level {}",
                seq,
                i
            );
            assert!(
                full_snapshot.ask_prices[i] > 0,
                "Full snapshot seq {} missing ask level {}",
                seq,
                i
            );
        }
    }
}

/// Test: all_zero_snapshot
///
/// Verifies handling of pathological case where snapshot has all zeros
#[test]
fn test_all_zero_snapshot() {
    let mut orderbook = L2OrderBook::new(1);

    // Start with valid state
    let valid_snapshot = create_test_snapshot(100, true);
    orderbook.sync_from_snapshot(&valid_snapshot);

    // Create all-zero snapshot (invalid but shouldn't crash)
    let zero_snapshot = SnapshotBuilder::new()
        .market_id(1)
        .sequence(101)
        .timestamp(0)
        .best_bid(0, 0)
        .best_ask(0, 0)
        .full_snapshot()
        .build();

    // This should be handled gracefully (not crash)
    orderbook.sync_from_snapshot(&zero_snapshot);

    // Orderbook should have zero values now
    assert_eq!(orderbook.bid_prices[0], 0);
    assert_eq!(orderbook.ask_prices[0], 0);
}

/// Test: crossed_orderbook_snapshot
///
/// Verifies handling of crossed orderbook in snapshot
#[test]
fn test_crossed_orderbook_snapshot() {
    let mut orderbook = L2OrderBook::new(1);

    // Create a crossed snapshot (bid > ask - invalid)
    let mut crossed_snapshot = create_test_snapshot(100, true);
    crossed_snapshot.best_bid_price = 50_020_000_000_000; // $50,020
    crossed_snapshot.best_ask_price = 50_010_000_000_000; // $50,010 - CROSSED!
    crossed_snapshot.bid_prices[0] = crossed_snapshot.best_bid_price;
    crossed_snapshot.ask_prices[0] = crossed_snapshot.best_ask_price;

    // Apply the crossed snapshot (orderbook should accept it, validation happens elsewhere)
    orderbook.sync_from_snapshot(&crossed_snapshot);

    // Verify the crossed state was stored
    assert_eq!(orderbook.bid_prices[0], crossed_snapshot.best_bid_price);
    assert_eq!(orderbook.ask_prices[0], crossed_snapshot.best_ask_price);
    assert!(
        orderbook.bid_prices[0] > orderbook.ask_prices[0],
        "Crossed state preserved"
    );

    // The circuit breaker or validator should catch this, not the orderbook itself
    // Orderbook is just a data structure, validation is a separate concern
}
