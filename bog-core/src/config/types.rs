use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub huginn: HuginnConfig,
    pub execution: ExecutionConfig,
    pub strategy: StrategyConfig,
    pub risk: RiskConfig,
    pub metrics: MetricsConfig,
}

/// Huginn connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuginnConfig {
    /// Market ID to connect to
    pub market_id: u64,

    /// Optional: DEX type (1 = Lighter, 2 = Binance, etc.)
    #[serde(default)]
    pub dex_type: Option<u8>,
}

/// Execution mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Execution mode: "live" or "simulated"
    pub mode: String,

    /// Detect when Huginn replay ends (for backtesting)
    #[serde(default)]
    pub detect_replay_end: bool,

    /// Timeout for replay end detection (seconds)
    #[serde(default = "default_replay_timeout")]
    pub replay_end_timeout_secs: u64,

    /// Lighter DEX API configuration (for live mode)
    #[serde(default)]
    pub lighter: Option<LighterConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LighterConfig {
    /// API endpoint URL
    pub api_url: String,

    /// WebSocket URL for order updates
    pub ws_url: String,

    /// API key (or load from environment)
    pub api_key: Option<String>,

    /// Private key path for signing
    pub private_key_path: Option<PathBuf>,
}

/// Strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Strategy type: "simple_spread" or "inventory_based"
    #[serde(rename = "type")]
    pub strategy_type: String,

    /// Simple spread strategy parameters
    #[serde(default)]
    pub simple_spread: Option<SimpleSpreadParams>,

    /// Inventory-based strategy parameters
    #[serde(default)]
    pub inventory_based: Option<InventoryBasedParams>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleSpreadParams {
    /// Spread in basis points (e.g., 10 = 0.1%)
    pub spread_bps: f64,

    /// Order size
    pub order_size: Decimal,

    /// Minimum spread in basis points (safety)
    #[serde(default = "default_min_spread")]
    pub min_spread_bps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryBasedParams {
    /// Target inventory (neutral position)
    pub target_inventory: Decimal,

    /// Risk aversion parameter (gamma)
    pub risk_aversion: f64,

    /// Order size
    pub order_size: Decimal,

    /// Volatility estimate (for spread calculation)
    pub volatility: f64,

    /// Time horizon in seconds
    pub time_horizon_secs: f64,
}

/// Risk management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Maximum long position
    pub max_position: Decimal,

    /// Maximum short position (positive value)
    pub max_short: Decimal,

    /// Maximum order size
    pub max_order_size: Decimal,

    /// Minimum order size
    #[serde(default = "default_min_order_size")]
    pub min_order_size: Decimal,

    /// Maximum number of outstanding orders
    #[serde(default = "default_max_orders")]
    pub max_outstanding_orders: usize,

    /// Maximum daily loss (circuit breaker)
    pub max_daily_loss: Decimal,

    /// Maximum drawdown percentage (0.0 to 1.0)
    #[serde(default = "default_max_drawdown")]
    pub max_drawdown_pct: f64,
}

/// Metrics and monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Prometheus metrics port
    #[serde(default = "default_prometheus_port")]
    pub prometheus_port: u16,

    /// Log level: "trace", "debug", "info", "warn", "error"
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Enable JSON logging
    #[serde(default)]
    pub json_logs: bool,
}

// Default value functions
fn default_replay_timeout() -> u64 {
    1 // 1 second
}

fn default_min_spread() -> f64 {
    1.0 // 1 basis point
}

fn default_min_order_size() -> Decimal {
    Decimal::new(1, 4) // 0.0001
}

fn default_max_orders() -> usize {
    10
}

fn default_max_drawdown() -> f64 {
    0.20 // 20%
}

fn default_prometheus_port() -> u16 {
    9090
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            prometheus_port: default_prometheus_port(),
            log_level: default_log_level(),
            json_logs: false,
        }
    }
}
