//! Trading Strategy System (Legacy)
//!
//! **⚠️  DEPRECATION NOTICE**: This module contains the legacy Strategy trait.
//! **New code should use** [`crate::engine::Strategy`] instead.
//!
//! ## Migration Status
//!
//! The codebase is migrating from this trait to the new const-generic Engine API:
//!
//! **Old API** (this module):
//! ```rust,ignore
//! trait Strategy {
//!     fn on_update(&mut self, snapshot: &MarketSnapshot, orderbook: &OrderBookManager)
//!         -> Option<Signal>;
//! }
//! ```
//!
//! **New API** ([`crate::engine::Strategy`]):
//! ```rust,ignore
//! trait Strategy {
//!     fn calculate(&mut self, snapshot: &MarketSnapshot, position: &Position)
//!         -> Option<Signal>;
//! }
//! ```
//!
//! **Key differences:**
//! - New API passes `Position` instead of `OrderBookManager`
//! - Renamed `on_update` → `calculate` for clarity
//! - Simpler interface with fewer dependencies
//! - Enables full const-generic monomorphization
//!
//! ## Legacy Strategies
//!
//! This module still contains the legacy implementations:
//!
//! - [`SimpleSpreadStrategy`] - Basic market making (legacy)
//! - [`InventoryBasedStrategy`] - Avellaneda-Stoikov (legacy)
//!
//! **For new strategies**, see:
//! - [`bog_strategies::SimpleSpread`] - New zero-sized implementation
//! - [`bog_strategies::InventoryBased`] - New zero-sized implementation
//!
//! ## Usage (Legacy)
//!
//! ```rust,ignore
//! use bog_core::strategy::{Strategy, SimpleSpreadStrategy};
//! use bog_core::orderbook::OrderBookManager;
//!
//! let mut strategy = SimpleSpreadStrategy::new(10.0, dec!(0.1), 1.0);
//! let mut orderbook = OrderBookManager::new(1);
//!
//! // Process market update
//! if let Some(signal) = strategy.on_update(&snapshot, &orderbook) {
//!     // Execute signal
//! }
//! ```
//!
//! ## For New Strategies
//!
//! See [`crate::engine::Strategy`] for the current API and examples.

pub mod types;
pub mod simple_spread;
pub mod inventory_based;

pub use types::{Quote, Signal, StrategyState};
pub use simple_spread::SimpleSpreadStrategy;
pub use inventory_based::InventoryBasedStrategy;

use crate::data::MarketSnapshot;
use crate::execution::Fill;
use crate::orderbook::OrderBookManager;

/// Strategy trait - all trading strategies must implement this (LEGACY)
///
/// **⚠️  DEPRECATED**: Use [`crate::engine::Strategy`] instead for new code.
///
/// This trait is part of the legacy dynamic-dispatch engine that used
/// `Box<dyn Strategy>`. The new engine uses const generics for full
/// monomorphization and zero-overhead dispatch.
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
