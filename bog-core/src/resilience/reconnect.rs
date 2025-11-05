//! Resilient market feed wrapper with automatic reconnection
//!
//! Wraps huginn::MarketFeed with automatic reconnection logic,
//! exponential backoff, and connection health monitoring.

use super::backoff::{BackoffConfig, ExponentialBackoff};
use anyhow::{Context, Result};
use huginn::{ConsumerStats, MarketFeed, MarketSnapshot};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Connection state for resilient feed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connected and operational
    Connected,
    /// Disconnected, attempting to reconnect
    Reconnecting,
    /// Failed to connect after max retries
    Failed,
}

/// Configuration for resilient market feed
#[derive(Debug, Clone)]
pub struct ResilientConfig {
    /// Market ID to connect to
    pub market_id: u64,
    /// Optional DEX type
    pub dex_type: Option<u8>,
    /// Backoff configuration for reconnection attempts
    pub backoff_config: BackoffConfig,
    /// Connection timeout for each attempt
    pub connection_timeout: Duration,
    /// Health check interval (detect stale connections)
    pub health_check_interval: Duration,
    /// Consider connection stale after this many empty polls
    pub stale_threshold: usize,
}

impl Default for ResilientConfig {
    fn default() -> Self {
        Self {
            market_id: 1,
            dex_type: Some(1), // Lighter
            backoff_config: BackoffConfig::default(),
            connection_timeout: Duration::from_secs(5),
            health_check_interval: Duration::from_secs(30),
            stale_threshold: 10000, // 10k empty polls
        }
    }
}

/// Statistics for resilient feed
#[derive(Debug, Clone, Default)]
pub struct ReconnectionStats {
    /// Total number of reconnection attempts
    pub reconnection_attempts: u64,
    /// Successful reconnections
    pub successful_reconnections: u64,
    /// Failed reconnections
    pub failed_reconnections: u64,
    /// Current connection uptime
    pub connection_uptime: Duration,
    /// Last reconnection time
    pub last_reconnection: Option<Instant>,
}

/// Resilient wrapper around MarketFeed with automatic reconnection
pub struct ResilientMarketFeed {
    config: ResilientConfig,
    feed: Option<MarketFeed>,
    state: ConnectionState,
    backoff: ExponentialBackoff,
    stats: ReconnectionStats,
    last_health_check: Instant,
    empty_polls_since_health_check: usize,
    connection_established_at: Option<Instant>,
}

impl ResilientMarketFeed {
    /// Create a new resilient feed and attempt initial connection
    pub fn new(config: ResilientConfig) -> Result<Self> {
        info!(
            "Creating resilient market feed for market {} (dex_type: {:?})",
            config.market_id, config.dex_type
        );

        let backoff = ExponentialBackoff::with_config(config.backoff_config.clone());

        let mut resilient = Self {
            config,
            feed: None,
            state: ConnectionState::Reconnecting,
            backoff,
            stats: ReconnectionStats::default(),
            last_health_check: Instant::now(),
            empty_polls_since_health_check: 0,
            connection_established_at: None,
        };

        // Attempt initial connection
        resilient.connect()?;

        Ok(resilient)
    }

    /// Attempt to connect (or reconnect) to Huginn
    fn connect(&mut self) -> Result<()> {
        self.stats.reconnection_attempts += 1;

        let result = if let Some(dex_type) = self.config.dex_type {
            MarketFeed::connect_with_dex(dex_type, self.config.market_id)
        } else {
            MarketFeed::connect(self.config.market_id)
        };

        match result {
            Ok(feed) => {
                info!(
                    "Successfully connected to market {} (attempt #{})",
                    self.config.market_id, self.stats.reconnection_attempts
                );

                self.feed = Some(feed);
                self.state = ConnectionState::Connected;
                self.stats.successful_reconnections += 1;
                self.stats.last_reconnection = Some(Instant::now());
                self.connection_established_at = Some(Instant::now());
                self.backoff.reset();

                Ok(())
            }
            Err(e) => {
                error!(
                    "Failed to connect to market {}: {}",
                    self.config.market_id, e
                );

                self.state = ConnectionState::Reconnecting;
                self.stats.failed_reconnections += 1;

                Err(e).context("Connection attempt failed")
            }
        }
    }

    /// Try to reconnect with exponential backoff
    fn try_reconnect(&mut self) -> Result<()> {
        if !self.backoff.can_retry() {
            error!(
                "Max reconnection attempts reached for market {}",
                self.config.market_id
            );
            self.state = ConnectionState::Failed;
            return Err(anyhow::anyhow!("Max reconnection attempts exceeded"));
        }

        if let Some(delay) = self.backoff.next_delay() {
            debug!(
                "Waiting {:?} before reconnection attempt #{}",
                delay,
                self.backoff.attempt_number()
            );
            std::thread::sleep(delay);
        }

        self.connect()
    }

    /// Try to receive a snapshot, with automatic reconnection on failure
    pub fn try_recv(&mut self) -> Option<MarketSnapshot> {
        // If failed state, don't even try
        if self.state == ConnectionState::Failed {
            return None;
        }

        // If disconnected, try to reconnect
        if self.state == ConnectionState::Reconnecting {
            if let Err(e) = self.try_reconnect() {
                warn!("Reconnection failed: {}", e);
                return None;
            }
        }

        // Try to receive from feed
        if let Some(feed) = &mut self.feed {
            match feed.try_recv() {
                Some(snapshot) => {
                    self.empty_polls_since_health_check = 0;
                    return Some(snapshot);
                }
                None => {
                    self.empty_polls_since_health_check += 1;

                    // Periodic health check
                    self.perform_health_check_if_due();

                    return None;
                }
            }
        }

        None
    }

    /// Perform periodic health check to detect stale connections
    fn perform_health_check_if_due(&mut self) {
        if self.last_health_check.elapsed() < self.config.health_check_interval {
            return;
        }

        self.last_health_check = Instant::now();

        // Check if connection appears stale
        if self.empty_polls_since_health_check >= self.config.stale_threshold {
            warn!(
                "Connection may be stale ({} empty polls), triggering reconnection",
                self.empty_polls_since_health_check
            );

            self.state = ConnectionState::Reconnecting;
            self.feed = None;
        }

        self.empty_polls_since_health_check = 0;
    }

    /// Get connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Check if currently connected
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    /// Get reconnection statistics
    pub fn reconnection_stats(&self) -> &ReconnectionStats {
        &self.stats
    }

    /// Get consumer statistics from underlying feed
    pub fn stats(&self) -> Option<&ConsumerStats> {
        self.feed.as_ref().map(|f| f.stats())
    }

    /// Get market ID
    pub fn market_id(&self) -> u64 {
        self.config.market_id
    }

    /// Get queue depth (0 if disconnected)
    pub fn queue_depth(&self) -> usize {
        self.feed.as_ref().map_or(0, |f| f.queue_depth())
    }

    /// Check if caught up (always false if disconnected)
    pub fn is_caught_up(&self) -> bool {
        self.feed.as_ref().map_or(false, |f| f.is_caught_up())
    }

    /// Get connection uptime
    pub fn connection_uptime(&self) -> Duration {
        self.connection_established_at
            .map(|t| t.elapsed())
            .unwrap_or(Duration::from_secs(0))
    }

    /// Force reconnection (useful for testing or manual recovery)
    pub fn force_reconnect(&mut self) -> Result<()> {
        info!("Forcing reconnection for market {}", self.config.market_id);
        self.feed = None;
        self.state = ConnectionState::Reconnecting;
        self.backoff.reset();
        self.try_reconnect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resilient_config_default() {
        let config = ResilientConfig::default();
        assert_eq!(config.market_id, 1);
        assert_eq!(config.dex_type, Some(1));
        assert!(config.stale_threshold > 0);
    }

    #[test]
    fn test_reconnection_stats_default() {
        let stats = ReconnectionStats::default();
        assert_eq!(stats.reconnection_attempts, 0);
        assert_eq!(stats.successful_reconnections, 0);
        assert_eq!(stats.failed_reconnections, 0);
    }

    // Note: Full integration tests with real Huginn feed would go in tests/
    // These are just unit tests for the structure
}
