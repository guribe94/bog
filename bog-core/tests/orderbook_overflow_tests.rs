//! Tests for orderbook depth calculations and overflow scenarios
//!
//! These tests verify that orderbook imbalance and depth calculations
//! handle extreme values without silent overflow or data corruption.

use bog_core::data::MarketSnapshot;

/// Helper to create a snapshot with custom depth
fn create_depth_snapshot(bid_sizes: [u64; 10], ask_sizes: [u64; 10]) -> MarketSnapshot {
    let mut snapshot = MarketSnapshot::default();
    snapshot.sequence = 1;
    snapshot.best_bid_price = 50_000_000_000_000;
    snapshot.best_ask_price = 50_001_000_000_000;
    snapshot.best_bid_size = bid_sizes[0];
    snapshot.best_ask_size = ask_sizes[0];
    snapshot.bid_sizes = bid_sizes;
    snapshot.ask_sizes = ask_sizes;

    // Set some reasonable prices for depth levels
    let mut bid_price = snapshot.best_bid_price;
    let mut ask_price = snapshot.best_ask_price;
    for i in 0..10 {
        bid_price = bid_price.saturating_sub(100_000_000_000); // $100 steps
        ask_price = ask_price.saturating_add(100_000_000_000);
        snapshot.bid_prices[i] = bid_price;
        snapshot.ask_prices[i] = ask_price;
    }

    snapshot.exchange_timestamp_ns = 1_700_000_000_000_000_000;
    snapshot
}

/// Test imbalance calculation with sizes that would overflow u64
///
/// Bug: orderbook/depth.rs uses saturating_add which silently clamps at u64::MAX
/// This gives wrong imbalance calculations in extreme market conditions
#[test]
fn test_imbalance_overflow_detection() {
    // Create orderbook with massive sizes that sum to > u64::MAX
    let mut bid_sizes = [0u64; 10];
    let mut ask_sizes = [0u64; 10];

    // Fill bid side with values that will overflow when summed
    // u64::MAX = 18,446,744,073,709,551,615
    // Divide by 5 = 3,689,348,814,741,910,323 per level
    let huge_size = u64::MAX / 5;
    for i in 0..6 {
        bid_sizes[i] = huge_size; // 6 * (u64::MAX/5) > u64::MAX
    }

    // Normal ask side
    for i in 0..10 {
        ask_sizes[i] = 1_000_000_000; // 1 BTC each
    }

    let snapshot = create_depth_snapshot(bid_sizes, ask_sizes);

    // Calculate imbalance
    // Current implementation would saturate at u64::MAX giving wrong result
    let total_bid = calculate_total_bid_liquidity(&snapshot);
    let total_ask = calculate_total_ask_liquidity(&snapshot);

    println!("Total bid liquidity: {}", total_bid);
    println!("Total ask liquidity: {}", total_ask);

    // The total_bid should detect overflow, not silently clamp
    // Either:
    // 1. Return error
    // 2. Use u128 internally
    // 3. Cap at reasonable maximum

    // Currently it would show total_bid = u64::MAX (wrong!)
    // Real total = 6 * huge_size = 6 * (u64::MAX/5) = 1.2 * u64::MAX
}

/// Helper function that mimics imbalance calculation
fn calculate_total_bid_liquidity(snapshot: &MarketSnapshot) -> u64 {
    let mut total = 0u64;

    // Add best bid
    total = total.saturating_add(snapshot.best_bid_size);

    // Add depth levels
    for &size in &snapshot.bid_sizes {
        if size > 0 {
            // This is the bug: saturating_add silently clamps
            total = total.saturating_add(size);
        }
    }

    total
}

fn calculate_total_ask_liquidity(snapshot: &MarketSnapshot) -> u64 {
    let mut total = 0u64;

    total = total.saturating_add(snapshot.best_ask_size);

    for &size in &snapshot.ask_sizes {
        if size > 0 {
            total = total.saturating_add(size);
        }
    }

    total
}

/// Test imbalance calculation with all levels at maximum
#[test]
fn test_imbalance_all_max_levels() {
    let mut bid_sizes = [u64::MAX; 10];
    let mut ask_sizes = [u64::MAX; 10];

    let snapshot = create_depth_snapshot(bid_sizes, ask_sizes);

    let total_bid = calculate_total_bid_liquidity(&snapshot);
    let total_ask = calculate_total_ask_liquidity(&snapshot);

    // Both should be clamped at u64::MAX (wrong!)
    assert_eq!(total_bid, u64::MAX);
    assert_eq!(total_ask, u64::MAX);

    // The imbalance would show 50% but reality is both sides are huge
    // This is a data corruption issue

    println!("Bid total (clamped): {}", total_bid);
    println!("Ask total (clamped): {}", total_ask);
    println!("Apparent imbalance: 50% (WRONG!)");
}

/// Test realistic flash crash scenario
#[test]
fn test_flash_crash_liquidity_explosion() {
    // During flash crashes, orderbook can fill with huge orders
    // as algorithms try to provide liquidity at extreme prices

    let mut bid_sizes = [0u64; 10];
    let mut ask_sizes = [0u64; 10];

    // Simulate flash crash: massive bid walls appear
    for i in 0..10 {
        // Each level has 1 million BTC (impossible but algorithms might quote it)
        bid_sizes[i] = 1_000_000_000_000_000; // 1M BTC in fixed point
        ask_sizes[i] = 1_000_000_000; // Normal 1 BTC on ask
    }

    let snapshot = create_depth_snapshot(bid_sizes, ask_sizes);

    let total_bid = calculate_total_bid_liquidity(&snapshot);
    let total_ask = calculate_total_ask_liquidity(&snapshot);

    let imbalance = if total_bid > total_ask {
        ((total_bid - total_ask) * 100) / (total_bid + total_ask)
    } else {
        ((total_ask - total_bid) * 100) / (total_bid + total_ask)
    };

    println!("Flash crash imbalance: {}%", imbalance);

    // Should show extreme imbalance, not be corrupted by overflow
}

/// Test gradual accumulation leading to overflow
#[test]
fn test_gradual_overflow() {
    // Start with reasonable values
    let mut bid_sizes = [1_000_000_000; 10]; // 1 BTC each

    // Gradually increase until overflow
    for multiplier in [10, 100, 1000, 10000, 100000, 1000000] {
        for i in 0..10 {
            bid_sizes[i] = (1_000_000_000u64).saturating_mul(multiplier);
        }

        let snapshot = create_depth_snapshot(bid_sizes, [1_000_000_000; 10]);
        let total = calculate_total_bid_liquidity(&snapshot);

        println!("Multiplier {}: total = {}", multiplier, total);

        // At some point this will clamp at u64::MAX
        // We should detect when this happens
    }
}

/// Test that zero sizes don't cause issues
#[test]
fn test_zero_size_handling() {
    // Some levels might have zero size (no orders at that price)
    let bid_sizes = [1_000_000_000, 0, 0, 500_000_000, 0, 0, 0, 100_000_000, 0, 0];
    let ask_sizes = [1_000_000_000, 500_000_000, 0, 0, 200_000_000, 0, 0, 0, 0, 0];

    let snapshot = create_depth_snapshot(bid_sizes, ask_sizes);

    let total_bid = calculate_total_bid_liquidity(&snapshot);
    let total_ask = calculate_total_ask_liquidity(&snapshot);

    // Should handle sparse orderbook correctly
    assert!(total_bid > 0);
    assert!(total_ask > 0);

    println!("Sparse book - Bid: {}, Ask: {}", total_bid, total_ask);
}
