use anyhow::Result;
use bog_core::core::{Position, Signal, SignalAction, Side as CoreSide};
use bog_core::data::{MarketSnapshot, SnapshotBuilder};
use bog_core::engine::{Engine, Executor, Strategy};
use bog_core::execution::{Fill, OrderId, Side};
use rust_decimal_macros::dec;

struct ScenarioStrategy;

impl Strategy for ScenarioStrategy {
    fn calculate(
        &mut self,
        snapshot: &MarketSnapshot,
        position: &Position,
    ) -> Option<Signal> {
        let price = snapshot.best_bid_price;
        let qty = position.get_quantity();

        if price == 10_000_000_000_000 && qty == 0 {
            Some(Signal::take_position(CoreSide::Buy, 1_000_000_000))
        } else if price == 20_000_000_000_000 && qty > 0 {
            Some(Signal::take_position(CoreSide::Sell, 1_000_000_000))
        } else if price == 20_001_000_000_000 && qty == 0 {
             Some(Signal::take_position(CoreSide::Buy, 1_000_000_000))
        } else {
            None
        }
    }

    fn name(&self) -> &'static str {
        "ScenarioStrategy"
    }
}

// Simple executor that instantly fills orders
struct InstantExecutor {
    fills: Vec<Fill>,
}

impl InstantExecutor {
    fn new() -> Self {
        Self { fills: Vec::new() }
    }
}

impl Executor for InstantExecutor {
    fn execute(&mut self, signal: Signal, _position: &Position) -> Result<()> {
        if let SignalAction::TakePosition = signal.action {
             let fill = Fill::new(
                OrderId::new_random(),
                match signal.side {
                    CoreSide::Buy => Side::Buy,
                    CoreSide::Sell => Side::Sell,
                },
                rust_decimal::Decimal::from(signal.bid_price) / rust_decimal::Decimal::from(1_000_000_000),
                rust_decimal::Decimal::from(signal.size) / rust_decimal::Decimal::from(1_000_000_000),
            );
            self.fills.push(fill);
        }
        Ok(())
    }

    fn cancel_all(&mut self) -> Result<()> {
        Ok(())
    }

    fn get_fills(&mut self, fills: &mut Vec<Fill>) {
        fills.append(&mut self.fills);
    }

    fn dropped_fill_count(&self) -> u64 {
        0
    }

    fn name(&self) -> &'static str {
        "InstantExecutor"
    }

    fn get_open_exposure(&self) -> (i64, i64) {
        (0, 0)
    }
}

#[test]
fn test_drawdown_includes_unrealized_pnl() -> Result<()> {
    let strategy = ScenarioStrategy;
    let executor = InstantExecutor::new();
    let mut engine = Engine::new(strategy, executor);

    // 1. Buy 1 BTC @ $10k
    let s1 = SnapshotBuilder::new()
        .best_bid(10_000_000_000_000, 1_000_000_000)
        .best_ask(10_000_000_000_000, 1_000_000_000)
        .build();
    engine.process_tick(&s1, true)?;
    assert_eq!(engine.position().get_quantity(), 1_000_000_000);

    // 2. Sell 1 BTC @ $20k (Realize $10k profit)
    let s2 = SnapshotBuilder::new()
        .best_bid(20_000_000_000_000, 1_000_000_000)
        .best_ask(20_000_000_000_000, 1_000_000_000)
        .build();
    engine.process_tick(&s2, true)?;
    assert_eq!(engine.position().get_quantity(), 0);
    
    // 3. Re-enter Buy 1 BTC @ $20,001
    let s3 = SnapshotBuilder::new()
        .best_bid(20_001_000_000_000, 1_000_000_000)
        .best_ask(20_001_000_000_000, 1_000_000_000)
        .build();
    engine.process_tick(&s3, true)?;
    assert_eq!(engine.position().get_quantity(), 1_000_000_000);

    // 4. Market Crash to $1k
    // Unrealized PnL: ($1k - $20,001) * 1 = -$19,001
    // Total PnL: $10,000 (Realized) - $19,001 (Unrealized) = -$9,001
    // Peak PnL: $10,000
    // Drawdown: $10,000 - (-$9,001) = $19,001
    // Allowed Drawdown (5% of Peak): $500
    
    let s4 = SnapshotBuilder::new()
        .best_bid(1_000_000_000_000, 1_000_000_000)
        .best_ask(1_000_000_000_000, 1_000_000_000)
        .build();
    
    let result = engine.process_tick(&s4, true);

    // Should halt due to drawdown exceeding limit
    assert!(result.is_err(), "Engine should have halted due to drawdown (Unrealized PnL must be included)");
    
    // Optional: verify error message contains "Drawdown limit exceeded"
    if let Err(e) = result {
        let msg = e.to_string();
        assert!(msg.contains("Drawdown limit exceeded"), "Error message mismatch: {}", msg);
    }

    Ok(())
}
