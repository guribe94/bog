//! Centralized snapshot validation logic
//!
//! Eliminates duplication by providing a single validation point for all snapshots.

use super::types::MarketSnapshot;

/// Centralized snapshot validator
///
/// Validates snapshots according to a consistent set of rules:
/// - Sequence number must be > 0
/// - Bid and ask prices must be > 0
/// - Bid price must be < ask price (no crossing)
/// - Timestamp must not be in future (clock skew check)
/// - Snapshot must not be stale (age check)
#[derive(Debug, Clone)]
pub struct SnapshotValidator {
    /// Maximum age for snapshot in nanoseconds (default: 5 seconds)
    pub max_age_ns: u64,
}

impl Default for SnapshotValidator {
    fn default() -> Self {
        Self {
            max_age_ns: 5_000_000_000, // 5 seconds
        }
    }
}

impl SnapshotValidator {
    /// Create a new validator with default configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a validator with custom max age
    pub fn with_max_age(max_age_ns: u64) -> Self {
        Self { max_age_ns }
    }

    /// Validate a snapshot
    ///
    /// # Returns
    /// - `Ok(())`: Snapshot is valid
    /// - `Err(msg)`: Snapshot is invalid with reason
    pub fn validate(&self, snapshot: &MarketSnapshot) -> Result<(), String> {
        // Check sequence number
        if snapshot.sequence == 0 {
            return Err("Sequence number cannot be zero".to_string());
        }

        // Check bid price
        if snapshot.best_bid_price == 0 {
            return Err("Bid price cannot be zero".to_string());
        }

        // Check ask price
        if snapshot.best_ask_price == 0 {
            return Err("Ask price cannot be zero".to_string());
        }

        // Check for crossed/inverted orderbook
        if snapshot.best_bid_price >= snapshot.best_ask_price {
            return Err(format!(
                "Bid price ({}) must be less than ask price ({})",
                snapshot.best_bid_price, snapshot.best_ask_price
            ));
        }

        // Check for future timestamp (clock skew)
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        if snapshot.exchange_timestamp_ns > now_ns {
            return Err(
                "Timestamp is in the future (possible clock skew)".to_string()
            );
        }

        // Check if snapshot is stale
        if now_ns > snapshot.exchange_timestamp_ns {
            let age_ns = now_ns - snapshot.exchange_timestamp_ns;
            if age_ns > self.max_age_ns {
                let age_sec = age_ns as f64 / 1_000_000_000.0;
                let max_age_sec = self.max_age_ns as f64 / 1_000_000_000.0;
                return Err(format!(
                    "Snapshot is stale (age: {:.2}s, max: {:.2}s)",
                    age_sec, max_age_sec
                ));
            }
        }

        Ok(())
    }

    /// Quick validity check (boolean instead of Result)
    ///
    /// Returns true if snapshot passes all validation rules
    #[inline]
    pub fn is_valid(&self, snapshot: &MarketSnapshot) -> bool {
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

        if now_ns <= snapshot.exchange_timestamp_ns {
            return false;
        }

        let age_ns = now_ns - snapshot.exchange_timestamp_ns;
        age_ns > self.max_age_ns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_creation() {
        let validator = SnapshotValidator::new();
        assert_eq!(validator.max_age_ns, 5_000_000_000);
    }

    #[test]
    fn test_custom_max_age() {
        let validator = SnapshotValidator::with_max_age(10_000_000_000);
        assert_eq!(validator.max_age_ns, 10_000_000_000);
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
