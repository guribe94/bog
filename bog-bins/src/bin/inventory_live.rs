//! Inventory-Based Strategy with Live Execution (Lighter DEX)
//!
//! This binary combines:
//! - InventoryBased strategy (Avellaneda-Stoikov model stub)
//! - LighterExecutor (real exchange connection)
//! - Full compile-time monomorphization
//!
//! Target: <1μs tick-to-trade latency
//!
//! NOTE: Both InventoryBased strategy AND live executor are stubs
//! This demonstrates the architecture for future implementation.

use anyhow::Result;
use bog_bins::common::{init_logging, CommonArgs};
use clap::Parser;

fn main() -> Result<()> {
    // Parse CLI arguments
    let args = CommonArgs::parse();

    // Initialize logging
    init_logging(&args.log_level)?;

    tracing::info!("=== Bog: Inventory-Based + Live Executor (Lighter) ===");
    tracing::warn!("LIVE TRADING MODE - REAL FUNDS AT RISK");
    tracing::warn!("NOTE: Both InventoryBased strategy and live executor are stubs");
    tracing::info!("Market ID: {}", args.market_id);

    // TODO: Implement both InventoryBased strategy and live executor
    tracing::error!("This binary combination is not yet fully implemented");
    tracing::info!("Use bog-simple-spread-simulated for testing");
    tracing::info!("Use bog-inventory-simulated for inventory strategy testing");

    Ok(())
}

/*
// Future implementation would look like:

use bog_core::engine::Engine;
use bog_core::execution::LighterExecutor;
use bog_strategies::InventoryBased;

fn main() -> Result<()> {
    // ... CLI and setup ...

    let strategy = InventoryBased;

    // Create live executor with Lighter connection
    let executor = LighterExecutor::new(
        config.rpc_url,
        config.ws_url,
        config.private_key,
    )?;

    // Create engine with inventory tracking
    let mut engine = Engine::new(strategy, executor);

    // Connect to Huginn market feed
    let feed = huginn::connect(market_id)?;

    // Run the engine with inventory-aware execution
    let stats = engine.run(|| feed.next_snapshot())?;

    print_stats(&stats);
    Ok(())
}
*/
