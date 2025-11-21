//! Edge case tests for position and PnL calculations
//!
//! These tests verify that position tracking and PnL calculations
//! handle corrupted or edge case data without panicking.

use bog_core::core::Position;

/// Test PnL percentage calculation with zero position value
///
/// Bug: risk/mod.rs line 207 divides by position_value without proper error handling
/// Comment says "would indicate critical bug" but doesn't handle it gracefully
#[test]
fn test_pnl_percentage_with_zero_position_value() {
    let position = Position::new();

    // Simulate a corrupted position state where quantity exists but value is zero
    // This could happen due to:
    // 1. Bug in fill processing
    // 2. Entry price corruption
    // 3. Overflow in calculations

    // Set quantity but corrupt the entry price
    position.quantity.store(1_000_000_000, std::sync::atomic::Ordering::Release); // 1 BTC
    position.entry_price.store(0, std::sync::atomic::Ordering::Release); // Corrupted!

    // Now if we try to calculate PnL percentage, it would divide by zero
    // position_value = quantity * entry_price = 1BTC * 0 = 0
    // pnl_percentage = pnl / position_value = X / 0 = PANIC!

    // This should be handled gracefully, not panic
    // Since we don't have direct access to risk manager here, we test the position methods
    let unrealized_pnl = position.get_unrealized_pnl(50_000_000_000_000);

    // The calculation should handle the zero entry price case
    // Either return 0 or a sentinel value, but NOT panic
    println!("Unrealized PnL with zero entry: {}", unrealized_pnl);
}

/// Test position calculations with extreme values
#[test]
fn test_position_extreme_values() {
    let position = Position::new();

    // Test with maximum possible position
    let max_qty = i64::MAX;
    let max_price = u64::MAX / 1_000_000_000; // Max that fits in fixed point

    // This should not panic or overflow
    position.quantity.store(max_qty, std::sync::atomic::Ordering::Release);
    position.entry_price.store(max_price, std::sync::atomic::Ordering::Release);

    // Calculate PnL at current price
    let current_price = max_price / 2; // Price dropped by half
    let pnl = position.get_unrealized_pnl(current_price);

    println!("PnL at extreme values: {}", pnl);

    // Should handle without overflow/panic
}

/// Test position value calculation edge cases
#[test]
fn test_position_value_edge_cases() {
    let position = Position::new();

    // Test case 1: Zero quantity, non-zero entry price
    position.quantity.store(0, std::sync::atomic::Ordering::Release);
    position.entry_price.store(50_000_000_000_000, std::sync::atomic::Ordering::Release);

    // Position value should be 0, not cause issues
    let qty = position.get_quantity();
    let entry = position.get_entry_price();

    // Value = qty * entry / SCALE
    // = 0 * 50000 / 1e9 = 0
    assert_eq!(qty, 0);
    assert_eq!(entry, 50_000_000_000_000);

    // Test case 2: Negative quantity (short position)
    position.quantity.store(-1_000_000_000, std::sync::atomic::Ordering::Release);

    // Should handle negative positions properly
    let qty = position.get_quantity();
    assert_eq!(qty, -1_000_000_000);
}

/// Test that position handles fill processing with overflow protection
#[test]
fn test_position_fill_overflow_protection() {
    let position = Position::new();

    // Start with a large position
    let large_qty = i64::MAX / 2;
    position.quantity.store(large_qty, std::sync::atomic::Ordering::Release);

    // Try to add another large position (would overflow)
    let result = position.process_fill_fixed(
        0, // Buy side
        50_000_000_000_000, // Price
        (i64::MAX / 2) as u64, // Size that would cause overflow
    );

    // Should detect overflow and return error
    assert!(result.is_err());

    // Position should remain unchanged after failed update
    assert_eq!(position.get_quantity(), large_qty);
}

/// Test weighted average entry price calculation
#[test]
fn test_entry_price_weighted_average_overflow() {
    let position = Position::new();

    // Scenario that could overflow in weighted average calculation:
    // Large existing position at high price
    let huge_price = u64::MAX / 1_000_000_000 - 1; // Near max
    let huge_qty = u64::MAX / 1_000_000_000 / 2; // Large quantity

    // First fill
    let result = position.process_fill_fixed(0, huge_price, huge_qty);
    assert!(result.is_ok());

    // Second fill at similar price (weighted average calculation)
    let result2 = position.process_fill_fixed(0, huge_price, huge_qty);

    // The weighted average calculation:
    // (old_entry * old_qty + new_price * new_qty) / total_qty
    // Could overflow in the numerator

    // Should either handle with u128 internally or return error
    println!("Second fill result: {:?}", result2);
}

/// Test that corrupted position state is detected
#[test]
fn test_corrupted_position_detection() {
    let position = Position::new();

    // Create an inconsistent state:
    // - Positive quantity but negative PnL that doesn't make sense
    // - Or quantity that doesn't match fill history

    position.quantity.store(1_000_000_000, std::sync::atomic::Ordering::Release);
    position.entry_price.store(50_000_000_000_000, std::sync::atomic::Ordering::Release);
    position.realized_pnl.store(i64::MIN, std::sync::atomic::Ordering::Release); // Corrupted

    // Getting metrics should detect inconsistency
    let realized = position.get_realized_pnl();
    let daily = position.get_daily_pnl();

    println!("Realized PnL (corrupted): {}", realized);
    println!("Daily PnL: {}", daily);

    // The system should handle this gracefully
    // Either detect corruption or have bounds checks
}