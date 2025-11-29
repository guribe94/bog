//! Monitoring and observability module
//!
//! Provides Prometheus metrics export, HTTP server for scraping,
//! alerting system, and integration with the execution layer for
//! comprehensive monitoring.

pub mod alert_rules;
pub mod alerts;
pub mod metrics;
pub mod server;

pub use alert_rules::{AlertRule, RuleContext, RuleEngine};
pub use alerts::{
    Alert, AlertCategory, AlertId, AlertManager, AlertManagerConfig, AlertOutput, AlertSeverity,
};
pub use metrics::{
    MetricsRegistry, PerformanceMetrics, RiskMetrics, SystemMetrics, TradingMetrics,
};
pub use server::{MetricsServer, MetricsServerConfig};
