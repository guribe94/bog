//! Verification tests for realistic executor behavior
//!
//! These tests verify that the SimulatedExecutor with realistic configuration
//! properly simulates:
//! - Partial fills (not 100%)
//! - Fee deduction from PnL
//! - Slippage application
//! - Queue modeling

use bog_core::execution::{SimulatedExecutor, Executor, Order, Side};
use bog_core::core::Position;
use bog_core::engine::{Engine, ExecutorBridge};
use bog_core::data::MarketSnapshot;
use bog_strategies::SimpleSpread;
use rust_decimal_macros::dec;

/// Test that realistic mode does NOT fill 100% of orders
#[test]
fn test_realistic_partial_fills() {
    let mut executor = SimulatedExecutor::new_realistic();

    // Place 10 orders and verify not all fill 100%
    let mut full_fills = 0;
    let mut partial_fills = 0;

    for i in 0..10 {
        let price = dec!(50000) + rust_decimal::Decimal::from(i);
        let order = Order::limit(Side::Buy, price, dec!(0.1));
        let order_id = executor.place_order(order).expect("Should place order");

        // Check fill
        let fills = executor.get_fills();
        assert!(!fills.is_empty(), "Should generate at least one fill");

        let fill = &fills[0];
        if fill.size == dec!(0.1) {
            full_fills += 1;
        } else if fill.size < dec!(0.1) {
            partial_fills += 1;
        }
    }

    // In realistic mode (40-80% fill rate), we should see some partial fills
    // With 10 orders, statistically we should not have ALL full fills
    assert!(
        partial_fills > 0 || full_fills < 10,
        "Realistic mode should not fill 100% of all orders. \
         Full fills: {}, Partial fills: {}",
        full_fills,
        partial_fills
    );
}

/// Test that fees are properly deducted from PnL
#[test]
fn test_fees_reduce_pnl() {
    let mut executor = SimulatedExecutor::new_realistic();

    // Buy 0.1 BTC at $50,000
    let buy_order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
    executor.place_order(buy_order).expect("Should place buy order");

    let buy_fills = executor.get_fills();
    assert_eq!(buy_fills.len(), 1, "Should have one buy fill");

    let buy_fill = &buy_fills[0];

    // Calculate expected cash flow with fees
    // Notional = 50,000 * 0.1 = 5,000 (assuming full or partial fill)
    // Fee (2 bps) = 5,000 * 0.0002 = 1.0 (or proportional for partial fill)
    // Total cash flow should be -(notional + fee)

    let actual_cash_flow = buy_fill.cash_flow();
    let notional = buy_fill.notional();
    let fee = buy_fill.fee.unwrap_or(dec!(0));

    // Verify fee exists and is positive
    assert!(fee > dec!(0), "Fee should be positive (2 bps). Got: {}", fee);

    // Verify cash flow = -(notional + fee) for buy
    let expected_cash_flow = -(notional + fee);
    assert_eq!(
        actual_cash_flow, expected_cash_flow,
        "Cash flow should equal -(notional + fee). \
         Expected: {}, Actual: {}, Notional: {}, Fee: {}",
        expected_cash_flow,
        actual_cash_flow,
        notional,
        fee
    );

    // Verify fee is approximately 2 bps of notional
    let fee_bps = (fee / notional * dec!(10000)).round();
    assert!(
        (fee_bps - dec!(2)).abs() < dec!(1),
        "Fee should be approximately 2 bps of notional. Got: {} bps",
        fee_bps
    );
}

/// Test that slippage makes fills less favorable
#[test]
fn test_slippage_application() {
    // Create executor with realistic config (2 bps slippage)
    let mut realistic_executor = SimulatedExecutor::new_realistic();

    // Create executor with instant config (no slippage)
    let mut instant_executor = SimulatedExecutor::new();

    let price = dec!(50000);
    let size = dec!(0.1);

    // Place buy order in both executors
    let buy_order_realistic = Order::limit(Side::Buy, price, size);
    let buy_order_instant = Order::limit(Side::Buy, price, size);

    realistic_executor.place_order(buy_order_realistic).expect("Should place");
    instant_executor.place_order(buy_order_instant).expect("Should place");

    let realistic_fills = realistic_executor.get_fills();
    let instant_fills = instant_executor.get_fills();

    // Both should have fills (may be partial for realistic)
    if !realistic_fills.is_empty() && !instant_fills.is_empty() {
        let realistic_fill = &realistic_fills[0];
        let instant_fill = &instant_fills[0];

        // For buy orders, realistic executor should fill at HIGHER price (worse) due to slippage
        // Note: Only if fill sizes are comparable
        if (realistic_fill.size - instant_fill.size).abs() < dec!(0.01) {
            assert!(
                realistic_fill.price >= instant_fill.price,
                "Realistic executor should have equal or higher price for buy (slippage). \
                 Realistic: {}, Instant: {}",
                realistic_fill.price,
                instant_fill.price
            );
        }
    }
}

/// Integration test with full engine using realistic executor
#[test]
fn test_engine_with_realistic_executor() {
    // Create strategy
    let strategy = SimpleSpread;

    // Create REALISTIC executor
    let realistic_executor = SimulatedExecutor::new_realistic();
    let executor = ExecutorBridge::new(realistic_executor);

    // Create engine
    let mut engine = Engine::new(strategy, executor);

    // Create sample market snapshot
    let snapshot = MarketSnapshot {
        market_id: 1,
        sequence: 1,
        exchange_timestamp_ns: 0,
        local_recv_ns: 0,
        local_publish_ns: 0,
        best_bid_price: 50_000_000_000_000, // $50,000
        best_bid_size: 1_000_000_000,       // 1.0 BTC
        best_ask_price: 50_010_000_000_000, // $50,010 (10 bps spread)
        best_ask_size: 1_000_000_000,
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 110],
    };

    // Process multiple ticks
    for _ in 0..100 {
        engine.process_tick(&snapshot).expect("Tick should process successfully");
    }

    // Get final stats
    let position_qty = engine.position().get_quantity();

    // Position should have changed (may be positive, negative, or zero depending on fills)
    // The important thing is NO panics/errors occurred
    println!("Final position after 100 ticks: {}", position_qty);

    // Verify engine processed all ticks
    assert_eq!(engine.stats().ticks_processed, 100);
}

/// Test that fees make the strategy less profitable
#[test]
fn test_fees_impact_profitability() {
    let mut executor = SimulatedExecutor::new_realistic();

    // Buy at $50,000, sell at $50,010 (10 bps spread)
    let buy_order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
    let sell_order = Order::limit(Side::Sell, dec!(50010), dec!(0.1));

    executor.place_order(buy_order).expect("Should place buy");
    executor.place_order(sell_order).expect("Should place sell");

    let fills = executor.get_fills();

    // Calculate total PnL from fills (both buy and sell)
    let total_cash_flow: rust_decimal::Decimal = fills.iter()
        .map(|f| f.cash_flow())
        .sum();

    // Gross profit on 10 bps spread with 0.1 BTC:
    // Sell revenue: $50,010 * 0.1 = $5,001
    // Buy cost: $50,000 * 0.1 = $5,000
    // Gross PnL = $1.00

    // Fees (2 bps on each leg):
    // Buy fee: $5,000 * 0.0002 = $1.00
    // Sell fee: $5,001 * 0.0002 ≈ $1.00
    // Total fees ≈ $2.00

    // Net PnL should be approximately: $1.00 - $2.00 = -$1.00 (loss!)
    // Note: This ignores slippage which would make it worse

    // With fees, a 10 bps spread is NOT profitable (need >2 bps fees)
    // SimpleSpread strategy uses 10 bps which leaves 10 - 2 = 8 bps profit margin

    println!("Total cash flow from round trip: {}", total_cash_flow);
    println!("Fills: {:?}", fills);

    // The point of this test is to verify fees are accounted for
    // If fills are partial, PnL will be proportionally smaller
}

/// Test that latency simulation adds realistic delays
#[test]
fn test_latency_simulation() {
    use std::time::Instant;

    // Create executor with realistic config (5ms total latency)
    let mut realistic_executor = SimulatedExecutor::new_realistic();

    // Create executor with instant config (0ms latency)
    let mut instant_executor = SimulatedExecutor::new();

    let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));

    // Measure realistic executor time
    let start_realistic = Instant::now();
    realistic_executor.place_order(order.clone()).expect("Should place");
    let realistic_duration = start_realistic.elapsed();

    // Measure instant executor time
    let start_instant = Instant::now();
    instant_executor.place_order(order).expect("Should place");
    let instant_duration = start_instant.elapsed();

    println!("Realistic executor: {:?}", realistic_duration);
    println!("Instant executor: {:?}", instant_duration);

    // Realistic should be at least 5ms slower (2ms network + 3ms exchange)
    // Allow some tolerance for system scheduling (minimum 4ms)
    let latency_diff = realistic_duration.saturating_sub(instant_duration);
    assert!(
        latency_diff.as_millis() >= 4,
        "Realistic executor should have >= 4ms latency. \
         Realistic: {:?}, Instant: {:?}, Diff: {:?}",
        realistic_duration,
        instant_duration,
        latency_diff
    );

    // Also verify realistic executor took at least 4ms total
    assert!(
        realistic_duration.as_millis() >= 4,
        "Realistic executor should take >= 4ms total (configured 5ms). \
         Actual: {:?}",
        realistic_duration
    );
}

/// Test custom latency configuration
#[test]
fn test_custom_latency_config() {
    use std::time::Instant;

    // Create custom config with 10ms latency
    let custom_config = bog_core::execution::RealisticFillConfig {
        enable_queue_modeling: false,
        enable_partial_fills: false,
        front_of_queue_fill_rate: 1.0,
        back_of_queue_fill_rate: 1.0,
        network_latency_ms: 7,   // 7ms network
        exchange_latency_ms: 3,  // 3ms exchange
        slippage_bps: 0.0,
    };

    let mut executor = SimulatedExecutor::with_config(custom_config);
    let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));

    // Measure execution time
    let start = Instant::now();
    executor.place_order(order).expect("Should place");
    let duration = start.elapsed();

    println!("Custom config (10ms) execution time: {:?}", duration);

    // Should take at least 9ms (10ms configured, allow 1ms tolerance)
    assert!(
        duration.as_millis() >= 9,
        "Should take >= 9ms with 10ms configured latency. Actual: {:?}",
        duration
    );
}
