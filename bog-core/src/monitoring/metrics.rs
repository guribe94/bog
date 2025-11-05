//! Prometheus metrics for trading system monitoring
//!
//! Provides comprehensive metrics for:
//! - Trading activity (orders, fills, volume)
//! - Performance (latency, throughput)
//! - Risk (position, PnL, limits)
//! - System health (connections, errors)

use prometheus::{
    Counter, Gauge, Histogram, HistogramOpts, HistogramVec, IntCounter,
    IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry,
};
use std::sync::Arc;
use tracing::info;

/// Central registry for all Prometheus metrics
#[derive(Clone)]
pub struct MetricsRegistry {
    registry: Arc<Registry>,
    trading: Arc<TradingMetrics>,
    performance: Arc<PerformanceMetrics>,
    risk: Arc<RiskMetrics>,
    system: Arc<SystemMetrics>,
}

impl MetricsRegistry {
    /// Create a new metrics registry with all metric families
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Arc::new(Registry::new());

        let trading = Arc::new(TradingMetrics::new(&registry)?);
        let performance = Arc::new(PerformanceMetrics::new(&registry)?);
        let risk = Arc::new(RiskMetrics::new(&registry)?);
        let system = Arc::new(SystemMetrics::new(&registry)?);

        info!("Prometheus metrics registry initialized");

        Ok(Self {
            registry,
            trading,
            performance,
            risk,
            system,
        })
    }

    /// Get the underlying Prometheus registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Get trading metrics
    pub fn trading(&self) -> &TradingMetrics {
        &self.trading
    }

    /// Get performance metrics
    pub fn performance(&self) -> &PerformanceMetrics {
        &self.performance
    }

    /// Get risk metrics
    pub fn risk(&self) -> &RiskMetrics {
        &self.risk
    }

    /// Get system metrics
    pub fn system(&self) -> &SystemMetrics {
        &self.system
    }
}

impl Default for MetricsRegistry {
    #[allow(clippy::panic)] // Critical infrastructure - must succeed or abort
    fn default() -> Self {
        // Metrics creation is critical infrastructure
        // If it fails, the system cannot operate correctly
        Self::new().unwrap_or_else(|e| {
            tracing::error!("FATAL: Failed to create metrics registry: {}", e);
            panic!("Critical: Cannot create metrics registry")
        })
    }
}

/// Trading activity metrics
pub struct TradingMetrics {
    /// Total orders submitted
    pub orders_total: IntCounterVec,
    /// Total fills received
    pub fills_total: IntCounterVec,
    /// Total trading volume in USD
    pub volume_total: Counter,
    /// Total order rejections
    pub rejections_total: IntCounterVec,
    /// Total order cancellations
    pub cancellations_total: IntCounterVec,
    /// Current order fill rate (0.0 to 1.0)
    pub fill_rate: Gauge,
    /// Orders by status
    pub orders_by_status: IntGaugeVec,
}

impl TradingMetrics {
    fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let orders_total = IntCounterVec::new(
            Opts::new("trading_orders_total", "Total number of orders submitted")
                .namespace("bog"),
            &["market", "side", "type"],
        )?;
        registry.register(Box::new(orders_total.clone()))?;

        let fills_total = IntCounterVec::new(
            Opts::new("trading_fills_total", "Total number of fills received")
                .namespace("bog"),
            &["market", "side"],
        )?;
        registry.register(Box::new(fills_total.clone()))?;

        let volume_total = Counter::new(
            "bog_trading_volume_usd_total",
            "Total trading volume in USD",
        )?;
        registry.register(Box::new(volume_total.clone()))?;

        let rejections_total = IntCounterVec::new(
            Opts::new(
                "trading_rejections_total",
                "Total number of order rejections",
            )
            .namespace("bog"),
            &["reason"],
        )?;
        registry.register(Box::new(rejections_total.clone()))?;

        let cancellations_total = IntCounterVec::new(
            Opts::new(
                "trading_cancellations_total",
                "Total number of order cancellations",
            )
            .namespace("bog"),
            &["market"],
        )?;
        registry.register(Box::new(cancellations_total.clone()))?;

        let fill_rate = Gauge::new("bog_trading_fill_rate", "Current order fill rate (0.0 to 1.0)")?;
        registry.register(Box::new(fill_rate.clone()))?;

        let orders_by_status = IntGaugeVec::new(
            Opts::new("trading_orders_by_status", "Number of orders by status")
                .namespace("bog"),
            &["status"],
        )?;
        registry.register(Box::new(orders_by_status.clone()))?;

        Ok(Self {
            orders_total,
            fills_total,
            volume_total,
            rejections_total,
            cancellations_total,
            fill_rate,
            orders_by_status,
        })
    }
}

/// Performance metrics
pub struct PerformanceMetrics {
    /// Tick-to-trade latency distribution (nanoseconds)
    pub tick_to_trade_latency_ns: Histogram,
    /// Strategy calculation latency (nanoseconds)
    pub strategy_latency_ns: HistogramVec,
    /// Risk validation latency (nanoseconds)
    pub risk_validation_latency_ns: Histogram,
    /// Order execution latency (microseconds)
    pub execution_latency_us: Histogram,
    /// Ticks processed per second
    pub ticks_per_second: Gauge,
    /// Orders per second
    pub orders_per_second: Gauge,
}

impl PerformanceMetrics {
    fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let tick_to_trade_latency_ns = Histogram::with_opts(
            HistogramOpts::new(
                "bog_performance_tick_to_trade_latency_ns",
                "Tick-to-trade latency in nanoseconds",
            )
            .buckets(vec![
                10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0,
            ]),
        )?;
        registry.register(Box::new(tick_to_trade_latency_ns.clone()))?;

        let strategy_latency_ns = HistogramVec::new(
            HistogramOpts::new(
                "performance_strategy_latency_ns",
                "Strategy calculation latency in nanoseconds",
            )
            .namespace("bog")
            .buckets(vec![1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0]),
            &["strategy"],
        )?;
        registry.register(Box::new(strategy_latency_ns.clone()))?;

        let risk_validation_latency_ns = Histogram::with_opts(
            HistogramOpts::new(
                "bog_performance_risk_validation_latency_ns",
                "Risk validation latency in nanoseconds",
            )
            .buckets(vec![1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0]),
        )?;
        registry.register(Box::new(risk_validation_latency_ns.clone()))?;

        let execution_latency_us = Histogram::with_opts(
            HistogramOpts::new(
                "bog_performance_execution_latency_us",
                "Order execution latency in microseconds",
            )
            .buckets(vec![10.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0]),
        )?;
        registry.register(Box::new(execution_latency_us.clone()))?;

        let ticks_per_second =
            Gauge::new("bog_performance_ticks_per_second", "Market ticks processed per second")?;
        registry.register(Box::new(ticks_per_second.clone()))?;

        let orders_per_second =
            Gauge::new("bog_performance_orders_per_second", "Orders submitted per second")?;
        registry.register(Box::new(orders_per_second.clone()))?;

        Ok(Self {
            tick_to_trade_latency_ns,
            strategy_latency_ns,
            risk_validation_latency_ns,
            execution_latency_us,
            ticks_per_second,
            orders_per_second,
        })
    }
}

/// Risk management metrics
pub struct RiskMetrics {
    /// Current position (in BTC)
    pub position_btc: Gauge,
    /// Current position utilization (0.0 to 1.0)
    pub position_utilization: Gauge,
    /// Realized PnL (USD)
    pub realized_pnl_usd: Gauge,
    /// Unrealized PnL (USD)
    pub unrealized_pnl_usd: Gauge,
    /// Daily PnL (USD)
    pub daily_pnl_usd: Gauge,
    /// Total risk violations
    pub risk_violations_total: IntCounterVec,
    /// Position limit (BTC)
    pub position_limit_btc: Gauge,
    /// Daily loss limit (USD)
    pub daily_loss_limit_usd: Gauge,
}

impl RiskMetrics {
    fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let position_btc = Gauge::new("bog_risk_position_btc", "Current position in BTC")?;
        registry.register(Box::new(position_btc.clone()))?;

        let position_utilization = Gauge::new(
            "bog_risk_position_utilization",
            "Position utilization ratio (0.0 to 1.0)",
        )?;
        registry.register(Box::new(position_utilization.clone()))?;

        let realized_pnl_usd =
            Gauge::new("bog_risk_realized_pnl_usd", "Realized profit and loss in USD")?;
        registry.register(Box::new(realized_pnl_usd.clone()))?;

        let unrealized_pnl_usd = Gauge::new(
            "bog_risk_unrealized_pnl_usd",
            "Unrealized profit and loss in USD",
        )?;
        registry.register(Box::new(unrealized_pnl_usd.clone()))?;

        let daily_pnl_usd = Gauge::new("bog_risk_daily_pnl_usd", "Daily profit and loss in USD")?;
        registry.register(Box::new(daily_pnl_usd.clone()))?;

        let risk_violations_total = IntCounterVec::new(
            Opts::new("risk_violations_total", "Total number of risk violations")
                .namespace("bog"),
            &["type"],
        )?;
        registry.register(Box::new(risk_violations_total.clone()))?;

        let position_limit_btc =
            Gauge::new("bog_risk_position_limit_btc", "Position limit in BTC")?;
        registry.register(Box::new(position_limit_btc.clone()))?;

        let daily_loss_limit_usd = Gauge::new(
            "bog_risk_daily_loss_limit_usd",
            "Daily loss limit in USD",
        )?;
        registry.register(Box::new(daily_loss_limit_usd.clone()))?;

        Ok(Self {
            position_btc,
            position_utilization,
            realized_pnl_usd,
            unrealized_pnl_usd,
            daily_pnl_usd,
            risk_violations_total,
            position_limit_btc,
            daily_loss_limit_usd,
        })
    }
}

/// System health metrics
pub struct SystemMetrics {
    /// Huginn connection status (1 = connected, 0 = disconnected)
    pub huginn_connected: IntGauge,
    /// Total Huginn messages received
    pub huginn_messages_total: IntCounter,
    /// Huginn sequence gaps detected
    pub huginn_sequence_gaps_total: IntCounter,
    /// Huginn queue depth
    pub huginn_queue_depth: IntGauge,
    /// Exchange connection status (1 = connected, 0 = disconnected)
    pub exchange_connected: IntGauge,
    /// Total system errors
    pub errors_total: IntCounterVec,
    /// CPU usage percentage
    pub cpu_usage_percent: Gauge,
    /// Memory usage in bytes
    pub memory_usage_bytes: IntGauge,
    /// Uptime in seconds
    pub uptime_seconds: IntGauge,
}

impl SystemMetrics {
    fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let huginn_connected = IntGauge::new(
            "bog_system_huginn_connected",
            "Huginn connection status (1 = connected, 0 = disconnected)",
        )?;
        registry.register(Box::new(huginn_connected.clone()))?;

        let huginn_messages_total = IntCounter::new(
            "bog_system_huginn_messages_total",
            "Total Huginn messages received",
        )?;
        registry.register(Box::new(huginn_messages_total.clone()))?;

        let huginn_sequence_gaps_total = IntCounter::new(
            "bog_system_huginn_sequence_gaps_total",
            "Total Huginn sequence gaps detected",
        )?;
        registry.register(Box::new(huginn_sequence_gaps_total.clone()))?;

        let huginn_queue_depth =
            IntGauge::new("bog_system_huginn_queue_depth", "Huginn queue depth")?;
        registry.register(Box::new(huginn_queue_depth.clone()))?;

        let exchange_connected = IntGauge::new(
            "bog_system_exchange_connected",
            "Exchange connection status (1 = connected, 0 = disconnected)",
        )?;
        registry.register(Box::new(exchange_connected.clone()))?;

        let errors_total = IntCounterVec::new(
            Opts::new("system_errors_total", "Total system errors")
                .namespace("bog"),
            &["component", "severity"],
        )?;
        registry.register(Box::new(errors_total.clone()))?;

        let cpu_usage_percent =
            Gauge::new("bog_system_cpu_usage_percent", "CPU usage percentage")?;
        registry.register(Box::new(cpu_usage_percent.clone()))?;

        let memory_usage_bytes = IntGauge::new(
            "bog_system_memory_usage_bytes",
            "Memory usage in bytes",
        )?;
        registry.register(Box::new(memory_usage_bytes.clone()))?;

        let uptime_seconds = IntGauge::new("bog_system_uptime_seconds", "System uptime in seconds")?;
        registry.register(Box::new(uptime_seconds.clone()))?;

        Ok(Self {
            huginn_connected,
            huginn_messages_total,
            huginn_sequence_gaps_total,
            huginn_queue_depth,
            exchange_connected,
            errors_total,
            cpu_usage_percent,
            memory_usage_bytes,
            uptime_seconds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registry_creation() {
        let registry = MetricsRegistry::new().unwrap();
        assert!(registry.registry().gather().len() > 0);
    }

    #[test]
    fn test_trading_metrics() {
        let registry = MetricsRegistry::new().unwrap();

        // Test trading metrics
        registry.trading().orders_total.with_label_values(&["BTC-USD", "buy", "limit"]).inc();
        registry.trading().fills_total.with_label_values(&["BTC-USD", "buy"]).inc();
        registry.trading().volume_total.inc_by(50000.0);

        let metrics = registry.registry().gather();
        assert!(metrics.len() > 0);
    }

    #[test]
    fn test_performance_metrics() {
        let registry = MetricsRegistry::new().unwrap();

        // Test performance metrics
        registry.performance().tick_to_trade_latency_ns.observe(27.5);
        registry.performance().strategy_latency_ns.with_label_values(&["simple_spread"]).observe(5.0);
        registry.performance().ticks_per_second.set(1000.0);

        let metrics = registry.registry().gather();
        assert!(metrics.len() > 0);
    }

    #[test]
    fn test_risk_metrics() {
        let registry = MetricsRegistry::new().unwrap();

        // Test risk metrics
        registry.risk().position_btc.set(0.5);
        registry.risk().realized_pnl_usd.set(100.0);
        registry.risk().position_utilization.set(0.25);

        let metrics = registry.registry().gather();
        assert!(metrics.len() > 0);
    }

    #[test]
    fn test_system_metrics() {
        let registry = MetricsRegistry::new().unwrap();

        // Test system metrics
        registry.system().huginn_connected.set(1);
        registry.system().huginn_messages_total.inc();
        registry.system().cpu_usage_percent.set(25.5);

        let metrics = registry.registry().gather();
        assert!(metrics.len() > 0);
    }
}
