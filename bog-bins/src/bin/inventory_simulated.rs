//! Inventory-Based Strategy with Simulated Execution
//!
//! This binary combines:
//! - InventoryBased strategy (Avellaneda-Stoikov model stub)
//! - SimulatedExecutor (lock-free, object pools)
//! - Full compile-time monomorphization
//!
//! Target: <1μs tick-to-trade latency
//!
//! NOTE: InventoryBased strategy is currently a stub (Phase 4 TODO)

use anyhow::Result;
use bog_bins::common::{init_logging, print_stats, setup_performance, CommonArgs};
use bog_core::data::{MarketSnapshot, SnapshotBuilder};
use bog_core::engine::{Engine, SimulatedExecutor};
use bog_core::resilience::install_panic_handler;
use bog_strategies::InventoryBased;
use clap::Parser;

fn main() -> Result<()> {
    // Parse CLI arguments
    let args = CommonArgs::parse();

    // Initialize logging
    init_logging(&args.log_level)?;

    // Install panic handler for graceful shutdown
    install_panic_handler();

    tracing::info!("=== Bog: Inventory-Based + Simulated Executor ===");
    tracing::warn!("NOTE: InventoryBased strategy is currently a stub");
    tracing::info!("Market ID: {}", args.market_id);

    // Setup performance (CPU pinning, real-time priority)
    setup_performance(args.cpu_core, args.realtime)?;

    // Create strategy (zero-sized type - 0 bytes!)
    let strategy = InventoryBased;
    tracing::info!("Strategy size: {} bytes", std::mem::size_of_val(&strategy));

    // Create executor with default pool sizes
    let executor = SimulatedExecutor::new_default();

    // Create engine with full compile-time monomorphization
    // Type: Engine<InventoryBased, SimulatedExecutor>
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

        // Generate synthetic market data with volatility
        let base_price = 50_000_000_000_000; // $50,000 in fixed-point
        let spread = 10_000_000_000; // $10 spread

        // Add price movement and volatility for inventory testing
        let price_offset = ((tick_count as f64 * 0.1).sin() * 1000.0) as i64 * 1_000_000_000;

        let bid_price = if price_offset >= 0 {
            base_price + price_offset as u64
        } else {
            base_price.saturating_sub((-price_offset) as u64)
        };

        // Use realistic current timestamps
        let current_ns = start_time_ns + (tick_count * 100_000); // 100μs per tick

        // Data is always fresh in simulated feed
        let data_fresh = true;

        let snapshot = SnapshotBuilder::new()
            .market_id(market_id)
            .sequence(tick_count)
            .timestamp(current_ns)
            .best_bid(bid_price, 1_000_000_000)
            .best_ask(bid_price + spread, 1_000_000_000)
            .incremental_snapshot()
            .build();

        Ok((Some(snapshot), data_fresh))
    }
}
