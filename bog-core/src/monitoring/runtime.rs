//! Runtime monitoring for 24-hour paper trading runs
//!
//! Provides simple logging-based monitoring for development and testing.
//! Complements the Prometheus-based monitoring with periodic status reports.

use std::time::{Duration, Instant};
use tracing::{info, warn, error};

/// Configuration for runtime monitoring
#[derive(Debug, Clone)]
pub struct RuntimeMonitorConfig {
    /// Memory usage warning threshold (0.0 to 1.0)
    pub memory_threshold: f64,
    /// How often to log status (ticks)
    pub log_interval_ticks: usize,
    /// How often to log position reconciliation (ticks)
    pub reconciliation_interval_ticks: usize,
}

impl Default for RuntimeMonitorConfig {
    fn default() -> Self {
        Self {
            memory_threshold: 0.80, // 80% threshold as specified
            log_interval_ticks: 1000, // Log every 1000 ticks
            reconciliation_interval_ticks: 500, // Reconcile every 500 ticks
        }
    }
}

/// Runtime monitor for periodic logging and alerts
pub struct RuntimeMonitor {
    config: RuntimeMonitorConfig,
    start_time: Instant,
    tick_count: usize,
    last_memory_check: Instant,
    peak_memory_bytes: usize,
    initial_memory_bytes: usize,
}

impl RuntimeMonitor {
    /// Create a new runtime monitor
    pub fn new(config: RuntimeMonitorConfig) -> Self {
        let current_memory = Self::get_memory_usage();

        info!(
            "🔍 Runtime Monitor initialized\n\
             - Memory threshold: {:.0}%\n\
             - Status log interval: every {} ticks\n\
             - Position reconciliation: every {} ticks\n\
             - Initial memory: {} bytes ({:.2} MB)",
            config.memory_threshold * 100.0,
            config.log_interval_ticks,
            config.reconciliation_interval_ticks,
            current_memory,
            current_memory as f64 / 1_048_576.0
        );

        Self {
            config,
            start_time: Instant::now(),
            tick_count: 0,
            last_memory_check: Instant::now(),
            peak_memory_bytes: current_memory,
            initial_memory_bytes: current_memory,
        }
    }

    /// Record a tick and perform periodic checks
    pub fn on_tick(&mut self) {
        self.tick_count += 1;

        // Periodic status logging
        if self.tick_count % self.config.log_interval_ticks == 0 {
            self.log_status();
        }

        // Memory check (every 100 ticks or 10 seconds, whichever is more frequent)
        if self.tick_count % 100 == 0 || self.last_memory_check.elapsed() > Duration::from_secs(10) {
            self.check_memory();
            self.last_memory_check = Instant::now();
        }
    }

    /// Log position state for reconciliation
    pub fn log_position_reconciliation(&self, position_qty: i64, realized_pnl: i64, trades: usize) {
        if self.tick_count % self.config.reconciliation_interval_ticks == 0 {
            info!(
                "POSITION RECONCILIATION (tick {})\n\
                 - Quantity: {} (fixed-point, divide by 1e9 for BTC)\n\
                 - Realized PnL: {} (fixed-point, divide by 1e9 for USD)\n\
                 - Total Trades: {}\n\
                 - Uptime: {}",
                self.tick_count,
                position_qty,
                realized_pnl,
                trades,
                format_duration(self.start_time.elapsed())
            );
        }
    }

    /// Log performance metrics
    pub fn log_performance(&self, tick_latency_ns: u64, fills_processed: usize) {
        if self.tick_count % self.config.log_interval_ticks == 0 {
            info!(
                "⚡ PERFORMANCE (tick {})\n\
                 - Tick latency: {}ns ({:.2}μs)\n\
                 - Fills processed this tick: {}\n\
                 - Average ticks/sec: {:.1}",
                self.tick_count,
                tick_latency_ns,
                tick_latency_ns as f64 / 1_000.0,
                fills_processed,
                self.tick_count as f64 / self.start_time.elapsed().as_secs_f64()
            );
        }
    }

    /// Check memory usage and alert if threshold exceeded
    fn check_memory(&mut self) {
        let current_memory = Self::get_memory_usage();

        // Update peak
        if current_memory > self.peak_memory_bytes {
            self.peak_memory_bytes = current_memory;
        }

        // Calculate growth from baseline
        let growth_factor = current_memory as f64 / self.initial_memory_bytes as f64;

        // Alert if memory grew beyond threshold
        if growth_factor > (1.0 + self.config.memory_threshold) {
            error!(
                "MEMORY THRESHOLD EXCEEDED \n\
                 - Current: {} bytes ({:.2} MB)\n\
                 - Initial: {} bytes ({:.2} MB)\n\
                 - Growth: {:.1}% (threshold: {:.0}%)\n\
                 - Peak: {} bytes ({:.2} MB)\n\
                 - Uptime: {}\n\
                  POTENTIAL MEMORY LEAK - Monitor closely!",
                current_memory,
                current_memory as f64 / 1_048_576.0,
                self.initial_memory_bytes,
                self.initial_memory_bytes as f64 / 1_048_576.0,
                (growth_factor - 1.0) * 100.0,
                self.config.memory_threshold * 100.0,
                self.peak_memory_bytes,
                self.peak_memory_bytes as f64 / 1_048_576.0,
                format_duration(self.start_time.elapsed())
            );
        } else if growth_factor > 1.5 {
            // Warning at 50% growth
            warn!(
                " Memory usage growing\n\
                 - Current: {} bytes ({:.2} MB)\n\
                 - Growth: {:.1}% from baseline\n\
                 - Peak: {} bytes ({:.2} MB)",
                current_memory,
                current_memory as f64 / 1_048_576.0,
                (growth_factor - 1.0) * 100.0,
                self.peak_memory_bytes,
                self.peak_memory_bytes as f64 / 1_048_576.0
            );
        }
    }

    /// Log overall status
    fn log_status(&self) {
        let current_memory = Self::get_memory_usage();
        let uptime = self.start_time.elapsed();
        let tps = self.tick_count as f64 / uptime.as_secs_f64();

        info!(
            "STATUS REPORT (tick {})\n\
             - Uptime: {}\n\
             - Ticks processed: {}\n\
             - Average TPS: {:.1}\n\
             - Memory: {} bytes ({:.2} MB)\n\
             - Peak memory: {} bytes ({:.2} MB)\n\
             - Memory growth: {:.1}%",
            self.tick_count,
            format_duration(uptime),
            self.tick_count,
            tps,
            current_memory,
            current_memory as f64 / 1_048_576.0,
            self.peak_memory_bytes,
            self.peak_memory_bytes as f64 / 1_048_576.0,
            ((current_memory as f64 / self.initial_memory_bytes as f64) - 1.0) * 100.0
        );
    }

    /// Get current process memory usage (RSS)
    #[cfg(target_os = "macos")]
    fn get_memory_usage() -> usize {
        use std::mem::MaybeUninit;

        let mut info: MaybeUninit<libc::rusage> = MaybeUninit::uninit();
        unsafe {
            if libc::getrusage(libc::RUSAGE_SELF, info.as_mut_ptr()) == 0 {
                let info = info.assume_init();
                // ru_maxrss is in bytes on macOS
                info.ru_maxrss as usize
            } else {
                0
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn get_memory_usage() -> usize {
        use std::mem::MaybeUninit;

        let mut info: MaybeUninit<libc::rusage> = MaybeUninit::uninit();
        unsafe {
            if libc::getrusage(libc::RUSAGE_SELF, info.as_mut_ptr()) == 0 {
                let info = info.assume_init();
                // ru_maxrss is in kilobytes on Linux
                (info.ru_maxrss as usize) * 1024
            } else {
                0
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn get_memory_usage() -> usize {
        // Fallback for unsupported platforms
        0
    }

    /// Get current tick count
    pub fn tick_count(&self) -> usize {
        self.tick_count
    }

    /// Get uptime
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
}

/// Format duration in human-readable form
fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_monitor_creation() {
        let config = RuntimeMonitorConfig::default();
        let monitor = RuntimeMonitor::new(config);

        assert_eq!(monitor.tick_count(), 0);
        assert!(monitor.uptime().as_secs() < 1);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
        assert_eq!(format_duration(Duration::from_secs(3665)), "1h 1m 5s");
    }

    #[test]
    fn test_memory_usage() {
        let memory = RuntimeMonitor::get_memory_usage();
        // Should return a non-zero value on supported platforms
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        assert!(memory > 0, "Memory usage should be non-zero");
    }
}
