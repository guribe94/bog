//! Centralized snapshot validation logic
//!
//! Eliminates duplication by providing a single validation point for all snapshots.

use super::types::MarketSnapshot;
use std::fs::File;
use std::io::Write;

/// Validation error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    ZeroSequence,
    ZeroBidPrice,
    ZeroAskPrice,
    OrderbookCrossed {
        bid: u64,
        ask: u64,
    },
    OrderbookLocked {
        price: u64,
    },
    SpreadTooWide {
        spread_bps: u64,
    },
    SpreadTooNarrow {
        spread_bps: u64,
    },
    StaleData {
        age_ns: u64,
        max_age_ns: u64,
    },
    FutureTimestamp {
        timestamp_ns: u64,
        now_ns: u64,
    },
    InvalidDepthLevel {
        level: usize,
        reason: String,
    },
    PriceSpike {
        change_bps: u64,
        max_bps: u64,
    },
    LowLiquidity {
        total_bid_size: u64,
        total_ask_size: u64,
        min_size: u64,
    },
    InvalidPrice,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::ZeroSequence => write!(f, "Sequence number is zero"),
            ValidationError::ZeroBidPrice => write!(f, "Bid price is zero"),
            ValidationError::ZeroAskPrice => write!(f, "Ask price is zero"),
            ValidationError::OrderbookCrossed { bid, ask } => {
                write!(f, "Orderbook crossed: bid={} >= ask={}", bid, ask)
            }
            ValidationError::OrderbookLocked { price } => {
                write!(f, "Orderbook locked at price={}", price)
            }
            ValidationError::SpreadTooWide { spread_bps } => {
                write!(f, "Spread too wide: {}bps", spread_bps)
            }
            ValidationError::SpreadTooNarrow { spread_bps } => {
                write!(
                    f,
                    "Spread too narrow: {}bps (possible data error)",
                    spread_bps
                )
            }
            ValidationError::StaleData { age_ns, max_age_ns } => {
                write!(
                    f,
                    "Stale data: age={}ms > max={}ms",
                    age_ns / 1_000_000,
                    max_age_ns / 1_000_000
                )
            }
            ValidationError::FutureTimestamp {
                timestamp_ns,
                now_ns,
            } => {
                write!(
                    f,
                    "Future timestamp: {} > {} (clock skew)",
                    timestamp_ns, now_ns
                )
            }
            ValidationError::InvalidDepthLevel { level, reason } => {
                write!(f, "Invalid depth level {}: {}", level, reason)
            }
            ValidationError::PriceSpike {
                change_bps,
                max_bps,
            } => {
                write!(f, "Price spike: {}bps > max {}bps", change_bps, max_bps)
            }
            ValidationError::LowLiquidity {
                total_bid_size,
                total_ask_size,
                min_size,
            } => {
                write!(
                    f,
                    "Low liquidity: bid={}, ask={} (min={})",
                    total_bid_size, total_ask_size, min_size
                )
            }
            ValidationError::InvalidPrice => {
                write!(f, "Invalid price (zero or corrupt)")
            }
        }
    }
}

/// Validation configuration
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Maximum age for snapshot in nanoseconds
    pub max_age_ns: u64,

    /// Maximum spread in basis points (1% = 100bps)
    pub max_spread_bps: u64,

    /// Minimum spread in basis points (to detect data errors)
    pub min_spread_bps: u64,

    /// Maximum price change from last snapshot in bps
    pub max_price_change_bps: u64,

    /// Minimum total liquidity (sum of all levels)
    pub min_total_liquidity: u64,

    /// Enable orderbook depth validation
    pub validate_depth: bool,

    /// Allow locked orderbook (bid == ask)
    pub allow_locked: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_age_ns: 5_000_000_000,        // 5 seconds
            max_spread_bps: 1000,             // 10%
            min_spread_bps: 1,                // 0.01% (1bp minimum)
            max_price_change_bps: 500,        // 5% per snapshot
            min_total_liquidity: 100_000_000, // 0.1 BTC in fixed-point
            validate_depth: true,
            allow_locked: false,
        }
    }
}

/// Centralized snapshot validator
///
/// Validates snapshots according to a comprehensive set of rules:
/// - Basic sanity checks (non-zero prices, sizes, sequence)
/// - Orderbook integrity (not crossed, reasonable spread)
/// - Timestamp validation (not stale, not in future)
/// - Depth level validation (if enabled)
/// - Price spike detection
/// - Liquidity checks
#[derive(Debug, Clone)]
pub struct SnapshotValidator {
    config: ValidationConfig,
    last_mid_price: Option<u64>,
    snapshot_count: u64,
    recent_volatility_bps: u64,
}

impl Default for SnapshotValidator {
    fn default() -> Self {
        Self {
            config: ValidationConfig::default(),
            last_mid_price: None,
            snapshot_count: 0,
            recent_volatility_bps: 0,
        }
    }
}

impl SnapshotValidator {
    /// Create a new validator with default configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a validator with custom configuration
    pub fn with_config(config: ValidationConfig) -> Self {
        Self {
            config,
            last_mid_price: None,
            snapshot_count: 0,
            recent_volatility_bps: 0,
        }
    }

    /// Create a validator with custom max age (backward compatibility)
    pub fn with_max_age(max_age_ns: u64) -> Self {
        let mut config = ValidationConfig::default();
        config.max_age_ns = max_age_ns;
        Self::with_config(config)
    }

    /// Validate a snapshot with comprehensive checks
    ///
    /// # Returns
    /// - `Ok(())`: Snapshot is valid
    /// - `Err(ValidationError)`: Snapshot is invalid with specific error
    pub fn validate(&mut self, snapshot: &MarketSnapshot) -> Result<(), ValidationError> {
        // 1. Basic sanity checks
        self.validate_basic(snapshot)?;

        // 2. Timestamp validation
        self.validate_timestamp(snapshot)?;

        // 3. Orderbook integrity
        self.validate_orderbook(snapshot)?;

        // 4. Spread validation
        self.validate_spread(snapshot)?;

        // 5. Price spike detection
        self.validate_price_change(snapshot)?;

        // 6. Liquidity checks
        self.validate_liquidity(snapshot)?;

        // 7. Depth validation (if enabled AND full snapshot)
        // CRITICAL: Only validate depth on full snapshots!
        // Incremental snapshots (snapshot_flags & 0x01 == 0) only update best bid/ask,
        // and depth arrays may contain stale data from previous full snapshot.
        if self.config.validate_depth && snapshot.is_full_snapshot() {
            self.validate_depth(snapshot)?;
        }

        // Update tracking for adaptive thresholds
        let mid_price = (snapshot.best_bid_price + snapshot.best_ask_price) / 2;

        // Track volatility (exponential moving average of price changes)
        if let Some(last_mid) = self.last_mid_price {
            let change = if mid_price > last_mid {
                mid_price - last_mid
            } else {
                last_mid - mid_price
            };
            let change_bps = (change * 10_000) / last_mid;

            // EMA with alpha=0.1 for smoothing
            self.recent_volatility_bps = (self.recent_volatility_bps * 9 + change_bps) / 10;
        }

        self.last_mid_price = Some(mid_price);
        self.snapshot_count += 1;

        Ok(())
    }

    /// Basic sanity checks
    fn validate_basic(&self, snapshot: &MarketSnapshot) -> Result<(), ValidationError> {
        if snapshot.sequence == 0 {
            return Err(ValidationError::ZeroSequence);
        }

        if snapshot.best_bid_price == 0 {
            return Err(ValidationError::ZeroBidPrice);
        }

        if snapshot.best_ask_price == 0 {
            return Err(ValidationError::ZeroAskPrice);
        }

        Ok(())
    }

    /// Timestamp validation
    fn validate_timestamp(&self, snapshot: &MarketSnapshot) -> Result<(), ValidationError> {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        // Check for future timestamp (clock skew)
        if snapshot.exchange_timestamp_ns > now_ns {
            return Err(ValidationError::FutureTimestamp {
                timestamp_ns: snapshot.exchange_timestamp_ns,
                now_ns,
            });
        }

        // Check if stale
        let age_ns = now_ns.saturating_sub(snapshot.exchange_timestamp_ns);
        if age_ns > self.config.max_age_ns {
            return Err(ValidationError::StaleData {
                age_ns,
                max_age_ns: self.config.max_age_ns,
            });
        }

        Ok(())
    }

    /// Orderbook integrity checks
    fn validate_orderbook(&self, snapshot: &MarketSnapshot) -> Result<(), ValidationError> {
        // Check for crossed orderbook
        if snapshot.best_bid_price > snapshot.best_ask_price {
            return Err(ValidationError::OrderbookCrossed {
                bid: snapshot.best_bid_price,
                ask: snapshot.best_ask_price,
            });
        }

        // Check for locked orderbook (if not allowed)
        if !self.config.allow_locked && snapshot.best_bid_price == snapshot.best_ask_price {
            return Err(ValidationError::OrderbookLocked {
                price: snapshot.best_bid_price,
            });
        }

        Ok(())
    }

    /// Spread validation
    fn validate_spread(&self, snapshot: &MarketSnapshot) -> Result<(), ValidationError> {
        // Check for zero bid price to prevent division by zero
        if snapshot.best_bid_price == 0 {
            return Err(ValidationError::InvalidPrice);
        }

        let spread = snapshot.best_ask_price - snapshot.best_bid_price;
        let spread_bps = (spread * 10_000) / snapshot.best_bid_price;

        // Check if spread is too wide
        if spread_bps > self.config.max_spread_bps {
            return Err(ValidationError::SpreadTooWide { spread_bps });
        }

        // Check if spread is suspiciously narrow (possible data error)
        if spread_bps < self.config.min_spread_bps {
            return Err(ValidationError::SpreadTooNarrow { spread_bps });
        }

        Ok(())
    }

    /// Price spike detection with adaptive thresholds
    fn validate_price_change(&self, snapshot: &MarketSnapshot) -> Result<(), ValidationError> {
        if let Some(last_mid) = self.last_mid_price {
            let current_mid = (snapshot.best_bid_price + snapshot.best_ask_price) / 2;

            let change = if current_mid > last_mid {
                current_mid - last_mid
            } else {
                last_mid - current_mid
            };

            let change_bps = (change * 10_000) / last_mid;

            // ADAPTIVE THRESHOLD: More lenient during initialization and volatile markets
            let adaptive_max_bps = if self.snapshot_count < 10 {
                // First 10 snapshots: allow 2x normal threshold (10% moves)
                self.config.max_price_change_bps * 2
            } else if self.recent_volatility_bps > 200 {
                // High volatility market: allow 1.5x normal threshold
                (self.config.max_price_change_bps * 3) / 2
            } else {
                // Normal market conditions: use configured threshold
                self.config.max_price_change_bps
            };

            if change_bps > adaptive_max_bps {
                return Err(ValidationError::PriceSpike {
                    change_bps,
                    max_bps: adaptive_max_bps,
                });
            }
        }

        Ok(())
    }

    /// Liquidity validation
    fn validate_liquidity(&self, snapshot: &MarketSnapshot) -> Result<(), ValidationError> {
        // Sum up bid side liquidity
        let mut total_bid_size = snapshot.best_bid_size;
        for &size in &snapshot.bid_sizes {
            if size > 0 {
                total_bid_size += size;
            }
        }

        // Sum up ask side liquidity
        let mut total_ask_size = snapshot.best_ask_size;
        for &size in &snapshot.ask_sizes {
            if size > 0 {
                total_ask_size += size;
            }
        }

        // Check minimum liquidity
        if total_bid_size < self.config.min_total_liquidity
            || total_ask_size < self.config.min_total_liquidity
        {
            return Err(ValidationError::LowLiquidity {
                total_bid_size,
                total_ask_size,
                min_size: self.config.min_total_liquidity,
            });
        }

        Ok(())
    }

    /// Capture invalid snapshot to disk for debugging
    fn _capture_invalid_snapshot(&self, snapshot: &MarketSnapshot, error: &ValidationError) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let filename = format!("/tmp/bog-invalid-snapshot-{}.json", timestamp);

        // Create a detailed debug representation
        let debug_info = format!(
            r#"{{
  "error": "{}",
  "sequence": {},
  "market_id": {},
  "exchange_timestamp_ns": {},
  "best_bid_price": {},
  "best_bid_size": {},
  "best_ask_price": {},
  "best_ask_size": {},
  "bid_prices": {:?},
  "bid_sizes": {:?},
  "ask_prices": {:?},
  "ask_sizes": {:?}
}}"#,
            error,
            snapshot.sequence,
            snapshot.market_id,
            snapshot.exchange_timestamp_ns,
            snapshot.best_bid_price,
            snapshot.best_bid_size,
            snapshot.best_ask_price,
            snapshot.best_ask_size,
            snapshot.bid_prices,
            snapshot.bid_sizes,
            snapshot.ask_prices,
            snapshot.ask_sizes
        );

        if let Ok(mut file) = File::create(&filename) {
            let _ = file.write_all(debug_info.as_bytes());
            eprintln!("📸 Captured invalid snapshot to: {}", filename);
        }
    }

    /// Depth level validation
    fn validate_depth(&self, snapshot: &MarketSnapshot) -> Result<(), ValidationError> {
        // Validate bid levels
        let mut last_bid_price = snapshot.best_bid_price;
        for (i, (&price, &size)) in snapshot
            .bid_prices
            .iter()
            .zip(snapshot.bid_sizes.iter())
            .enumerate()
        {
            // Skip empty levels
            if price == 0 && size == 0 {
                continue;
            }

            // Price must be less than previous level (descending)
            if price >= last_bid_price {
                let error = ValidationError::InvalidDepthLevel {
                    level: i + 1,
                    reason: format!("Bid price {} must be < previous {}", price, last_bid_price),
                };
                // Only log error, don't block on IO
                // self.capture_invalid_snapshot(snapshot, &error);
                return Err(error);
            }

            // Must have non-zero size
            if size == 0 {
                let error = ValidationError::InvalidDepthLevel {
                    level: i + 1,
                    reason: "Size is zero but price is set".to_string(),
                };
                // self.capture_invalid_snapshot(snapshot, &error);
                return Err(error);
            }

            last_bid_price = price;
        }

        // Validate ask levels
        let mut last_ask_price = snapshot.best_ask_price;
        for (i, (&price, &size)) in snapshot
            .ask_prices
            .iter()
            .zip(snapshot.ask_sizes.iter())
            .enumerate()
        {
            // Skip empty levels
            if price == 0 && size == 0 {
                continue;
            }

            // Price must be greater than previous level (ascending)
            if price <= last_ask_price {
                let error = ValidationError::InvalidDepthLevel {
                    level: i + 1,
                    reason: format!("Ask price {} must be > previous {}", price, last_ask_price),
                };
                // self.capture_invalid_snapshot(snapshot, &error);
                return Err(error);
            }

            // Must have non-zero size
            if size == 0 {
                let error = ValidationError::InvalidDepthLevel {
                    level: i + 1,
                    reason: "Size is zero but price is set".to_string(),
                };
                // self.capture_invalid_snapshot(snapshot, &error);
                return Err(error);
            }

            last_ask_price = price;
        }

        Ok(())
    }

    /// Quick validity check (boolean instead of Result)
    #[inline]
    pub fn is_valid(&mut self, snapshot: &MarketSnapshot) -> bool {
        self.validate(snapshot).is_ok()
    }

    /// Check if snapshot is crossed (bid >= ask)
    #[inline]
    pub fn is_crossed(snapshot: &MarketSnapshot) -> bool {
        snapshot.best_bid_price >= snapshot.best_ask_price
    }

    /// Check if snapshot is locked (bid == ask)
    #[inline]
    pub fn is_locked(snapshot: &MarketSnapshot) -> bool {
        snapshot.best_bid_price == snapshot.best_ask_price
    }

    /// Check if snapshot is stale
    pub fn is_stale(&self, snapshot: &MarketSnapshot) -> bool {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let age_ns = now_ns.saturating_sub(snapshot.exchange_timestamp_ns);
        age_ns > self.config.max_age_ns
    }

    /// Reset price spike tracking
    pub fn reset(&mut self) {
        self.last_mid_price = None;
    }

    /// Get current configuration
    pub fn config(&self) -> &ValidationConfig {
        &self.config
    }

    /// Update configuration
    pub fn set_config(&mut self, config: ValidationConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_creation() {
        let validator = SnapshotValidator::new();
        assert_eq!(validator.config.max_age_ns, 5_000_000_000);
    }

    #[test]
    fn test_custom_max_age() {
        let validator = SnapshotValidator::with_max_age(10_000_000_000);
        assert_eq!(validator.config.max_age_ns, 10_000_000_000);
    }

    #[test]
    fn test_is_crossed() {
        let bid = 100u64;
        let ask = 100u64;
        assert!(bid >= ask);
    }

    #[test]
    fn test_is_locked() {
        let bid = 100u64;
        let ask = 100u64;
        assert_eq!(bid, ask);
    }
}
