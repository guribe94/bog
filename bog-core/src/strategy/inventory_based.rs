use super::{Signal, Strategy, StrategyState, StrategyStats};
use crate::data::MarketSnapshot;
use crate::execution::Fill;
use crate::orderbook::OrderBookManager;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use tracing::{debug, info};

/// Inventory-based market making strategy (Avellaneda-Stoikov model)
/// Adjusts quotes based on inventory risk to stay near target position
///
/// Reference: "High-frequency trading in a limit order book" (Avellaneda & Stoikov, 2008)
pub struct InventoryBasedStrategy {
    /// Target inventory (neutral position)
    target_inventory: Decimal,

    /// Current inventory
    current_inventory: Decimal,

    /// Risk aversion parameter (gamma)
    /// Higher values = more aggressive inventory management
    risk_aversion: f64,

    /// Order size per quote
    order_size: Decimal,

    /// Volatility estimate (annualized)
    volatility: f64,

    /// Time horizon in seconds (T)
    time_horizon_secs: f64,

    /// Strategy state
    state: StrategyState,

    /// Statistics
    stats: StrategyStats,
}

impl InventoryBasedStrategy {
    pub fn new(
        target_inventory: Decimal,
        risk_aversion: f64,
        order_size: Decimal,
        volatility: f64,
        time_horizon_secs: f64,
    ) -> Self {
        info!(
            "Initialized InventoryBasedStrategy: target={}, gamma={}, size={}, vol={}, horizon={}s",
            target_inventory, risk_aversion, order_size, volatility, time_horizon_secs
        );

        Self {
            target_inventory,
            current_inventory: Decimal::ZERO,
            risk_aversion,
            order_size,
            volatility,
            time_horizon_secs,
            state: StrategyState::Active,
            stats: StrategyStats::default(),
        }
    }

    /// Calculate reservation price (indifference price)
    /// r = s - q * gamma * sigma^2 * T
    /// where:
    ///   s = mid price
    ///   q = inventory (relative to target)
    ///   gamma = risk aversion
    ///   sigma = volatility
    ///   T = time horizon
    fn calculate_reservation_price(&self, mid_price: Decimal) -> Decimal {
        let q = (self.current_inventory - self.target_inventory)
            .to_f64()
            .unwrap_or(0.0);

        let adjustment =
            q * self.risk_aversion * self.volatility.powi(2) * self.time_horizon_secs;

        let adjustment_decimal = Decimal::try_from(adjustment).unwrap_or(Decimal::ZERO);

        mid_price - adjustment_decimal
    }

    /// Calculate optimal spread
    /// delta = gamma * sigma^2 * T + (2/gamma) * ln(1 + gamma/k)
    /// Simplified to: gamma * sigma^2 * T (assuming high-frequency limit)
    fn calculate_optimal_spread(&self) -> Decimal {
        let spread = self.risk_aversion * self.volatility.powi(2) * self.time_horizon_secs;

        Decimal::try_from(spread).unwrap_or(Decimal::from(10)) // Min 10 default
    }

    /// Calculate quote prices
    fn calculate_quotes(&self, mid_price: Decimal) -> (Decimal, Decimal) {
        let reservation_price = self.calculate_reservation_price(mid_price);
        let half_spread = self.calculate_optimal_spread() / Decimal::from(2);

        let bid_price = reservation_price - half_spread;
        let ask_price = reservation_price + half_spread;

        (bid_price, ask_price)
    }

    /// Update inventory from fill
    fn update_inventory(&mut self, fill: &Fill) {
        let position_change = fill.position_change();
        self.current_inventory += position_change;

        info!(
            "Inventory updated: {} (change: {}, distance from target: {})",
            self.current_inventory,
            position_change,
            self.current_inventory - self.target_inventory
        );
    }

    /// Check if inventory is too far from target
    fn is_inventory_risk_high(&self) -> bool {
        let distance = (self.current_inventory - self.target_inventory).abs();
        let threshold = self.order_size * Decimal::from(10); // 10x order size

        distance > threshold
    }
}

impl Strategy for InventoryBasedStrategy {
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

        // Warn if inventory risk is high
        if self.is_inventory_risk_high() {
            debug!(
                "High inventory risk: current={}, target={}, distance={}",
                self.current_inventory,
                self.target_inventory,
                (self.current_inventory - self.target_inventory).abs()
            );
        }

        // Calculate our quote prices based on inventory
        let (bid_price, ask_price) = self.calculate_quotes(mid_price);

        // Ensure prices are positive
        if bid_price <= Decimal::ZERO || ask_price <= Decimal::ZERO {
            debug!("Calculated negative prices, skipping");
            return None;
        }

        // Ensure bid < ask
        if bid_price >= ask_price {
            debug!("Bid >= Ask after inventory adjustment, skipping");
            return None;
        }

        // Generate signal
        self.stats.signals_generated += 1;

        let reservation_price = self.calculate_reservation_price(mid_price);
        debug!(
            "Generated inventory-adjusted quote: bid={}, ask={}, mid={}, reservation={}, inventory={}",
            bid_price, ask_price, mid_price, reservation_price, self.current_inventory
        );

        Some(Signal::QuoteBoth {
            bid_price,
            ask_price,
            size: self.order_size,
        })
    }

    fn on_fill(&mut self, fill: &Fill) {
        self.stats.fills_received += 1;

        // Update inventory
        self.update_inventory(fill);

        let volume = fill.notional().to_f64().unwrap_or(0.0);
        match fill.side {
            crate::execution::Side::Buy => self.stats.total_buy_volume += volume,
            crate::execution::Side::Sell => self.stats.total_sell_volume += volume,
        }

        info!(
            "Fill received: {} {} @ {} (notional: {}, new inventory: {})",
            fill.side,
            fill.size,
            fill.price,
            fill.notional(),
            self.current_inventory
        );
    }

    fn state(&self) -> StrategyState {
        self.state
    }

    fn pause(&mut self) {
        info!("Pausing InventoryBasedStrategy");
        self.state = StrategyState::Paused;
    }

    fn resume(&mut self) {
        info!("Resuming InventoryBasedStrategy");
        self.state = StrategyState::Active;
    }

    fn stop(&mut self) {
        info!("Stopping InventoryBasedStrategy");
        self.state = StrategyState::Stopped;
    }

    fn name(&self) -> &str {
        "InventoryBased"
    }

    fn stats(&self) -> StrategyStats {
        self.stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::conversions::f64_to_u64;
    use crate::execution::{OrderId, Side};
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
        let strategy = InventoryBasedStrategy::new(
            dec!(0), // Target neutral
            0.1,     // Risk aversion
            dec!(0.1),
            0.02,  // 2% volatility
            300.0, // 5 minute horizon
        );

        assert_eq!(strategy.target_inventory, dec!(0));
        assert_eq!(strategy.current_inventory, dec!(0));
        assert_eq!(strategy.state(), StrategyState::Active);
        assert_eq!(strategy.name(), "InventoryBased");
    }

    #[test]
    fn test_reservation_price() {
        let mut strategy = InventoryBasedStrategy::new(
            dec!(0),
            0.1,
            dec!(0.1),
            0.02,
            300.0,
        );

        let mid_price = dec!(50000);

        // At neutral inventory, reservation price = mid price
        let r = strategy.calculate_reservation_price(mid_price);
        assert!((r - mid_price).abs() < dec!(1)); // Should be very close

        // Long inventory -> lower reservation price (want to sell)
        strategy.current_inventory = dec!(1.0);
        let r = strategy.calculate_reservation_price(mid_price);
        assert!(r < mid_price);

        // Short inventory -> higher reservation price (want to buy)
        strategy.current_inventory = dec!(-1.0);
        let r = strategy.calculate_reservation_price(mid_price);
        assert!(r > mid_price);
    }

    #[test]
    fn test_optimal_spread() {
        let strategy = InventoryBasedStrategy::new(
            dec!(0),
            0.1,
            dec!(0.1),
            0.02,
            300.0,
        );

        let spread = strategy.calculate_optimal_spread();
        assert!(spread > Decimal::ZERO);
    }

    #[test]
    fn test_signal_generation() {
        let mut strategy = InventoryBasedStrategy::new(
            dec!(0),
            0.1,
            dec!(0.1),
            0.02,
            300.0,
        );

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
            assert!(bid_price > Decimal::ZERO);
            assert!(ask_price > bid_price);
            assert_eq!(size, dec!(0.1));
        } else {
            panic!("Expected QuoteBoth signal");
        }
    }

    #[test]
    fn test_inventory_adjustment() {
        let mut strategy = InventoryBasedStrategy::new(
            dec!(0),
            0.1,
            dec!(0.1),
            0.02,
            300.0,
        );

        let mut orderbook = OrderBookManager::new(1);
        let snapshot = create_test_snapshot(50000.0, 50010.0);
        orderbook.sync_from_snapshot(&snapshot);

        // Get neutral quotes
        let signal_neutral = strategy.on_update(&snapshot, &orderbook).unwrap();

        // Simulate buying (become long)
        let fill = Fill::new(OrderId::new_random(), Side::Buy, dec!(50000), dec!(1.0));
        strategy.on_fill(&fill);

        assert_eq!(strategy.current_inventory, dec!(1.0));

        // Get adjusted quotes (should be skewed to encourage selling)
        let signal_long = strategy.on_update(&snapshot, &orderbook).unwrap();

        // When long, bid should be lower and ask should be lower (skewed to sell)
        if let (
            Signal::QuoteBoth {
                bid_price: bid_neutral,
                ask_price: ask_neutral,
                ..
            },
            Signal::QuoteBoth {
                bid_price: bid_long,
                ask_price: ask_long,
                ..
            },
        ) = (signal_neutral, signal_long)
        {
            // Both prices should shift down when long
            assert!(bid_long < bid_neutral);
            assert!(ask_long < ask_neutral);
        }
    }

    #[test]
    fn test_high_inventory_risk() {
        let mut strategy = InventoryBasedStrategy::new(
            dec!(0),
            0.1,
            dec!(0.1),
            0.02,
            300.0,
        );

        assert!(!strategy.is_inventory_risk_high());

        // Large position
        strategy.current_inventory = dec!(10.0);
        assert!(strategy.is_inventory_risk_high());
    }

    #[test]
    fn test_paused_strategy() {
        let mut strategy = InventoryBasedStrategy::new(
            dec!(0),
            0.1,
            dec!(0.1),
            0.02,
            300.0,
        );

        strategy.pause();

        let mut orderbook = OrderBookManager::new(1);
        let snapshot = create_test_snapshot(50000.0, 50010.0);
        orderbook.sync_from_snapshot(&snapshot);

        let signal = strategy.on_update(&snapshot, &orderbook);
        assert!(signal.is_none());
    }
}
