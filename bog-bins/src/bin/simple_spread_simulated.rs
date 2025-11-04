//! Simple Spread Strategy with Simulated Execution
//!
//! This binary combines:
//! - SimpleSpread strategy (zero-sized type)
//! - SimulatedExecutor (lock-free, object pools)
//! - Full compile-time monomorphization
//!
//! Target: <1μs tick-to-trade latency

use anyhow::Result;
use bog_bins::common::{init_logging, print_stats, setup_performance, CommonArgs};
use bog_core::data::MarketSnapshot;
use bog_core::engine::{Engine, SimulatedExecutor};
use bog_strategies::SimpleSpread;
use clap::Parser;

fn main() -> Result<()> {
    // Parse CLI arguments
    let args = CommonArgs::parse();

    // Initialize logging
    init_logging(&args.log_level)?;

    tracing::info!("=== Bog: Simple Spread + Simulated Executor ===");
    tracing::info!("Market ID: {}", args.market_id);

    // Setup performance (CPU pinning, real-time priority)
    setup_performance(args.cpu_core, args.realtime)?;

    // Create strategy (zero-sized type - 0 bytes!)
    let strategy = SimpleSpread;
    tracing::info!("Strategy size: {} bytes", std::mem::size_of_val(&strategy));

    // Create executor with default pool sizes
    let executor = SimulatedExecutor::new_default();

    // Create engine with full compile-time monomorphization
    // Type: Engine<SimpleSpread, SimulatedExecutor>
    let mut engine = Engine::new(strategy, executor);

    // Create market data feed (stub for now - would be Huginn in production)
    let feed_fn = create_test_feed(args.market_id);

    // Run the engine
    tracing::info!("Starting engine...");
    let stats = engine.run(feed_fn)?;

    // Print final statistics
    print_stats(&stats);

    Ok(())
}

/// Create a test market feed (replace with Huginn in production)
fn create_test_feed(market_id: u64) -> impl FnMut() -> Result<Option<MarketSnapshot>> {
    let mut tick_count = 0u64;
    const MAX_TICKS: u64 = 1000;

    move || {
        if tick_count >= MAX_TICKS {
            return Ok(None);
        }

        tick_count += 1;

        // Generate synthetic market data
        let base_price = 50_000_000_000_000; // $50,000 in fixed-point
        let spread = 10_000_000_000; // $10 spread

        // Add some price movement
        let price_offset = (tick_count % 100) * 1_000_000_000;

        Ok(Some(MarketSnapshot {
            market_id,
            sequence: tick_count,
            exchange_timestamp_ns: tick_count * 100_000, // 100μs per tick
            local_recv_ns: tick_count * 100_000,
            local_publish_ns: tick_count * 100_000,
            best_bid_price: base_price + price_offset,
            best_bid_size: 1_000_000_000, // 1.0 BTC
            best_ask_price: base_price + price_offset + spread,
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 3],
            ask_prices: [0; 3],
            dex_type: 1,
            _padding: [0; 7],
        }))
    }
}
