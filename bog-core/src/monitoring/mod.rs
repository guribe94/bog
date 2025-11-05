//! Monitoring and observability module
//!
//! Provides Prometheus metrics export, HTTP server for scraping,
//! and integration with the execution layer for comprehensive monitoring.

pub mod metrics;
pub mod server;

pub use metrics::{MetricsRegistry, TradingMetrics};
pub use server::{MetricsServer, MetricsServerConfig};
