//! Configuration System
//!
//! Bog uses a **two-tier configuration system** for maximum performance:
//!
//! 1. **Compile-time configuration** via Cargo features (primary)
//! 2. **Runtime configuration** via TOML files (secondary, for non-hot-path settings)
//!
//! ## Philosophy
//!
//! **Hot path settings** (strategy parameters, risk limits) are **compile-time only**
//! to enable full const propagation and inlining by the compiler. This eliminates
//! runtime branches and achieves zero-overhead configuration.
//!
//! **Cold path settings** (logging, monitoring, API endpoints) use **runtime TOML**
//! files for operational flexibility without performance impact.
//!
//! ## Compile-Time Configuration (Primary)
//!
//! ### Strategy Parameters
//!
//! ```toml
//! [dependencies]
//! bog-strategies = { features = [
//!     "spread-10bps",      # 10 basis point spread
//!     "size-medium",       # 0.1 BTC orders
//!     "min-spread-1bps",   # Min market spread
//! ] }
//! ```
//!
//! **Performance impact**: Zero - values are constants, fully inlined
//!
//! ### Risk Limits
//!
//! ```toml
//! bog-core = { features = [
//!     "max-position-one",     # Max 1.0 BTC position
//!     "max-order-half",       # Max 0.5 BTC per order
//!     "max-daily-loss-1000",  # Max $1000 daily loss
//! ] }
//! ```
//!
//! **Performance impact**: Zero - limits are const, checks compile to constant comparisons
//!
//! ### Pre-configured Profiles
//!
//! Meta-features bundle related settings:
//!
//! ```toml
//! # Conservative profile
//! bog-core = { features = ["conservative"] }
//! # Equivalent to: max-position-half, max-order-tenth, max-daily-loss-100
//!
//! # Aggressive profile
//! bog-core = { features = ["aggressive"] }
//! # Equivalent to: max-position-2btc, max-order-one, max-daily-loss-10000
//!
//! # Testing profile
//! bog-core = { features = ["testing"] }
//! # Equivalent to: max-position-tenth, max-order-centi, max-daily-loss-100
//! ```
//!
//! See [bog-core/Cargo.toml](../Cargo.toml#L158-L162) for profile definitions.
//!
//! ## Runtime Configuration (Secondary)
//!
//! TOML files configure **non-performance-critical** settings:
//!
//! ### Example: config/default.toml
//!
//! ```toml
//! [huginn]
//! market_id = 1
//! dex_type = 1  # Lighter DEX
//!
//! [execution]
//! mode = "simulated"  # or "live"
//!
//! [metrics]
//! log_level = "info"
//! prometheus_port = 9090
//!
//! [monitoring]
//! enable_prometheus = true
//! metrics_addr = "127.0.0.1:9090"
//!
//! [alerts]
//! console_output = true
//! rate_limit_secs = 60
//! ```
//!
//! ### Loading Configuration
//!
//! ```rust
//! use bog_core::config::Config;
//!
//! // Load from file
//! let config = Config::load("config/production.toml")?;
//!
//! // Or use default
//! let config = Config::load_default()?;
//!
//! // Validation happens automatically
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Configuration Precedence
//!
//! Settings are applied in order (later overrides earlier):
//!
//! 1. **Compile-time defaults** - From Cargo feature flags
//! 2. **TOML file** - From `config/*.toml`
//! 3. **Environment variables** - `BOG_*` prefix (e.g., `BOG_METRICS__LOG_LEVEL=debug`)
//!
//! ### Environment Variable Examples
//!
//! ```bash
//! # Override log level
//! BOG_METRICS__LOG_LEVEL=debug ./bog-simple-spread-simulated
//!
//! # Override Prometheus port
//! BOG_MONITORING__METRICS_ADDR=0.0.0.0:9091 ./bog-simple-spread-simulated
//! ```
//!
//! Note: Double underscore (`__`) separates nested fields.
//!
//! ## Available Profiles
//!
//! See [`ConfigProfile`] for pre-built configuration sets:
//!
//! - **Development** - Low limits, verbose logging, simulated execution
//! - **Testing** - Minimal limits for integration tests
//! - **Production** - Production-ready settings with monitoring
//!
//! ## Constants Module
//!
//! See [`constants`] for compile-time configuration values based on active features:
//!
//! - `MAX_POSITION` - Position limit from features
//! - `MAX_ORDER_SIZE` - Order size limit from features
//! - `INVENTORY_IMPACT_BPS` - Inventory skew adjustment
//! - `VOLATILITY_THRESHOLD_BPS` - Volatility filter
//! - And more...
//!
//! ## Migration from Runtime to Compile-Time
//!
//! The codebase is migrating from TOML-based runtime configuration to
//! compile-time feature flags for performance-critical settings.
//!
//! **Status**:
//! - ✅ Strategy parameters → Compile-time (bog-strategies features)
//! - ✅ Risk limits → Compile-time (bog-core features)
//! - 🚧 Execution mode → Partial (still runtime in some places)
//! - ✅ Metrics/monitoring → Runtime (appropriate for cold path)

pub mod constants;
pub mod profiles;
pub mod types;

pub use constants::*;
pub use profiles::{ConfigProfile, ProfileName};
pub use types::*;

use anyhow::Result;
// TODO: Add config crate dependency for TOML file loading
// use config::{Config as ConfigLoader, Environment, File};
use std::path::Path;

impl Config {
    /// Load configuration from file with optional environment variable overrides
    ///
    /// TODO: Implement with config crate for TOML loading
    pub fn load<P: AsRef<Path>>(_path: P) -> Result<Self> {
        // Temporary: Use development profile as default
        Ok(ConfigProfile::development())

        /* TODO: Uncomment when config crate is added
        let config_path = path.as_ref();

        let config = ConfigLoader::builder()
            // Start with default values
            .set_default("execution.detect_replay_end", false)?
            .set_default("execution.replay_end_timeout_secs", 1)?
            .set_default("metrics.prometheus_port", 9090)?
            .set_default("metrics.log_level", "info")?
            .set_default("metrics.json_logs", false)?
            .set_default("risk.min_order_size", "0.0001")?
            .set_default("risk.max_outstanding_orders", 10)?
            .set_default("risk.max_drawdown_pct", 0.20)?
            // Monitoring defaults
            .set_default("monitoring.enable_prometheus", true)?
            .set_default("monitoring.metrics_addr", "127.0.0.1:9090")?
            .set_default("monitoring.enable_journal", true)?
            .set_default("monitoring.journal_path", "./data/execution.jsonl")?
            // Alert defaults
            .set_default("alerts.enable_alerts", true)?
            .set_default("alerts.console_output", true)?
            .set_default("alerts.console_min_severity", "Warning")?
            .set_default("alerts.rate_limit_secs", 60)?
            // Load from TOML file
            .add_source(File::from(config_path))
            // Override with environment variables (BOG_)
            .add_source(Environment::with_prefix("BOG").separator("__"))
            .build()
            .context("Failed to build configuration")?;

        // Deserialize into Config struct
        let cfg: Config = config
            .try_deserialize()
            .context("Failed to deserialize configuration")?;

        // Validate configuration
        cfg.validate()?;

        Ok(cfg)
        */
    }

    /// Load from default location (./config/default.toml)
    pub fn load_default() -> Result<Self> {
        Self::load("config/default.toml")
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        // Validate execution mode
        if self.execution.mode != "live" && self.execution.mode != "simulated" {
            anyhow::bail!(
                "Invalid execution mode '{}', must be 'live' or 'simulated'",
                self.execution.mode
            );
        }

        // Validate strategy type
        if self.strategy.strategy_type != "simple_spread"
            && self.strategy.strategy_type != "inventory_based"
        {
            anyhow::bail!(
                "Invalid strategy type '{}', must be 'simple_spread' or 'inventory_based'",
                self.strategy.strategy_type
            );
        }

        // Validate strategy parameters exist
        match self.strategy.strategy_type.as_str() {
            "simple_spread" => {
                if self.strategy.simple_spread.is_none() {
                    anyhow::bail!("simple_spread strategy selected but no parameters provided");
                }
            }
            "inventory_based" => {
                if self.strategy.inventory_based.is_none() {
                    anyhow::bail!("inventory_based strategy selected but no parameters provided");
                }
            }
            _ => unreachable!(),
        }

        // Validate risk parameters
        if self.risk.max_position <= rust_decimal::Decimal::ZERO {
            anyhow::bail!("max_position must be positive");
        }

        if self.risk.max_short <= rust_decimal::Decimal::ZERO {
            anyhow::bail!("max_short must be positive");
        }

        if self.risk.max_order_size <= rust_decimal::Decimal::ZERO {
            anyhow::bail!("max_order_size must be positive");
        }

        if self.risk.max_order_size > self.risk.max_position {
            anyhow::bail!("max_order_size cannot exceed max_position");
        }

        if self.risk.max_daily_loss <= rust_decimal::Decimal::ZERO {
            anyhow::bail!("max_daily_loss must be positive");
        }

        // Validate log level
        let valid_log_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_log_levels.contains(&self.metrics.log_level.as_str()) {
            anyhow::bail!(
                "Invalid log level '{}', must be one of: {:?}",
                self.metrics.log_level,
                valid_log_levels
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_config_validation() {
        let mut config = Config {
            huginn: HuginnConfig {
                market_id: 1,
                dex_type: None,
            },
            execution: ExecutionConfig {
                mode: "simulated".to_string(),
                detect_replay_end: false,
                replay_end_timeout_secs: 1,
                lighter: None,
            },
            strategy: StrategyConfig {
                strategy_type: "simple_spread".to_string(),
                simple_spread: Some(SimpleSpreadParams {
                    spread_bps: 10.0,
                    order_size: dec!(0.1),
                    min_spread_bps: 1.0,
                }),
                inventory_based: None,
            },
            risk: RiskConfig {
                max_position: dec!(1.0),
                max_short: dec!(1.0),
                max_order_size: dec!(0.5),
                min_order_size: dec!(0.0001),
                max_outstanding_orders: 10,
                max_daily_loss: dec!(5000.0),
                max_drawdown_pct: 0.20,
            },
            metrics: MetricsConfig::default(),
            monitoring: MonitoringConfig::default(),
            alerts: AlertConfig::default(),
        };

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Invalid execution mode
        config.execution.mode = "invalid".to_string();
        assert!(config.validate().is_err());
        config.execution.mode = "simulated".to_string();

        // Invalid strategy type
        config.strategy.strategy_type = "invalid".to_string();
        assert!(config.validate().is_err());
        config.strategy.strategy_type = "simple_spread".to_string();

        // Missing strategy params
        config.strategy.simple_spread = None;
        assert!(config.validate().is_err());
    }
}
