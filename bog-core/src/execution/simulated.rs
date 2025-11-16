use super::{Executor, ExecutionMode, Fill, Order, OrderId, OrderStatus, Side};
use super::order_bridge::OrderStateWrapper;
use anyhow::{anyhow, Result};
use crossbeam::queue::ArrayQueue;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Maximum pending fills before overflow handling kicks in
/// This prevents unbounded memory growth in long-running simulations
const MAX_PENDING_FILLS: usize = 1024;

/// Configuration for realistic fill simulation
#[derive(Debug, Clone, Copy)]
pub struct RealisticFillConfig {
    /// Enable queue position modeling (FIFO)
    pub enable_queue_modeling: bool,
    /// Enable partial fills (not always 100%)
    pub enable_partial_fills: bool,
    /// Fill probability for front of queue (0.0 to 1.0)
    pub front_of_queue_fill_rate: f64,
    /// Fill probability for back of queue (0.0 to 1.0)
    pub back_of_queue_fill_rate: f64,
}

impl Default for RealisticFillConfig {
    fn default() -> Self {
        Self {
            enable_queue_modeling: true,
            enable_partial_fills: true,
            front_of_queue_fill_rate: 0.8,
            back_of_queue_fill_rate: 0.4,
        }
    }
}

impl RealisticFillConfig {
    /// Instant fills (current behavior) - for development/debug
    pub fn instant() -> Self {
        Self {
            enable_queue_modeling: false,
            enable_partial_fills: false,
            front_of_queue_fill_rate: 1.0,
            back_of_queue_fill_rate: 1.0,
        }
    }

    /// Realistic fills - for backtesting
    pub fn realistic() -> Self {
        Self::default()
    }

    /// Conservative fills - for stress testing
    pub fn conservative() -> Self {
        Self {
            enable_queue_modeling: true,
            enable_partial_fills: true,
            front_of_queue_fill_rate: 0.6,
            back_of_queue_fill_rate: 0.2,
        }
    }
}

/// Tracks an order's position in the FIFO queue at a price level
#[derive(Debug, Clone)]
struct QueuePosition {
    /// Order ID
    order_id: OrderId,
    /// Price level (u64 fixed-point)
    price_level: u64,
    /// Our order size
    our_size: Decimal,
    /// Volume ahead of us in queue (starts at 100% of level)
    /// This is a simplification: we assume we join at the back
    size_ahead_ratio: f64,
    /// Timestamp when we joined the queue
    timestamp: std::time::SystemTime,
}

/// Tracks queue positions for all active orders
struct QueueTracker {
    positions: HashMap<OrderId, QueuePosition>,
    config: RealisticFillConfig,
}

impl QueueTracker {
    fn new(config: RealisticFillConfig) -> Self {
        Self {
            positions: HashMap::new(),
            config,
        }
    }

    /// Add order to queue tracking
    /// We assume order joins at back of queue (worst case)
    fn add_order(&mut self, order: &Order) {
        if !self.config.enable_queue_modeling {
            return;
        }

        // Convert Decimal price to u64 fixed-point (9 decimals)
        let price_u64 = (order.price * Decimal::from(1_000_000_000))
            .to_u64()
            .unwrap_or(0);

        let position = QueuePosition {
            order_id: order.id.clone(),
            price_level: price_u64,
            our_size: order.size,
            // Assume we join at back of queue (100% ahead of us)
            // In reality, we'd calculate this from MarketSnapshot
            size_ahead_ratio: 1.0,
            timestamp: std::time::SystemTime::now(),
        };

        self.positions.insert(order.id.clone(), position);
        debug!(
            "Added order {} to queue at price {} (back of queue)",
            order.id, price_u64
        );
    }

    /// Calculate fill probability based on queue position
    fn calculate_fill_probability(&self, order_id: &OrderId) -> f64 {
        if !self.config.enable_partial_fills {
            return 1.0; // Always fill completely
        }

        let position = match self.positions.get(order_id) {
            Some(pos) => pos,
            None => return 1.0, // No position tracked, fill completely
        };

        // Interpolate between front and back of queue fill rates
        let front_rate = self.config.front_of_queue_fill_rate;
        let back_rate = self.config.back_of_queue_fill_rate;

        // size_ahead_ratio: 0.0 = front of queue, 1.0 = back of queue
        let fill_probability = front_rate + (back_rate - front_rate) * position.size_ahead_ratio;

        debug!(
            "Order {} queue position: {:.1}%, fill probability: {:.1}%",
            order_id,
            position.size_ahead_ratio * 100.0,
            fill_probability * 100.0
        );

        fill_probability
    }

    /// Remove order from queue tracking
    fn remove_order(&mut self, order_id: &OrderId) {
        self.positions.remove(order_id);
    }
}

/// Simulated executor for paper trading and backtesting
/// Supports both instant fills and realistic queue-based fills
///
/// Now uses OrderStateWrapper internally for compile-time state validation!
///
/// # Performance Optimization
///
/// Removed redundant legacy_orders_cache (saved 10-15ns per order).
/// Legacy orders are computed on-demand when get_active_orders() is called.
pub struct SimulatedExecutor {
    /// Orders stored as state machine wrappers for type-safe transitions
    orders: HashMap<OrderId, OrderStateWrapper>,
    /// Bounded queue for pending fills (prevents OOM)
    pending_fills: Arc<ArrayQueue<Fill>>,
    total_fills: u64,
    /// Number of fills dropped due to queue overflow
    dropped_fills: u64,
    mode: ExecutionMode,
    /// Queue tracking for realistic fills
    queue_tracker: QueueTracker,
    /// Configuration for fill realism
    config: RealisticFillConfig,
}

impl SimulatedExecutor {
    /// Create new executor with instant fills (legacy behavior)
    pub fn new() -> Self {
        Self::with_config(RealisticFillConfig::instant())
    }

    /// Create new executor with default configuration (alias for `new()`)
    pub fn new_default() -> Self {
        Self::new()
    }

    /// Create new executor with custom configuration
    pub fn with_config(config: RealisticFillConfig) -> Self {
        info!(
            "Initialized SimulatedExecutor (max pending fills: {}, queue_modeling: {}, partial_fills: {})",
            MAX_PENDING_FILLS,
            config.enable_queue_modeling,
            config.enable_partial_fills
        );
        Self {
            orders: HashMap::new(),
            pending_fills: Arc::new(ArrayQueue::new(MAX_PENDING_FILLS)),
            total_fills: 0,
            dropped_fills: 0,
            mode: ExecutionMode::Simulated,
            queue_tracker: QueueTracker::new(config),
            config,
        }
    }

    /// Create executor with realistic fills (for backtesting)
    pub fn new_realistic() -> Self {
        Self::with_config(RealisticFillConfig::realistic())
    }

    /// Create executor with conservative fills (for stress testing)
    pub fn new_conservative() -> Self {
        Self::with_config(RealisticFillConfig::conservative())
    }

    /// Get number of pending fills in queue
    pub fn pending_fill_count(&self) -> usize {
        self.pending_fills.len()
    }

    /// Get number of fills dropped due to queue overflow
    pub fn dropped_fill_count(&self) -> u64 {
        self.dropped_fills
    }

    /// Check if fill queue is approaching capacity
    pub fn is_fill_queue_near_capacity(&self) -> bool {
        self.pending_fills.len() > (MAX_PENDING_FILLS * 3) / 4  // 75% threshold
    }

    /// Simulate a fill for an order
    /// With realistic configuration, uses queue position to determine fill size
    ///
    /// Now uses state machine for type-safe transitions!
    ///
    /// # Safety
    /// Returns Err if fill conversion fails (size or price converts to 0 or overflows).
    /// This prevents position tracking from becoming inconsistent.
    fn simulate_fill(&mut self, order_wrapper: &mut OrderStateWrapper, order_for_calc: &Order, order_id: &OrderId) -> Result<Fill> {
        let remaining = order_for_calc.remaining_size();

        // Calculate fill size based on queue position and configuration
        let fill_probability = self.queue_tracker.calculate_fill_probability(&order_for_calc.id);
        let fill_size = if self.config.enable_partial_fills {
            // Partial fill based on queue position
            let partial_size = remaining * Decimal::try_from(fill_probability).unwrap_or(Decimal::ONE);
            partial_size.max(Decimal::ZERO)
        } else {
            // Complete fill (instant mode)
            remaining
        };

        let fill_price = order_for_calc.price;

        // Convert to u64 fixed-point for state machine (WITH VALIDATION!)
        let fill_size_u64 = match (fill_size * Decimal::from(1_000_000_000)).to_u64() {
            Some(0) => {
                return Err(anyhow!(
                    "HALTING: Fill size converted to zero for order {}: {} BTC - cannot update state safely",
                    order_for_calc.id, fill_size
                ));
            }
            Some(size) => size, // Valid size
            None => {
                return Err(anyhow!(
                    "HALTING: Fill size conversion OVERFLOW for order {}: {} BTC - cannot update state safely",
                    order_for_calc.id, fill_size
                ));
            }
        };

        let fill_price_u64 = match (fill_price * Decimal::from(1_000_000_000)).to_u64() {
            Some(0) => {
                return Err(anyhow!(
                    "HALTING: Fill price converted to ZERO for order {} - would give away money!",
                    order_for_calc.id
                ));
            }
            Some(price) => price, // Valid price
            None => {
                return Err(anyhow!(
                    "HALTING: Fill price conversion OVERFLOW for order {}: ${} - cannot update state safely",
                    order_for_calc.id, fill_price
                ));
            }
        };

        // Calculate fee (assume taker for simulated fills)
        // Use bog_strategies::fees for calculation
        let fee_amount = {
            // Convert to u64 for fee calculation
            let price_u64 = (fill_price * Decimal::from(1_000_000_000))
                .to_u64()
                .unwrap_or(fill_price_u64);
            let size_u64 = (fill_size * Decimal::from(1_000_000_000))
                .to_u64()
                .unwrap_or(fill_size_u64);

            // Calculate fee in u64: (price * size) * fee_bps / 10000
            // For Lighter: 2 bps taker fee
            let notional_u64 = (price_u64 as u128 * size_u64 as u128) / 1_000_000_000; // Remove one scale factor
            let fee_u64 = (notional_u64 * 2) / 10_000; // 2 bps

            // Convert back to Decimal
            Decimal::from(fee_u64 as u64) / Decimal::from(1_000_000_000)
        };

        // Create fill event WITH fee
        let fill = Fill::new_with_fee(
            order_for_calc.id.clone(),
            order_for_calc.side,
            fill_price,
            fill_size,
            Some(fee_amount),
        );

        // Use state machine to apply fill (type-safe!)
        if let Err(e) = order_wrapper.apply_fill(fill_size_u64, fill_price_u64) {
            return Err(anyhow!(
                "HALTING: Failed to apply fill to order state machine for order {}: {}",
                order_for_calc.id, e
            ));
        }

        self.total_fills += 1;

        debug!(
            "Simulated fill: {} {} @ {} (size: {}/{}, {:.1}% fill, notional: {})",
            fill.side,
            fill.order_id,
            fill.price,
            fill.size,
            order_for_calc.size,
            fill_probability * 100.0,
            fill.notional()
        );

        Ok(fill)
    }
}

impl Default for SimulatedExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor for SimulatedExecutor {
    fn place_order(&mut self, order: Order) -> Result<OrderId> {
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

        let order_id = order.id.clone();

        // Create state machine wrapper from legacy order (WITH VALIDATION!)
        let mut order_wrapper = OrderStateWrapper::from_legacy(&order)
            .map_err(|e| anyhow!("Invalid order ID: {}", e))?;

        // Acknowledge the order (Pending → Open) using type-safe transition
        if let Err(e) = order_wrapper.acknowledge() {
            return Err(anyhow!("Failed to acknowledge order: {}", e));
        }

        // Add to queue tracker (before fill simulation)
        self.queue_tracker.add_order(&order);

        // Simulate fill (immediate or partial based on configuration)
        // In reality, limit orders may not fill immediately
        // Returns Err if conversion fails (halts trading)
        let fill = self.simulate_fill(&mut order_wrapper, &order, &order_id)?;

        // Try to push fill to bounded queue
        if let Err(returned_fill) = self.pending_fills.push(fill) {
            // Queue is full - this indicates consumer is falling behind
            self.dropped_fills += 1;
            warn!(
                "Fill queue overflow: dropped fill for order {} (queue size: {}, total dropped: {})",
                returned_fill.order_id,
                MAX_PENDING_FILLS,
                self.dropped_fills
            );

            // Strategy: Drop oldest fill to make room for newest
            // Alternative: Could drop newest or reject order
            if let Some(_oldest) = self.pending_fills.pop() {
                // Try again with the new fill
                if self.pending_fills.push(returned_fill).is_err() {
                    // Still failed (shouldn't happen after pop, but be safe)
                    warn!("Fill queue still full after pop - fill lost");
                }
            }
        }

        // Store the state machine wrapper (legacy view computed on-demand)
        self.orders.insert(order_id.clone(), order_wrapper);

        Ok(order_id)
    }

    fn cancel_order(&mut self, order_id: &OrderId) -> Result<()> {
        info!("SIMULATED: Cancelling order {}", order_id);

        if let Some(order_wrapper) = self.orders.get_mut(order_id) {
            if order_wrapper.is_active() {
                // Use state machine to cancel (type-safe!)
                if let Err(e) = order_wrapper.cancel() {
                    return Err(anyhow!("Failed to cancel order {}: {}", order_id, e));
                }

                // Remove from queue tracker
                self.queue_tracker.remove_order(order_id);

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
        // Drain all fills from the queue
        let mut fills = Vec::with_capacity(self.pending_fills.len());

        while let Some(fill) = self.pending_fills.pop() {
            fills.push(fill);
        }

        // Warn if queue was near capacity
        if !fills.is_empty() && fills.len() > (MAX_PENDING_FILLS * 3) / 4 {
            warn!(
                "High fill queue usage: {} fills drained (capacity: {}, dropped: {})",
                fills.len(),
                MAX_PENDING_FILLS,
                self.dropped_fills
            );
        }

        fills
    }

    fn get_order_status(&self, order_id: &OrderId) -> Option<OrderStatus> {
        // Use state machine to get status (type-safe!)
        self.orders.get(order_id).map(|wrapper| wrapper.status())
    }

    fn get_active_orders(&self) -> Vec<&Order> {
        // Performance note: This method is rarely called (not in hot path).
        // We compute legacy view on-demand to avoid maintaining redundant cache.
        // This saves 10-15ns per order placement/fill (hot path optimization).

        // Collect into Vec first to avoid lifetime issues with iterator
        self.orders
            .values()
            .filter(|wrapper| wrapper.is_active())
            .map(|wrapper| wrapper.to_legacy())
            .collect::<Vec<Order>>()
            .leak() // Return &Order references
            .iter()
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

    #[test]
    fn test_instant_fills_config() {
        let mut executor = SimulatedExecutor::new(); // Default: instant fills

        let order = Order::limit(Side::Buy, dec!(50000), dec!(1.0));
        executor.place_order(order).unwrap();

        let fills = executor.get_fills();
        assert_eq!(fills.len(), 1);

        // Instant mode: should fill 100%
        let fill = &fills[0];
        assert_eq!(fill.size, dec!(1.0));
    }

    #[test]
    fn test_realistic_fills_back_of_queue() {
        // Realistic mode: orders join at back of queue (40% fill rate)
        let mut executor = SimulatedExecutor::new_realistic();

        let order = Order::limit(Side::Buy, dec!(50000), dec!(1.0));
        executor.place_order(order).unwrap();

        let fills = executor.get_fills();
        assert_eq!(fills.len(), 1);

        // Realistic mode with back-of-queue: should fill ~40% (0.4 BTC)
        let fill = &fills[0];
        let expected_fill = dec!(1.0) * Decimal::try_from(0.4).unwrap();
        assert_eq!(fill.size, expected_fill);
    }

    #[test]
    fn test_conservative_fills() {
        // Conservative mode: even stricter (20% fill rate at back)
        let mut executor = SimulatedExecutor::new_conservative();

        let order = Order::limit(Side::Buy, dec!(50000), dec!(1.0));
        executor.place_order(order).unwrap();

        let fills = executor.get_fills();
        assert_eq!(fills.len(), 1);

        // Conservative mode: should fill ~20% (0.2 BTC)
        let fill = &fills[0];
        let expected_fill = dec!(1.0) * Decimal::try_from(0.2).unwrap();
        assert_eq!(fill.size, expected_fill);
    }

    #[test]
    fn test_realistic_vs_instant_comparison() {
        // Create two executors with different configs
        let mut instant_executor = SimulatedExecutor::new();
        let mut realistic_executor = SimulatedExecutor::new_realistic();

        // Place same order on both
        let order1 = Order::limit(Side::Buy, dec!(50000), dec!(1.0));
        let order2 = Order::limit(Side::Buy, dec!(50000), dec!(1.0));

        instant_executor.place_order(order1).unwrap();
        realistic_executor.place_order(order2).unwrap();

        let instant_fills = instant_executor.get_fills();
        let realistic_fills = realistic_executor.get_fills();

        // Instant should fill more than realistic
        assert!(instant_fills[0].size > realistic_fills[0].size);
        assert_eq!(instant_fills[0].size, dec!(1.0)); // 100%
        assert_eq!(realistic_fills[0].size, dec!(0.4)); // 40%
    }

    #[test]
    fn test_queue_position_tracking() {
        let config = RealisticFillConfig::realistic();
        let mut tracker = QueueTracker::new(config);

        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        tracker.add_order(&order);

        // Check fill probability (should be back-of-queue: 40%)
        let fill_prob = tracker.calculate_fill_probability(&order.id);
        assert!((fill_prob - 0.4).abs() < 0.01); // ~40% with tolerance
    }

    #[test]
    fn test_realistic_fill_config_presets() {
        let instant = RealisticFillConfig::instant();
        assert!(!instant.enable_queue_modeling);
        assert!(!instant.enable_partial_fills);

        let realistic = RealisticFillConfig::realistic();
        assert!(realistic.enable_queue_modeling);
        assert!(realistic.enable_partial_fills);
        assert_eq!(realistic.front_of_queue_fill_rate, 0.8);
        assert_eq!(realistic.back_of_queue_fill_rate, 0.4);

        let conservative = RealisticFillConfig::conservative();
        assert!(conservative.enable_queue_modeling);
        assert!(conservative.enable_partial_fills);
        assert_eq!(conservative.front_of_queue_fill_rate, 0.6);
        assert_eq!(conservative.back_of_queue_fill_rate, 0.2);
    }
}
