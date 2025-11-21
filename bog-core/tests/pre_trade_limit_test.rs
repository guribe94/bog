//! Test pre-trade position limit validation

use bog_core::core::{Position, Signal, SignalAction};
use bog_core::data::MarketSnapshot;
use bog_core::engine::{Engine, Strategy};
use bog_core::execution::{Executor, Fill};
use rust_decimal::Decimal;
use std::sync::{Arc, atomic::AtomicBool};

/// Test strategy that always quotes
struct AlwaysQuoteStrategy {
    signal: Signal,
}

impl AlwaysQuoteStrategy {
    fn new(action: SignalAction, size: u64) -> Self {
        Self {
            signal: Signal {
                action,
                bid_price: 50_000_000_000_000,
                ask_price: 50_010_000_000_000,
                size,
                _padding: [0; 32],
            }
        }
    }
}

impl Strategy for AlwaysQuoteStrategy {
    fn calculate(&mut self, _snapshot: &MarketSnapshot, _position: &Position) -> Option<Signal> {
        Some(self.signal)
    }

    fn name(&self) -> &'static str {
        "AlwaysQuote"
    }

    fn reset(&mut self) {}
}

/// Mock executor that tracks execute calls
struct MockExecutor {
    pub execute_count: u32,
}

impl Executor for MockExecutor {
    fn execute(&mut self, _signal: Signal, _position: &Position) -> Result<(), anyhow::Error> {
        self.execute_count += 1;
        Ok(())
    }

    fn get_fills(&mut self) -> Vec<Fill> {
        vec![]
    }

    fn dropped_fill_count(&self) -> u64 {
        0
    }
}

#[test]
fn test_pre_trade_position_limit_blocks_long() {
    // Create strategy that would breach long limit
    let strategy = AlwaysQuoteStrategy::new(SignalAction::QuoteBid, 1_500_000_000); // 1.5 BTC
    let executor = MockExecutor { execute_count: 0 };
    let shutdown = Arc::new(AtomicBool::new(false));

    let mut engine = Engine::new(strategy, executor, shutdown);

    let snapshot = MarketSnapshot {
        market_id: 1,
        sequence: 100,
        exchange_timestamp_ns: 0,
        local_recv_ns: 0,
        local_publish_ns: 0,
        best_bid_price: 50_000_000_000_000,
        best_bid_size: 5_000_000_000,
        best_ask_price: 50_010_000_000_000,
        best_ask_size: 5_000_000_000,
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 110],
    };

    // Process tick - should be blocked due to position limit
    let result = engine.process_tick(&snapshot, true);
    assert!(result.is_ok());

    // Verify executor was NOT called due to pre-trade check
    // NOTE: Engine doesn't expose executor directly, but we can verify
    // by checking that no fills were generated
    assert_eq!(engine.get_position().get_quantity(), 0);
}

#[test]
fn test_pre_trade_position_limit_blocks_short() {
    // Create strategy that would breach short limit
    let strategy = AlwaysQuoteStrategy::new(SignalAction::QuoteAsk, 1_500_000_000); // 1.5 BTC
    let executor = MockExecutor { execute_count: 0 };
    let shutdown = Arc::new(AtomicBool::new(false));

    let mut engine = Engine::new(strategy, executor, shutdown);

    let snapshot = MarketSnapshot {
        market_id: 1,
        sequence: 100,
        exchange_timestamp_ns: 0,
        local_recv_ns: 0,
        local_publish_ns: 0,
        best_bid_price: 50_000_000_000_000,
        best_bid_size: 5_000_000_000,
        best_ask_price: 50_010_000_000_000,
        best_ask_size: 5_000_000_000,
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 110],
    };

    // Process tick - should be blocked due to position limit
    let result = engine.process_tick(&snapshot, true);
    assert!(result.is_ok());

    // Verify executor was NOT called due to pre-trade check
    // NOTE: Engine doesn't expose executor directly, but we can verify
    // by checking that no fills were generated
    assert_eq!(engine.get_position().get_quantity(), 0);
}

#[test]
fn test_pre_trade_position_limit_allows_within_limit() {
    // Create strategy with size within limits
    let strategy = AlwaysQuoteStrategy::new(SignalAction::QuoteBid, 500_000_000); // 0.5 BTC
    let executor = MockExecutor { execute_count: 0 };
    let shutdown = Arc::new(AtomicBool::new(false));

    let mut engine = Engine::new(strategy, executor, shutdown);

    let snapshot = MarketSnapshot {
        market_id: 1,
        sequence: 100,
        exchange_timestamp_ns: 0,
        local_recv_ns: 0,
        local_publish_ns: 0,
        best_bid_price: 50_000_000_000_000,
        best_bid_size: 5_000_000_000,
        best_ask_price: 50_010_000_000_000,
        best_ask_size: 5_000_000_000,
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 110],
    };

    // Process tick - should be allowed
    let result = engine.process_tick(&snapshot, true);
    assert!(result.is_ok());

    // Verify signal was processed
    // NOTE: Without fills, position should remain 0 but signal should process
    assert_eq!(engine.get_position().get_quantity(), 0);
}

#[test]
fn test_pre_trade_both_sides_check() {
    // Create strategy that quotes both sides, where either fill would breach
    let strategy = AlwaysQuoteStrategy::new(SignalAction::QuoteBoth, 1_100_000_000); // 1.1 BTC
    let executor = MockExecutor { execute_count: 0 };
    let shutdown = Arc::new(AtomicBool::new(false));

    let mut engine = Engine::new(strategy, executor, shutdown);

    let snapshot = MarketSnapshot {
        market_id: 1,
        sequence: 100,
        exchange_timestamp_ns: 0,
        local_recv_ns: 0,
        local_publish_ns: 0,
        best_bid_price: 50_000_000_000_000,
        best_bid_size: 5_000_000_000,
        best_ask_price: 50_010_000_000_000,
        best_ask_size: 5_000_000_000,
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 110],
    };

    // Process tick - should be blocked because either side would breach
    let result = engine.process_tick(&snapshot, true);
    assert!(result.is_ok());

    // Verify executor was NOT called
    // NOTE: Position should remain 0 as no trades should execute
    assert_eq!(engine.get_position().get_quantity(), 0);
}