use bog_core::core::Position;
use bog_core::core::Signal;
use bog_core::data::MarketSnapshot;
use bog_core::engine::executor_bridge::ExecutorBridge;
use bog_core::engine::Engine;
use bog_core::engine::Executor; // Import trait for get_fills
use bog_core::engine::Strategy;
use bog_core::execution::simulated::SimulatedExecutor;
use bog_core::orderbook::L2OrderBook;
use rust_decimal_macros::dec;

// Strategy that always quotes 0.1 BTC on both sides
struct AlwaysQuoteStrategy;

impl Strategy for AlwaysQuoteStrategy {
    fn calculate(&mut self, _book: &L2OrderBook, _position: &Position) -> Option<Signal> {
        Some(Signal::quote_both(
            50_000_000_000_000, // $50,000
            50_010_000_000_000, // $50,010
            100_000_000,        // 0.1 BTC
        ))
    }

    fn name(&self) -> &'static str {
        "AlwaysQuoteStrategy"
    }
}

// Helper to create correctly padded snapshot
fn create_test_snapshot() -> MarketSnapshot {
    MarketSnapshot {
        market_id: 1,
        sequence: 1,
        exchange_timestamp_ns: 0,
        local_recv_ns: 0,
        local_publish_ns: 0,
        best_bid_price: 50_000_000_000_000,
        best_bid_size: 1_000_000_000,
        best_ask_price: 50_010_000_000_000,
        best_ask_size: 1_000_000_000,
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 54],
    }
}

#[test]
fn test_safety_rejects_low_balance() {
    // 1. Setup executor with ZERO balance
    let mut executor = SimulatedExecutor::new();
    executor.set_balance(dec!(0)); // $0 balance

    // 2. Wrap in bridge
    let bridged_executor = ExecutorBridge::new(executor);

    // 3. Setup engine
    let strategy = AlwaysQuoteStrategy;
    let mut engine = Engine::new(strategy, bridged_executor);

    // 4. Process tick
    let snapshot = create_test_snapshot();
    // We expect this to fail or do nothing because of insufficient balance
    // The engine propagates the error from the executor
    let _ = engine.process_tick(&snapshot, true);

    // 5. Verify NO trades occurred (order should be rejected pre-trade)
    // Since engine consumes fills, we check position stats
    let trade_count = engine.position().get_trade_count();
    assert_eq!(
        trade_count, 0,
        "Should have 0 trades with 0 balance! Safety check failed."
    );
}

#[test]
fn test_safety_allows_sufficient_balance() {
    // 1. Setup executor with SUFFICIENT balance
    let mut executor = SimulatedExecutor::new();
    executor.set_balance(dec!(100_000)); // $100k balance

    // 2. Wrap in bridge
    let bridged_executor = ExecutorBridge::new(executor);

    // 3. Setup engine
    let strategy = AlwaysQuoteStrategy;
    let mut engine = Engine::new(strategy, bridged_executor);

    // 4. Process tick
    let snapshot = create_test_snapshot();
    engine.process_tick(&snapshot, true).unwrap();

    // 5. Verify trades OCCURRED
    // Engine consumes fills and updates position
    let trade_count = engine.position().get_trade_count();
    assert!(
        trade_count > 0,
        "Should have trades with sufficient balance"
    );
    assert_eq!(trade_count, 2, "Should have exactly 2 trades (buy + sell)");
}
