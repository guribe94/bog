//! Alert rules engine for automatic alert triggering
//!
//! Defines rules that monitor system state and trigger alerts when
//! thresholds are exceeded or conditions are met.

use super::alerts::{Alert, AlertCategory, AlertManager, AlertSeverity};
use super::MetricsRegistry;
use crate::core::Position;
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::debug;

/// Rule evaluation context
pub struct RuleContext {
    /// Current position (if available)
    pub position: Option<Arc<Position>>,
    /// Prometheus metrics registry
    pub metrics: Arc<MetricsRegistry>,
    /// Current timestamp
    pub timestamp: SystemTime,
}

/// Alert rule trait
pub trait AlertRule: Send + Sync {
    /// Rule name for identification
    fn name(&self) -> &str;

    /// Rule category
    fn category(&self) -> AlertCategory;

    /// Evaluate rule and return alert if triggered
    fn evaluate(&self, context: &RuleContext) -> Option<Alert>;

    /// Check if rule is enabled
    fn is_enabled(&self) -> bool {
        true
    }
}

/// Position limit rule
pub struct PositionLimitRule {
    /// Maximum allowed position (absolute value, in base units)
    pub max_position: i64,
    /// Alert severity
    pub severity: AlertSeverity,
}

impl PositionLimitRule {
    pub fn new(max_position: i64, severity: AlertSeverity) -> Self {
        Self {
            max_position,
            severity,
        }
    }
}

impl AlertRule for PositionLimitRule {
    fn name(&self) -> &str {
        "position_limit_exceeded"
    }

    fn category(&self) -> AlertCategory {
        AlertCategory::Risk
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Alert> {
        if let Some(position) = &context.position {
            let current_position = position.get_quantity();
            let abs_position = current_position.abs();

            if abs_position > self.max_position {
                let alert = Alert::new(
                    self.category(),
                    self.name(),
                    self.severity,
                    format!(
                        "Position limit exceeded: {} > {}",
                        current_position, self.max_position
                    ),
                )
                .with_detail("current_position", current_position.to_string())
                .with_detail("limit", self.max_position.to_string())
                .with_detail(
                    "excess",
                    (abs_position - self.max_position).to_string(),
                );

                return Some(alert);
            }
        }

        None
    }
}

/// Daily loss limit rule
pub struct DailyLossLimitRule {
    /// Maximum allowed daily loss (positive value, in base units)
    pub max_daily_loss: i64,
    /// Alert severity
    pub severity: AlertSeverity,
}

impl DailyLossLimitRule {
    pub fn new(max_daily_loss: i64, severity: AlertSeverity) -> Self {
        Self {
            max_daily_loss,
            severity,
        }
    }
}

impl AlertRule for DailyLossLimitRule {
    fn name(&self) -> &str {
        "daily_loss_limit_exceeded"
    }

    fn category(&self) -> AlertCategory {
        AlertCategory::Risk
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Alert> {
        if let Some(position) = &context.position {
            let daily_pnl = position.get_daily_pnl();

            // Check if daily PnL is a loss (negative) and exceeds limit
            if daily_pnl < 0 && daily_pnl.abs() > self.max_daily_loss {
                let alert = Alert::new(
                    self.category(),
                    self.name(),
                    self.severity,
                    format!(
                        "Daily loss limit exceeded: {} < -{}",
                        daily_pnl, self.max_daily_loss
                    ),
                )
                .with_detail("daily_pnl", daily_pnl.to_string())
                .with_detail("limit", self.max_daily_loss.to_string())
                .with_detail(
                    "excess_loss",
                    (daily_pnl.abs() - self.max_daily_loss).to_string(),
                );

                return Some(alert);
            }
        }

        None
    }
}

/// High rejection rate rule
pub struct HighRejectionRateRule {
    /// Rejection rate threshold (0.0 to 1.0)
    pub threshold: f64,
    /// Minimum orders required to evaluate
    pub min_orders: u64,
    /// Alert severity
    pub severity: AlertSeverity,
}

impl HighRejectionRateRule {
    pub fn new(threshold: f64, min_orders: u64, severity: AlertSeverity) -> Self {
        Self {
            threshold,
            min_orders,
            severity,
        }
    }
}

impl AlertRule for HighRejectionRateRule {
    fn name(&self) -> &str {
        "high_rejection_rate"
    }

    fn category(&self) -> AlertCategory {
        AlertCategory::Trading
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Alert> {
        // Calculate rejection rate from metrics
        // This is a simplified implementation - production would aggregate over time window
        let metrics = context.metrics.trading();

        // Get total orders and rejections
        // Note: In production, we'd need to aggregate these from Prometheus
        // For now, we'll use a placeholder approach

        // TODO: Implement proper metric aggregation from Prometheus
        // For now, return None as we don't have easy access to current values

        None
    }
}

/// Huginn connection lost rule
pub struct HuginnConnectionRule {
    /// How long connection can be down before alerting
    pub grace_period: Duration,
    /// Alert severity
    pub severity: AlertSeverity,
}

impl HuginnConnectionRule {
    pub fn new(grace_period: Duration, severity: AlertSeverity) -> Self {
        Self {
            grace_period,
            severity,
        }
    }
}

impl AlertRule for HuginnConnectionRule {
    fn name(&self) -> &str {
        "huginn_connection_lost"
    }

    fn category(&self) -> AlertCategory {
        AlertCategory::System
    }

    fn evaluate(&self, context: &RuleContext) -> Option<Alert> {
        // Check Huginn connection status from metrics
        let huginn_connected = context.metrics.system().huginn_connected.get();

        if huginn_connected == 0 {
            let alert = Alert::new(
                self.category(),
                self.name(),
                self.severity,
                "Huginn market data connection lost",
            )
            .with_detail("grace_period_secs", self.grace_period.as_secs().to_string())
            .with_detail("action", "Check Huginn service and shared memory");

            return Some(alert);
        }

        None
    }
}

/// High latency rule
pub struct HighLatencyRule {
    /// Latency threshold in nanoseconds
    pub threshold_ns: f64,
    /// Alert severity
    pub severity: AlertSeverity,
}

impl HighLatencyRule {
    pub fn new(threshold_ns: f64, severity: AlertSeverity) -> Self {
        Self {
            threshold_ns,
            severity,
        }
    }
}

impl AlertRule for HighLatencyRule {
    fn name(&self) -> &str {
        "high_tick_to_trade_latency"
    }

    fn category(&self) -> AlertCategory {
        AlertCategory::Performance
    }

    fn evaluate(&self, _context: &RuleContext) -> Option<Alert> {
        // Get latest tick-to-trade latency from histogram
        // This requires accessing histogram samples, which is complex
        // For now, we'll implement this as a stub

        // TODO: Implement histogram percentile checking
        // Would need to check p99 or max latency against threshold

        None
    }
}

/// Rule engine that evaluates all rules periodically
pub struct RuleEngine {
    rules: Vec<Box<dyn AlertRule>>,
    alert_manager: Arc<AlertManager>,
}

impl RuleEngine {
    /// Create a new rule engine
    pub fn new(alert_manager: Arc<AlertManager>) -> Self {
        Self {
            rules: Vec::new(),
            alert_manager,
        }
    }

    /// Add a rule to the engine
    pub fn add_rule(&mut self, rule: Box<dyn AlertRule>) {
        debug!("Adding alert rule: {}", rule.name());
        self.rules.push(rule);
    }

    /// Add default production rules
    pub fn with_default_rules(mut self) -> Self {
        // Risk rules
        self.add_rule(Box::new(PositionLimitRule::new(
            1_000_000_000, // 1.0 BTC
            AlertSeverity::Critical,
        )));

        self.add_rule(Box::new(DailyLossLimitRule::new(
            1_000_000_000_000, // $1,000
            AlertSeverity::Critical,
        )));

        // System rules
        self.add_rule(Box::new(HuginnConnectionRule::new(
            Duration::from_secs(5),
            AlertSeverity::Critical,
        )));

        // Trading rules
        self.add_rule(Box::new(HighRejectionRateRule::new(
            0.1, // 10%
            10,  // min 10 orders
            AlertSeverity::Warning,
        )));

        // Performance rules
        self.add_rule(Box::new(HighLatencyRule::new(
            1_000_000.0, // 1ms (1,000,000 ns)
            AlertSeverity::Warning,
        )));

        self
    }

    /// Evaluate all rules and send alerts
    pub fn evaluate_all(&self, context: &RuleContext) -> Result<()> {
        for rule in &self.rules {
            if !rule.is_enabled() {
                continue;
            }

            if let Some(alert) = rule.evaluate(context) {
                debug!("Alert triggered: {}", alert.id.to_string());
                self.alert_manager.send(alert)?;
            }
        }

        Ok(())
    }

    /// Get number of rules
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::alerts::AlertManagerConfig;

    fn create_test_context() -> RuleContext {
        let position = Arc::new(Position::new());
        let metrics = Arc::new(MetricsRegistry::new().unwrap());

        RuleContext {
            position: Some(position),
            metrics,
            timestamp: SystemTime::now(),
        }
    }

    #[test]
    fn test_position_limit_rule() {
        let rule = PositionLimitRule::new(1_000_000_000, AlertSeverity::Critical);
        let context = create_test_context();

        // Within limit - no alert
        assert!(rule.evaluate(&context).is_none());

        // Simulate exceeding limit
        if let Some(position) = &context.position {
            // Set position to 1.5 BTC (exceeds 1.0 BTC limit)
            position.quantity.store(1_500_000_000, std::sync::atomic::Ordering::Relaxed);
        }

        // Should trigger alert
        let alert = rule.evaluate(&context);
        assert!(alert.is_some());

        let alert = alert.unwrap();
        assert_eq!(alert.severity, AlertSeverity::Critical);
        assert_eq!(alert.id.name, "position_limit_exceeded");
        assert!(alert.details.contains_key("current_position"));
    }

    #[test]
    fn test_daily_loss_limit_rule() {
        let rule = DailyLossLimitRule::new(1_000_000_000_000, AlertSeverity::Critical);
        let context = create_test_context();

        // Within limit - no alert
        assert!(rule.evaluate(&context).is_none());

        // Simulate daily loss
        if let Some(position) = &context.position {
            // Set daily PnL to -$1,500 (exceeds -$1,000 limit)
            position.daily_pnl.store(-1_500_000_000_000, std::sync::atomic::Ordering::Relaxed);
        }

        // Should trigger alert
        let alert = rule.evaluate(&context);
        assert!(alert.is_some());

        let alert = alert.unwrap();
        assert_eq!(alert.severity, AlertSeverity::Critical);
        assert_eq!(alert.id.name, "daily_loss_limit_exceeded");
    }

    #[test]
    fn test_huginn_connection_rule() {
        let rule = HuginnConnectionRule::new(Duration::from_secs(5), AlertSeverity::Critical);
        let context = create_test_context();

        // Connection is up (default) - no alert
        assert!(rule.evaluate(&context).is_none());

        // Simulate connection loss
        context.metrics.system().huginn_connected.set(0);

        // Should trigger alert
        let alert = rule.evaluate(&context);
        assert!(alert.is_some());

        let alert = alert.unwrap();
        assert_eq!(alert.severity, AlertSeverity::Critical);
        assert_eq!(alert.id.name, "huginn_connection_lost");
    }

    #[test]
    fn test_rule_engine_with_default_rules() {
        let alert_config = AlertManagerConfig::default();
        let alert_manager = Arc::new(AlertManager::new(alert_config));
        let engine = RuleEngine::new(alert_manager).with_default_rules();

        assert!(engine.rule_count() >= 4); // At least 4 default rules
    }

    #[test]
    fn test_rule_engine_evaluation() {
        let alert_config = AlertManagerConfig::default();
        let alert_manager = Arc::new(AlertManager::new(alert_config));
        let mut engine = RuleEngine::new(alert_manager);

        // Add a rule that will trigger
        engine.add_rule(Box::new(PositionLimitRule::new(
            500_000_000, // 0.5 BTC limit
            AlertSeverity::Warning,
        )));

        let context = create_test_context();

        // Set position to exceed limit
        if let Some(position) = &context.position {
            position.quantity.store(1_000_000_000, std::sync::atomic::Ordering::Relaxed); // 1.0 BTC
        }

        // Evaluate - should trigger alert
        engine.evaluate_all(&context).unwrap();
    }
}
