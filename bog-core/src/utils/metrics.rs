/// Metrics collector stub
/// TODO Phase 9: Integrate with Prometheus exporter
pub struct MetricsCollector {
    enabled: bool,
}

impl MetricsCollector {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    // Future: Add metrics recording methods
    // pub fn record_latency(&self, latency_us: u64) { ... }
    // pub fn record_fill(&self, side: Side, size: Decimal, price: Decimal) { ... }
    // pub fn record_signal(&self, strategy: &str) { ... }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new(false)
    }
}
