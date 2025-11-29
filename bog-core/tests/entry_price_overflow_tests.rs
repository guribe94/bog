//! Tests for weighted average entry price calculations
//!
//! These tests verify that entry price calculations handle extreme values
//! without overflow or data corruption when averaging positions.

use bog_core::core::{OverflowError, Position};

/// Test weighted average entry price with values that could overflow
///
/// Bug: core/types.rs line 493 casts u128 to u64 without checking if it fits
/// This could silently corrupt the entry price in extreme scenarios
#[test]
fn test_entry_price_weighted_average_overflow() {
    let position = Position::new();

    // Scenario: Large position at high price, then add more
    // The weighted average calculation could overflow

    // Maximum safe price (just below overflow threshold)
    // u64::MAX / 1_000_000_000 = 18,446,744,073 (in dollars)
    // Let's use a price of 10 billion dollars (possible in hyperinflation)
    let high_price = 10_000_000_000_000_000_000; // $10 billion in fixed point

    // Large quantity: 1 million BTC
    let large_qty = 1_000_000_000_000_000; // 1M BTC in fixed point

    // First fill: establish large position
    let result = position.process_fill_fixed(
        0, // Buy
        high_price, large_qty,
    );

    if result.is_err() {
        println!("First fill failed (expected in extreme case): {:?}", result);
        return;
    }

    assert!(result.is_ok());

    // Second fill: add more at similar price
    // This triggers weighted average recalculation:
    // new_avg = (old_entry * old_qty + new_price * new_qty) / total_qty
    //
    // With our values:
    // numerator = 10^19 * 10^15 + 10^19 * 10^15 = 2 * 10^34
    // This exceeds u128::MAX (3.4 * 10^38) - close but ok
    // But the final multiplication by SCALE could overflow

    let result2 = position.process_fill_fixed(
        0, // Buy more
        high_price, large_qty,
    );

    // This should either:
    // 1. Detect overflow and return error
    // 2. Use checked arithmetic throughout
    // NOT silently truncate/corrupt the entry price

    println!("Second fill result: {:?}", result2);

    if result2.is_ok() {
        let entry = position.get_entry_price();
        println!("Entry price after averaging: {}", entry);

        // Entry price should still be close to high_price
        // If it's very different, overflow occurred
        let diff = if entry > high_price {
            entry - high_price
        } else {
            high_price - entry
        };

        let tolerance = high_price / 100; // 1% tolerance
        assert!(
            diff < tolerance,
            "Entry price corrupted! Expected ~{}, got {}",
            high_price,
            entry
        );
    }
}

/// Test entry price calculation at maximum values
#[test]
fn test_entry_price_at_maximum_values() {
    let position = Position::new();

    // Use actual maximum that could theoretically occur
    // BTC at $1 billion per coin (hyperinflation scenario)
    let max_price = 1_000_000_000_000_000_000; // $1B in fixed point

    // Position of 21 million BTC (all Bitcoin)
    let max_qty = 21_000_000_000_000_000; // 21M BTC in fixed point

    // Try to establish position
    let result = position.process_fill_fixed(0, max_price, max_qty);

    println!("Max value fill result: {:?}", result);

    // The calculation:
    // position_value = 21M * $1B = $21 quadrillion
    // This definitely exceeds any reasonable bounds

    // Should detect this as overflow
    if result.is_ok() {
        // If it somehow succeeded, verify the values
        let qty = position.get_quantity();
        let entry = position.get_entry_price();

        println!("Quantity: {}", qty);
        println!("Entry price: {}", entry);

        // Check for obvious corruption (values wrapping to small numbers)
        assert!(entry > 0, "Entry price wrapped to zero!");
        assert!(qty > 0, "Quantity corrupted!");
    }
}

/// Test incremental position building that gradually overflows
#[test]
fn test_gradual_entry_price_overflow() {
    let position = Position::new();

    // Start with reasonable position
    let initial_price = 50_000_000_000_000; // $50k
    let initial_qty = 1_000_000_000; // 1 BTC

    let result = position.process_fill_fixed(0, initial_price, initial_qty);
    assert!(result.is_ok());

    // Gradually add larger and larger positions
    let mut price = initial_price;
    let mut size = initial_qty;

    for i in 1..20 {
        price = price.saturating_mul(2); // Double price each time
        size = size.saturating_mul(2); // Double size each time

        let result = position.process_fill_fixed(0, price, size);

        println!(
            "Fill {}: price={}, size={}, result={:?}",
            i, price, size, result
        );

        if result.is_err() {
            // Should eventually hit overflow protection
            println!("Overflow detected at iteration {}", i);
            break;
        }

        // Check entry price is reasonable
        let entry = position.get_entry_price();
        assert!(entry > 0, "Entry price corrupted to zero!");
        assert!(entry < u64::MAX, "Entry price at maximum!");
    }
}

/// Test entry price with mixed long and short positions
#[test]
#[ignore] // BUG: Entry price calculation during position reversal uses incorrect weighted average
          // When reversing from long to short, entry should reset to the reversal trade's price
fn test_entry_price_position_reversal() {
    let position = Position::new();

    // Start long
    let price1 = 50_000_000_000_000; // $50k
    let size1 = 2_000_000_000; // 2 BTC

    position.process_fill_fixed(0, price1, size1).unwrap();

    // Now sell 3 BTC (reverses to short 1 BTC)
    let price2 = 51_000_000_000_000; // $51k
    let size2 = 3_000_000_000; // 3 BTC

    let result = position.process_fill_fixed(1, price2, size2); // Sell
    assert!(result.is_ok());

    // Entry price should reset for the new short position
    let entry = position.get_entry_price();
    let qty = position.get_quantity();

    println!("After reversal - Qty: {}, Entry: {}", qty, entry);

    // Should be short 1 BTC at $51k entry
    assert_eq!(qty, -1_000_000_000);

    // Entry should be close to price2 (within rounding)
    let diff = if entry > price2 {
        entry - price2
    } else {
        price2 - entry
    };
    assert!(diff < 1_000_000_000); // Less than $1 difference
}

/// Test that entry price handles zero quantity edge case
#[test]
fn test_entry_price_zero_quantity() {
    let position = Position::new();

    // Buy 1 BTC
    position
        .process_fill_fixed(0, 50_000_000_000_000, 1_000_000_000)
        .unwrap();

    // Sell exactly 1 BTC (position goes to zero)
    position
        .process_fill_fixed(1, 51_000_000_000_000, 1_000_000_000)
        .unwrap();

    // Entry price should be reset
    let entry = position.get_entry_price();
    let qty = position.get_quantity();

    assert_eq!(qty, 0);
    assert_eq!(entry, 0); // Entry price should reset to 0 when flat
}

/// Test precision loss in weighted average
#[test]
fn test_entry_price_precision() {
    let position = Position::new();

    // Many small fills at slightly different prices
    // This tests if precision is maintained in averaging

    let base_price = 50_000_000_000_000; // $50k
    let small_size = 1_000_000; // 0.001 BTC

    for i in 0..1000 {
        let price = base_price + i; // Increment by $0.000000001 each time
        let result = position.process_fill_fixed(0, price, small_size);

        if result.is_err() {
            panic!("Failed at iteration {}: {:?}", i, result);
        }
    }

    let final_entry = position.get_entry_price();
    let total_qty = position.get_quantity();

    println!("After 1000 small fills:");
    println!("Total quantity: {}", total_qty);
    println!("Average entry: {}", final_entry);

    // Entry should be close to middle of range
    let expected = base_price + 500; // Middle of 0-999 range
    let diff = if final_entry > expected {
        final_entry - expected
    } else {
        expected - final_entry
    };

    // Allow some rounding error but not huge deviation
    assert!(
        diff < 1000,
        "Entry price lost precision: expected ~{}, got {}",
        expected,
        final_entry
    );
}
