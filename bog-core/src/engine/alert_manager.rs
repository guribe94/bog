//! Comprehensive Alert System for Trading Anomaly Detection
//!
//! Provides real-time monitoring and alerting for various trading conditions
//! including data quality issues, position anomalies, and system failures.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};
use tracing::{error, warn, info};

/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AlertSeverity {
    /// Informational message
    Info = 1,
    /// Warning - potential issue but not critical
    Warning = 2,
    /// Error - significant issue that needs attention
    Error = 3,
    /// Critical - immediate action required, may halt trading
    Critical = 4,
}

/// Types of alerts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlertType {
    // Data Feed Issues
    DataStale,
    DataGap,
    DataInvalid,
    HuginnRestart,
    HighQueueDepth,

    // Position Issues
    PositionDrift,
    PositionLimitExceeded,
    UnexpectedFill,

    // Market Conditions
    OrderbookCrossed,
    SpreadTooWide,
    LowLiquidity,
    PriceSpike,

    // System Issues
    HighLatency,
    MemoryPressure,
    RecoveryFailed,
    CircuitBreakerTriggered,

    // Trading Issues
    OrderRejected,
    ExecutionFailed,
    RiskLimitHit,
}

impl AlertType {
    /// Get default severity for this alert type
    pub fn default_severity(&self) -> AlertSeverity {
        match self {
            // Info level
            AlertType::HighQueueDepth => AlertSeverity::Info,

            // Warning level
            AlertType::DataStale |
            AlertType::SpreadTooWide |
            AlertType::LowLiquidity |
            AlertType::HighLatency => AlertSeverity::Warning,

            // Error level
            AlertType::DataGap |
            AlertType::HuginnRestart |
            AlertType::PositionDrift |
            AlertType::OrderRejected |
            AlertType::ExecutionFailed => AlertSeverity::Error,

            // Critical level
            AlertType::DataInvalid |
            AlertType::OrderbookCrossed |
            AlertType::PriceSpike |
            AlertType::PositionLimitExceeded |
            AlertType::UnexpectedFill |
            AlertType::RecoveryFailed |
            AlertType::CircuitBreakerTriggered |
            AlertType::RiskLimitHit |
            AlertType::MemoryPressure => AlertSeverity::Critical,
        }
    }
}

/// Alert instance
#[derive(Debug, Clone)]
pub struct Alert {
    /// Alert type
    pub alert_type: AlertType,

    /// Severity level
    pub severity: AlertSeverity,

    /// Alert message
    pub message: String,

    /// Additional context
    pub context: HashMap<String, String>,

    /// Timestamp
    pub timestamp: SystemTime,

    /// Alert ID (unique)
    pub id: u64,
}

/// Alert configuration
#[derive(Debug, Clone)]
pub struct AlertConfig {
    /// Enable/disable specific alert types
    pub enabled_alerts: HashMap<AlertType, bool>,

    /// Custom severity overrides
    pub severity_overrides: HashMap<AlertType, AlertSeverity>,

    /// Rate limiting - max alerts per minute per type
    pub rate_limit_per_minute: u32,

    /// Per-severity rate limits (overrides general rate_limit_per_minute)
    /// Critical alerts should have higher or no limits
    pub rate_limit_by_severity: HashMap<AlertSeverity, u32>,

    /// Alert aggregation window
    pub aggregation_window: Duration,

    /// Log alerts to file
    pub log_to_file: bool,

    /// Send alerts to monitoring system
    pub send_to_monitoring: bool,

    /// Halt trading on critical alerts
    pub halt_on_critical: bool,
}

impl Default for AlertConfig {
    fn default() -> Self {
        let mut enabled_alerts = HashMap::new();
        // Enable all alerts by default
        for alert_type in [
            AlertType::DataStale,
            AlertType::DataGap,
            AlertType::DataInvalid,
            AlertType::HuginnRestart,
            AlertType::HighQueueDepth,
            AlertType::PositionDrift,
            AlertType::PositionLimitExceeded,
            AlertType::UnexpectedFill,
            AlertType::OrderbookCrossed,
            AlertType::SpreadTooWide,
            AlertType::LowLiquidity,
            AlertType::PriceSpike,
            AlertType::HighLatency,
            AlertType::MemoryPressure,
            AlertType::RecoveryFailed,
            AlertType::CircuitBreakerTriggered,
            AlertType::OrderRejected,
            AlertType::ExecutionFailed,
            AlertType::RiskLimitHit,
        ] {
            enabled_alerts.insert(alert_type, true);
        }

        // Set per-severity rate limits
        let mut rate_limit_by_severity = HashMap::new();
        rate_limit_by_severity.insert(AlertSeverity::Info, 20);       // 20 info alerts per minute
        rate_limit_by_severity.insert(AlertSeverity::Warning, 15);    // 15 warnings per minute
        rate_limit_by_severity.insert(AlertSeverity::Error, 10);      // 10 errors per minute
        rate_limit_by_severity.insert(AlertSeverity::Critical, 100);  // 100 critical alerts (effectively unlimited)

        Self {
            enabled_alerts,
            severity_overrides: HashMap::new(),
            rate_limit_per_minute: 10,  // Default fallback
            rate_limit_by_severity,
            aggregation_window: Duration::from_secs(60),
            log_to_file: true,
            send_to_monitoring: false,
            halt_on_critical: true,
        }
    }
}

/// Alert statistics
#[derive(Debug, Clone)]
pub struct AlertStats {
    pub total_alerts: u64,
    pub info_count: u64,
    pub warning_count: u64,
    pub error_count: u64,
    pub critical_count: u64,
    pub alerts_by_type: HashMap<AlertType, u64>,
    pub last_alert_time: Option<Instant>,
}

/// Alert manager for centralized alert handling
pub struct AlertManager {
    config: AlertConfig,
    next_alert_id: AtomicU64,
    alert_history: Vec<Alert>,
    alert_counts: HashMap<AlertType, Vec<Instant>>,
    stats: AlertStats,
    trading_halted: bool,
}

impl AlertManager {
    /// Create new alert manager
    pub fn new(config: AlertConfig) -> Self {
        Self {
            config,
            next_alert_id: AtomicU64::new(1),
            alert_history: Vec::new(),
            alert_counts: HashMap::new(),
            stats: AlertStats {
                total_alerts: 0,
                info_count: 0,
                warning_count: 0,
                error_count: 0,
                critical_count: 0,
                alerts_by_type: HashMap::new(),
                last_alert_time: None,
            },
            trading_halted: false,
        }
    }

    /// Raise an alert
    pub fn raise_alert(
        &mut self,
        alert_type: AlertType,
        message: String,
        context: HashMap<String, String>,
    ) -> Result<()> {
        // Check if alert type is enabled
        if !self.config.enabled_alerts.get(&alert_type).unwrap_or(&true) {
            return Ok(());
        }

        // Check rate limiting
        if !self.check_rate_limit(alert_type) {
            return Ok(());
        }

        // Get severity (with overrides)
        let severity = self.config.severity_overrides
            .get(&alert_type)
            .copied()
            .unwrap_or_else(|| alert_type.default_severity());

        // Create alert
        let alert = Alert {
            alert_type,
            severity,
            message: message.clone(),
            context,
            timestamp: SystemTime::now(),
            id: self.next_alert_id.fetch_add(1, Ordering::Relaxed),
        };

        // Log alert
        self.log_alert(&alert);

        // Update statistics
        self.update_stats(&alert);

        // Store in history
        self.alert_history.push(alert.clone());

        // Check if we should halt trading
        if severity == AlertSeverity::Critical && self.config.halt_on_critical {
            self.trading_halted = true;
            error!("🚨 CRITICAL ALERT: Trading halted due to {}: {}", alert_type.to_string(), message);
        }

        Ok(())
    }

    /// Check rate limiting for alert type with severity-aware limits
    fn check_rate_limit(&mut self, alert_type: AlertType) -> bool {
        let now = Instant::now();
        let window_start = now - Duration::from_secs(60);

        // Get severity for this alert type
        let severity = self.config.severity_overrides
            .get(&alert_type)
            .copied()
            .unwrap_or_else(|| alert_type.default_severity());

        // Get the appropriate rate limit based on severity
        let rate_limit = self.config.rate_limit_by_severity
            .get(&severity)
            .copied()
            .unwrap_or(self.config.rate_limit_per_minute);

        // Get or create count vector for this alert type
        let counts = self.alert_counts.entry(alert_type).or_insert_with(Vec::new);

        // Remove old entries outside the window
        counts.retain(|&time| time > window_start);

        // Check if we're at the limit
        if counts.len() >= rate_limit as usize {
            // Don't suppress critical alerts - just warn about rate
            if severity == AlertSeverity::Critical {
                warn!("Critical alert rate high: {} alerts in last minute", counts.len());
                // Still allow critical alerts through
            } else {
                return false;
            }
        }

        // Add this alert
        counts.push(now);
        true
    }

    /// Log alert based on severity
    fn log_alert(&self, alert: &Alert) {
        let context_str = alert.context
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(", ");

        match alert.severity {
            AlertSeverity::Info => {
                info!("ℹ️ ALERT [{}]: {} | {}", alert.alert_type.to_string(), alert.message, context_str);
            }
            AlertSeverity::Warning => {
                warn!("⚠️ ALERT [{}]: {} | {}", alert.alert_type.to_string(), alert.message, context_str);
            }
            AlertSeverity::Error => {
                error!("❌ ALERT [{}]: {} | {}", alert.alert_type.to_string(), alert.message, context_str);
            }
            AlertSeverity::Critical => {
                error!("🚨 CRITICAL [{}]: {} | {}", alert.alert_type.to_string(), alert.message, context_str);
            }
        }
    }

    /// Update statistics
    fn update_stats(&mut self, alert: &Alert) {
        self.stats.total_alerts += 1;
        self.stats.last_alert_time = Some(Instant::now());

        match alert.severity {
            AlertSeverity::Info => self.stats.info_count += 1,
            AlertSeverity::Warning => self.stats.warning_count += 1,
            AlertSeverity::Error => self.stats.error_count += 1,
            AlertSeverity::Critical => self.stats.critical_count += 1,
        }

        *self.stats.alerts_by_type.entry(alert.alert_type).or_insert(0) += 1;
    }

    /// Check if trading is halted
    pub fn is_trading_halted(&self) -> bool {
        self.trading_halted
    }

    /// Reset trading halt
    pub fn reset_halt(&mut self) {
        self.trading_halted = false;
        info!("Trading halt reset - resuming normal operation");
    }

    /// Get alert statistics
    pub fn stats(&self) -> &AlertStats {
        &self.stats
    }

    /// Get recent alerts
    pub fn recent_alerts(&self, count: usize) -> Vec<&Alert> {
        let start = self.alert_history.len().saturating_sub(count);
        self.alert_history[start..].iter().collect()
    }

    /// Clear alert history
    pub fn clear_history(&mut self) {
        self.alert_history.clear();
        self.alert_counts.clear();
    }

    /// Log statistics summary
    pub fn log_summary(&self) {
        info!("Alert Summary:");
        info!("  Total Alerts: {}", self.stats.total_alerts);
        info!("  Info: {}, Warning: {}, Error: {}, Critical: {}",
            self.stats.info_count,
            self.stats.warning_count,
            self.stats.error_count,
            self.stats.critical_count
        );

        if !self.stats.alerts_by_type.is_empty() {
            info!("  By Type:");
            for (alert_type, count) in &self.stats.alerts_by_type {
                info!("    - {}: {}", alert_type.to_string(), count);
            }
        }
    }
}

impl AlertType {
    fn to_string(&self) -> &'static str {
        match self {
            AlertType::DataStale => "DATA_STALE",
            AlertType::DataGap => "DATA_GAP",
            AlertType::DataInvalid => "DATA_INVALID",
            AlertType::HuginnRestart => "HUGINN_RESTART",
            AlertType::HighQueueDepth => "HIGH_QUEUE_DEPTH",
            AlertType::PositionDrift => "POSITION_DRIFT",
            AlertType::PositionLimitExceeded => "POSITION_LIMIT",
            AlertType::UnexpectedFill => "UNEXPECTED_FILL",
            AlertType::OrderbookCrossed => "ORDERBOOK_CROSSED",
            AlertType::SpreadTooWide => "SPREAD_WIDE",
            AlertType::LowLiquidity => "LOW_LIQUIDITY",
            AlertType::PriceSpike => "PRICE_SPIKE",
            AlertType::HighLatency => "HIGH_LATENCY",
            AlertType::MemoryPressure => "MEMORY_PRESSURE",
            AlertType::RecoveryFailed => "RECOVERY_FAILED",
            AlertType::CircuitBreakerTriggered => "CIRCUIT_BREAKER",
            AlertType::OrderRejected => "ORDER_REJECTED",
            AlertType::ExecutionFailed => "EXECUTION_FAILED",
            AlertType::RiskLimitHit => "RISK_LIMIT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_severity_ordering() {
        assert!(AlertSeverity::Critical > AlertSeverity::Error);
        assert!(AlertSeverity::Error > AlertSeverity::Warning);
        assert!(AlertSeverity::Warning > AlertSeverity::Info);
    }

    #[test]
    fn test_alert_manager_creation() {
        let config = AlertConfig::default();
        let manager = AlertManager::new(config);
        assert!(!manager.is_trading_halted());
        assert_eq!(manager.stats().total_alerts, 0);
    }

    #[test]
    fn test_alert_raising() {
        let config = AlertConfig::default();
        let mut manager = AlertManager::new(config);

        let mut context = HashMap::new();
        context.insert("gap_size".to_string(), "100".to_string());

        manager.raise_alert(
            AlertType::DataGap,
            "Sequence gap detected".to_string(),
            context,
        ).unwrap();

        assert_eq!(manager.stats().total_alerts, 1);
        assert_eq!(manager.stats().error_count, 1);
    }

    #[test]
    fn test_critical_alert_halts_trading() {
        let mut config = AlertConfig::default();
        config.halt_on_critical = true;
        let mut manager = AlertManager::new(config);

        manager.raise_alert(
            AlertType::DataInvalid,
            "Invalid data detected".to_string(),
            HashMap::new(),
        ).unwrap();

        assert!(manager.is_trading_halted());
    }

    #[test]
    fn test_rate_limiting() {
        let mut config = AlertConfig::default();
        // HighLatency has Warning severity, so we need to set that limit
        config.rate_limit_by_severity.insert(AlertSeverity::Warning, 2);
        let mut manager = AlertManager::new(config);

        // First two alerts should succeed
        for i in 0..2 {
            manager.raise_alert(
                AlertType::HighLatency,
                format!("High latency {}", i),
                HashMap::new(),
            ).unwrap();
        }

        assert_eq!(manager.stats().total_alerts, 2);

        // Third alert should be rate limited
        manager.raise_alert(
            AlertType::HighLatency,
            "High latency 3".to_string(),
            HashMap::new(),
        ).unwrap();

        // Should still be 2 due to rate limiting
        assert_eq!(manager.stats().total_alerts, 2);
    }
}