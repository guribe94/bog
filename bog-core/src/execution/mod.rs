pub mod types;
pub mod simulated;
pub mod lighter;
pub mod production;
pub mod order_bridge;

pub use types::{
    ExecutionMode, Fill, Order, OrderId, OrderStatus, OrderType, Side, TimeInForce,
};
pub use simulated::{SimulatedExecutor, RealisticFillConfig};
pub use lighter::LighterExecutor;
pub use production::{ProductionExecutor, ProductionExecutorConfig, ExecutionMetrics};
pub use order_bridge::{OrderStateWrapper, order_state_to_legacy, legacy_order_to_pending};

use anyhow::Result;

/// Executor trait - abstraction over order execution
/// Implementations: SimulatedExecutor (paper/backtest), LighterExecutor (live)
pub trait Executor: Send {
    /// Place an order
    fn place_order(&mut self, order: Order) -> Result<OrderId>;

    /// Cancel an order
    fn cancel_order(&mut self, order_id: &OrderId) -> Result<()>;

    /// Get recent fills (since last call)
    fn get_fills(&mut self) -> Vec<Fill>;

    /// Get order status
    fn get_order_status(&self, order_id: &OrderId) -> Option<OrderStatus>;

    /// Get all active orders
    fn get_active_orders(&self) -> Vec<&Order>;

    /// Get execution mode
    fn execution_mode(&self) -> ExecutionMode;
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
