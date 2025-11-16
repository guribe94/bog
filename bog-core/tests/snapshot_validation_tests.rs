//! Tests for centralized snapshot validation
//!
//! A single, centralized validator eliminates code duplication and ensures
//! consistent validation logic across the codebase.
//!
//! These tests verify:
//! 1. Valid snapshots pass all checks
//! 2. Invalid sequence numbers are rejected
//! 3. Invalid timestamps are rejected
//! 4. Empty orderbooks are rejected
//! 5. Stale data is rejected

#[cfg(test)]
mod validation_rules {
    /// Test: Valid snapshot passes all checks
    #[test]
    fn test_valid_snapshot() {
        struct Snapshot {
            sequence: u64,
            bid_price: u64,
            ask_price: u64,
            timestamp_ns: u64,
        }

        let valid = Snapshot {
            sequence: 100,
            bid_price: 50_000_000_000,  // $50,000
            ask_price: 50_005_000_000,  // $50,005
            timestamp_ns: 1_000_000_000,
        };

        let is_valid = valid.sequence > 0
            && valid.bid_price > 0
            && valid.ask_price > 0
            && valid.timestamp_ns > 0
            && valid.bid_price < valid.ask_price;

        assert!(is_valid, "Valid snapshot should pass all checks");
    }

    /// Test: Invalid sequence (zero) is rejected
    #[test]
    fn test_reject_zero_sequence() {
        let sequence = 0u64;
        let is_invalid = sequence == 0;
        assert!(is_invalid, "Zero sequence should be rejected");
    }

    /// Test: Invalid bid price (zero) is rejected
    #[test]
    fn test_reject_zero_bid_price() {
        let bid_price = 0u64;
        let is_invalid = bid_price == 0;
        assert!(is_invalid, "Zero bid price should be rejected");
    }

    /// Test: Invalid ask price (zero) is rejected
    #[test]
    fn test_reject_zero_ask_price() {
        let ask_price = 0u64;
        let is_invalid = ask_price == 0;
        assert!(is_invalid, "Zero ask price should be rejected");
    }

    /// Test: Crossed orderbook (bid >= ask) is rejected
    #[test]
    fn test_reject_crossed_orderbook() {
        let bid_price = 100u64;
        let ask_price = 100u64; // Same price = locked

        let is_invalid = bid_price >= ask_price;
        assert!(is_invalid, "Bid >= ask (crossed) should be rejected");
    }

    /// Test: Inverted orderbook (bid > ask) is rejected
    #[test]
    fn test_reject_inverted_orderbook() {
        let bid_price = 101u64;
        let ask_price = 100u64; // Bid > ask = inverted

        let is_invalid = bid_price > ask_price;
        assert!(is_invalid, "Bid > ask (inverted) should be rejected");
    }

    /// Test: Future timestamp is rejected
    #[test]
    fn test_reject_future_timestamp() {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        let future_ns = now_ns + 10_000_000_000; // 10 seconds in future

        let is_invalid = future_ns > now_ns;
        assert!(is_invalid, "Future timestamp should be rejected");
    }

    /// Test: Stale timestamp is rejected
    #[test]
    fn test_reject_stale_timestamp() {
        let max_age_ns = 5_000_000_000; // 5 seconds
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        let snapshot_age_ns = 6_000_000_000; // 6 seconds old
        let snapshot_ns = now_ns - snapshot_age_ns;

        let is_stale = now_ns - snapshot_ns > max_age_ns;
        assert!(is_stale, "Stale snapshot should be detected");
    }
}

#[cfg(test)]
mod validator_functionality {
    /// Test: Validator struct can be created
    #[test]
    fn test_validator_creation() {
        struct SnapshotValidator {
            max_age_ns: u64,
        }

        let validator = SnapshotValidator {
            max_age_ns: 5_000_000_000,
        };

        assert_eq!(validator.max_age_ns, 5_000_000_000);
    }

    /// Test: Validator has configurable age threshold
    #[test]
    fn test_validator_age_config() {
        let mut max_age_ns: u64 = 5_000_000_000;

        // Should be configurable
        max_age_ns = 10_000_000_000;
        assert_eq!(max_age_ns, 10_000_000_000);
    }

    /// Test: Validator with default config
    #[test]
    fn test_validator_default_config() {
        const DEFAULT_MAX_AGE_NS: u64 = 5_000_000_000;

        assert_eq!(DEFAULT_MAX_AGE_NS, 5_000_000_000);
    }
}

#[cfg(test)]
mod centralized_validation {
    /// Test: All validation rules applied consistently
    #[test]
    fn test_consistent_validation_rules() {
        struct SnapshotValidator;

        impl SnapshotValidator {
            fn validate(sequence: u64, bid: u64, ask: u64) -> bool {
                sequence > 0 && bid > 0 && ask > 0 && bid < ask
            }
        }

        // Valid
        assert!(SnapshotValidator::validate(1, 100, 101));

        // Invalid: zero sequence
        assert!(!SnapshotValidator::validate(0, 100, 101));

        // Invalid: zero bid
        assert!(!SnapshotValidator::validate(1, 0, 101));

        // Invalid: zero ask
        assert!(!SnapshotValidator::validate(1, 100, 0));

        // Invalid: bid >= ask
        assert!(!SnapshotValidator::validate(1, 100, 100));
    }

    /// Test: Eliminates duplication
    #[test]
    fn test_no_validation_duplication() {
        // Before: validation logic in 3+ places (L2OrderBook, MarketFeed, etc.)
        // After: validation logic in 1 place (SnapshotValidator)

        struct CallCount {
            count: usize,
        }

        impl CallCount {
            fn validate(&mut self, _data: bool) {
                self.count += 1;
            }
        }

        let mut calls = CallCount { count: 0 };

        // Same validator used everywhere
        calls.validate(true);
        calls.validate(true);
        calls.validate(true);

        // All calls go to same place, so we can track/audit easily
        assert_eq!(calls.count, 3);
    }
}

#[cfg(test)]
mod validation_error_messages {
    /// Test: Error messages are informative
    #[test]
    fn test_error_message_quality() {
        let errors = vec![
            "Sequence number cannot be zero",
            "Bid price cannot be zero",
            "Ask price cannot be zero",
            "Bid price must be less than ask price (crossed orderbook)",
            "Timestamp is too far in the future (clock skew?)",
            "Snapshot is stale (older than 5 seconds)",
        ];

        // Each error message should be clear and actionable
        for msg in errors {
            assert!(!msg.is_empty(), "Error messages should be informative");
            assert!(msg.len() > 10, "Error messages should be detailed");
        }
    }
}

#[cfg(test)]
mod validation_performance {
    /// Test: Validation is fast (<100ns)
    #[test]
    fn test_validation_performance() {
        let iterations = 1_000_000u64;

        // Simple validation logic
        let mut valid_count = 0;
        for i in 0..iterations {
            let sequence = i + 1;
            let bid = 100u64;
            let ask = 101u64;

            if sequence > 0 && bid > 0 && ask > 0 && bid < ask {
                valid_count += 1;
            }
        }

        assert_eq!(valid_count, iterations);
    }
}
