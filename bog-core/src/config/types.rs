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
    pub monitoring: MonitoringConfig,
    pub alerts: AlertConfig,
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

/// Monitoring and observability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable Prometheus metrics server
    #[serde(default = "default_true")]
    pub enable_prometheus: bool,

    /// Prometheus metrics server address
    #[serde(default = "default_metrics_addr")]
    pub metrics_addr: String,

    /// Metrics path (default: /metrics)
    #[serde(default = "default_metrics_path")]
    pub metrics_path: String,

    /// Enable execution journal
    #[serde(default = "default_true")]
    pub enable_journal: bool,

    /// Journal file path
    #[serde(default = "default_journal_path")]
    pub journal_path: PathBuf,

    /// Recover from journal on startup
    #[serde(default = "default_true")]
    pub recover_on_startup: bool,

    /// Validate recovered state
    #[serde(default = "default_true")]
    pub validate_recovery: bool,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enable_prometheus: true,
            metrics_addr: default_metrics_addr(),
            metrics_path: default_metrics_path(),
            enable_journal: true,
            journal_path: default_journal_path(),
            recover_on_startup: true,
            validate_recovery: true,
        }
    }
}

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Enable alerting system
    #[serde(default = "default_true")]
    pub enable_alerts: bool,

    /// Console output for alerts
    #[serde(default = "default_true")]
    pub console_output: bool,

    /// Minimum severity for console (Info/Warning/Error/Critical)
    #[serde(default = "default_console_severity")]
    pub console_min_severity: String,

    /// File output for alerts
    #[serde(default = "default_true")]
    pub file_output: bool,

    /// Alert log file path
    #[serde(default = "default_alert_log_path")]
    pub alert_log_path: PathBuf,

    /// Minimum severity for file output
    #[serde(default = "default_file_severity")]
    pub file_min_severity: String,

    /// Webhook output for alerts
    #[serde(default)]
    pub webhook_output: bool,

    /// Webhook URL (e.g., PagerDuty, Slack)
    #[serde(default)]
    pub webhook_url: Option<String>,

    /// Minimum severity for webhook
    #[serde(default = "default_webhook_severity")]
    pub webhook_min_severity: String,

    /// Rate limit in seconds (minimum time between same alerts)
    #[serde(default = "default_rate_limit")]
    pub rate_limit_secs: u64,

    /// Auto-resolve alerts after inactivity (seconds)
    #[serde(default = "default_auto_resolve")]
    pub auto_resolve_secs: u64,

    /// Alert rules configuration
    pub rules: AlertRulesConfig,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enable_alerts: true,
            console_output: true,
            console_min_severity: default_console_severity(),
            file_output: true,
            alert_log_path: default_alert_log_path(),
            file_min_severity: default_file_severity(),
            webhook_output: false,
            webhook_url: None,
            webhook_min_severity: default_webhook_severity(),
            rate_limit_secs: default_rate_limit(),
            auto_resolve_secs: default_auto_resolve(),
            rules: AlertRulesConfig::default(),
        }
    }
}

/// Alert rules configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRulesConfig {
    /// Enable position limit rule
    #[serde(default = "default_true")]
    pub position_limit: bool,

    /// Position limit in base units (from risk.max_position by default)
    #[serde(default)]
    pub position_limit_override: Option<Decimal>,

    /// Enable daily loss limit rule
    #[serde(default = "default_true")]
    pub daily_loss_limit: bool,

    /// Daily loss limit override (from risk.max_daily_loss by default)
    #[serde(default)]
    pub daily_loss_limit_override: Option<Decimal>,

    /// Enable Huginn connection rule
    #[serde(default = "default_true")]
    pub huginn_connection: bool,

    /// Grace period for Huginn disconnect (seconds)
    #[serde(default = "default_huginn_grace")]
    pub huginn_grace_period_secs: u64,

    /// Enable rejection rate rule
    #[serde(default = "default_true")]
    pub rejection_rate: bool,

    /// Rejection rate threshold (0.0 to 1.0)
    #[serde(default = "default_rejection_threshold")]
    pub rejection_threshold: f64,

    /// Enable latency rule
    #[serde(default = "default_true")]
    pub latency: bool,

    /// Latency threshold in microseconds
    #[serde(default = "default_latency_threshold")]
    pub latency_threshold_us: f64,
}

impl Default for AlertRulesConfig {
    fn default() -> Self {
        Self {
            position_limit: true,
            position_limit_override: None,
            daily_loss_limit: true,
            daily_loss_limit_override: None,
            huginn_connection: true,
            huginn_grace_period_secs: default_huginn_grace(),
            rejection_rate: true,
            rejection_threshold: default_rejection_threshold(),
            latency: true,
            latency_threshold_us: default_latency_threshold(),
        }
    }
}

// Additional default value functions
fn default_true() -> bool {
    true
}

fn default_metrics_addr() -> String {
    "127.0.0.1:9090".to_string()
}

fn default_metrics_path() -> String {
    "/metrics".to_string()
}

fn default_journal_path() -> PathBuf {
    PathBuf::from("./data/execution.jsonl")
}

fn default_alert_log_path() -> PathBuf {
    PathBuf::from("./data/alerts.log")
}

fn default_console_severity() -> String {
    "Warning".to_string()
}

fn default_file_severity() -> String {
    "Info".to_string()
}

fn default_webhook_severity() -> String {
    "Critical".to_string()
}

fn default_rate_limit() -> u64 {
    60 // 1 minute
}

fn default_auto_resolve() -> u64 {
    300 // 5 minutes
}

fn default_huginn_grace() -> u64 {
    5 // 5 seconds
}

fn default_rejection_threshold() -> f64 {
    0.1 // 10%
}

fn default_latency_threshold() -> f64 {
    1000.0 // 1ms = 1000μs
}
