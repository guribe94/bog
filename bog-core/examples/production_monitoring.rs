//! Production Monitoring Example
//!
//! Demonstrates how to set up comprehensive monitoring for a production trading bot:
//! - Prometheus metrics export
//! - Alert rules for risk and system health
//! - ProductionExecutor with integrated observability
//!
//! This example shows the complete setup pattern for a production deployment.

use bog_core::monitoring::{
    Alert, AlertCategory, AlertManager, AlertManagerConfig, AlertOutput, AlertSeverity,
    MetricsRegistry, MetricsServer, MetricsServerConfig, RuleContext, RuleEngine,
};
use bog_core::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

fn main() -> Result<()> {
    // Initialize tracing for console output
    tracing_subscriber::fmt::init();

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║   Production Monitoring Setup Example                    ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    // ===================================================================
    // STEP 1: Set up Prometheus metrics registry
    // ===================================================================
    println!("📊 Setting up Prometheus metrics...");

    let metrics = Arc::new(MetricsRegistry::new()?);
    println!("   ✓ Created metrics registry with 30+ metrics");

    // Simulate some initial metric values
    metrics.trading().orders_total.with_label_values(&["BTC-USD", "buy", "limit"]).inc();
    metrics.trading().volume_total.inc_by(50000.0);
    metrics.performance().ticks_per_second.set(1000.0);
    metrics.risk().position_btc.set(0.5);
    metrics.system().huginn_connected.set(1);

    println!("   ✓ Initialized sample metric values\n");

    // ===================================================================
    // STEP 2: Set up AlertManager with multiple outputs
    // ===================================================================
    println!("🚨 Setting up AlertManager...");

    let alert_config = AlertManagerConfig {
        outputs: vec![
            // Console output for Critical and Error alerts
            AlertOutput::Console {
                min_severity: AlertSeverity::Error,
            },
            // File output for all alerts (JSONL format)
            AlertOutput::File {
                path: PathBuf::from("/tmp/bog_alerts.log"),
                min_severity: AlertSeverity::Info,
            },
            // Webhook for Critical alerts (placeholder)
            AlertOutput::Webhook {
                url: "https://hooks.example.com/alerts".to_string(),
                min_severity: AlertSeverity::Critical,
                timeout_ms: 5000,
            },
        ],
        rate_limit_secs: 60,      // 1 minute between same alerts
        auto_resolve_secs: 300,   // 5 minutes auto-resolve
        enable_aggregation: false,
    };

    let alert_manager = Arc::new(AlertManager::new(alert_config));
    println!("   ✓ Created AlertManager with 3 outputs:");
    println!("     - Console (Error+)");
    println!("     - File (/tmp/bog_alerts.log, all levels)");
    println!("     - Webhook (Critical only)\n");

    // ===================================================================
    // STEP 3: Set up RuleEngine with production rules
    // ===================================================================
    println!("📋 Setting up alert rules...");

    let rule_engine = RuleEngine::new(alert_manager.clone()).with_default_rules();
    println!("   ✓ Added {} production alert rules:", rule_engine.rule_count());
    println!("     - Position limit: 1.0 BTC (Critical)");
    println!("     - Daily loss limit: $1,000 (Critical)");
    println!("     - Huginn connection: 5s grace (Critical)");
    println!("     - Rejection rate: >10% (Warning)");
    println!("     - Tick-to-trade latency: >1ms (Warning)\n");

    // ===================================================================
    // STEP 4: Demonstrate alert triggering
    // ===================================================================
    println!("🔔 Testing alert system...\n");

    // Create a test position that exceeds limits
    let position = Arc::new(Position::new());
    position.quantity.store(
        1_500_000_000, // 1.5 BTC (exceeds 1.0 BTC limit)
        std::sync::atomic::Ordering::Relaxed,
    );

    // Create rule context
    let context = RuleContext {
        position: Some(position.clone()),
        metrics: metrics.clone(),
        timestamp: SystemTime::now(),
    };

    // Evaluate all rules
    println!("Evaluating alert rules...");
    rule_engine.evaluate_all(&context)?;
    println!("\nAlert triggered! Check console and /tmp/bog_alerts.log\n");

    // ===================================================================
    // STEP 5: Manual alert example
    // ===================================================================
    println!("Sending manual alert...");

    let manual_alert = Alert::new(
        AlertCategory::Trading,
        "high_slippage",
        AlertSeverity::Warning,
        "Unusual slippage detected on BTC-USD",
    )
    .with_detail("market", "BTC-USD")
    .with_detail("slippage_bps", "25")
    .with_detail("threshold_bps", "10");

    alert_manager.send(manual_alert)?;
    println!("   ✓ Manual alert sent\n");

    // ===================================================================
    // STEP 6: Metrics server setup (would run in separate tokio task)
    // ===================================================================
    println!("🌐 Metrics server configuration:");

    let server_config = MetricsServerConfig {
        listen_addr: "127.0.0.1:9090".parse().unwrap(),
        metrics_path: "/metrics".to_string(),
    };

    let _metrics_server = MetricsServer::new(server_config, metrics.clone());
    println!("   ✓ Configured HTTP server for Prometheus scraping");
    println!("     URL: http://127.0.0.1:9090/metrics");
    println!("     (Use tokio::spawn to run in production)\n");

    // ===================================================================
    // STEP 7: ProductionExecutor integration
    // ===================================================================
    println!("⚙️  ProductionExecutor integration:");

    use bog_core::execution::{ProductionExecutor, ProductionExecutorConfig};

    let exec_config = ProductionExecutorConfig {
        enable_journal: true,
        journal_path: PathBuf::from("/tmp/bog_execution.jsonl"),
        recover_on_startup: true,
        validate_recovery: true,
        fill_delay_ms: 100,
        fill_probability: 0.95,
        instant_fills: false,
    };

    let mut executor = ProductionExecutor::new(exec_config);
    executor.set_prometheus_metrics(metrics.clone());

    println!("   ✓ Created ProductionExecutor with:");
    println!("     - Execution journal enabled");
    println!("     - Prometheus metrics integration");
    println!("     - Fill simulation (100ms delay)\n");

    // ===================================================================
    // Summary
    // ===================================================================
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║   Production Monitoring Setup Complete                   ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    println!("Production Monitoring Stack:");
    println!("  1. ✅ Prometheus metrics (30+ metrics across 4 families)");
    println!("  2. ✅ HTTP metrics server (for Prometheus scraping)");
    println!("  3. ✅ AlertManager (Console, File, Webhook outputs)");
    println!("  4. ✅ RuleEngine (5 production alert rules)");
    println!("  5. ✅ ProductionExecutor (with metrics integration)");
    println!();
    println!("Alert Status:");
    println!("  • Active alerts: {}", alert_manager.active_count());
    println!("  • Alert log: /tmp/bog_alerts.log");
    println!();
    println!("Next Steps:");
    println!("  1. Deploy Prometheus to scrape http://127.0.0.1:9090/metrics");
    println!("  2. Configure Grafana dashboards for visualization");
    println!("  3. Set up webhook endpoint (PagerDuty, Slack, etc.)");
    println!("  4. Run periodic rule evaluation (every 1-5 seconds)");
    println!("  5. Monitor alert log for critical events");
    println!();

    Ok(())
}
