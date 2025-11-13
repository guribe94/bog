//! Simple Spread Strategy with Live Execution (Lighter DEX)
//!
//! This binary combines:
//! - SimpleSpread strategy (zero-sized type)
//! - LighterExecutor (real exchange connection)
//! - Full compile-time monomorphization
//!
//! Target: <1μs tick-to-trade latency
//!
//! NOTE: This requires a live connection to Lighter DEX
//! Use simulated mode for testing without real funds.

use anyhow::Result;
use bog_bins::common::{init_logging, print_stats, setup_performance, CommonArgs};
use bog_core::resilience::install_panic_handler;
use clap::Parser;

fn main() -> Result<()> {
    // Parse CLI arguments
    let args = CommonArgs::parse();

    // Initialize logging
    init_logging(&args.log_level)?;

    // Install panic handler for graceful shutdown
    install_panic_handler();

    tracing::info!("=== Bog: Simple Spread + Live Executor (Lighter) ===");
    tracing::warn!("LIVE TRADING MODE - REAL FUNDS AT RISK");
    tracing::info!("Market ID: {}", args.market_id);

    // Setup performance (CPU pinning, real-time priority)
    setup_performance(args.cpu_core, args.realtime)?;

    // TODO: Implement live executor integration with Lighter DEX
    // For now, this is a stub that shows the structure
    tracing::error!("Live executor not yet implemented");
    tracing::info!("Please use bog-simple-spread-simulated for testing");

    Ok(())
}

/*
// Future implementation would look like:

use bog_core::engine::{Engine};
use bog_core::execution::LighterExecutor;
use bog_strategies::SimpleSpread;

fn main() -> Result<()> {
    // ... CLI and setup ...

    let strategy = SimpleSpread;

    // Create live executor with Lighter connection
    let executor = LighterExecutor::new(
        config.rpc_url,
        config.ws_url,
        config.private_key,
    )?;

    // Create engine
    let mut engine = Engine::new(strategy, executor);

    // Connect to Huginn market feed
    let feed = huginn::connect(market_id)?;

    // Run the engine
    let stats = engine.run(|| feed.next_snapshot())?;

    print_stats(&stats);
    Ok(())
}
*/
