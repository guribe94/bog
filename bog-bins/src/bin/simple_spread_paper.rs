//! Simple Spread Strategy - PAPER TRADING
//!
//! WARNING: This binary uses JournaledExecutor in SIMULATED mode.
//! THIS IS FOR PAPER TRADING / TESTING ONLY
//! DO NOT USE FOR REAL MONEY TRADING
//!
//! This binary combines:
//! - SimpleSpread strategy (zero-sized type)
//! - JournaledExecutor (configured for simulation with journaling)
//! - Real Huginn market data via shared memory
//! - Full compile-time monomorphization
//!
//! Purpose: Test strategies with real market data without risking capital
//! For live trading: Ensure JournaledExecutor is configured for live execution.

use anyhow::Result;
use bog_bins::common::{init_logging, print_stats, setup_performance, CommonArgs};
use bog_core::data::types::encode_market_id;
use bog_core::data::{MarketFeed, MarketSnapshotExt, SnapshotValidator, ValidationConfig, ValidationError};
use bog_core::engine::executor_bridge::ExecutorBridge;
use bog_core::engine::{
    AlertConfig, AlertManager, AlertType, Engine, GapRecoveryConfig, GapRecoveryManager,
};
use bog_core::execution::{JournaledExecutor, JournaledExecutorConfig};
use bog_core::resilience::{install_panic_handler, KillSwitch};
use bog_strategies::simple_spread::{MIN_SPREAD_BPS, ORDER_SIZE, SPREAD_BPS};
use bog_strategies::SimpleSpread;
use clap::Parser;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

/// CLI arguments for simple spread paper trading
#[derive(Parser, Debug)]
#[command(author, version, about = "Simple Spread Paper Trading Bot")]
struct Args {
    #[command(flatten)]
    common: CommonArgs,

    /// Maximum market spread in basis points to consider valid for trading.
    /// Spreads wider than this will not trigger quotes.
    /// Default: 500 (5%). Set higher for illiquid markets.
    #[arg(long, default_value = "500")]
    max_market_spread_bps: u32,
}

fn main() -> Result<()> {
    // Parse CLI arguments
    let args = Args::parse();

    // Initialize logging
    init_logging(&args.common.log_level)?;

    // Install panic handler for graceful shutdown
    install_panic_handler();

    info!("=== Bog: Simple Spread + PAPER TRADING ===");
    warn!("PAPER TRADING MODE - NO REAL ORDERS WILL BE PLACED");
    warn!("Using SimulatedExecutor - This is for testing only!");
    warn!("DO NOT USE THIS BINARY FOR REAL MONEY TRADING");
    info!("Market ID: {}", args.common.market_id);
    info!("Max Market Spread: {} bps", args.max_market_spread_bps);

    // Setup performance (CPU pinning, real-time priority)
    setup_performance(args.common.cpu_core, args.common.realtime)?;

    // Create kill switch for emergency shutdown
    let kill_switch = KillSwitch::new();
    let kill_switch_engine = kill_switch.clone();

    // Setup Ctrl+C handler
    let kill_switch_ctrlc = kill_switch.clone();
    ctrlc::set_handler(move || {
        warn!("Received Ctrl+C, initiating graceful shutdown...");
        kill_switch_ctrlc.shutdown("User requested shutdown (Ctrl+C)");
    })?;

    // Connect to real Huginn shared memory
    // Encode market ID for Lighter DEX (dex_type=1)
    let encoded_market_id = encode_market_id(1, args.common.market_id);
    info!(
        "Connecting to Huginn shared memory for market {} (encoded: {})...",
        args.common.market_id, encoded_market_id
    );
    let mut feed = MarketFeed::connect(encoded_market_id)?;

    info!("Connected successfully. Waiting for initial market snapshot...");

    // Wait for initial snapshot to ensure we have valid data before trading
    // Increased to 300 retries × 500ms = 150s total to handle Huginn WebSocket connection delays
    let initial_snapshot = match feed.wait_for_initial_snapshot(300, Duration::from_millis(500)) {
        Ok(snapshot) => {
            // Log raw values from Huginn (may be incorrectly ordered)
            let raw_spread_bps = if snapshot.best_bid_price > 0 {
                ((snapshot.best_ask_price - snapshot.best_bid_price) * 10_000) / snapshot.best_bid_price
            } else { 0 };

            // Log corrected values using actual best bid/ask
            let corrected_spread_bps = snapshot.corrected_spread_bps();
            let actual_bid = snapshot.actual_best_bid();
            let actual_ask = snapshot.actual_best_ask();

            info!(
                "Received initial snapshot: seq={}, raw_bid={}, raw_ask={}, raw_spread={}bps",
                snapshot.sequence,
                snapshot.best_bid_price,
                snapshot.best_ask_price,
                raw_spread_bps
            );
            info!(
                "Corrected values: actual_bid={}, actual_ask={}, corrected_spread={}bps",
                actual_bid,
                actual_ask,
                corrected_spread_bps
            );

            if raw_spread_bps != corrected_spread_bps {
                warn!(
                    "SPREAD MISMATCH DETECTED! Raw={}bps vs Corrected={}bps. Orderbook data may be incorrectly ordered.",
                    raw_spread_bps, corrected_spread_bps
                );
            }

            // Validate initial snapshot
            if snapshot.best_bid_price == 0 || snapshot.best_ask_price == 0 {
                error!("Initial snapshot has zero prices! Aborting.");
                return Err(anyhow::anyhow!("Invalid initial snapshot"));
            }

            // Note: Crossed orderbook check removed for thin market support
            // On thin markets, one side frequently has zero size with a stale price
            // which appears as crossed. The validator handles this with allow_crossed_when_empty.
            if snapshot.best_ask_price < snapshot.best_bid_price
                && snapshot.best_bid_size > 0
                && snapshot.best_ask_size > 0
            {
                // Only abort if both sides have real liquidity and it's truly crossed
                error!("Initial snapshot has truly crossed orderbook (both sides have size)! Aborting.");
                return Err(anyhow::anyhow!("Crossed orderbook in initial snapshot"));
            }

            snapshot
        }
        Err(e) => {
            error!("Failed to get initial snapshot: {}", e);
            error!(
                "Is Huginn running? Check: ls -la /dev/shm/hg_m{}",
                args.common.market_id
            );
            return Err(e);
        }
    };

    // Log initial market conditions (using corrected best bid/ask)
    info!("Initial market conditions validated:");
    info!("  - Sequence: {}", initial_snapshot.sequence);
    info!(
        "  - Raw Bid: {} (size: {})",
        initial_snapshot.best_bid_price, initial_snapshot.best_bid_size
    );
    info!(
        "  - Raw Ask: {} (size: {})",
        initial_snapshot.best_ask_price, initial_snapshot.best_ask_size
    );
    info!(
        "  - Actual Best Bid: {}",
        initial_snapshot.actual_best_bid()
    );
    info!(
        "  - Actual Best Ask: {}",
        initial_snapshot.actual_best_ask()
    );
    info!(
        "  - Corrected Spread: {} bps",
        initial_snapshot.corrected_spread_bps()
    );
    info!(
        "  - Is Full Snapshot: {}",
        initial_snapshot.is_full_snapshot()
    );

    // Create strategy (non-zero sized type with volatility state)
    let strategy = SimpleSpread::new_with_max_spread(args.max_market_spread_bps);
    info!(
        "Strategy: SimpleSpread (size: {} bytes)",
        std::mem::size_of_val(&strategy)
    );
    info!("  - Target Spread: {} bps", SPREAD_BPS);
    info!("  - Order Size: {} (fixed-point)", ORDER_SIZE);
    info!("  - Min Market Spread: {} bps", MIN_SPREAD_BPS);
    info!("  - Max Market Spread: {} bps", args.max_market_spread_bps);

    // Create executor
    // PAPER TRADING: Replace with LighterExecutor for live trading
    warn!("PAPER TRADING: JournaledExecutor active (simulated mode) - NO REAL ORDERS!");
    warn!("All trades are simulated and journaled. No funds at risk.");
    
    let executor_config = JournaledExecutorConfig {
        enable_journal: true,
        journal_path: PathBuf::from("data/paper_trading_journal.jsonl"),
        recover_on_startup: true,
        validate_recovery: true,
        instant_fills: false, // Market-crossing fills: orders fill when market crosses price
        ..Default::default()
    };
    
    let executor = JournaledExecutor::new(executor_config);
    
    // Calculate net position from recovered journal (if any)
    let recovered_position = executor.calculate_net_position();
    if recovered_position != 0 {
        info!("Recovered net position from journal: {}", recovered_position);
    }

    // Bridge the executor to the engine trait
    let bridged_executor = ExecutorBridge::new(executor);

    // Create engine with full compile-time monomorphization
    let mut engine = Engine::new(strategy, bridged_executor);
    
    // Initialize engine position with recovered value
    if recovered_position != 0 {
        engine.position().update_quantity(recovered_position);
        info!("Engine position initialized to {}", recovered_position);
    }

    // Create gap recovery manager for automatic recovery
    let mut gap_recovery_config = GapRecoveryConfig::default();
    gap_recovery_config.pause_trading_during_recovery = true;
    gap_recovery_config.alert_on_gap = true;
    let mut gap_recovery = GapRecoveryManager::new(gap_recovery_config);
    info!("Gap recovery enabled with automatic recovery");

    // Create alert manager for comprehensive monitoring
    let mut alert_config = AlertConfig::default();
    alert_config.halt_on_critical = true; // Stop trading on critical alerts
    let mut alert_manager = AlertManager::new(alert_config);
    info!("Alert system enabled with critical halt protection");

    // Create snapshot validator with enhanced checks
    let mut validation_config = ValidationConfig::default();
    validation_config.max_spread_bps = 1000; // 10% max spread
    validation_config.min_spread_bps = 0; // Allow 0bps spreads (Lighter DEX has tight markets)
    validation_config.allow_locked = true; // Allow locked orderbooks (common on Lighter DEX)
    validation_config.max_price_change_bps = 500; // 5% max change per snapshot
    validation_config.validate_depth = true; // Validate all 10 levels
    validation_config.min_total_liquidity = 0; // Allow zero liquidity (altcoins like FARTCOIN often have empty sides)
    validation_config.allow_crossed_when_empty = true; // Thin market support: allow crossed when one side is empty
    let mut validator = SnapshotValidator::with_config(validation_config);
    info!(
        "Enhanced data validation enabled (depth={}, spike detection={}bps)",
        validator.config().validate_depth,
        validator.config().max_price_change_bps
    );

    // Track last sequence for gap detection
    let mut last_sequence = initial_snapshot.sequence;

    // Periodic status logging to diagnose quoting decisions
    let last_status_time = Cell::new(Instant::now());
    let status_interval = Duration::from_secs(10);

    // Create feed function that provides data from Huginn
    let feed_fn = || {
        // Check kill switch
        if kill_switch_engine.should_stop() {
            info!("Kill switch activated, stopping feed");
            return Ok((None, false));
        }

        // Check if recovery is in progress
        if gap_recovery.is_recovering() {
            // Don't process new data during recovery
            return Ok((None, false));
        }

        // Try to receive snapshot from Huginn
        let mut snapshot = feed.try_recv();
        let data_fresh = feed.is_data_fresh();

        // Validate snapshot if we got one
        if let Some(ref snap) = snapshot {
            if let Err(validation_error) = validator.validate(snap) {
                // Log validation failure
                error!("Snapshot validation failed: {}", validation_error);

                // Determine alert type and raise alert
                let (alert_type, message) = match &validation_error {
                    ValidationError::OrderbookCrossed { bid, ask } => {
                        let mut ctx = HashMap::new();
                        ctx.insert("bid".to_string(), bid.to_string());
                        ctx.insert("ask".to_string(), ask.to_string());
                        (
                            AlertType::OrderbookCrossed,
                            (format!("Orderbook crossed: bid={}, ask={}", bid, ask), ctx),
                        )
                    }
                    ValidationError::SpreadTooWide { spread_bps } => {
                        let mut ctx = HashMap::new();
                        ctx.insert("spread_bps".to_string(), spread_bps.to_string());
                        (
                            AlertType::SpreadTooWide,
                            (format!("Spread too wide: {}bps", spread_bps), ctx),
                        )
                    }
                    ValidationError::PriceSpike {
                        change_bps,
                        max_bps,
                    } => {
                        let mut ctx = HashMap::new();
                        ctx.insert("change_bps".to_string(), change_bps.to_string());
                        ctx.insert("max_bps".to_string(), max_bps.to_string());
                        (
                            AlertType::PriceSpike,
                            (
                                format!("Price spike: {}bps (max {}bps)", change_bps, max_bps),
                                ctx,
                            ),
                        )
                    }
                    ValidationError::LowLiquidity {
                        total_bid_size,
                        total_ask_size,
                        min_size,
                    } => {
                        let mut ctx = HashMap::new();
                        ctx.insert("bid_size".to_string(), total_bid_size.to_string());
                        ctx.insert("ask_size".to_string(), total_ask_size.to_string());
                        ctx.insert("min_size".to_string(), min_size.to_string());
                        (
                            AlertType::LowLiquidity,
                            (
                                format!(
                                    "Low liquidity: bid={}, ask={}",
                                    total_bid_size, total_ask_size
                                ),
                                ctx,
                            ),
                        )
                    }
                    _ => (
                        AlertType::DataInvalid,
                        (validation_error.to_string(), HashMap::new()),
                    ),
                };

                alert_manager
                    .raise_alert(alert_type, message.0, message.1)
                    .ok();

                // Reject this snapshot - don't trade on invalid data
                snapshot = None;
            } else {
                // Validation passed - if trading was halted, resume it
                // This handles automatic recovery when market data becomes valid again
                if alert_manager.is_trading_halted() {
                    info!("Valid market data received - trading resumed");
                    alert_manager.reset_halt();
                }
            }
        }

        // Alert for stale data
        if !data_fresh {
            warn!("Data is stale! State: {:?}", feed.stale_state());
            let mut context = HashMap::new();
            context.insert("state".to_string(), format!("{:?}", feed.stale_state()));
            alert_manager
                .raise_alert(
                    AlertType::DataStale,
                    "Market data is stale".to_string(),
                    context,
                )
                .ok();
        }

        // Handle gaps with automatic recovery
        if let Some(ref snap) = snapshot {
            if feed.gap_detected() {
                let gap_size = feed.last_gap_size();

                // Alert for data gap
                let mut context = HashMap::new();
                context.insert("gap_size".to_string(), gap_size.to_string());
                context.insert("last_seq".to_string(), last_sequence.to_string());
                context.insert("current_seq".to_string(), snap.sequence.to_string());
                alert_manager
                    .raise_alert(
                        AlertType::DataGap,
                        format!("Sequence gap of {} messages detected", gap_size),
                        context,
                    )
                    .ok();

                // Trigger automatic recovery
                match gap_recovery.handle_gap(&mut feed, gap_size, last_sequence, snap.sequence) {
                    Ok(Some(recovery_snapshot)) => {
                        info!(
                            "Gap recovered successfully, resuming from seq={}",
                            recovery_snapshot.sequence
                        );
                        snapshot = Some(recovery_snapshot);
                        last_sequence = recovery_snapshot.sequence;
                    }
                    Ok(None) => {
                        warn!(
                            "Gap detected but recovery skipped (manual intervention may be needed)"
                        );
                    }
                    Err(e) => {
                        error!("Gap recovery failed: {}", e);

                        // Alert for recovery failure
                        let mut context = HashMap::new();
                        context.insert("error".to_string(), e.to_string());
                        alert_manager
                            .raise_alert(
                                AlertType::RecoveryFailed,
                                "Gap recovery failed".to_string(),
                                context,
                            )
                            .ok();

                        // Check if we should abandon
                        if gap_recovery.should_abandon() {
                            error!("Too many consecutive gap recovery failures, shutting down!");
                            kill_switch_engine.shutdown("Gap recovery failure threshold exceeded");
                            return Err(anyhow::anyhow!(
                                "Gap recovery abandoned after repeated failures"
                            ));
                        }
                    }
                }
            } else {
                // Update last sequence for next gap check
                last_sequence = snap.sequence;

                // Note: Crossed orderbook check removed - now handled by validator with
                // allow_crossed_when_empty flag for thin market support

                // Check for wide spreads (using corrected best bid/ask)
                let spread_bps = snap.corrected_spread_bps();
                if spread_bps > 100 {
                    // > 1% spread
                    let mut context = HashMap::new();
                    context.insert("spread_bps".to_string(), spread_bps.to_string());
                    context.insert("actual_bid".to_string(), snap.actual_best_bid().to_string());
                    context.insert("actual_ask".to_string(), snap.actual_best_ask().to_string());
                    alert_manager
                        .raise_alert(
                            AlertType::SpreadTooWide,
                            format!("Spread is {}bps (corrected)", spread_bps),
                            context,
                        )
                        .ok();
                }

                // Periodic status logging (every 10 seconds)
                if last_status_time.get().elapsed() >= status_interval {
                    let should_quote = spread_bps >= MIN_SPREAD_BPS as u64 && spread_bps <= 500;
                    info!(
                        "STATUS: seq={}, bid={}, ask={}, spread={}bps, target={}bps, should_quote={}",
                        snap.sequence,
                        snap.actual_best_bid(),
                        snap.actual_best_ask(),
                        spread_bps,
                        SPREAD_BPS,
                        should_quote
                    );
                    last_status_time.set(Instant::now());
                }
            }
        }

        // Monitor queue depth
        let queue_depth = feed.queue_depth();
        if queue_depth > 100 {
            warn!("High queue depth: {} messages", queue_depth);
            let mut context = HashMap::new();
            context.insert("queue_depth".to_string(), queue_depth.to_string());
            alert_manager
                .raise_alert(
                    AlertType::HighQueueDepth,
                    format!("Queue depth: {}", queue_depth),
                    context,
                )
                .ok();
        }

        // Check for epoch changes (Huginn restart)
        if feed.check_epoch_change() {
            warn!("Huginn restart detected (epoch changed)");

            // Alert for Huginn restart
            alert_manager
                .raise_alert(
                    AlertType::HuginnRestart,
                    "Huginn producer restarted".to_string(),
                    HashMap::new(),
                )
                .ok();

            // Reset gap recovery stats after Huginn restart
            gap_recovery.reset_stats();
            // Clear alert history for fresh start
            alert_manager.clear_history();
            // Reset validator price spike tracking
            validator.reset();
            info!("Validator reset after Huginn restart");
        }

        // Don't trade during recovery or if alerts have halted trading
        let pause_for_recovery = gap_recovery.should_pause_trading();
        let trading_halted = alert_manager.is_trading_halted();
        let should_trade = !pause_for_recovery && !trading_halted && data_fresh;

        // DEBUG: Log should_trade decision periodically
        if last_status_time.get().elapsed() >= status_interval {
            if !should_trade {
                warn!(
                    "NOT TRADING: pause_for_recovery={}, trading_halted={}, data_fresh={}",
                    pause_for_recovery, trading_halted, data_fresh
                );
            }
        }

        Ok((snapshot, should_trade))
    };

    // Run the engine with real market data
    info!("Starting trading engine with live market data...");
    info!("Press Ctrl+C to stop gracefully");

    let stats = match engine.run(feed_fn) {
        Ok(stats) => stats,
        Err(e) => {
            error!("Engine error: {}", e);
            error!("Attempting emergency shutdown...");

            // Try to cancel all orders before exiting
            // engine.emergency_shutdown()?;

            return Err(e);
        }
    };

    // Print final statistics
    print_stats(&stats);

    // Additional feed statistics
    info!("Feed Statistics:");
    let feed_stats = feed.feed_stats();
    info!("  - Messages Received: {}", feed_stats.messages_received);
    info!("  - Empty Polls: {}", feed_stats.empty_polls);
    info!("  - Sequence Gaps: {}", feed_stats.sequence_gaps);
    info!("  - Max Queue Depth: {}", feed_stats.max_queue_depth);

    let consumer_stats = feed.consumer_stats();
    info!("  - Total Reads: {}", consumer_stats.total_reads);
    info!(
        "  - Read Success Rate: {:.2}%",
        consumer_stats.read_success_rate()
    );

    // Gap recovery statistics
    info!("Gap Recovery Statistics:");
    let gap_stats = gap_recovery.stats();
    gap_stats.log();

    // Alert statistics
    alert_manager.log_summary();

    Ok(())
}
