//! Configuration profiles for different environments
//!
//! Provides pre-configured profiles for:
//! - Development: Relaxed limits, verbose logging, simulated execution
//! - Staging: Production-like but with safety guards
//! - Production: Strict limits, critical alerts only, live execution

use super::types::*;
use rust_decimal_macros::dec;
use std::path::PathBuf;

/// Configuration profile name
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileName {
    /// Development profile (local testing)
    Development,
    /// Staging profile (pre-production)
    Staging,
    /// Production profile (live trading)
    Production,
}

impl ProfileName {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "dev" | "development" => Some(Self::Development),
            "staging" | "stage" => Some(Self::Staging),
            "prod" | "production" => Some(Self::Production),
            _ => None,
        }
    }
}

/// Configuration profile with environment-specific defaults
pub struct ConfigProfile;

impl ConfigProfile {
    /// Create a development configuration
    ///
    /// - Simulated execution
    /// - Relaxed risk limits
    /// - Verbose logging (debug level)
    /// - All monitoring enabled
    /// - Lower alert thresholds for testing
    pub fn development() -> Config {
        Config {
            huginn: HuginnConfig {
                market_id: 1,
                dex_type: Some(1), // Lighter
            },
            execution: ExecutionConfig {
                mode: "simulated".to_string(),
                detect_replay_end: true,
                replay_end_timeout_secs: 2,
                lighter: None,
            },
            strategy: StrategyConfig {
                strategy_type: "simple_spread".to_string(),
                simple_spread: Some(SimpleSpreadParams {
                    spread_bps: 20.0,       // Wide spread for safety
                    order_size: dec!(0.01), // Small size
                    min_spread_bps: 5.0,
                }),
                inventory_based: None,
            },
            risk: RiskConfig {
                max_position: dec!(0.1),     // 0.1 BTC max
                max_short: dec!(0.1),        // 0.1 BTC max short
                max_order_size: dec!(0.05),  // 0.05 BTC per order
                min_order_size: dec!(0.001), // 0.001 BTC min
                max_outstanding_orders: 5,
                max_daily_loss: dec!(100.0), // $100 daily loss limit
                max_drawdown_pct: 0.10,      // 10% drawdown
            },
            metrics: MetricsConfig {
                prometheus_port: 9090,
                log_level: "debug".to_string(),
                json_logs: false,
            },
            monitoring: MonitoringConfig {
                enable_prometheus: true,
                metrics_addr: "127.0.0.1:9090".to_string(),
                metrics_path: "/metrics".to_string(),
                enable_journal: true,
                journal_path: PathBuf::from("./dev-data/execution.jsonl"),
                recover_on_startup: false, // Don't recover in dev
                validate_recovery: true,
            },
            alerts: AlertConfig {
                enable_alerts: true,
                console_output: true,
                console_min_severity: "Info".to_string(), // Show all in dev
                file_output: true,
                alert_log_path: PathBuf::from("./dev-data/alerts.log"),
                file_min_severity: "Info".to_string(),
                webhook_output: false,
                webhook_url: None,
                webhook_min_severity: "Critical".to_string(),
                rate_limit_secs: 10, // Shorter rate limit for testing
                auto_resolve_secs: 60,
                rules: AlertRulesConfig {
                    position_limit: true,
                    position_limit_override: None,
                    daily_loss_limit: true,
                    daily_loss_limit_override: None,
                    huginn_connection: true,
                    huginn_grace_period_secs: 2, // Short grace for dev
                    rejection_rate: true,
                    rejection_threshold: 0.2, // 20% threshold
                    latency: true,
                    latency_threshold_us: 10000.0, // 10ms in dev
                },
            },
        }
    }

    /// Create a staging configuration
    ///
    /// - Live or simulated execution
    /// - Production-like risk limits but smaller
    /// - Info-level logging
    /// - All monitoring enabled
    /// - Production alert thresholds
    pub fn staging() -> Config {
        Config {
            huginn: HuginnConfig {
                market_id: 1,
                dex_type: Some(1),
            },
            execution: ExecutionConfig {
                mode: "simulated".to_string(), // Default to simulated
                detect_replay_end: false,
                replay_end_timeout_secs: 1,
                lighter: None, // Can be overridden
            },
            strategy: StrategyConfig {
                strategy_type: "simple_spread".to_string(),
                simple_spread: Some(SimpleSpreadParams {
                    spread_bps: 10.0,
                    order_size: dec!(0.05),
                    min_spread_bps: 2.0,
                }),
                inventory_based: None,
            },
            risk: RiskConfig {
                max_position: dec!(0.5),    // 0.5 BTC max
                max_short: dec!(0.5),       // 0.5 BTC max short
                max_order_size: dec!(0.25), // 0.25 BTC per order
                min_order_size: dec!(0.0001),
                max_outstanding_orders: 10,
                max_daily_loss: dec!(500.0), // $500 daily loss
                max_drawdown_pct: 0.15,      // 15%
            },
            metrics: MetricsConfig {
                prometheus_port: 9090,
                log_level: "info".to_string(),
                json_logs: true, // JSON logs for staging
            },
            monitoring: MonitoringConfig {
                enable_prometheus: true,
                metrics_addr: "0.0.0.0:9090".to_string(), // Expose externally
                metrics_path: "/metrics".to_string(),
                enable_journal: true,
                journal_path: PathBuf::from("./staging-data/execution.jsonl"),
                recover_on_startup: true,
                validate_recovery: true,
            },
            alerts: AlertConfig {
                enable_alerts: true,
                console_output: true,
                console_min_severity: "Warning".to_string(),
                file_output: true,
                alert_log_path: PathBuf::from("./staging-data/alerts.log"),
                file_min_severity: "Info".to_string(),
                webhook_output: true, // Enable webhook for staging
                webhook_url: Some("https://hooks.slack.com/staging-alerts".to_string()),
                webhook_min_severity: "Error".to_string(),
                rate_limit_secs: 60,
                auto_resolve_secs: 300,
                rules: AlertRulesConfig {
                    position_limit: true,
                    position_limit_override: None,
                    daily_loss_limit: true,
                    daily_loss_limit_override: None,
                    huginn_connection: true,
                    huginn_grace_period_secs: 5,
                    rejection_rate: true,
                    rejection_threshold: 0.1,
                    latency: true,
                    latency_threshold_us: 2000.0, // 2ms
                },
            },
        }
    }

    /// Create a production configuration
    ///
    /// - Live execution (must be explicitly configured)
    /// - Strict risk limits
    /// - Warning-level logging
    /// - All monitoring and persistence enabled
    /// - Strict alert thresholds
    pub fn production() -> Config {
        Config {
            huginn: HuginnConfig {
                market_id: 1,
                dex_type: Some(1),
            },
            execution: ExecutionConfig {
                mode: "live".to_string(), // Production uses live
                detect_replay_end: false,
                replay_end_timeout_secs: 1,
                lighter: None, // MUST be configured via file/env
            },
            strategy: StrategyConfig {
                strategy_type: "simple_spread".to_string(),
                simple_spread: Some(SimpleSpreadParams {
                    spread_bps: 5.0,       // Tight spread
                    order_size: dec!(0.1), // Standard size
                    min_spread_bps: 1.0,   // 1 bps minimum
                }),
                inventory_based: None,
            },
            risk: RiskConfig {
                max_position: dec!(1.0),   // 1.0 BTC max
                max_short: dec!(1.0),      // 1.0 BTC max short
                max_order_size: dec!(0.5), // 0.5 BTC per order
                min_order_size: dec!(0.0001),
                max_outstanding_orders: 20,
                max_daily_loss: dec!(5000.0), // $5k daily loss
                max_drawdown_pct: 0.20,       // 20%
            },
            metrics: MetricsConfig {
                prometheus_port: 9090,
                log_level: "warn".to_string(), // Only warnings/errors
                json_logs: true,               // Structured logs
            },
            monitoring: MonitoringConfig {
                enable_prometheus: true,
                metrics_addr: "0.0.0.0:9090".to_string(),
                metrics_path: "/metrics".to_string(),
                enable_journal: true,
                journal_path: PathBuf::from("/var/lib/bog/execution.jsonl"),
                recover_on_startup: true,
                validate_recovery: true,
            },
            alerts: AlertConfig {
                enable_alerts: true,
                console_output: true,
                console_min_severity: "Error".to_string(), // Only errors to console
                file_output: true,
                alert_log_path: PathBuf::from("/var/log/bog/alerts.log"),
                file_min_severity: "Warning".to_string(),
                webhook_output: true,
                webhook_url: Some("https://hooks.pagerduty.com/production-critical".to_string()),
                webhook_min_severity: "Critical".to_string(),
                rate_limit_secs: 60,
                auto_resolve_secs: 300,
                rules: AlertRulesConfig {
                    position_limit: true,
                    position_limit_override: None,
                    daily_loss_limit: true,
                    daily_loss_limit_override: None,
                    huginn_connection: true,
                    huginn_grace_period_secs: 5,
                    rejection_rate: true,
                    rejection_threshold: 0.1, // 10%
                    latency: true,
                    latency_threshold_us: 1000.0, // 1ms
                },
            },
        }
    }

    /// Load profile by name
    pub fn load(profile: ProfileName) -> Config {
        match profile {
            ProfileName::Development => Self::development(),
            ProfileName::Staging => Self::staging(),
            ProfileName::Production => Self::production(),
        }
    }

    /// Load profile from environment variable BOG_PROFILE
    pub fn from_env() -> Config {
        let profile = std::env::var("BOG_PROFILE")
            .ok()
            .and_then(|s| ProfileName::from_str(&s))
            .unwrap_or(ProfileName::Development);

        Self::load(profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_name_from_str() {
        assert_eq!(ProfileName::from_str("dev"), Some(ProfileName::Development));
        assert_eq!(
            ProfileName::from_str("development"),
            Some(ProfileName::Development)
        );
        assert_eq!(ProfileName::from_str("staging"), Some(ProfileName::Staging));
        assert_eq!(ProfileName::from_str("prod"), Some(ProfileName::Production));
        assert_eq!(
            ProfileName::from_str("production"),
            Some(ProfileName::Production)
        );
        assert_eq!(ProfileName::from_str("invalid"), None);
    }

    #[test]
    fn test_development_profile() {
        let config = ConfigProfile::development();
        assert_eq!(config.execution.mode, "simulated");
        assert_eq!(config.metrics.log_level, "debug");
        assert_eq!(config.alerts.console_min_severity, "Info");
        assert_eq!(config.risk.max_position, dec!(0.1));
    }

    #[test]
    fn test_staging_profile() {
        let config = ConfigProfile::staging();
        assert_eq!(config.execution.mode, "simulated");
        assert_eq!(config.metrics.log_level, "info");
        assert_eq!(config.alerts.console_min_severity, "Warning");
        assert_eq!(config.risk.max_position, dec!(0.5));
        assert!(config.alerts.webhook_output);
    }

    #[test]
    fn test_production_profile() {
        let config = ConfigProfile::production();
        assert_eq!(config.execution.mode, "live");
        assert_eq!(config.metrics.log_level, "warn");
        assert_eq!(config.alerts.console_min_severity, "Error");
        assert_eq!(config.risk.max_position, dec!(1.0));
        assert!(config.metrics.json_logs);
    }

    #[test]
    fn test_profile_validation() {
        // All profiles should pass validation
        assert!(ConfigProfile::development().validate().is_ok());
        assert!(ConfigProfile::staging().validate().is_ok());
        assert!(ConfigProfile::production().validate().is_ok());
    }
}
