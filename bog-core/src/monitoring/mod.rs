//! Monitoring and observability module
//!
//! Provides Prometheus metrics export, HTTP server for scraping,
//! alerting system, and integration with the execution layer for
//! comprehensive monitoring.

pub mod metrics;
pub mod server;
pub mod alerts;
pub mod alert_rules;

pub use metrics::{MetricsRegistry, TradingMetrics, PerformanceMetrics, RiskMetrics, SystemMetrics};
pub use server::{MetricsServer, MetricsServerConfig};
pub use alerts::{Alert, AlertCategory, AlertId, AlertManager, AlertManagerConfig, AlertOutput, AlertSeverity};
pub use alert_rules::{AlertRule, RuleContext, RuleEngine};
