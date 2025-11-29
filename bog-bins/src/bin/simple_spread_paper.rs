//! Simple Spread Strategy - PAPER TRADING ONLY
//!
//! WARNING: This binary uses SimulatedExecutor and does NOT place real orders!
//! THIS IS FOR PAPER TRADING / TESTING ONLY
//! DO NOT USE FOR REAL MONEY TRADING
//!
//! This binary combines:
//! - SimpleSpread strategy (zero-sized type)
//! - SimulatedExecutor (PAPER TRADING ONLY - no real orders)
//! - Real Huginn market data via shared memory
//! - Full compile-time monomorphization
//!
//! Purpose: Test strategies with real market data without risking capital
//! For live trading: Implement LighterExecutor first

use anyhow::Result;
use bog_bins::common::{init_logging, print_stats, setup_performance, CommonArgs};
use bog_core::data::{MarketFeed, SnapshotValidator, ValidationConfig, ValidationError};
use bog_core::data::types::encode_market_id;
use bog_core::engine::{
    Engine, SimulatedExecutor, GapRecoveryManager, GapRecoveryConfig,
    AlertManager, AlertConfig, AlertType
};
use bog_core::resilience::{install_panic_handler, KillSwitch};
use bog_strategies::SimpleSpread;
use bog_strategies::simple_spread::{SPREAD_BPS, ORDER_SIZE, MIN_SPREAD_BPS};
use clap::Parser;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{error, info, warn};

fn main() -> Result<()> {
    // Parse CLI arguments
    let args = CommonArgs::parse();

    // Initialize logging
    init_logging(&args.log_level)?;

    // Install panic handler for graceful shutdown
    install_panic_handler();

    info!("=== Bog: Simple Spread + PAPER TRADING ===");
    warn!("PAPER TRADING MODE - NO REAL ORDERS WILL BE PLACED");
    warn!("Using SimulatedExecutor - This is for testing only!");
    warn!("DO NOT USE THIS BINARY FOR REAL MONEY TRADING");
    info!("Market ID: {}", args.market_id);

    // Setup performance (CPU pinning, real-time priority)
    setup_performance(args.cpu_core, args.realtime)?;

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
    let encoded_market_id = encode_market_id(1, args.market_id);
    info!("Connecting to Huginn shared memory for market {} (encoded: {})...", args.market_id, encoded_market_id);
    let mut feed = MarketFeed::connect(encoded_market_id)?;

    info!("Connected successfully. Waiting for initial market snapshot...");

    // Wait for initial snapshot to ensure we have valid data before trading
    // Increased to 300 retries × 500ms = 150s total to handle Huginn WebSocket connection delays
    let initial_snapshot = match feed.wait_for_initial_snapshot(300, Duration::from_millis(500)) {
        Ok(snapshot) => {
            info!(
                "Received initial snapshot: seq={}, bid={}, ask={}, spread={}bps",
                snapshot.sequence,
                snapshot.best_bid_price,
                snapshot.best_ask_price,
                ((snapshot.best_ask_price - snapshot.best_bid_price) * 10_000) / snapshot.best_bid_price
            );

            // Validate initial snapshot
            if snapshot.best_bid_price == 0 || snapshot.best_ask_price == 0 {
                error!("Initial snapshot has zero prices! Aborting.");
                return Err(anyhow::anyhow!("Invalid initial snapshot"));
            }

            if snapshot.best_ask_price <= snapshot.best_bid_price {
                error!("Initial snapshot has crossed orderbook! Aborting.");
                return Err(anyhow::anyhow!("Crossed orderbook in initial snapshot"));
            }

            snapshot
        }
        Err(e) => {
            error!("Failed to get initial snapshot: {}", e);
            error!("Is Huginn running? Check: ls -la /dev/shm/hg_m{}", args.market_id);
            return Err(e);
        }
    };

    // Log initial market conditions
    info!("Initial market conditions validated:");
    info!("  - Sequence: {}", initial_snapshot.sequence);
    info!("  - Best Bid: {} (size: {})", initial_snapshot.best_bid_price, initial_snapshot.best_bid_size);
    info!("  - Best Ask: {} (size: {})", initial_snapshot.best_ask_price, initial_snapshot.best_ask_size);
    info!("  - Spread: {} bps", ((initial_snapshot.best_ask_price - initial_snapshot.best_bid_price) * 10_000) / initial_snapshot.best_bid_price);
    info!("  - Is Full Snapshot: {}", initial_snapshot.is_full_snapshot());

    // Create strategy (non-zero sized type with volatility state)
    let strategy = SimpleSpread::new();
    info!("Strategy: SimpleSpread (size: {} bytes)", std::mem::size_of_val(&strategy));
    info!("  - Target Spread: {} bps", SPREAD_BPS);
    info!("  - Order Size: {} (fixed-point)", ORDER_SIZE);
    info!("  - Min Market Spread: {} bps", MIN_SPREAD_BPS);

    // Create executor
    // PAPER TRADING: Replace with LighterExecutor for live trading
    warn!("PAPER TRADING: SimulatedExecutor active - NO REAL ORDERS!");
    warn!("All trades are simulated. No funds at risk.");
    let executor = SimulatedExecutor::new_default();

    // Create engine with full compile-time monomorphization
    let mut engine = Engine::new(strategy, executor);

    // Create gap recovery manager for automatic recovery
    let mut gap_recovery_config = GapRecoveryConfig::default();
    gap_recovery_config.pause_trading_during_recovery = true;
    gap_recovery_config.alert_on_gap = true;
    let mut gap_recovery = GapRecoveryManager::new(gap_recovery_config);
    info!("Gap recovery enabled with automatic recovery");

    // Create alert manager for comprehensive monitoring
    let mut alert_config = AlertConfig::default();
    alert_config.halt_on_critical = true;  // Stop trading on critical alerts
    let mut alert_manager = AlertManager::new(alert_config);
    info!("Alert system enabled with critical halt protection");

    // Create snapshot validator with enhanced checks
    let mut validation_config = ValidationConfig::default();
    validation_config.max_spread_bps = 1000;        // 10% max spread
    validation_config.min_spread_bps = 1;            // 1bp minimum
    validation_config.max_price_change_bps = 500;    // 5% max change per snapshot
    validation_config.validate_depth = true;         // Validate all 10 levels
    let mut validator = SnapshotValidator::with_config(validation_config);
    info!("Enhanced data validation enabled (depth={}, spike detection={}bps)",
        validator.config().validate_depth,
        validator.config().max_price_change_bps
    );

    // Track last sequence for gap detection
    let mut last_sequence = initial_snapshot.sequence;

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
                        (AlertType::OrderbookCrossed, (format!("Orderbook crossed: bid={}, ask={}", bid, ask), ctx))
                    }
                    ValidationError::SpreadTooWide { spread_bps } => {
                        let mut ctx = HashMap::new();
                        ctx.insert("spread_bps".to_string(), spread_bps.to_string());
                        (AlertType::SpreadTooWide, (format!("Spread too wide: {}bps", spread_bps), ctx))
                    }
                    ValidationError::PriceSpike { change_bps, max_bps } => {
                        let mut ctx = HashMap::new();
                        ctx.insert("change_bps".to_string(), change_bps.to_string());
                        ctx.insert("max_bps".to_string(), max_bps.to_string());
                        (AlertType::PriceSpike, (format!("Price spike: {}bps (max {}bps)", change_bps, max_bps), ctx))
                    }
                    ValidationError::LowLiquidity { total_bid_size, total_ask_size, min_size } => {
                        let mut ctx = HashMap::new();
                        ctx.insert("bid_size".to_string(), total_bid_size.to_string());
                        ctx.insert("ask_size".to_string(), total_ask_size.to_string());
                        ctx.insert("min_size".to_string(), min_size.to_string());
                        (AlertType::LowLiquidity, (format!("Low liquidity: bid={}, ask={}", total_bid_size, total_ask_size), ctx))
                    }
                    _ => {
                        (AlertType::DataInvalid, (validation_error.to_string(), HashMap::new()))
                    }
                };

                alert_manager.raise_alert(alert_type, message.0, message.1).ok();

                // Reject this snapshot - don't trade on invalid data
                snapshot = None;
            }
        }

        // Alert for stale data
        if !data_fresh {
            warn!("Data is stale! State: {:?}", feed.stale_state());
            let mut context = HashMap::new();
            context.insert("state".to_string(), format!("{:?}", feed.stale_state()));
            alert_manager.raise_alert(
                AlertType::DataStale,
                "Market data is stale".to_string(),
                context,
            ).ok();
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
                alert_manager.raise_alert(
                    AlertType::DataGap,
                    format!("Sequence gap of {} messages detected", gap_size),
                    context,
                ).ok();

                // Trigger automatic recovery
                match gap_recovery.handle_gap(&mut feed, gap_size, last_sequence, snap.sequence) {
                    Ok(Some(recovery_snapshot)) => {
                        info!("Gap recovered successfully, resuming from seq={}", recovery_snapshot.sequence);
                        snapshot = Some(recovery_snapshot);
                        last_sequence = recovery_snapshot.sequence;
                    }
                    Ok(None) => {
                        warn!("Gap detected but recovery skipped (manual intervention may be needed)");
                    }
                    Err(e) => {
                        error!("Gap recovery failed: {}", e);

                        // Alert for recovery failure
                        let mut context = HashMap::new();
                        context.insert("error".to_string(), e.to_string());
                        alert_manager.raise_alert(
                            AlertType::RecoveryFailed,
                            "Gap recovery failed".to_string(),
                            context,
                        ).ok();

                        // Check if we should abandon
                        if gap_recovery.should_abandon() {
                            error!("Too many consecutive gap recovery failures, shutting down!");
                            kill_switch_engine.shutdown("Gap recovery failure threshold exceeded");
                            return Err(anyhow::anyhow!("Gap recovery abandoned after repeated failures"));
                        }
                    }
                }
            } else {
                // Update last sequence for next gap check
                last_sequence = snap.sequence;

                // Check for invalid data conditions
                if snap.best_bid_price >= snap.best_ask_price {
                    let mut context = HashMap::new();
                    context.insert("bid".to_string(), snap.best_bid_price.to_string());
                    context.insert("ask".to_string(), snap.best_ask_price.to_string());
                    alert_manager.raise_alert(
                        AlertType::OrderbookCrossed,
                        "Orderbook is crossed".to_string(),
                        context,
                    ).ok();
                }

                // Check for wide spreads
                let spread_bps = ((snap.best_ask_price - snap.best_bid_price) * 10_000) / snap.best_bid_price;
                if spread_bps > 100 {  // > 1% spread
                    let mut context = HashMap::new();
                    context.insert("spread_bps".to_string(), spread_bps.to_string());
                    alert_manager.raise_alert(
                        AlertType::SpreadTooWide,
                        format!("Spread is {}bps", spread_bps),
                        context,
                    ).ok();
                }
            }
        }

        // Monitor queue depth
        let queue_depth = feed.queue_depth();
        if queue_depth > 100 {
            warn!("High queue depth: {} messages", queue_depth);
            let mut context = HashMap::new();
            context.insert("queue_depth".to_string(), queue_depth.to_string());
            alert_manager.raise_alert(
                AlertType::HighQueueDepth,
                format!("Queue depth: {}", queue_depth),
                context,
            ).ok();
        }

        // Check for epoch changes (Huginn restart)
        if feed.check_epoch_change() {
            warn!("Huginn restart detected (epoch changed)");

            // Alert for Huginn restart
            alert_manager.raise_alert(
                AlertType::HuginnRestart,
                "Huginn producer restarted".to_string(),
                HashMap::new(),
            ).ok();

            // Reset gap recovery stats after Huginn restart
            gap_recovery.reset_stats();
            // Clear alert history for fresh start
            alert_manager.clear_history();
            // Reset validator price spike tracking
            validator.reset();
            info!("Validator reset after Huginn restart");
        }

        // Don't trade during recovery or if alerts have halted trading
        let should_trade = !gap_recovery.should_pause_trading()
            && !alert_manager.is_trading_halted()
            && data_fresh;

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
    info!("  - Read Success Rate: {:.2}%", consumer_stats.read_success_rate());

    // Gap recovery statistics
    info!("Gap Recovery Statistics:");
    let gap_stats = gap_recovery.stats();
    gap_stats.log();

    // Alert statistics
    alert_manager.log_summary();

    Ok(())
}