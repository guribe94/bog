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
use bog_core::data::{MarketSnapshot, SnapshotBuilder};
use bog_core::engine::{Engine, SimulatedExecutor};
use bog_core::resilience::install_panic_handler;
use bog_strategies::SimpleSpread;
use clap::Parser;

fn main() -> Result<()> {
    // Parse CLI arguments
    let args = CommonArgs::parse();

    // Initialize logging
    init_logging(&args.log_level)?;

    // Install panic handler for graceful shutdown
    install_panic_handler();

    tracing::info!("=== Bog: Simple Spread + Simulated Executor ===");
    tracing::info!("Market ID: {}", args.market_id);

    // Setup performance (CPU pinning, real-time priority)
    setup_performance(args.cpu_core, args.realtime)?;

    // Create strategy (non-zero sized type with volatility state)
    let strategy = SimpleSpread::new();
    tracing::info!("Strategy size: {} bytes", std::mem::size_of_val(&strategy));

    // Create executor with default configuration
    // This is the HFT-optimized SimulatedExecutor (instant fills, fee accounting enabled)
    tracing::info!("Using HFT SimulatedExecutor (instant fills with fee accounting)");
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
/// Returns (snapshot, is_data_fresh) tuple
fn create_test_feed(market_id: u64) -> impl FnMut() -> Result<(Option<MarketSnapshot>, bool)> {
    let mut tick_count = 0u64;
    const MAX_TICKS: u64 = 1000;
    let start_time_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    move || {
        if tick_count >= MAX_TICKS {
            return Ok((None, false));
        }

        tick_count += 1;

        // Generate synthetic market data
        let base_price = 50_000_000_000_000; // $50,000 in fixed-point
        let spread = 10_000_000_000; // $10 spread

        // Add some price movement
        let price_offset = (tick_count % 100) * 1_000_000_000;

        // Use realistic current timestamps
        let current_ns = start_time_ns + (tick_count * 100_000); // 100μs per tick

        // Data is always fresh in simulated feed
        let data_fresh = true;

        let snapshot = SnapshotBuilder::new()
            .market_id(market_id)
            .sequence(tick_count)
            .timestamp(current_ns)
            .best_bid(base_price + price_offset, 1_000_000_000)
            .best_ask(base_price + price_offset + spread, 1_000_000_000)
            .incremental_snapshot()
            .build();

        Ok((Some(snapshot), data_fresh))
    }
}
