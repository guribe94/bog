use super::{Signal, Strategy, StrategyState, StrategyStats};
use crate::data::MarketSnapshot;
use crate::execution::Fill;
use crate::orderbook::OrderBookManager;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use tracing::{debug, info};

/// Simple spread-based market making strategy
/// Posts quotes at a fixed spread around mid price
pub struct SimpleSpreadStrategy {
    spread_bps: f64,
    min_spread_bps: f64,
    order_size: Decimal,
    state: StrategyState,
    stats: StrategyStats,
}

impl SimpleSpreadStrategy {
    pub fn new(spread_bps: f64, order_size: Decimal, min_spread_bps: f64) -> Self {
        info!(
            "Initialized SimpleSpreadStrategy: spread={}bps, size={}, min_spread={}bps",
            spread_bps, order_size, min_spread_bps
        );

        Self {
            spread_bps,
            min_spread_bps,
            order_size,
            state: StrategyState::Active,
            stats: StrategyStats::default(),
        }
    }

    /// Calculate quote prices based on mid price and spread
    fn calculate_quotes(&self, mid_price: Decimal) -> (Decimal, Decimal) {
        // Convert basis points to decimal factor (safe: spread_bps is small)
        let spread_factor =
            Decimal::try_from(self.spread_bps / 10000.0 / 2.0).unwrap_or(Decimal::ZERO);

        let bid_price = mid_price * (Decimal::ONE - spread_factor);
        let ask_price = mid_price * (Decimal::ONE + spread_factor);

        (bid_price, ask_price)
    }
}

impl Strategy for SimpleSpreadStrategy {
    fn on_update(
        &mut self,
        _snapshot: &MarketSnapshot,
        orderbook: &OrderBookManager,
    ) -> Option<Signal> {
        // Don't generate signals if not active
        if self.state != StrategyState::Active {
            return None;
        }

        // Check if orderbook is valid
        if !orderbook.is_valid() {
            debug!("OrderBook invalid, skipping signal generation");
            return None;
        }

        // Get mid price
        let mid_price = orderbook.mid_price();
        if mid_price <= Decimal::ZERO {
            debug!("Invalid mid price, skipping signal generation");
            return None;
        }

        // Check market spread
        let market_spread_bps = orderbook.spread_bps() as f64; // spread_bps() now returns u32
        if market_spread_bps < self.min_spread_bps {
            debug!(
                "Market spread too tight ({:.2}bps < {:.2}bps), skipping",
                market_spread_bps, self.min_spread_bps
            );
            return None;
        }

        // Calculate our quote prices
        let (bid_price, ask_price) = self.calculate_quotes(mid_price);

        // Generate signal
        self.stats.signals_generated += 1;

        debug!(
            "Generated quote: bid={}, ask={}, mid={}, spread={:.2}bps",
            bid_price, ask_price, mid_price, self.spread_bps
        );

        Some(Signal::QuoteBoth {
            bid_price,
            ask_price,
            size: self.order_size,
        })
    }

    fn on_fill(&mut self, fill: &Fill) {
        self.stats.fills_received += 1;

        let volume = fill.notional().to_f64().unwrap_or(0.0);
        match fill.side {
            crate::execution::Side::Buy => self.stats.total_buy_volume += volume,
            crate::execution::Side::Sell => self.stats.total_sell_volume += volume,
        }

        info!(
            "Fill received: {} {} @ {} (notional: {})",
            fill.side,
            fill.size,
            fill.price,
            fill.notional()
        );
    }

    fn state(&self) -> StrategyState {
        self.state
    }

    fn pause(&mut self) {
        info!("Pausing SimpleSpreadStrategy");
        self.state = StrategyState::Paused;
    }

    fn resume(&mut self) {
        info!("Resuming SimpleSpreadStrategy");
        self.state = StrategyState::Active;
    }

    fn stop(&mut self) {
        info!("Stopping SimpleSpreadStrategy");
        self.state = StrategyState::Stopped;
    }

    fn name(&self) -> &str {
        "SimpleSpread"
    }

    fn stats(&self) -> StrategyStats {
        self.stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::conversions::f64_to_u64;
    use rust_decimal_macros::dec;

    fn create_test_snapshot(bid: f64, ask: f64) -> MarketSnapshot {
        MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: f64_to_u64(bid),
            best_bid_size: f64_to_u64(1.0),
            best_ask_price: f64_to_u64(ask),
            best_ask_size: f64_to_u64(1.0),
            dex_type: 1,
            ..Default::default()
        }
    }

    #[test]
    fn test_strategy_creation() {
        let strategy = SimpleSpreadStrategy::new(10.0, dec!(0.1), 1.0);

        assert_eq!(strategy.spread_bps, 10.0);
        assert_eq!(strategy.order_size, dec!(0.1));
        assert_eq!(strategy.state(), StrategyState::Active);
        assert_eq!(strategy.name(), "SimpleSpread");
    }

    #[test]
    fn test_calculate_quotes() {
        let strategy = SimpleSpreadStrategy::new(20.0, dec!(0.1), 1.0);

        let mid_price = dec!(50000);
        let (bid, ask) = strategy.calculate_quotes(mid_price);

        // 20bps spread = 0.2% = 0.001 each side
        // bid = 50000 * (1 - 0.001) = 49950
        // ask = 50000 * (1 + 0.001) = 50050
        assert!(bid < mid_price);
        assert!(ask > mid_price);
        assert!(bid > dec!(49900) && bid < dec!(50000));
        assert!(ask > dec!(50000) && ask < dec!(50100));
    }

    #[test]
    fn test_signal_generation() {
        let mut strategy = SimpleSpreadStrategy::new(10.0, dec!(0.1), 1.0);
        let mut orderbook = OrderBookManager::new(1);

        let snapshot = create_test_snapshot(50000.0, 50010.0);
        orderbook.sync_from_snapshot(&snapshot);

        let signal = strategy.on_update(&snapshot, &orderbook);

        assert!(signal.is_some());
        if let Some(Signal::QuoteBoth {
            bid_price,
            ask_price,
            size,
        }) = signal
        {
            assert!(bid_price < orderbook.mid_price());
            assert!(ask_price > orderbook.mid_price());
            assert_eq!(size, dec!(0.1));
        } else {
            panic!("Expected QuoteBoth signal");
        }

        assert_eq!(strategy.stats().signals_generated, 1);
    }

    #[test]
    fn test_paused_strategy() {
        let mut strategy = SimpleSpreadStrategy::new(10.0, dec!(0.1), 1.0);
        let mut orderbook = OrderBookManager::new(1);

        strategy.pause();
        assert_eq!(strategy.state(), StrategyState::Paused);

        let snapshot = create_test_snapshot(50000.0, 50010.0);
        orderbook.sync_from_snapshot(&snapshot);

        let signal = strategy.on_update(&snapshot, &orderbook);
        assert!(signal.is_none()); // Should not generate signals when paused
    }

    #[test]
    fn test_min_spread_check() {
        let mut strategy = SimpleSpreadStrategy::new(10.0, dec!(0.1), 5.0);
        let mut orderbook = OrderBookManager::new(1);

        // Market spread = 1bp (too tight)
        let snapshot = create_test_snapshot(50000.0, 50005.0); // 1bp spread
        orderbook.sync_from_snapshot(&snapshot);

        let signal = strategy.on_update(&snapshot, &orderbook);
        assert!(signal.is_none()); // Should not trade when spread too tight
    }

    #[test]
    fn test_on_fill() {
        use crate::execution::{OrderId, Side};

        let mut strategy = SimpleSpreadStrategy::new(10.0, dec!(0.1), 1.0);

        let fill = Fill::new(OrderId::new_random(), Side::Buy, dec!(50000), dec!(0.1));

        strategy.on_fill(&fill);

        let stats = strategy.stats();
        assert_eq!(stats.fills_received, 1);
        assert!(stats.total_buy_volume > 0.0);
    }
}
