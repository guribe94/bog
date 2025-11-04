pub mod types;
pub mod simple_spread;
pub mod inventory_based;

pub use types::{Quote, Signal, StrategyState};
pub use simple_spread::SimpleSpreadStrategy;
pub use inventory_based::InventoryBasedStrategy;

use crate::data::MarketSnapshot;
use crate::execution::Fill;
use crate::orderbook::OrderBookManager;
use anyhow::Result;

/// Strategy trait - all trading strategies must implement this
pub trait Strategy: Send {
    /// Called on each market data update
    /// Returns a signal if the strategy wants to take action
    fn on_update(
        &mut self,
        snapshot: &MarketSnapshot,
        orderbook: &OrderBookManager,
    ) -> Option<Signal>;

    /// Called when an order is filled
    fn on_fill(&mut self, fill: &Fill);

    /// Get current strategy state
    fn state(&self) -> StrategyState;

    /// Pause the strategy (stop generating signals)
    fn pause(&mut self);

    /// Resume the strategy
    fn resume(&mut self);

    /// Stop the strategy (cannot be resumed)
    fn stop(&mut self);

    /// Get strategy name
    fn name(&self) -> &str;

    /// Get strategy statistics (optional, for monitoring)
    fn stats(&self) -> StrategyStats {
        StrategyStats::default()
    }
}

/// Strategy statistics
#[derive(Debug, Clone, Default)]
pub struct StrategyStats {
    pub signals_generated: u64,
    pub fills_received: u64,
    pub total_buy_volume: f64,
    pub total_sell_volume: f64,
}

/// Strategy factory - creates strategies from config
/// TODO: Remove in favor of const generic approach
// pub struct StrategyFactory;

/*
impl StrategyFactory {
    pub fn create(config: &crate::config::StrategyConfig) -> Result<Box<dyn Strategy>> {
        match config.strategy_type.as_str() {
            "simple_spread" => {
                let params = config
                    .simple_spread
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Missing simple_spread parameters"))?;

                Ok(Box::new(SimpleSpreadStrategy::new(
                    params.spread_bps,
                    params.order_size,
                    params.min_spread_bps,
                )))
            }
            "inventory_based" => {
                let params = config
                    .inventory_based
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Missing inventory_based parameters"))?;

                Ok(Box::new(InventoryBasedStrategy::new(
                    params.target_inventory,
                    params.risk_aversion,
                    params.order_size,
                    params.volatility,
                    params.time_horizon_secs,
                )))
            }
            other => Err(anyhow::anyhow!("Unknown strategy type: {}", other)),
        }
    }
}
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_stats_default() {
        let stats = StrategyStats::default();
        assert_eq!(stats.signals_generated, 0);
        assert_eq!(stats.fills_received, 0);
    }
}
