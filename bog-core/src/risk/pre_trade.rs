//! Pre-Trade Validation - Final Safety Check Before Order Placement
//!
//! This module provides the last line of defense before placing orders.
//! All checks are performed immediately before calling the exchange API.
//!
//! ## Validation Layers
//!
//! ```text
//! Strategy → Risk Limits → Circuit Breaker → Rate Limiter → PRE-TRADE → Exchange
//!            ✓ Position     ✓ Flash crash   ✓ Not spam     ✓ Final
//!            ✓ Order size   ✓ Spread        ✓ Burst OK     ✓ checks
//!            ✓ Daily loss   ✓ Liquidity
//! ```
//!
//! ## Checks Performed
//!
//! 1. **Connection Health** - Is exchange API reachable?
//! 2. **Account Balance** - Do we have sufficient funds? (stub for now)
//! 3. **Margin Check** - Is margin available? (stub for now)
//! 4. **Exchange Rules** - Min size, tick size, etc.
//! 5. **Sanity Checks** - Price/size reasonableness
//! 6. **Kill Switch** - Is trading enabled?

use crate::resilience::KillSwitch;
use anyhow::Result;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use tracing::{debug, warn};

/// Pre-trade validation result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreTradeResult {
    /// Order is valid and can be placed
    Allowed,
    /// Order should be rejected
    Rejected(PreTradeRejection),
}

impl PreTradeResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, PreTradeResult::Allowed)
    }
}

/// Reason for pre-trade rejection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreTradeRejection {
    /// Kill switch is active
    KillSwitchActive,
    /// Trading is paused
    TradingPaused,
    /// Exchange connection unhealthy
    ConnectionUnhealthy,
    /// Insufficient account balance
    InsufficientBalance {
        required: Decimal,
        available: Decimal,
    },
    /// Insufficient margin
    InsufficientMargin,
    /// Order size below exchange minimum
    SizeBelowMinimum { size: Decimal, minimum: Decimal },
    /// Order size above exchange maximum
    SizeAboveMaximum { size: Decimal, maximum: Decimal },
    /// Price not on valid tick
    InvalidTick { price: Decimal, tick_size: Decimal },
    /// Price too far from market (safety check)
    PriceTooFarFromMarket {
        price: Decimal,
        mid: Decimal,
        max_distance_bps: u32,
    },
}

impl std::fmt::Display for PreTradeRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreTradeRejection::KillSwitchActive => write!(f, "Kill switch active"),
            PreTradeRejection::TradingPaused => write!(f, "Trading paused"),
            PreTradeRejection::ConnectionUnhealthy => write!(f, "Exchange connection unhealthy"),
            PreTradeRejection::InsufficientBalance {
                required,
                available,
            } => {
                write!(
                    f,
                    "Insufficient balance: need {}, have {}",
                    required, available
                )
            }
            PreTradeRejection::InsufficientMargin => write!(f, "Insufficient margin"),
            PreTradeRejection::SizeBelowMinimum { size, minimum } => {
                write!(f, "Size {} below minimum {}", size, minimum)
            }
            PreTradeRejection::SizeAboveMaximum { size, maximum } => {
                write!(f, "Size {} above maximum {}", size, maximum)
            }
            PreTradeRejection::InvalidTick { price, tick_size } => {
                write!(f, "Price {} not on tick size {}", price, tick_size)
            }
            PreTradeRejection::PriceTooFarFromMarket {
                price,
                mid,
                max_distance_bps,
            } => {
                write!(
                    f,
                    "Price {} too far from mid {} (max: {}bps)",
                    price, mid, max_distance_bps
                )
            }
        }
    }
}

/// Exchange-specific trading rules
#[derive(Debug, Clone)]
pub struct ExchangeRules {
    /// Minimum order size
    pub min_order_size: Decimal,
    /// Maximum order size
    pub max_order_size: Decimal,
    /// Tick size (price increment)
    pub tick_size: Decimal,
    /// Maximum price distance from mid (in bps) - safety check
    pub max_price_distance_bps: u32,
}

impl ExchangeRules {
    /// Lighter DEX rules (BTC/USD)
    pub fn lighter_btc_usd() -> Self {
        use rust_decimal_macros::dec;
        Self {
            min_order_size: dec!(0.001), // 0.001 BTC minimum
            max_order_size: dec!(10.0),  // 10 BTC maximum per order
            tick_size: dec!(0.01),       // $0.01 tick size
            max_price_distance_bps: 500, // 5% from mid (safety check)
        }
    }
}

/// Pre-trade validator
pub struct PreTradeValidator {
    rules: ExchangeRules,
    kill_switch: Option<KillSwitch>,
}

impl PreTradeValidator {
    /// Create a new pre-trade validator
    pub fn new(rules: ExchangeRules) -> Self {
        Self {
            rules,
            kill_switch: None,
        }
    }

    /// Create with kill switch integration
    pub fn with_kill_switch(rules: ExchangeRules, kill_switch: KillSwitch) -> Self {
        Self {
            rules,
            kill_switch: Some(kill_switch),
        }
    }

    /// Validate order before placement
    ///
    /// Performs all pre-trade checks and returns Allowed or Rejected.
    pub fn validate(&self, price: Decimal, size: Decimal, mid_price: Decimal) -> PreTradeResult {
        // 1. Check kill switch
        if let Some(ref ks) = self.kill_switch {
            if ks.should_stop() {
                debug!("Pre-trade check: Kill switch active");
                return PreTradeResult::Rejected(PreTradeRejection::KillSwitchActive);
            }
            if ks.is_paused() {
                debug!("Pre-trade check: Trading paused");
                return PreTradeResult::Rejected(PreTradeRejection::TradingPaused);
            }
        }

        // 2. Size validation
        if size < self.rules.min_order_size {
            warn!(
                "Pre-trade check: Size below minimum ({} < {})",
                size, self.rules.min_order_size
            );
            return PreTradeResult::Rejected(PreTradeRejection::SizeBelowMinimum {
                size,
                minimum: self.rules.min_order_size,
            });
        }

        if size > self.rules.max_order_size {
            warn!(
                "Pre-trade check: Size above maximum ({} > {})",
                size, self.rules.max_order_size
            );
            return PreTradeResult::Rejected(PreTradeRejection::SizeAboveMaximum {
                size,
                maximum: self.rules.max_order_size,
            });
        }

        // 3. Price tick validation
        if !self.is_on_tick(price) {
            warn!(
                "Pre-trade check: Price {} not on tick size {}",
                price, self.rules.tick_size
            );
            return PreTradeResult::Rejected(PreTradeRejection::InvalidTick {
                price,
                tick_size: self.rules.tick_size,
            });
        }

        // 4. Price sanity check (not too far from market)
        if mid_price > Decimal::ZERO {
            let distance = if price > mid_price {
                price - mid_price
            } else {
                mid_price - price
            };

            let distance_bps = ((distance / mid_price) * Decimal::from(10000))
                .to_u32()
                .unwrap_or(u32::MAX);

            if distance_bps > self.rules.max_price_distance_bps {
                warn!(
                    "Pre-trade check: Price too far from mid ({}bps > {}bps)",
                    distance_bps, self.rules.max_price_distance_bps
                );
                return PreTradeResult::Rejected(PreTradeRejection::PriceTooFarFromMarket {
                    price,
                    mid: mid_price,
                    max_distance_bps: self.rules.max_price_distance_bps,
                });
            }
        }

        // All checks passed!
        PreTradeResult::Allowed
    }

    /// Check if price is on a valid tick
    fn is_on_tick(&self, price: Decimal) -> bool {
        if self.rules.tick_size == Decimal::ZERO {
            return true; // No tick size restriction
        }

        let remainder = price % self.rules.tick_size;
        remainder == Decimal::ZERO
    }

    /// Validate account balance (stub - implement when SDK available)
    pub fn check_balance(&self, _required: Decimal) -> Result<()> {
        // TODO: Implement when Lighter SDK available
        // Query account balance from exchange
        // Verify we have enough funds
        Ok(())
    }

    /// Validate margin availability (stub - implement when SDK available)
    pub fn check_margin(&self, _required: Decimal) -> Result<()> {
        // TODO: Implement when Lighter SDK available
        // Query margin status
        // Verify we have enough margin
        Ok(())
    }

    /// Check connection health (stub - implement when SDK available)
    pub fn check_connection(&self) -> Result<()> {
        // TODO: Implement when Lighter SDK available
        // Ping health endpoint
        // Verify API is responding
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn create_test_validator() -> PreTradeValidator {
        PreTradeValidator::new(ExchangeRules::lighter_btc_usd())
    }

    #[test]
    fn test_valid_order() {
        let validator = create_test_validator();

        let result = validator.validate(
            dec!(50000.00), // Price
            dec!(0.1),      // Size
            dec!(50000.50), // Mid
        );

        assert!(result.is_allowed());
    }

    #[test]
    fn test_size_below_minimum() {
        let validator = create_test_validator();

        let result = validator.validate(
            dec!(50000.00),
            dec!(0.0001), // Below 0.001 minimum
            dec!(50000.50),
        );

        assert!(matches!(
            result,
            PreTradeResult::Rejected(PreTradeRejection::SizeBelowMinimum { .. })
        ));
    }

    #[test]
    fn test_size_above_maximum() {
        let validator = create_test_validator();

        let result = validator.validate(
            dec!(50000.00),
            dec!(15.0), // Above 10.0 maximum
            dec!(50000.50),
        );

        assert!(matches!(
            result,
            PreTradeResult::Rejected(PreTradeRejection::SizeAboveMaximum { .. })
        ));
    }

    #[test]
    fn test_invalid_tick() {
        let validator = create_test_validator();

        let result = validator.validate(
            dec!(50000.123), // Not on $0.01 tick
            dec!(0.1),
            dec!(50000.50),
        );

        assert!(matches!(
            result,
            PreTradeResult::Rejected(PreTradeRejection::InvalidTick { .. })
        ));
    }

    #[test]
    fn test_valid_tick() {
        let validator = create_test_validator();

        let result = validator.validate(
            dec!(50000.00), // On $0.01 tick
            dec!(0.1),
            dec!(50000.50),
        );

        assert!(result.is_allowed());
    }

    #[test]
    fn test_price_too_far_from_mid() {
        let validator = create_test_validator();

        let result = validator.validate(
            dec!(55000.00), // 10% above mid (exceeds 5% limit)
            dec!(0.1),
            dec!(50000.00),
        );

        assert!(matches!(
            result,
            PreTradeResult::Rejected(PreTradeRejection::PriceTooFarFromMarket { .. })
        ));
    }

    #[test]
    fn test_kill_switch_integration() {
        let kill_switch = KillSwitch::new();
        let validator = PreTradeValidator::with_kill_switch(
            ExchangeRules::lighter_btc_usd(),
            kill_switch.clone(),
        );

        // Should allow when running
        let result = validator.validate(dec!(50000), dec!(0.1), dec!(50000));
        assert!(result.is_allowed());

        // Activate kill switch
        kill_switch.shutdown("Test");

        // Should reject
        let result = validator.validate(dec!(50000), dec!(0.1), dec!(50000));
        assert!(matches!(
            result,
            PreTradeResult::Rejected(PreTradeRejection::KillSwitchActive)
        ));
    }

    #[test]
    fn test_pause_integration() {
        let kill_switch = KillSwitch::new();
        let validator = PreTradeValidator::with_kill_switch(
            ExchangeRules::lighter_btc_usd(),
            kill_switch.clone(),
        );

        // Pause trading
        kill_switch.pause();

        let result = validator.validate(dec!(50000), dec!(0.1), dec!(50000));
        assert!(matches!(
            result,
            PreTradeResult::Rejected(PreTradeRejection::TradingPaused)
        ));

        // Resume
        kill_switch.resume();

        let result = validator.validate(dec!(50000), dec!(0.1), dec!(50000));
        assert!(result.is_allowed());
    }
}
