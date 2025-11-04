//! Trading Engine
//!
//! This module contains the trading engine implementations:
//! - `generic`: Const generic zero-overhead engine (NEW - HFT optimized)
//! - Legacy dynamic dispatch engine (deprecated, commented out)

// New const generic engine (HFT optimized)
pub mod generic;
pub mod traits;

// Re-export new engine types
pub use generic::{Engine, EngineStats, Executor, Strategy};

/*
// === OLD ENGINE (DEPRECATED) ===
// This engine uses dynamic dispatch and will be removed.
// Kept temporarily for reference during migration.

use crate::config::Config;
use crate::data::{MarketFeed, MarketSnapshot};
use crate::execution::{Executor, ExecutionMode};
use crate::orderbook::OrderBookManager;
use crate::risk::RiskManager;
use crate::strategy::Strategy;
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Main trading engine
pub struct TradingEngine {
    config: Config,
    shutdown: Arc<AtomicBool>,
}

impl TradingEngine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn shutdown_signal(&self) -> Arc<AtomicBool> {
        self.shutdown.clone()
    }

    pub fn run(
        &self,
        mut feed: MarketFeed,
        mut strategy: Box<dyn Strategy>,     // ❌ Dynamic dispatch
        mut executor: Box<dyn Executor>,     // ❌ Dynamic dispatch
    ) -> Result<()> {
        // ... old implementation
    }
}
*/
