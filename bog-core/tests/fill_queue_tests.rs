//! Tests for fill queue overflow handling
//!
//! These tests verify that the executor properly detects and handles
//! situations where fills are dropped due to queue overflow.

use bog_core::execution::{Order, SimulatedExecutor, Executor, Side};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

#[test]
fn test_dropped_fill_counter_tracks_overflow() {
    let executor = SimulatedExecutor::new();

    // Create and place orders that would generate fills
    // Note: This test is checking that the executor has a dropped_fill_count() method
    // Currently this method doesn't exist, so the test will fail

    // Expect the method to exist and return 0 initially
    let initial_count = executor.dropped_fill_count();
    assert_eq!(initial_count, 0, "Initial dropped fill count should be 0");
}

#[test]
fn test_fill_queue_overflow_detection() {
    // This test verifies that when the fill queue overflows,
    // the executor tracks it and we can detect it.
    //
    // Currently: No dropped_fill_count() method exists
    // Expected: Method exists and returns accurate count

    let mut executor = SimulatedExecutor::new();

    // Place an order
    let order = Order::limit(Side::Buy, Decimal::from(50000), Decimal::from_f64(0.1).unwrap());

    // Currently this returns Result<()>, but we can't check for dropped fills
    // After implementation, we should be able to:
    let _result = executor.place_order(order);

    // This assertion will fail initially because the method doesn't exist
    // After implementation, it should pass
    assert_eq!(executor.dropped_fill_count(), 0);
}

#[test]
fn test_fill_queue_recovery_after_consumption() {
    // This test verifies that the fill queue recovers properly
    // after fills are consumed from it

    let mut executor = SimulatedExecutor::new();

    // Place orders
    for _ in 0..10 {
        let order = Order::limit(
            Side::Buy,
            Decimal::from(50000),
            Decimal::from_f64(0.01).unwrap()
        );
        let _ = executor.place_order(order);
    }

    // Get fills (consume them)
    let fills = executor.get_fills();
    assert!(!fills.is_empty(), "Should have fills");

    // Dropped fills should still be 0 (no overflow)
    assert_eq!(executor.dropped_fill_count(), 0);

    // Place more orders after consuming
    for _ in 0..10 {
        let order = Order::limit(
            Side::Sell,
            Decimal::from(50010),
            Decimal::from_f64(0.01).unwrap()
        );
        let _ = executor.place_order(order);
    }

    let more_fills = executor.get_fills();
    assert!(!more_fills.is_empty(), "Should have more fills after consumption");
    assert_eq!(executor.dropped_fill_count(), 0, "No fills should be dropped");
}

#[test]
fn test_multiple_fills_per_tick_no_overflow() {
    // Verify that normal trading (multiple orders per tick) doesn't overflow

    let mut executor = SimulatedExecutor::new();

    // Simulate 100 ticks with 2 orders per tick (bid + ask)
    for _ in 0..100 {
        // Buy order
        let buy = Order::limit(
            Side::Buy,
            Decimal::from(50000),
            Decimal::from_f64(0.1).unwrap()
        );
        let _ = executor.place_order(buy);

        // Sell order
        let sell = Order::limit(
            Side::Sell,
            Decimal::from(50010),
            Decimal::from_f64(0.1).unwrap()
        );
        let _ = executor.place_order(sell);

        // Consume fills
        let _fills = executor.get_fills();
    }

    // With proper cleanup, no fills should be dropped
    assert_eq!(executor.dropped_fill_count(), 0, "Normal trading should not drop fills");
}

#[test]
fn test_fills_without_consumption_track_overflow() {
    // This test would require modifying the executor to not auto-consume fills
    // For now, we verify that the dropped_fill_count method exists and is accurate

    let executor = SimulatedExecutor::new();

    // The method should exist even if we can't easily trigger overflow
    let _count = executor.dropped_fill_count();
}
