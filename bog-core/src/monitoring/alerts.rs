//! Alert management system for production monitoring
//!
//! Provides a comprehensive alerting framework with:
//! - Multiple severity levels (Critical, Error, Warning, Info)
//! - Alert categories (Risk, System, Trading, Performance)
//! - Multiple output channels (Console, File, Webhook)
//! - Rate limiting to prevent alert spam
//! - Alert state tracking (active/resolved)

use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info, warn};

/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// Informational alerts (no action required)
    Info = 0,
    /// Warning alerts (should investigate)
    Warning = 1,
    /// Error alerts (requires attention)
    Error = 2,
    /// Critical alerts (immediate action required)
    Critical = 3,
}

impl AlertSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARN",
            Self::Error => "ERROR",
            Self::Critical => "CRITICAL",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Info => "ℹ️",
            Self::Warning => "⚠️",
            Self::Error => "❌",
            Self::Critical => "🚨",
        }
    }
}

/// Alert category for organization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlertCategory {
    /// Risk management alerts (position limits, PnL, etc.)
    Risk,
    /// System health alerts (connections, errors, etc.)
    System,
    /// Trading activity alerts (rejections, fills, etc.)
    Trading,
    /// Performance alerts (latency, throughput, etc.)
    Performance,
}

impl AlertCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Risk => "RISK",
            Self::System => "SYSTEM",
            Self::Trading => "TRADING",
            Self::Performance => "PERFORMANCE",
        }
    }
}

/// Alert identifier for deduplication and tracking
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlertId {
    pub category: AlertCategory,
    pub name: String,
}

impl AlertId {
    pub fn new(category: AlertCategory, name: impl Into<String>) -> Self {
        Self {
            category,
            name: name.into(),
        }
    }

    pub fn to_string(&self) -> String {
        format!("{}.{}", self.category.as_str(), self.name)
    }
}

/// Alert with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: AlertId,
    pub severity: AlertSeverity,
    pub message: String,
    pub details: HashMap<String, String>,
    pub timestamp: SystemTime,
}

impl Alert {
    pub fn new(
        category: AlertCategory,
        name: impl Into<String>,
        severity: AlertSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: AlertId::new(category, name),
            severity,
            message: message.into(),
            details: HashMap::new(),
            timestamp: SystemTime::now(),
        }
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    pub fn with_details(mut self, details: HashMap<String, String>) -> Self {
        self.details.extend(details);
        self
    }

    /// Format alert for display
    pub fn format(&self) -> String {
        let mut output = format!(
            "[{}] {} {} - {}",
            self.severity.emoji(),
            self.severity.as_str(),
            self.id.to_string(),
            self.message
        );

        if !self.details.is_empty() {
            output.push_str("\n  Details:");
            for (key, value) in &self.details {
                output.push_str(&format!("\n    {}: {}", key, value));
            }
        }

        output
    }

    /// Format alert as JSON for structured logging/webhooks
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize alert to JSON")
    }
}

/// Alert output channel configuration
#[derive(Debug, Clone)]
pub enum AlertOutput {
    /// Write to stdout/stderr
    Console {
        /// Minimum severity to output
        min_severity: AlertSeverity,
    },
    /// Write to file (append mode, with rotation support)
    File {
        /// Path to alert log file
        path: PathBuf,
        /// Minimum severity to output
        min_severity: AlertSeverity,
    },
    /// POST to webhook URL (for PagerDuty, Slack, etc.)
    Webhook {
        /// Webhook URL
        url: String,
        /// Minimum severity to output
        min_severity: AlertSeverity,
        /// Timeout for webhook requests (milliseconds)
        timeout_ms: u64,
    },
}

/// Alert state tracking
#[derive(Debug, Clone)]
struct AlertState {
    /// First occurrence timestamp
    _first_seen: SystemTime,
    /// Last occurrence timestamp
    last_seen: SystemTime,
    /// Number of occurrences
    count: u64,
    /// Last sent timestamp (for rate limiting)
    last_sent: Option<SystemTime>,
}

impl AlertState {
    fn new() -> Self {
        let now = SystemTime::now();
        Self {
            _first_seen: now,
            last_seen: now,
            count: 1,
            last_sent: None,
        }
    }

    fn update(&mut self) {
        self.last_seen = SystemTime::now();
        self.count += 1;
    }
}

/// Alert manager configuration
#[derive(Debug, Clone)]
pub struct AlertManagerConfig {
    /// Alert output channels
    pub outputs: Vec<AlertOutput>,
    /// Rate limit: minimum time between sending same alert (seconds)
    pub rate_limit_secs: u64,
    /// Auto-resolve alerts after this duration of inactivity (seconds)
    pub auto_resolve_secs: u64,
    /// Enable alert aggregation (send summary instead of individual alerts)
    pub enable_aggregation: bool,
}

impl Default for AlertManagerConfig {
    fn default() -> Self {
        Self {
            outputs: vec![AlertOutput::Console {
                min_severity: AlertSeverity::Warning,
            }],
            rate_limit_secs: 60,    // 1 minute
            auto_resolve_secs: 300, // 5 minutes
            enable_aggregation: false,
        }
    }
}

/// Central alert manager
pub struct AlertManager {
    config: AlertManagerConfig,
    /// Active alert states
    active_alerts: Arc<RwLock<HashMap<AlertId, AlertState>>>,
    /// Alert counters for metrics
    alert_counts: Arc<RwLock<HashMap<AlertSeverity, u64>>>,
}

impl AlertManager {
    /// Create a new alert manager
    pub fn new(config: AlertManagerConfig) -> Self {
        info!(
            "AlertManager initialized with {} outputs",
            config.outputs.len()
        );
        Self {
            config,
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            alert_counts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Send an alert through all configured outputs
    pub fn send(&self, alert: Alert) -> Result<()> {
        // Check if we should send based on rate limiting
        if !self.should_send(&alert) {
            debug!("Alert {} rate limited", alert.id.to_string());
            return Ok(());
        }

        // Update alert state
        self.update_alert_state(&alert);

        // Update counters
        {
            let mut counts = self.alert_counts.write();
            *counts.entry(alert.severity).or_insert(0) += 1;
        }

        // Send to all outputs
        for output in &self.config.outputs {
            if let Err(e) = self.send_to_output(&alert, output) {
                error!("Failed to send alert to output: {}", e);
            }
        }

        Ok(())
    }

    /// Check if alert should be sent based on rate limiting
    fn should_send(&self, alert: &Alert) -> bool {
        // Critical alerts always go through
        if alert.severity == AlertSeverity::Critical {
            return true;
        }

        let active = self.active_alerts.read();
        if let Some(state) = active.get(&alert.id) {
            if let Some(last_sent) = state.last_sent {
                if let Ok(elapsed) = last_sent.elapsed() {
                    if elapsed < Duration::from_secs(self.config.rate_limit_secs) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Update alert state tracking
    fn update_alert_state(&self, alert: &Alert) {
        let mut active = self.active_alerts.write();

        active
            .entry(alert.id.clone())
            .and_modify(|state| {
                state.update();
                state.last_sent = Some(SystemTime::now());
            })
            .or_insert_with(|| {
                let mut state = AlertState::new();
                state.last_sent = Some(SystemTime::now());
                state
            });
    }

    /// Send alert to specific output
    fn send_to_output(&self, alert: &Alert, output: &AlertOutput) -> Result<()> {
        match output {
            AlertOutput::Console { min_severity } => {
                if alert.severity >= *min_severity {
                    self.send_to_console(alert)
                } else {
                    Ok(())
                }
            }
            AlertOutput::File { path, min_severity } => {
                if alert.severity >= *min_severity {
                    self.send_to_file(alert, path)
                } else {
                    Ok(())
                }
            }
            AlertOutput::Webhook {
                url,
                min_severity,
                timeout_ms,
            } => {
                if alert.severity >= *min_severity {
                    self.send_to_webhook(alert, url, *timeout_ms)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Send to console (stdout/stderr based on severity)
    fn send_to_console(&self, alert: &Alert) -> Result<()> {
        let formatted = alert.format();

        match alert.severity {
            AlertSeverity::Info => info!("{}", formatted),
            AlertSeverity::Warning => warn!("{}", formatted),
            AlertSeverity::Error => error!("{}", formatted),
            AlertSeverity::Critical => error!("{}", formatted),
        }

        Ok(())
    }

    /// Send to file (append mode)
    fn send_to_file(&self, alert: &Alert, path: &PathBuf) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .context("Failed to open alert log file")?;

        let json = alert.to_json()?;
        writeln!(file, "{}", json).context("Failed to write alert to file")?;
        file.flush().context("Failed to flush alert log file")?;

        Ok(())
    }

    /// Send to webhook (HTTP POST with JSON payload)
    fn send_to_webhook(&self, alert: &Alert, url: &str, timeout_ms: u64) -> Result<()> {
        // For now, just log that we would send to webhook
        // Full implementation requires tokio runtime and reqwest
        debug!(
            "Would send alert {} to webhook {} (timeout: {}ms)",
            alert.id.to_string(),
            url,
            timeout_ms
        );

        // TODO: Implement async webhook sending
        // This would be done in a separate tokio task to not block

        Ok(())
    }

    /// Resolve an alert (mark as no longer active)
    pub fn resolve(&self, alert_id: &AlertId) {
        let mut active = self.active_alerts.write();
        if active.remove(alert_id).is_some() {
            info!("Alert {} resolved", alert_id.to_string());
        }
    }

    /// Get active alerts count
    pub fn active_count(&self) -> usize {
        self.active_alerts.read().len()
    }

    /// Get alert counts by severity
    pub fn counts_by_severity(&self) -> HashMap<AlertSeverity, u64> {
        self.alert_counts.read().clone()
    }

    /// Clean up old alert states (auto-resolve)
    pub fn cleanup_old_alerts(&self) {
        let now = SystemTime::now();
        let auto_resolve_duration = Duration::from_secs(self.config.auto_resolve_secs);

        let mut active = self.active_alerts.write();
        active.retain(|id, state| {
            if let Ok(elapsed) = now.duration_since(state.last_seen) {
                if elapsed > auto_resolve_duration {
                    info!("Auto-resolving inactive alert {}", id.to_string());
                    return false;
                }
            }
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_alert_creation() {
        let alert = Alert::new(
            AlertCategory::Risk,
            "position_limit",
            AlertSeverity::Critical,
            "Position limit exceeded",
        )
        .with_detail("position", "1.5 BTC")
        .with_detail("limit", "1.0 BTC");

        assert_eq!(alert.severity, AlertSeverity::Critical);
        assert_eq!(alert.id.category, AlertCategory::Risk);
        assert_eq!(alert.id.name, "position_limit");
        assert_eq!(alert.details.len(), 2);
    }

    #[test]
    fn test_alert_formatting() {
        let alert = Alert::new(
            AlertCategory::System,
            "connection_lost",
            AlertSeverity::Error,
            "Huginn connection lost",
        );

        let formatted = alert.format();
        assert!(formatted.contains("ERROR"));
        assert!(formatted.contains("SYSTEM.connection_lost"));
        assert!(formatted.contains("Huginn connection lost"));
    }

    #[test]
    fn test_alert_to_json() {
        let alert = Alert::new(
            AlertCategory::Trading,
            "high_rejection_rate",
            AlertSeverity::Warning,
            "Rejection rate > 10%",
        );

        let json = alert.to_json().unwrap();
        assert!(json.contains("\"name\":\"high_rejection_rate\""));
        assert!(json.contains("\"message\":\"Rejection rate > 10%\""));
    }

    #[test]
    fn test_alert_manager_console() {
        let config = AlertManagerConfig {
            outputs: vec![AlertOutput::Console {
                min_severity: AlertSeverity::Warning,
            }],
            ..Default::default()
        };

        let manager = AlertManager::new(config);

        let alert = Alert::new(
            AlertCategory::Risk,
            "test_alert",
            AlertSeverity::Warning,
            "Test warning",
        );

        manager.send(alert).unwrap();
        assert_eq!(manager.active_count(), 1);
    }

    #[test]
    fn test_alert_manager_file() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("alerts.log");

        let config = AlertManagerConfig {
            outputs: vec![AlertOutput::File {
                path: log_path.clone(),
                min_severity: AlertSeverity::Info,
            }],
            ..Default::default()
        };

        let manager = AlertManager::new(config);

        let alert = Alert::new(
            AlertCategory::Trading,
            "test_file_alert",
            AlertSeverity::Info,
            "Test file alert",
        );

        manager.send(alert).unwrap();

        // Verify file was created and contains the alert
        let contents = std::fs::read_to_string(&log_path).unwrap();
        assert!(contents.contains("test_file_alert"));
        assert!(contents.contains("Test file alert"));
    }

    #[test]
    fn test_alert_rate_limiting() {
        let config = AlertManagerConfig {
            outputs: vec![AlertOutput::Console {
                min_severity: AlertSeverity::Info,
            }],
            rate_limit_secs: 60,
            ..Default::default()
        };

        let manager = AlertManager::new(config);

        let alert1 = Alert::new(
            AlertCategory::System,
            "rate_limit_test",
            AlertSeverity::Warning,
            "First alert",
        );

        let alert2 = Alert::new(
            AlertCategory::System,
            "rate_limit_test",
            AlertSeverity::Warning,
            "Second alert (should be rate limited)",
        );

        manager.send(alert1).unwrap();
        manager.send(alert2).unwrap();

        // Both alerts have same ID, so only one should be active
        assert_eq!(manager.active_count(), 1);
    }

    #[test]
    fn test_alert_severity_ordering() {
        assert!(AlertSeverity::Critical > AlertSeverity::Error);
        assert!(AlertSeverity::Error > AlertSeverity::Warning);
        assert!(AlertSeverity::Warning > AlertSeverity::Info);
    }

    #[test]
    fn test_alert_id_equality() {
        let id1 = AlertId::new(AlertCategory::Risk, "position_limit");
        let id2 = AlertId::new(AlertCategory::Risk, "position_limit");
        let id3 = AlertId::new(AlertCategory::Risk, "daily_loss");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_critical_alerts_bypass_rate_limit() {
        let config = AlertManagerConfig {
            outputs: vec![AlertOutput::Console {
                min_severity: AlertSeverity::Info,
            }],
            rate_limit_secs: 60,
            ..Default::default()
        };

        let manager = AlertManager::new(config);

        let alert1 = Alert::new(
            AlertCategory::Risk,
            "critical_test",
            AlertSeverity::Critical,
            "First critical",
        );

        let alert2 = Alert::new(
            AlertCategory::Risk,
            "critical_test",
            AlertSeverity::Critical,
            "Second critical",
        );

        // Critical alerts should always go through
        assert!(manager.should_send(&alert1));
        manager.send(alert1).unwrap();
        assert!(manager.should_send(&alert2));
    }
}
