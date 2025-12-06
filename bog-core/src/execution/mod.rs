//! Order Execution System
//!
//! Provides execution backends for order placement and fill simulation/tracking.
//!
//! ## Architecture
//!
//! All executors implement the [`Executor`] trait and handle:
//! - Order placement and cancellation
//! - Fill generation and tracking
//! - Order state management
//! - Metrics collection
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │              Executor Trait                          │
//! ├──────────────────────────────────────────────────────┤
//! │  place_order()  cancel_order()  get_fills()          │
//! └──────────────────────────────────────────────────────┘
//!            │                  │                  │
//!            v                  v                  v
//!   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
//!   │  Simulated   │  │    Lighter   │  │  Production  │
//!   │   Executor   │  │   Executor   │  │   Executor   │
//!   └──────────────┘  └──────────────┘  └──────────────┘
//!    Instant fills     Real DEX API     Full production
//!    Paper trading     Live trading     w/ journaling
//! ```
//!
//! ## Available Executors
//!
//! ### [`SimulatedExecutor`] - Paper Trading
//!
//! Instant fill simulation with realistic market dynamics:
//! - **Zero latency** - Fills generated immediately
//! - **Fee accounting** - 2 bps taker fee simulation
//! - **Object pools** - Lock-free fill queue (1024 capacity)
//! - **Performance**: ~50ns per execution
//!
//! **Use case**: Backtesting, strategy development, paper trading
//!
//! ### [`LighterExecutor`] - Live Trading (Stub)
//!
//! Real order placement via Lighter DEX API:
//! - **REST API** - Order placement and cancellation
//! - **WebSocket** - Fill updates
//! - **Async execution** - Non-blocking order submission
//!
//! **Status**: 🚧 Stub implementation
//!
//! ### [`JournaledExecutor`] - Full Production (Legacy)
//!
//! Complete production executor with:
//! - **State journaling** - Crash recovery
//! - **Metrics** - Prometheus integration
//! - **Fill reconciliation** - Exchange vs local state
//!
//! **Status**: Legacy implementation, needs migration to new Engine API
//!
//! ## Object Pool Architecture
//!
//! Executors use lock-free object pools for zero-allocation execution:
//!
//! ```text
//! Order Pool (256 entries)          Fill Pool (1024 entries)
//! ┌─────────────────────┐          ┌─────────────────────┐
//! │ [Order][Order]...   │          │ [Fill][Fill][Fill]  │
//! │  ↑acquire  ↑release │          │   ↑push    ↑pop     │
//! │  └─────────┘        │          │   crossbeam queue   │
//! └─────────────────────┘          └─────────────────────┘
//!   Lock-free ArrayQueue             Lock-free ArrayQueue
//! ```
//!
//! See [`crate::perf::pools`] for pool implementation details.
//!
//! ## Usage Example
//!
//! ```rust
//! use bog_core::execution::{Executor, RealisticFillConfig, SimulatedExecutor};
//!
//! // Create executor with realistic fill simulation
//! let config = RealisticFillConfig {
//!     enable_queue_modeling: true,
//!     enable_partial_fills: true,
//!     front_of_queue_fill_rate: 0.8,
//!     back_of_queue_fill_rate: 0.4,
//! };
//! let mut executor = SimulatedExecutor::with_config(config);
//!
//! // Place orders / pull fills via the executor API
//! # use rust_decimal_macros::dec;
//! # use bog_core::execution::{Order, Side};
//! let order = Order::post_only(Side::Buy, dec!(50_000), dec!(0.1));
//! let order_id = executor.place_order(order)?;
//! let mut fills = Vec::new();
//! executor.get_fills(&mut fills);
//! # executor.cancel_order(&order_id)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Types
//!
//! - [`Order`] - Order request with price, size, side, type
//! - [`OrderId`] - Unique 128-bit order identifier
//! - [`Fill`] - Executed trade with price, size, fees
//! - [`Side`] - Buy or Sell
//! - [`OrderStatus`] - Order lifecycle state
//! - [`ExecutionMode`] - Simulated vs Live
//!
//! ## Performance Characteristics
//!
//! | Operation | Target | SimulatedExecutor |
//! |-----------|--------|-------------------|
//! | **execute()** | <200ns | **~50ns** |
//! | **get_fills()** | <100ns | **~25ns** |
//! | **Fill generation** | <150ns | **~75ns** |
//!
//! See [execution benchmarks](../../docs/benchmarks/) for details.

pub mod lighter;
pub mod order_bridge;
pub mod journaled;
pub mod simulated;
pub mod types;
pub mod journal;

pub use lighter::LighterExecutor;
pub use order_bridge::{legacy_order_to_pending, order_state_to_legacy, OrderStateWrapper};
pub use journaled::{ExecutionMetrics, JournaledExecutor, JournaledExecutorConfig};
pub use simulated::{RealisticFillConfig, SimulatedExecutor};
pub use types::{ExecutionMode, Fill, Order, OrderId, OrderStatus, OrderType, Side, TimeInForce};

use anyhow::Result;

/// Executor trait - abstraction over order execution backends
///
/// All order execution backends implement this trait, enabling:
/// - Strategy-independent order placement
/// - Polymorphic execution (simulated, paper, live)
/// - Consistent fill tracking interface
///
/// # Implementations
///
/// - [`SimulatedExecutor`] - Instant fills for backtesting
/// - [`LighterExecutor`] - Live trading via Lighter DEX
/// - [`JournaledExecutor`] - Full production with journaling
///
/// # Example
///
/// See module-level documentation for usage examples.
pub trait Executor: Send {
    /// Place an order
    fn place_order(&mut self, order: Order) -> Result<OrderId>;

    /// Cancel an order
    fn cancel_order(&mut self, order_id: &OrderId) -> Result<()>;

    /// Get recent fills (since last call)
    /// Appends fills to the provided buffer to avoid allocation.
    fn get_fills(&mut self, fills: &mut Vec<Fill>);

    /// Get order status
    fn get_order_status(&self, order_id: &OrderId) -> Option<OrderStatus>;

    /// Get all active orders
    fn get_active_orders(&self) -> Vec<&Order>;

    /// Get execution mode
    fn execution_mode(&self) -> ExecutionMode;

    /// Get total open exposure (long, short) in fixed-point
    ///
    /// Returns tuple of (long_exposure, short_exposure):
    /// - long_exposure: Sum of remaining size for all open Buy orders
    /// - short_exposure: Sum of remaining size for all open Sell orders
    ///
    /// Used for conservative pre-trade risk checks.
    fn get_open_exposure(&self) -> (i64, i64);

    /// Get the count of fills that were dropped due to queue overflow
    /// (Default implementation returns 0 for backends that don't support this)
    fn dropped_fill_count(&self) -> u64 {
        0
    }

    /// Cancel all active orders (default implementation iterates via get_active_orders)
    fn cancel_all_orders(&mut self) -> Result<()> {
        let order_ids: Vec<OrderId> = self
            .get_active_orders()
            .into_iter()
            .map(|order| order.id.clone())
            .collect();

        for order_id in order_ids {
            self.cancel_order(&order_id)?;
        }
        Ok(())
    }

    /// Atomic order amendment (replace)
    ///
    /// Updates an existing order's price or size.
    /// - If size decreases: Queue priority SHOULD be preserved.
    /// - If size increases: Queue priority MUST be reset (new order at back of queue).
    /// - If price changes: Queue priority MUST be reset.
    ///
    /// Returns the order ID of the amended order (which may be the same ID or a new one).
    ///
    /// Default implementation is non-atomic: Cancel + Place.
    fn amend_order(&mut self, order_id: &OrderId, new_order: Order) -> Result<OrderId> {
        self.cancel_order(order_id)?;
        self.place_order(new_order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test that ExecutionMode can be used
    #[test]
    fn test_execution_mode() {
        let mode = ExecutionMode::Simulated;
        assert_eq!(format!("{}", mode), "SIMULATED");

        let mode = ExecutionMode::Live;
        assert_eq!(format!("{}", mode), "LIVE");
    }
}
