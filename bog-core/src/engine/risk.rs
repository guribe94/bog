//! Zero-Overhead Risk Validation
//!
//! This module provides compile-time risk limits and inline validation
//! for sub-microsecond HFT performance.
//!
//! Key features:
//! - Const parameters from Cargo features
//! - Inline validation (<50ns target)
//! - No heap allocations
//! - Branch-free where possible
//! - Uses existing Position atomics

use crate::core::{Position, Signal, SignalAction, Side};
use anyhow::{anyhow, Result};

// ===== CONST RISK LIMITS FROM CARGO FEATURES =====

/// Maximum long position (fixed-point, 9 decimals)
/// Default: 1.0 BTC
#[cfg(not(any(
    feature = "max-position-half",
    feature = "max-position-one",
    feature = "max-position-five"
)))]
pub const MAX_POSITION: i64 = 1_000_000_000; // 1.0 BTC

#[cfg(feature = "max-position-half")]
pub const MAX_POSITION: i64 = 500_000_000; // 0.5 BTC

#[cfg(feature = "max-position-one")]
pub const MAX_POSITION: i64 = 1_000_000_000; // 1.0 BTC

#[cfg(feature = "max-position-five")]
pub const MAX_POSITION: i64 = 5_000_000_000; // 5.0 BTC

/// Maximum short position (fixed-point, 9 decimals)
/// Default: 1.0 BTC
#[cfg(not(any(
    feature = "max-short-half",
    feature = "max-short-one",
    feature = "max-short-five"
)))]
pub const MAX_SHORT: i64 = 1_000_000_000; // 1.0 BTC

#[cfg(feature = "max-short-half")]
pub const MAX_SHORT: i64 = 500_000_000;

#[cfg(feature = "max-short-one")]
pub const MAX_SHORT: i64 = 1_000_000_000;

#[cfg(feature = "max-short-five")]
pub const MAX_SHORT: i64 = 5_000_000_000;

/// Maximum order size (fixed-point, 9 decimals)
/// Default: 0.5 BTC
#[cfg(not(any(
    feature = "max-order-tenth",
    feature = "max-order-half",
    feature = "max-order-one"
)))]
pub const MAX_ORDER_SIZE: u64 = 500_000_000; // 0.5 BTC

#[cfg(feature = "max-order-tenth")]
pub const MAX_ORDER_SIZE: u64 = 100_000_000;

#[cfg(feature = "max-order-half")]
pub const MAX_ORDER_SIZE: u64 = 500_000_000;

#[cfg(feature = "max-order-one")]
pub const MAX_ORDER_SIZE: u64 = 1_000_000_000;

/// Minimum order size (fixed-point, 9 decimals)
/// Default: 0.01 BTC
#[cfg(not(any(
    feature = "min-order-milli",
    feature = "min-order-centi",
    feature = "min-order-tenth"
)))]
pub const MIN_ORDER_SIZE: u64 = 10_000_000; // 0.01 BTC

#[cfg(feature = "min-order-milli")]
pub const MIN_ORDER_SIZE: u64 = 1_000_000;

#[cfg(feature = "min-order-centi")]
pub const MIN_ORDER_SIZE: u64 = 10_000_000;

#[cfg(feature = "min-order-tenth")]
pub const MIN_ORDER_SIZE: u64 = 100_000_000;

/// Maximum daily loss (fixed-point, 9 decimals)
/// Default: 1000 USD
#[cfg(not(any(
    feature = "max-daily-loss-100",
    feature = "max-daily-loss-1000",
    feature = "max-daily-loss-10000"
)))]
pub const MAX_DAILY_LOSS: i64 = 1_000_000_000_000; // 1000 USD

#[cfg(feature = "max-daily-loss-100")]
pub const MAX_DAILY_LOSS: i64 = 100_000_000_000;

#[cfg(feature = "max-daily-loss-1000")]
pub const MAX_DAILY_LOSS: i64 = 1_000_000_000_000;

#[cfg(feature = "max-daily-loss-10000")]
pub const MAX_DAILY_LOSS: i64 = 10_000_000_000_000;

// ===== INLINE RISK VALIDATION =====

/// Risk violation reasons (zero allocation)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RiskViolation {
    OrderTooSmall = 0,
    OrderTooLarge = 1,
    PositionLimitLong = 2,
    PositionLimitShort = 3,
    DailyLossLimit = 4,
    NoViolation = 255,
}

impl RiskViolation {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskViolation::OrderTooSmall => "Order size below minimum",
            RiskViolation::OrderTooLarge => "Order size exceeds maximum",
            RiskViolation::PositionLimitLong => "Long position limit exceeded",
            RiskViolation::PositionLimitShort => "Short position limit exceeded",
            RiskViolation::DailyLossLimit => "Daily loss limit breached",
            RiskViolation::NoViolation => "No violation",
        }
    }
}

/// Validate signal against risk limits (inline, <50ns target)
///
/// This function is designed to be branch-free where possible.
/// Returns Ok(()) if valid, Err with violation reason if invalid.
#[inline(always)]
pub fn validate_signal(signal: &Signal, position: &Position) -> Result<()> {
    // Skip validation for no-action signals
    if signal.action == SignalAction::NoAction || signal.action == SignalAction::CancelAll {
        return Ok(());
    }

    // Validate order size for all order types
    if signal.requires_action() && signal.size > 0 {
        // Check min/max order size
        if signal.size < MIN_ORDER_SIZE {
            return Err(anyhow!(RiskViolation::OrderTooSmall.as_str()));
        }

        if signal.size > MAX_ORDER_SIZE {
            return Err(anyhow!(RiskViolation::OrderTooLarge.as_str()));
        }
    }

    // Validate position limits
    validate_position_limits(signal, position)
}

/// Validate position limits (inline)
#[inline(always)]
fn validate_position_limits(signal: &Signal, position: &Position) -> Result<()> {
    let current_position = position.get_quantity();

    match signal.action {
        SignalAction::QuoteBoth => {
            // Both sides - check worst case for each side
            let buy_projection = current_position.saturating_add(signal.size as i64);
            let sell_projection = current_position.saturating_sub(signal.size as i64);

            // Check long limit
            if buy_projection > MAX_POSITION {
                return Err(anyhow!(RiskViolation::PositionLimitLong.as_str()));
            }

            // Check short limit
            if sell_projection < -MAX_SHORT {
                return Err(anyhow!(RiskViolation::PositionLimitShort.as_str()));
            }
        }
        SignalAction::QuoteBid | SignalAction::TakePosition if signal.side == Side::Buy => {
            // Buying - check long limit
            let projection = current_position.saturating_add(signal.size as i64);
            if projection > MAX_POSITION {
                return Err(anyhow!(RiskViolation::PositionLimitLong.as_str()));
            }
        }
        SignalAction::QuoteAsk | SignalAction::TakePosition if signal.side == Side::Sell => {
            // Selling - check short limit
            let projection = current_position.saturating_sub(signal.size as i64);
            if projection < -MAX_SHORT {
                return Err(anyhow!(RiskViolation::PositionLimitShort.as_str()));
            }
        }
        _ => {}
    }

    // Check daily loss limit
    let daily_pnl = position.get_realized_pnl();
    if daily_pnl < -MAX_DAILY_LOSS {
        return Err(anyhow!(RiskViolation::DailyLossLimit.as_str()));
    }

    Ok(())
}

/// Fast position check without detailed validation
///
/// Returns true if position is within limits (branch-free)
#[inline(always)]
pub fn position_within_limits(position_qty: i64) -> bool {
    // Branch-free: check if position is between -MAX_SHORT and MAX_POSITION
    (position_qty >= -MAX_SHORT) & (position_qty <= MAX_POSITION)
}

/// Fast order size check (branch-free)
#[inline(always)]
pub fn order_size_valid(size: u64) -> bool {
    (size >= MIN_ORDER_SIZE) & (size <= MAX_ORDER_SIZE)
}

// ===== COMPILE-TIME VERIFICATION =====

#[cfg(test)]
const _: () = {
    // Verify limits are sane
    assert!(MAX_POSITION > 0, "MAX_POSITION must be positive");
    assert!(MAX_SHORT > 0, "MAX_SHORT must be positive");
    assert!(MAX_ORDER_SIZE > 0, "MAX_ORDER_SIZE must be positive");
    assert!(MIN_ORDER_SIZE > 0, "MIN_ORDER_SIZE must be positive");
    assert!(
        MAX_ORDER_SIZE <= MAX_POSITION as u64,
        "MAX_ORDER_SIZE must not exceed MAX_POSITION"
    );
    assert!(
        MIN_ORDER_SIZE <= MAX_ORDER_SIZE,
        "MIN_ORDER_SIZE must be <= MAX_ORDER_SIZE"
    );
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_const_limits() {
        // Verify const values are defined
        println!("MAX_POSITION: {}", MAX_POSITION);
        println!("MAX_SHORT: {}", MAX_SHORT);
        println!("MAX_ORDER_SIZE: {}", MAX_ORDER_SIZE);
        println!("MIN_ORDER_SIZE: {}", MIN_ORDER_SIZE);
        println!("MAX_DAILY_LOSS: {}", MAX_DAILY_LOSS);

        // Verify they're sane
        assert!(MAX_POSITION > 0);
        assert!(MAX_SHORT > 0);
        assert!(MAX_ORDER_SIZE > 0);
        assert!(MIN_ORDER_SIZE > 0);
        assert!(MAX_ORDER_SIZE as i64 <= MAX_POSITION);
        assert!(MIN_ORDER_SIZE <= MAX_ORDER_SIZE);
    }

    #[test]
    fn test_order_size_valid() {
        // Too small
        assert!(!order_size_valid(MIN_ORDER_SIZE - 1));

        // Valid
        assert!(order_size_valid(MIN_ORDER_SIZE));
        assert!(order_size_valid(MAX_ORDER_SIZE));

        // Too large
        assert!(!order_size_valid(MAX_ORDER_SIZE + 1));
    }

    #[test]
    fn test_position_within_limits() {
        // Valid long positions
        assert!(position_within_limits(0));
        assert!(position_within_limits(MAX_POSITION));
        assert!(position_within_limits(MAX_POSITION / 2));

        // Valid short positions
        assert!(position_within_limits(-MAX_SHORT));
        assert!(position_within_limits(-MAX_SHORT / 2));

        // Invalid positions
        assert!(!position_within_limits(MAX_POSITION + 1));
        assert!(!position_within_limits(-MAX_SHORT - 1));
    }

    #[test]
    fn test_validate_no_action() {
        let position = Position::new();
        let signal = Signal::no_action();

        // NoAction should always be valid
        assert!(validate_signal(&signal, &position).is_ok());
    }

    #[test]
    fn test_validate_cancel_all() {
        let position = Position::new();
        let signal = Signal::cancel_all();

        // CancelAll should always be valid
        assert!(validate_signal(&signal, &position).is_ok());
    }

    #[test]
    fn test_validate_order_size() {
        let position = Position::new();

        // Too small
        let signal = Signal::quote_both(
            50_000_000_000_000,
            50_005_000_000_000,
            MIN_ORDER_SIZE - 1,
        );
        assert!(validate_signal(&signal, &position).is_err());

        // Valid
        let signal = Signal::quote_both(
            50_000_000_000_000,
            50_005_000_000_000,
            MIN_ORDER_SIZE,
        );
        assert!(validate_signal(&signal, &position).is_ok());

        // Too large
        let signal = Signal::quote_both(
            50_000_000_000_000,
            50_005_000_000_000,
            MAX_ORDER_SIZE + 1,
        );
        assert!(validate_signal(&signal, &position).is_err());
    }

    #[test]
    fn test_validate_position_limits_long() {
        let position = Position::new();

        // Position starts at 0, simulate adding to it
        // Buy that would exceed long limit
        let signal = Signal::quote_bid(50_000_000_000_000, MAX_POSITION as u64 + 1);
        assert!(validate_signal(&signal, &position).is_err());

        // Valid buy
        let signal = Signal::quote_bid(50_000_000_000_000, MAX_ORDER_SIZE);
        assert!(validate_signal(&signal, &position).is_ok());
    }

    #[test]
    fn test_validate_position_limits_short() {
        let position = Position::new();

        // Sell that would exceed short limit
        let signal = Signal::quote_ask(50_000_000_000_000, MAX_SHORT as u64 + 1);
        assert!(validate_signal(&signal, &position).is_err());

        // Valid sell
        let signal = Signal::quote_ask(50_000_000_000_000, MAX_ORDER_SIZE);
        assert!(validate_signal(&signal, &position).is_ok());
    }

    #[test]
    fn test_validate_quote_both() {
        let position = Position::new();

        // QuoteBoth needs to check both sides
        let signal = Signal::quote_both(
            50_000_000_000_000,
            50_005_000_000_000,
            MAX_ORDER_SIZE,
        );
        assert!(validate_signal(&signal, &position).is_ok());

        // Size that would violate both sides
        let signal = Signal::quote_both(
            50_000_000_000_000,
            50_005_000_000_000,
            MAX_POSITION as u64 + 1,
        );
        assert!(validate_signal(&signal, &position).is_err());
    }

    #[test]
    fn test_risk_violation_enum() {
        assert_eq!(RiskViolation::OrderTooSmall.as_str(), "Order size below minimum");
        assert_eq!(RiskViolation::NoViolation.as_str(), "No violation");
    }

    #[test]
    fn test_violation_enum_size() {
        // Verify enum is single byte
        assert_eq!(std::mem::size_of::<RiskViolation>(), 1);
    }
}
