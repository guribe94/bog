//! Integration tests for depth-aware trading strategies
//!
//! These tests verify the full tick-to-trade pipeline with orderbook depth data.

use bog_core::core::{Position, Signal, SignalAction};
use bog_core::data::MarketSnapshot;
use bog_core::engine::{Engine, SimulatedExecutor, Strategy};
use bog_strategies::SimpleSpread;

// Test helper to create snapshot with depth
fn create_depth_snapshot(
    best_bid: u64,
    best_ask: u64,
    depth_levels: usize,
    tick_size: u64,
    size_per_level: u64,
) -> MarketSnapshot {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let mut snapshot = MarketSnapshot {
        market_id: 1,
        sequence: 1,
        exchange_timestamp_ns: now_ns,
        local_recv_ns: now_ns,
        local_publish_ns: now_ns,
        best_bid_price: best_bid,
        best_bid_size: size_per_level,
        best_ask_price: best_ask,
        best_ask_size: size_per_level,
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 110],
    };

    // Populate depth
    for level in 0..depth_levels.min(10) {
        snapshot.bid_prices[level] = best_bid.saturating_sub(level as u64 * tick_size);
        snapshot.bid_sizes[level] = size_per_level;
        snapshot.ask_prices[level] = best_ask + (level as u64 * tick_size);
        snapshot.ask_sizes[level] = size_per_level;
    }

    snapshot
}

#[test]
#[ignore]  // Will pass once depth features are implemented
fn test_depth_aware_strategy_end_to_end() {
    // Create depth-aware strategy
    let strategy = SimpleSpread;
    let executor = SimulatedExecutor::new_default();
    let mut engine = Engine::new(strategy, executor);

    // Create a feed function that provides depth data
    let mut tick_count = 0u64;
    let mut snapshots_generated = 0;

    let feed_fn = || {
        tick_count += 1;

        if tick_count > 100 {
            return Ok((None, false)); // Stop after 100 ticks
        }

        // Generate snapshot with 5 levels of depth
        let snapshot = create_depth_snapshot(
            50_000_000_000_000 + (tick_count * 1_000_000_000),  // Slowly rising bid
            50_010_000_000_000 + (tick_count * 1_000_000_000),  // Slowly rising ask
            5,                      // 5 levels
            5_000_000_000,         // $5 tick
            100_000_000,           // 0.1 BTC per level
        );

        snapshots_generated += 1;
        Ok((Some(snapshot), true))
    };

    // Run engine
    let stats = engine.run(feed_fn).expect("Engine should run successfully");

    // Verify engine processed snapshots
    assert!(stats.ticks_processed > 0, "Should have processed ticks");
    assert_eq!(snapshots_generated, stats.ticks_processed);

    // With depth-aware strategy, should generate signals
    assert!(
        stats.signals_generated > 0,
        "Depth-aware strategy should generate signals"
    );
}

#[test]
#[ignore]  // Will pass once depth features are implemented
fn test_depth_strategy_performance() {
    // Verify depth-aware strategy still meets performance targets
    let mut strategy = SimpleSpread;

    let snapshot = create_depth_snapshot(
        50_000_000_000_000,
        50_010_000_000_000,
        10,  // All 10 levels
        5_000_000_000,
        100_000_000,
    );

    // Warm up
    for _ in 0..100 {
        let _ = strategy.calculate(&snapshot);
    }

    // Measure performance (rough check)
    use std::time::Instant;
    let iterations = 10_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let _ = strategy.calculate(&snapshot);
    }

    let elapsed = start.elapsed();
    let avg_ns = elapsed.as_nanos() / iterations;

    // Should still be under 100ns target even with depth calculations
    assert!(
        avg_ns < 100,
        "Depth-aware strategy should be <100ns (got {}ns)",
        avg_ns
    );
}

#[test]
#[ignore]  // Will pass once depth features are implemented
fn test_depth_strategy_zst_property() {
    // Verify SimpleSpread remains zero-sized even with depth features
    let strategy = SimpleSpread;

    assert_eq!(
        std::mem::size_of_val(&strategy),
        0,
        "SimpleSpread must remain zero-sized type"
    );

    assert_eq!(
        std::mem::size_of::<SimpleSpread>(),
        0,
        "SimpleSpread type must be 0 bytes"
    );
}

#[test]
#[ignore]  // Will pass once depth features are implemented
fn test_depth_aware_with_varying_imbalance() {
    // Test strategy adapts to varying orderbook imbalance
    let mut strategy = SimpleSpread;

    // Scenario 1: Bullish imbalance (3x more bids)
    let bullish_snapshot = {
        let mut snap = create_depth_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            5,
            5_000_000_000,
            100_000_000,
        );

        // Make bid side 3x larger
        for i in 0..5 {
            snap.bid_sizes[i] = 300_000_000;  // 0.3 BTC
        }

        snap
    };

    let signal_bullish = strategy.calculate(&bullish_snapshot);
    assert!(signal_bullish.is_some(), "Should generate signal on bullish book");

    // Scenario 2: Bearish imbalance (3x more asks)
    let bearish_snapshot = {
        let mut snap = create_depth_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            5,
            5_000_000_000,
            100_000_000,
        );

        // Make ask side 3x larger
        for i in 0..5 {
            snap.ask_sizes[i] = 300_000_000;  // 0.3 BTC
        }

        snap
    };

    let signal_bearish = strategy.calculate(&bearish_snapshot);
    assert!(signal_bearish.is_some(), "Should generate signal on bearish book");

    // With imbalance detection, quotes should differ
    // (This will be verified when implementation is complete)
}

#[test]
#[ignore]  // Will pass once depth features are implemented
fn test_depth_aware_multi_tick_accumulation() {
    // Test position accumulation over multiple ticks with depth data
    let strategy = SimpleSpread;
    let executor = SimulatedExecutor::new_default();
    let mut engine = Engine::new(strategy, executor);

    let mut tick_count = 0;

    let feed_fn = || {
        tick_count += 1;

        if tick_count > 50 {
            return Ok((None, false));
        }

        // Generate depth snapshots with varying market conditions
        let snapshot = create_depth_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            5,
            5_000_000_000,
            100_000_000,
        );

        Ok((Some(snapshot), true))
    };

    let stats = engine.run(feed_fn).expect("Engine run should succeed");

    // Should process all ticks
    assert_eq!(stats.ticks_processed, 50);

    // Should generate signals on good market data
    assert!(stats.signals_generated > 0);
}

#[test]
#[ignore]  // Will pass once depth features are implemented
fn test_depth_strategy_handles_sparse_depth() {
    // Test that strategy handles sparse depth gracefully
    let mut strategy = SimpleSpread;

    // Create snapshot with only some levels populated
    let sparse_snapshot = {
        let mut snap = create_depth_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            10,
            5_000_000_000,
            0,  // Start with zero sizes
        );

        // Set top-of-book sizes (required for validation)
        snap.best_bid_size = 100_000_000;
        snap.best_ask_size = 100_000_000;

        // Only populate levels 0, 3, 7 in depth arrays
        snap.bid_sizes[0] = 100_000_000;
        snap.bid_sizes[3] = 100_000_000;
        snap.bid_sizes[7] = 100_000_000;
        snap.ask_sizes[0] = 100_000_000;
        snap.ask_sizes[3] = 100_000_000;
        snap.ask_sizes[7] = 100_000_000;

        snap
    };

    // Should handle sparse depth without panicking
    let signal = strategy.calculate(&sparse_snapshot);

    // Should still generate valid signal (using available levels)
    assert!(signal.is_some(), "Should handle sparse depth");
}
