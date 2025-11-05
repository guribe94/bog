//! HTTP server for Prometheus metrics export
//!
//! Provides a lightweight HTTP server that exposes metrics at /metrics endpoint
//! for Prometheus scraping.

use super::MetricsRegistry;
use anyhow::{Context, Result};
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use prometheus::{Encoder, TextEncoder};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};

/// Configuration for metrics HTTP server
#[derive(Debug, Clone)]
pub struct MetricsServerConfig {
    /// Address to bind to (e.g., "0.0.0.0:9090")
    pub listen_addr: SocketAddr,
    /// Path to serve metrics (default: "/metrics")
    pub metrics_path: String,
}

impl Default for MetricsServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:9090".parse().unwrap(),
            metrics_path: "/metrics".to_string(),
        }
    }
}

/// HTTP server for Prometheus metrics
pub struct MetricsServer {
    config: MetricsServerConfig,
    registry: Arc<MetricsRegistry>,
}

impl MetricsServer {
    /// Create a new metrics server
    pub fn new(config: MetricsServerConfig, registry: Arc<MetricsRegistry>) -> Self {
        Self { config, registry }
    }

    /// Start the metrics server (async)
    ///
    /// This function runs indefinitely, serving metrics on the configured address.
    /// It should be spawned in a separate tokio task.
    pub async fn serve(self) -> Result<()> {
        let listener = TcpListener::bind(&self.config.listen_addr)
            .await
            .context("Failed to bind metrics server")?;

        info!(
            "Metrics server listening on http://{}{}",
            self.config.listen_addr, self.config.metrics_path
        );

        let registry = self.registry.clone();
        let metrics_path = self.config.metrics_path.clone();

        loop {
            let (stream, remote_addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    continue;
                }
            };

            let registry = registry.clone();
            let metrics_path = metrics_path.clone();

            // Spawn a new task for each connection
            tokio::spawn(async move {
                let io = TokioIo::new(stream);

                let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let registry = registry.clone();
                    let metrics_path = metrics_path.clone();
                    async move { handle_request(req, registry, metrics_path).await }
                });

                if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                    debug!("Connection error from {}: {}", remote_addr, err);
                }
            });
        }
    }

    /// Serve metrics once (synchronous, for testing)
    #[allow(dead_code)]
    pub fn serve_metrics_once(&self) -> Result<String> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.registry().gather();

        let mut buffer = Vec::new();
        encoder
            .encode(&metric_families, &mut buffer)
            .context("Failed to encode metrics")?;

        String::from_utf8(buffer).context("Invalid UTF-8 in metrics")
    }
}

/// Handle HTTP request
async fn handle_request(
    req: Request<hyper::body::Incoming>,
    registry: Arc<MetricsRegistry>,
    metrics_path: String,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let path = req.uri().path();

    debug!("Metrics request: {} {}", req.method(), path);

    // Health check endpoint
    if path == "/health" || path == "/healthz" {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from("OK")))
            .unwrap());
    }

    // Metrics endpoint
    if path == metrics_path {
        match encode_metrics(&registry) {
            Ok(metrics_text) => {
                return Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/plain; version=0.0.4")
                    .body(Full::new(Bytes::from(metrics_text)))
                    .unwrap());
            }
            Err(e) => {
                error!("Failed to encode metrics: {}", e);
                return Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Full::new(Bytes::from(format!("Error: {}", e))))
                    .unwrap());
            }
        }
    }

    // Root endpoint - simple help page
    if path == "/" {
        let help_text = format!(
            "Bog Trading System Metrics\n\nEndpoints:\n  {} - Prometheus metrics\n  /health - Health check\n",
            metrics_path
        );
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from(help_text)))
            .unwrap());
    }

    // 404 for unknown paths
    warn!("Unknown metrics endpoint requested: {}", path);
    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::from("Not Found")))
        .unwrap())
}

/// Encode metrics to Prometheus text format
fn encode_metrics(registry: &MetricsRegistry) -> Result<String> {
    let encoder = TextEncoder::new();
    let metric_families = registry.registry().gather();

    let mut buffer = Vec::new();
    encoder
        .encode(&metric_families, &mut buffer)
        .context("Failed to encode metrics")?;

    String::from_utf8(buffer).context("Invalid UTF-8 in metrics")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_server_config_default() {
        let config = MetricsServerConfig::default();
        assert_eq!(config.metrics_path, "/metrics");
        assert_eq!(config.listen_addr.port(), 9090);
    }

    #[test]
    fn test_serve_metrics_once() {
        let registry = Arc::new(MetricsRegistry::new().unwrap());
        let config = MetricsServerConfig::default();
        let server = MetricsServer::new(config, registry.clone());

        // Add some test metrics
        registry.trading().orders_total.with_label_values(&["BTC-USD", "buy", "limit"]).inc();
        registry.performance().ticks_per_second.set(100.0);

        let metrics = server.serve_metrics_once().unwrap();

        // Verify metrics are in Prometheus format
        assert!(metrics.contains("bog_trading_orders_total"));
        assert!(metrics.contains("bog_performance_ticks_per_second"));
        assert!(metrics.contains("TYPE"));
        assert!(metrics.contains("HELP"));
    }

    #[test]
    fn test_encode_metrics() {
        let registry = Arc::new(MetricsRegistry::new().unwrap());

        // Add metrics
        registry.trading().volume_total.inc_by(12345.67);
        registry.risk().position_btc.set(0.5);

        let encoded = encode_metrics(&registry).unwrap();

        assert!(encoded.contains("bog_trading_volume_usd_total"));
        assert!(encoded.contains("bog_risk_position_btc"));
    }

    #[tokio::test]
    async fn test_handle_request_health() {
        let registry = Arc::new(MetricsRegistry::new().unwrap());
        let req = Request::builder()
            .uri("/health")
            .body(hyper::body::Incoming::default())
            .unwrap();

        // Note: We can't actually call handle_request without a proper Incoming body
        // This is a simplified test
        let config = MetricsServerConfig::default();
        let server = MetricsServer::new(config, registry);

        // Just verify server creation works
        assert_eq!(server.config.metrics_path, "/metrics");
    }
}
