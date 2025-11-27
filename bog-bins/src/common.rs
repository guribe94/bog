//! Common utilities for all binaries
//!
//! Shared initialization, CLI parsing, and setup code.

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Common CLI arguments for all binaries
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct CommonArgs {
    /// Market ID to trade
    #[arg(short, long, default_value = "1")]
    pub market_id: u64,

    /// CPU core to pin to (for performance)
    #[arg(short = 'c', long)]
    pub cpu_core: Option<usize>,

    /// Enable real-time priority (requires privileges)
    #[arg(long)]
    pub realtime: bool,

    /// Enable metrics output
    #[arg(long)]
    pub metrics: bool,

    /// Log level
    #[arg(short, long, default_value = "info")]
    pub log_level: String,
}

/// Initialize tracing/logging
pub fn init_logging(level: &str) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(level))?;

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false))
        .with(filter)
        .init();

    Ok(())
}

/// Setup CPU affinity and real-time priority
pub fn setup_performance(cpu_core: Option<usize>, realtime: bool) -> Result<()> {
    // Pin to CPU core if specified
    if let Some(core) = cpu_core {
        bog_core::perf::cpu::pin_to_core(core)?;
        tracing::info!("Pinned to CPU core {}", core);
    }

    // Set real-time priority if requested
    #[cfg(target_os = "linux")]
    if realtime {
        bog_core::perf::cpu::set_realtime_priority(50)?;
        tracing::info!("Enabled real-time priority");
    }

    #[cfg(not(target_os = "linux"))]
    if realtime {
        tracing::warn!("Real-time priority only supported on Linux");
    }

    Ok(())
}

/// Print final statistics
pub fn print_stats(stats: &bog_core::engine::EngineStats) {
    tracing::info!("=== Final Statistics ===");
    tracing::info!("Ticks processed: {}", stats.ticks_processed);
    tracing::info!("Signals generated: {}", stats.signals_generated);
    tracing::info!("Final position: {}", stats.final_position);
    tracing::info!("Realized PnL: {}", stats.realized_pnl);

    if stats.ticks_processed > 0 {
        let signal_rate = (stats.signals_generated as f64 / stats.ticks_processed as f64) * 100.0;
        tracing::info!("Signal rate: {:.2}%", signal_rate);
    }
}
