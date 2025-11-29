//! Simulated Executor - Zero-Overhead Implementation
//!
//! This executor simulates order execution for backtesting/paper trading with:
//! - Object pools for zero allocations
//! - u64 fixed-point arithmetic
//! - Lock-free data structures
//! - Cache-aligned atomics
//!
//! Target: <200ns per execution

use super::Executor;
use super::risk;
use crate::core::{OrderId, Position, Signal, SignalAction, Side as CoreSide};
use crate::execution::{Fill, Side as ExecutionSide};
use crate::perf::pools::ObjectPool;
use anyhow::Result;
use crossbeam::queue::ArrayQueue;
use rust_decimal::Decimal;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ===== POOL-COMPATIBLE TYPES =====

/// Pool-compatible Order (zero-overhead)
///
/// Uses u64 fixed-point (9 decimals) and avoids timestamps.
#[derive(Debug, Clone, Copy, Default)]
pub struct PooledOrder {
    pub id: OrderId,
    pub side: OrderSide,
    pub price: u64,         // Fixed-point, 9 decimals
    pub size: u64,          // Fixed-point, 9 decimals
    pub filled_size: u64,   // Fixed-point, 9 decimals
    pub status: OrderStatusBits,
}

impl PooledOrder {
    #[inline(always)]
    pub fn new(id: OrderId, side: OrderSide, price: u64, size: u64) -> Self {
        Self {
            id,
            side,
            price,
            size,
            filled_size: 0,
            status: OrderStatusBits::Open,
        }
    }

    #[inline(always)]
    pub fn remaining_size(&self) -> u64 {
        self.size.saturating_sub(self.filled_size)
    }

    #[inline(always)]
    pub fn is_filled(&self) -> bool {
        self.filled_size >= self.size
    }

    #[inline(always)]
    pub fn is_active(&self) -> bool {
        self.status == OrderStatusBits::Open || self.status == OrderStatusBits::PartiallyFilled
    }
}

/// Pool-compatible Fill (zero-overhead)
#[derive(Debug, Clone, Copy, Default)]
pub struct PooledFill {
    pub order_id: OrderId,
    pub side: OrderSide,
    pub price: u64,  // Fixed-point, 9 decimals
    pub size: u64,   // Fixed-point, 9 decimals
}

impl PooledFill {
    #[inline(always)]
    pub fn new(order_id: OrderId, side: OrderSide, price: u64, size: u64) -> Self {
        Self {
            order_id,
            side,
            price,
            size,
        }
    }

    #[inline(always)]
    pub fn notional(&self) -> u64 {
        // Calculate price * size (both in 9 decimal fixed-point)
        // Result needs to be scaled back
        ((self.price as u128 * self.size as u128) / 1_000_000_000) as u64
    }

    #[inline(always)]
    pub fn position_change(&self) -> i64 {
        match self.side {
            OrderSide::Buy => self.size as i64,
            OrderSide::Sell => -(self.size as i64),
        }
    }
}

/// Order side (Copy, 1 byte)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum OrderSide {
    #[default]
    Buy = 0,
    Sell = 1,
}

/// Order status (Copy, 1 byte)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum OrderStatusBits {
    #[default]
    Open = 0,
    PartiallyFilled = 1,
    Filled = 2,
    Cancelled = 3,
}

// ===== SIMULATED EXECUTOR =====

/// Simulated Executor with Object Pools
///
/// This executor immediately fills all orders (pessimistic simulation).
/// For HFT strategies, this represents worst-case execution.
pub struct SimulatedExecutor {
    /// Object pool for orders (256 capacity)
    /// Reserved for Phase 7: realistic simulation with order tracking
    #[allow(dead_code)]
    order_pool: Arc<ObjectPool<PooledOrder>>,

    /// Object pool for fills (1024 capacity)
    /// Reserved for Phase 7: realistic simulation with fill tracking
    #[allow(dead_code)]
    fill_pool: Arc<ObjectPool<PooledFill>>,

    /// Pending fills queue (lock-free)
    pending_fills: Arc<ArrayQueue<PooledFill>>,

    /// Active orders queue (lock-free)
    /// Reserved for Phase 7: realistic simulation with active order tracking
    #[allow(dead_code)]
    active_orders: Arc<ArrayQueue<PooledOrder>>,

    /// Total orders placed (metrics)
    total_orders: AtomicU64,

    /// Total fills generated (metrics)
    total_fills: AtomicU64,

    /// Total volume traded (fixed-point)
    total_volume: AtomicU64,

    /// Count of fills dropped due to queue overflow
    dropped_fills: AtomicU64,
}

impl SimulatedExecutor {
    /// Create new simulated executor with specified capacities
    pub fn new(order_capacity: usize, fill_capacity: usize) -> Self {
        tracing::info!(
            "Initializing SimulatedExecutor (orders: {}, fills: {})",
            order_capacity,
            fill_capacity
        );

        Self {
            order_pool: Arc::new(ObjectPool::new(order_capacity)),
            fill_pool: Arc::new(ObjectPool::new(fill_capacity)),
            pending_fills: Arc::new(ArrayQueue::new(fill_capacity)),
            active_orders: Arc::new(ArrayQueue::new(order_capacity)),
            total_orders: AtomicU64::new(0),
            total_fills: AtomicU64::new(0),
            total_volume: AtomicU64::new(0),
            dropped_fills: AtomicU64::new(0),
        }
    }

    /// Create with default capacities (256 orders, 1024 fills)
    pub fn new_default() -> Self {
        Self::new(256, 1024)
    }

    /// Simulate filling an order immediately
    ///
    /// This is pessimistic - assumes all orders get filled instantly.
    #[inline(always)]
    fn simulate_fill(&mut self, mut order: PooledOrder) -> PooledFill {
        let fill_size = order.remaining_size();
        let fill_price = order.price;

        // Update order
        order.filled_size += fill_size;
        order.status = if order.is_filled() {
            OrderStatusBits::Filled
        } else {
            OrderStatusBits::PartiallyFilled
        };

        // Create fill from pool
        let fill = PooledFill::new(order.id, order.side, fill_price, fill_size);

        // Update metrics
        self.total_fills.fetch_add(1, Ordering::Relaxed);
        self.total_volume
            .fetch_add(fill_size, Ordering::Relaxed);

        fill
    }

    /// Place a single order
    #[inline(always)]
    fn place_order_internal(&mut self, side: OrderSide, price: u64, size: u64) -> Result<()> {
        // Generate order ID
        let order_id = OrderId::generate();

        // Create order
        let order = PooledOrder::new(order_id, side, price, size);

        // Simulate immediate fill
        let fill = self.simulate_fill(order);

        // Push to pending fills (lock-free)
        if self.pending_fills.push(fill).is_err() {
            // CRITICAL: Fill queue is full - we MUST NOT drop fills!
            // Increment dropped counter for monitoring
            self.dropped_fills.fetch_add(1, Ordering::Relaxed);

            // Log critical error
            tracing::error!(
                "CRITICAL: Fill queue overflow! Dropping fill for order_id: {:?}. Total dropped: {}",
                order_id,
                self.dropped_fills.load(Ordering::Relaxed)
            );

            // Return error to halt trading - we cannot continue safely
            return Err(anyhow::anyhow!(
                "Fill queue overflow - position tracking corrupted! Halting trading."
            ));
        }

        // Update metrics
        self.total_orders.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Get execution statistics
    pub fn stats(&self) -> ExecutorStats {
        ExecutorStats {
            total_orders: self.total_orders.load(Ordering::Relaxed),
            total_fills: self.total_fills.load(Ordering::Relaxed),
            total_volume: self.total_volume.load(Ordering::Relaxed),
        }
    }
}

impl Executor for SimulatedExecutor {
    #[inline(always)]
    fn execute(&mut self, signal: Signal, position: &Position) -> Result<()> {
        // Validate signal against risk limits (<50ns target)
        risk::validate_signal(&signal, position)?;

        match signal.action {
            SignalAction::QuoteBoth => {
                // Place both bid and ask orders
                self.place_order_internal(OrderSide::Buy, signal.bid_price, signal.size)?;
                self.place_order_internal(OrderSide::Sell, signal.ask_price, signal.size)?;
            }
            SignalAction::QuoteBid => {
                // Place bid order only
                self.place_order_internal(OrderSide::Buy, signal.bid_price, signal.size)?;
            }
            SignalAction::QuoteAsk => {
                // Place ask order only
                self.place_order_internal(OrderSide::Sell, signal.ask_price, signal.size)?;
            }
            SignalAction::CancelAll => {
                // Cancel all orders (in simulation, this is a no-op)
                // Real executor would cancel active orders
            }
            SignalAction::NoAction => {
                // Do nothing
            }
            SignalAction::TakePosition => {
                // Take aggressive position
                match signal.side {
                    CoreSide::Buy => {
                        self.place_order_internal(OrderSide::Buy, signal.bid_price, signal.size)?;
                    }
                    CoreSide::Sell => {
                        self.place_order_internal(OrderSide::Sell, signal.ask_price, signal.size)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn cancel_all(&mut self) -> Result<()> {
        // In simulation, orders are immediately filled
        // So there's nothing to cancel
        Ok(())
    }

    fn get_fills(&mut self) -> Vec<crate::execution::Fill> {
        // Drain pending fills and convert to execution::Fill type
        let mut fills = Vec::new();
        while let Some(pooled_fill) = self.pending_fills.pop() {
            // Convert PooledFill to execution::Fill
            // Convert u64 fixed-point (9 decimals) to Decimal
            let price = Decimal::from(pooled_fill.price) / Decimal::from(1_000_000_000_i64);
            let size = Decimal::from(pooled_fill.size) / Decimal::from(1_000_000_000_i64);

            // Convert OrderSide to execution::Side
            let side = match pooled_fill.side {
                OrderSide::Buy => ExecutionSide::Buy,
                OrderSide::Sell => ExecutionSide::Sell,
            };

            // Create Fill with timestamp and fee
            // Fee is charged in quote currency as a percentage of notional.
            let notional = price * size;
            let fee_rate = Decimal::from(crate::config::DEFAULT_FEE_BPS)
                / Decimal::from(10_000u32);
            let fee = Some(notional * fee_rate);

            // Convert core::OrderId to execution::OrderId
            let order_id_str = format!("{:032x}", pooled_fill.order_id.0);
            let execution_order_id = crate::execution::OrderId::new(order_id_str);

            let fill = Fill::new_with_fee(
                execution_order_id,
                side,
                price,
                size,
                fee,
            );

            fills.push(fill);
        }
        fills
    }

    fn dropped_fill_count(&self) -> u64 {
        self.dropped_fills.load(Ordering::Relaxed)
    }

    fn name(&self) -> &'static str {
        "SimulatedExecutor"
    }

    fn get_open_exposure(&self) -> (i64, i64) {
        // In zero-overhead simulation, orders are filled immediately
        // so there is no open exposure.
        (0, 0)
    }
}

impl Default for SimulatedExecutor {
    fn default() -> Self {
        Self::new_default()
    }
}

/// Executor statistics
#[derive(Debug, Clone, Copy)]
pub struct ExecutorStats {
    pub total_orders: u64,
    pub total_fills: u64,
    pub total_volume: u64,
}

// ===== COMPILE-TIME VERIFICATIONS =====

#[cfg(test)]
const _: () = {
    // Verify pool types are reasonable sizes
    const ORDER_SIZE: usize = std::mem::size_of::<PooledOrder>();
    const FILL_SIZE: usize = std::mem::size_of::<PooledFill>();

    // Orders should be small (64 bytes or less)
    assert!(ORDER_SIZE <= 64, "PooledOrder too large");

    // Fills should be small (48 bytes or less)
    // OrderId(16) + OrderSide(1) + price(8) + size(8) + padding = ~40 bytes
    assert!(FILL_SIZE <= 48, "PooledFill too large");
};

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::prelude::FromPrimitive;

    #[test]
    fn test_pooled_order_size() {
        // Verify order is compact
        let size = std::mem::size_of::<PooledOrder>();
        println!("PooledOrder size: {} bytes", size);
        assert!(size <= 64);
    }

    #[test]
    fn test_pooled_fill_size() {
        // Verify fill is compact
        let size = std::mem::size_of::<PooledFill>();
        println!("PooledFill size: {} bytes", size);
        // OrderId(16) + OrderSide(1) + price(8) + size(8) + padding
        assert!(size <= 48);
    }

    #[test]
    fn test_order_creation() {
        let order = PooledOrder::new(
            OrderId::generate(),
            OrderSide::Buy,
            50_000_000_000_000, // $50k
            100_000_000,        // 0.1 BTC
        );

        assert_eq!(order.side, OrderSide::Buy);
        assert_eq!(order.price, 50_000_000_000_000);
        assert_eq!(order.size, 100_000_000);
        assert_eq!(order.filled_size, 0);
        assert!(order.is_active());
        assert!(!order.is_filled());
    }

    #[test]
    fn test_order_filling() {
        let mut order = PooledOrder::new(
            OrderId::generate(),
            OrderSide::Buy,
            50_000_000_000_000,
            100_000_000,
        );

        // Partially fill
        order.filled_size = 50_000_000;
        assert!(!order.is_filled());
        assert_eq!(order.remaining_size(), 50_000_000);

        // Fully fill
        order.filled_size = 100_000_000;
        assert!(order.is_filled());
        assert_eq!(order.remaining_size(), 0);
    }

    #[test]
    fn test_fill_calculations() {
        let fill = PooledFill::new(
            OrderId::generate(),
            OrderSide::Buy,
            50_000_000_000_000, // $50k in fixed-point
            100_000_000,        // 0.1 BTC in fixed-point
        );

        // Notional = 50000 * 0.1 = 5000 USD
        let notional = fill.notional();
        assert_eq!(notional, 5_000_000_000_000); // $5k in fixed-point

        // Position change for buy is positive
        assert_eq!(fill.position_change(), 100_000_000);
    }

    #[test]
    fn test_fill_sell() {
        let fill = PooledFill::new(
            OrderId::generate(),
            OrderSide::Sell,
            50_000_000_000_000,
            100_000_000,
        );

        // Position change for sell is negative
        assert_eq!(fill.position_change(), -100_000_000);
    }

    #[test]
    fn test_executor_creation() {
        let executor = SimulatedExecutor::new_default();
        let stats = executor.stats();

        assert_eq!(stats.total_orders, 0);
        assert_eq!(stats.total_fills, 0);
        assert_eq!(stats.total_volume, 0);
    }

    #[test]
    fn test_executor_signal_execution() {
        let mut executor = SimulatedExecutor::new_default();
        let position = Position::new();

        // Create a quote-both signal
        let signal = Signal::quote_both(
            49_995_000_000_000, // Bid at $49,995
            50_005_000_000_000, // Ask at $50,005
            100_000_000,        // 0.1 BTC
        );

        // Execute signal
        executor.execute(signal, &position).unwrap();

        // Check stats
        let stats = executor.stats();
        assert_eq!(stats.total_orders, 2); // Bid + Ask
        assert_eq!(stats.total_fills, 2);
        assert_eq!(stats.total_volume, 200_000_000); // 0.1 + 0.1 BTC
    }

    #[test]
    fn test_executor_fee_amount_matches_config_bps() {
        let mut executor = SimulatedExecutor::new_default();
        let position = Position::new();

        // Single bid quote
        let price_fixed: u64 = 50_000_000_000_000; // $50,000
        let size_fixed: u64 = 100_000_000; // 0.1 BTC
        let signal = Signal::quote_bid(price_fixed, size_fixed);

        executor.execute(signal, &position).unwrap();

        let fills = executor.get_fills();
        assert_eq!(fills.len(), 1);

        let fill = &fills[0];
        let notional = fill.notional();

        // Expected fee = notional * DEFAULT_FEE_BPS / 10_000
        let fee_rate = rust_decimal::Decimal::from(crate::config::DEFAULT_FEE_BPS)
            / rust_decimal::Decimal::from(10_000u32);
        let expected_fee = notional * fee_rate;

        let actual_fee = fill.fee.expect("fee should be present");

        // Allow for tiny rounding differences
        let diff = (expected_fee - actual_fee).abs();
        let epsilon = rust_decimal::Decimal::from_f64(0.0000001).unwrap();
        assert!(
            diff <= epsilon,
            "fee mismatch: expected {}, got {}, diff {}",
            expected_fee,
            actual_fee,
            diff
        );
    }

    #[test]
    fn test_executor_quote_bid_only() {
        let mut executor = SimulatedExecutor::new_default();
        let position = Position::new();

        let signal = Signal::quote_bid(50_000_000_000_000, 100_000_000);

        executor.execute(signal, &position).unwrap();

        let stats = executor.stats();
        assert_eq!(stats.total_orders, 1);
        assert_eq!(stats.total_fills, 1);
    }

    #[test]
    fn test_executor_no_action() {
        let mut executor = SimulatedExecutor::new_default();
        let position = Position::new();

        let signal = Signal::no_action();

        executor.execute(signal, &position).unwrap();

        let stats = executor.stats();
        assert_eq!(stats.total_orders, 0);
        assert_eq!(stats.total_fills, 0);
    }

    #[test]
    fn test_cancel_all() {
        let mut executor = SimulatedExecutor::new_default();

        // In simulation, this is a no-op
        executor.cancel_all().unwrap();
    }

    #[test]
    fn test_order_side_size() {
        assert_eq!(std::mem::size_of::<OrderSide>(), 1);
    }

    #[test]
    fn test_order_status_size() {
        assert_eq!(std::mem::size_of::<OrderStatusBits>(), 1);
    }
}
