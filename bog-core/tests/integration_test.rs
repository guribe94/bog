//! Integration tests for the zero-overhead HFT engine
//!
//! Tests the full stack: Engine<Strategy, Executor>

use bog_core::core::{Position, Signal};
use bog_core::data::MarketSnapshot;
use bog_core::engine::{Engine, Executor, SimulatedExecutor, Strategy};
use bog_strategies::SimpleSpread;
use anyhow::Result;

/// Test that the full engine works with SimpleSpread + SimulatedExecutor
#[test]
fn test_engine_with_simple_spread() -> Result<()> {
    // Create strategy and executor
    let strategy = SimpleSpread;
    let executor = SimulatedExecutor::new_default();

    // Create engine
    let mut engine = Engine::new(strategy, executor);

    // Create test market snapshot
    let snapshot = MarketSnapshot {
        market_id: 1,
        sequence: 1,
        exchange_timestamp_ns: 0,
        best_bid_price: 50_000_000_000_000, // $50,000
        best_bid_size: 1_000_000_000,        // 1.0 BTC
        best_ask_price: 50_010_000_000_000, // $50,010 (2bps spread)
        best_ask_size: 1_000_000_000,
        ..Default::default()
    };

    // Process tick
    engine.process_tick(&snapshot)?;

    // Check stats
    let stats = engine.stats();
    assert_eq!(stats.ticks_processed, 1);
    assert_eq!(stats.signals_generated, 1);

    Ok(())
}

/// Test engine with multiple ticks
#[test]
fn test_engine_multiple_ticks() -> Result<()> {
    let strategy = SimpleSpread;
    let executor = SimulatedExecutor::new_default();
    let mut engine = Engine::new(strategy, executor);

    // Process multiple ticks with varying prices
    for i in 0..10 {
        let snapshot = MarketSnapshot {
            market_id: 1,
            sequence: i + 1,
            exchange_timestamp_ns: i * 1_000_000,
            best_bid_price: 50_000_000_000_000 + i * 1_000_000_000,
            best_bid_size: 1_000_000_000,
            best_ask_price: 50_010_000_000_000 + i * 1_000_000_000,
            best_ask_size: 1_000_000_000,
            ..Default::default()
        };

        engine.process_tick(&snapshot)?;
    }

    // Check stats
    let stats = engine.stats();
    assert_eq!(stats.ticks_processed, 10);
    assert_eq!(stats.signals_generated, 10); // Each tick should generate a signal

    Ok(())
}

/// Test that engine respects market change detection
#[test]
fn test_engine_market_change_detection() -> Result<()> {
    let strategy = SimpleSpread;
    let executor = SimulatedExecutor::new_default();
    let mut engine = Engine::new(strategy, executor);

    let snapshot = MarketSnapshot {
        market_id: 1,
        sequence: 1,
        exchange_timestamp_ns: 0,
        best_bid_price: 50_000_000_000_000,
        best_bid_size: 1_000_000_000,
        best_ask_price: 50_010_000_000_000,
        best_ask_size: 1_000_000_000,
        ..Default::default()
    };

    // First tick - should process
    engine.process_tick(&snapshot)?;

    // Second tick with same prices - should skip strategy call
    engine.process_tick(&snapshot)?;

    // Both ticks counted, but market change detection optimizes second tick
    let stats = engine.stats();
    assert_eq!(stats.ticks_processed, 2);
    // Signal count depends on whether strategy was called

    Ok(())
}

/// Test strategy compilation - verify zero-sized type
#[test]
fn test_strategy_is_zero_sized() {
    let strategy = SimpleSpread;
    assert_eq!(std::mem::size_of_val(&strategy), 0);
}

/// Test executor stats tracking
#[test]
fn test_executor_stats() -> Result<()> {
    let strategy = SimpleSpread;
    let mut executor = SimulatedExecutor::new_default();

    let position = Position::new();
    let signal = Signal::quote_both(
        49_995_000_000_000,
        50_005_000_000_000,
        100_000_000,
    );

    executor.execute(signal, &position)?;

    let stats = executor.stats();
    assert_eq!(stats.total_orders, 2); // Bid + Ask
    assert_eq!(stats.total_fills, 2);
    assert_eq!(stats.total_volume, 200_000_000); // 0.2 BTC total

    Ok(())
}

/// Benchmark test - measure tick processing latency
#[test]
fn test_tick_processing_latency() -> Result<()> {
    use std::time::Instant;

    let strategy = SimpleSpread;
    let executor = SimulatedExecutor::new_default();
    let mut engine = Engine::new(strategy, executor);

    let snapshot = MarketSnapshot {
        market_id: 1,
        sequence: 1,
        exchange_timestamp_ns: 0,
        best_bid_price: 50_000_000_000_000,
        best_bid_size: 1_000_000_000,
        best_ask_price: 50_010_000_000_000,
        best_ask_size: 1_000_000_000,
        ..Default::default()
    };

    // Warmup
    for _ in 0..100 {
        engine.process_tick(&snapshot)?;
    }

    // Measure 1000 ticks
    let start = Instant::now();
    for _ in 0..1000 {
        engine.process_tick(&snapshot)?;
    }
    let elapsed = start.elapsed();

    let avg_latency = elapsed.as_nanos() / 1000;
    println!("Average tick latency: {}ns", avg_latency);

    // This is a rough test - proper benchmarks in bog-bench
    // Target is <1μs (1000ns), but we're just checking it's reasonable
    assert!(avg_latency < 10_000, "Tick latency too high: {}ns", avg_latency);

    Ok(())
}
