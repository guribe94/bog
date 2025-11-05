use super::{Executor, ExecutionMode, Fill, Order, OrderId, OrderStatus, Side};
use anyhow::{anyhow, Result};
use rust_decimal::Decimal;
use std::collections::HashMap;
use tracing::{debug, info};

/// Simulated executor for paper trading and backtesting
/// Immediately fills orders at requested prices (pessimistic simulation)
pub struct SimulatedExecutor {
    orders: HashMap<OrderId, Order>,
    pending_fills: Vec<Fill>,
    total_fills: u64,
    mode: ExecutionMode,
}

impl SimulatedExecutor {
    pub fn new() -> Self {
        info!("Initialized SimulatedExecutor");
        Self {
            orders: HashMap::new(),
            pending_fills: Vec::new(),
            total_fills: 0,
            mode: ExecutionMode::Simulated,
        }
    }

    /// Simulate a fill for an order
    fn simulate_fill(&mut self, order: &mut Order) -> Fill {
        let fill_size = order.remaining_size();
        let fill_price = order.price;

        // Update order
        order.filled_size += fill_size;
        order.status = if order.is_filled() {
            OrderStatus::Filled
        } else {
            OrderStatus::PartiallyFilled
        };
        order.updated_at = std::time::SystemTime::now();

        // Calculate average fill price
        order.avg_fill_price = Some(fill_price);

        // Create fill
        let fill = Fill::new(order.id.clone(), order.side, fill_price, fill_size);

        self.total_fills += 1;

        debug!(
            "Simulated fill: {} {} @ {} (size: {}, notional: {})",
            fill.side,
            fill.order_id,
            fill.price,
            fill.size,
            fill.notional()
        );

        fill
    }
}

impl Default for SimulatedExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor for SimulatedExecutor {
    fn place_order(&mut self, mut order: Order) -> Result<OrderId> {
        info!(
            "SIMULATED: Placing order {} {} @ {} (size: {})",
            order.side, order.id, order.price, order.size
        );

        // Validate order
        if order.size <= Decimal::ZERO {
            return Err(anyhow!("Order size must be positive"));
        }

        if order.price < Decimal::ZERO {
            return Err(anyhow!("Order price cannot be negative"));
        }

        // Update order status
        order.status = OrderStatus::Open;
        order.updated_at = std::time::SystemTime::now();

        // Simulate immediate fill (pessimistic for maker strategies)
        // In reality, limit orders may not fill immediately
        let fill = self.simulate_fill(&mut order);
        self.pending_fills.push(fill);

        let order_id = order.id.clone();
        self.orders.insert(order_id.clone(), order);

        Ok(order_id)
    }

    fn cancel_order(&mut self, order_id: &OrderId) -> Result<()> {
        info!("SIMULATED: Cancelling order {}", order_id);

        if let Some(order) = self.orders.get_mut(order_id) {
            if order.is_active() {
                order.status = OrderStatus::Cancelled;
                order.updated_at = std::time::SystemTime::now();
                debug!("Order {} cancelled", order_id);
                Ok(())
            } else {
                Err(anyhow!("Order {} is not active", order_id))
            }
        } else {
            Err(anyhow!("Order {} not found", order_id))
        }
    }

    fn get_fills(&mut self) -> Vec<Fill> {
        // Return and clear pending fills
        std::mem::take(&mut self.pending_fills)
    }

    fn get_order_status(&self, order_id: &OrderId) -> Option<OrderStatus> {
        self.orders.get(order_id).map(|o| o.status)
    }

    fn get_active_orders(&self) -> Vec<&Order> {
        self.orders
            .values()
            .filter(|o| o.is_active())
            .collect()
    }

    fn execution_mode(&self) -> ExecutionMode {
        self.mode
    }
}

/// More realistic fill simulator (for future enhancement)
/// This version considers market conditions and partial fills
pub struct RealisticSimulator {
    /// Probability of immediate fill (0.0 to 1.0)
    fill_probability: f64,
    /// Slippage in basis points for market orders
    slippage_bps: f64,
}

impl RealisticSimulator {
    pub fn new(fill_probability: f64, slippage_bps: f64) -> Self {
        Self {
            fill_probability,
            slippage_bps,
        }
    }

    /// Check if order should fill based on probability
    pub fn should_fill(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < self.fill_probability
    }

    /// Calculate slippage for an order
    pub fn apply_slippage(&self, price: Decimal, side: Side) -> Decimal {
        // Convert basis points to decimal (safe: slippage_bps is small)
        let slippage_decimal = Decimal::try_from(self.slippage_bps / 10000.0)
            .unwrap_or(Decimal::ZERO);
        let slippage_factor = Decimal::from(1) + slippage_decimal;

        match side {
            Side::Buy => price * slippage_factor,  // Pay more
            Side::Sell => price / slippage_factor, // Receive less
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_simulated_executor_creation() {
        let executor = SimulatedExecutor::new();
        assert_eq!(executor.execution_mode(), ExecutionMode::Simulated);
        assert_eq!(executor.get_active_orders().len(), 0);
    }

    #[test]
    fn test_place_and_fill_order() {
        let mut executor = SimulatedExecutor::new();

        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        let order_id = executor.place_order(order).unwrap();

        // Check order was placed
        assert!(executor.get_order_status(&order_id).is_some());

        // Check fill was created
        let fills = executor.get_fills();
        assert_eq!(fills.len(), 1);

        let fill = &fills[0];
        assert_eq!(fill.side, Side::Buy);
        assert_eq!(fill.price, dec!(50000));
        assert_eq!(fill.size, dec!(0.1));
        assert_eq!(fill.notional(), dec!(5000));
    }

    #[test]
    fn test_cancel_order() {
        let mut executor = SimulatedExecutor::new();

        // Place an order
        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        let order_id = executor.place_order(order).unwrap();

        // Clear fills
        executor.get_fills();

        // Try to cancel (but it's already filled in simulated mode)
        let result = executor.cancel_order(&order_id);
        assert!(result.is_err()); // Can't cancel filled order
    }

    #[test]
    fn test_invalid_order_size() {
        let mut executor = SimulatedExecutor::new();

        let order = Order::limit(Side::Buy, dec!(50000), dec!(0));
        let result = executor.place_order(order);

        assert!(result.is_err());
    }

    #[test]
    fn test_fill_calculations() {
        let mut executor = SimulatedExecutor::new();

        // Buy order
        let buy_order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        executor.place_order(buy_order).unwrap();

        let fills = executor.get_fills();
        let fill = &fills[0];

        assert_eq!(fill.position_change(), dec!(0.1)); // Position increases
        assert_eq!(fill.cash_flow(), dec!(-5000)); // Cash decreases

        // Sell order
        let sell_order = Order::limit(Side::Sell, dec!(50100), dec!(0.1));
        executor.place_order(sell_order).unwrap();

        let fills = executor.get_fills();
        let fill = &fills[0];

        assert_eq!(fill.position_change(), dec!(-0.1)); // Position decreases
        assert_eq!(fill.cash_flow(), dec!(5010)); // Cash increases
    }

    #[test]
    fn test_get_active_orders() {
        let mut executor = SimulatedExecutor::new();

        // In simulated mode, orders are immediately filled
        // So active orders will be 0 after placement

        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        executor.place_order(order).unwrap();

        // Order is filled, so no active orders
        assert_eq!(executor.get_active_orders().len(), 0);
    }

    #[test]
    fn test_realistic_simulator() {
        let sim = RealisticSimulator::new(0.8, 5.0);

        // Test slippage
        let buy_price = dec!(50000);
        let slipped_price = sim.apply_slippage(buy_price, Side::Buy);
        assert!(slipped_price > buy_price); // Buy pays more

        let sell_price = dec!(50000);
        let slipped_price = sim.apply_slippage(sell_price, Side::Sell);
        assert!(slipped_price < sell_price); // Sell receives less
    }
}
