//! Risk Management System
//!
//! Multi-layer risk protection for HFT trading with real-time validation.
//! Uses zero-overhead fixed-point arithmetic and atomic state tracking.
//!
//! ## Validation Layers
//!
//! ```text
//! Strategy → Risk Limits → Circuit Breaker → Rate Limiter → RISK MANAGER → Exchange
//!            Position     Flash crash   Not spam     Kill Switch
//!            Order size   Spread        Burst OK     Tick Size
//!            Daily loss   Liquidity                    Price Sanity
//! ```

use crate::config::constants::*;
use crate::core::{Position, Side as CoreSide, Signal, SignalAction};
use crate::core::fixed_point;
use crate::resilience::KillSwitch;
use anyhow::{anyhow, Result};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn, error, debug};
use std::fmt;

pub mod circuit_breaker;
pub mod rate_limiter;

pub use circuit_breaker::{BreakerState, CircuitBreaker, HaltReason};
pub use rate_limiter::{RateLimiter, RateLimiterConfig};

/// Risk limits configuration (i64 fixed-point)
#[derive(Debug, Clone, Copy)]
pub struct RiskLimits {
    /// Maximum long position size
    pub max_position: i64,

    /// Maximum short position size (positive value)
    pub max_short: i64,

    /// Maximum size per order
    pub max_order_size: i64,

    /// Minimum size per order
    pub min_order_size: i64,

    /// Maximum number of outstanding orders
    pub max_outstanding_orders: usize,

    /// Maximum daily loss (circuit breaker)
    pub max_daily_loss: i64,

    /// Maximum drawdown fraction (e.g. 50_000_000 for 5%)
    pub max_drawdown: i64,

    /// Tick size (price increment)
    pub tick_size: u64,

    /// Maximum price distance from mid (in bps) - safety check
    pub max_price_distance_bps: u32,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_position: MAX_POSITION,
            max_short: MAX_SHORT,
            // Default order limits if not in constants, assuming features control them or safe defaults
            max_order_size: MAX_POSITION / 2, 
            min_order_size: 1_000_000, // 0.001 BTC (assuming 9 decimals)
            max_outstanding_orders: 10,
            max_daily_loss: MAX_DAILY_LOSS,
            max_drawdown: MAX_DRAWDOWN,
            tick_size: TICK_SIZE, // Use centralized constant from config
            max_price_distance_bps: 50, // 0.5% from mid (more sensible for market making)
        }
    }
}

/// Risk violation types
#[derive(Debug, Clone)]
pub enum RiskViolation {
    OrderSizeTooSmall { size: i64, min: i64 },
    OrderSizeTooLarge { size: i64, max: i64 },
    PositionLimitExceeded { projected: i64, limit: i64 },
    ShortLimitExceeded { projected: i64, limit: i64 },
    TooManyOutstandingOrders { current: usize, max: usize },
    DailyLossLimitBreached { daily_pnl: i64, limit: i64 },
    DrawdownLimitBreached { drawdown: i64, limit: i64 },
    KillSwitchActive,
    TradingPaused,
    InvalidTick { price: u64, tick_size: u64 },
    PriceTooFarFromMid { price: u64, mid: u64, max_distance_bps: u32 },
}

impl fmt::Display for RiskViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RiskViolation::OrderSizeTooSmall { size, min } => {
                write!(f, "Order size {} is below minimum {}", size, min)
            }
            RiskViolation::OrderSizeTooLarge { size, max } => {
                write!(f, "Order size {} exceeds maximum {}", size, max)
            }
            RiskViolation::PositionLimitExceeded { projected, limit } => {
                write!(
                    f,
                    "Projected position {} would exceed limit {}",
                    projected, limit
                )
            }
            RiskViolation::ShortLimitExceeded { projected, limit } => {
                write!(
                    f,
                    "Projected short position {} would exceed limit {}",
                    projected, limit
                )
            }
            RiskViolation::TooManyOutstandingOrders { current, max } => {
                write!(
                    f,
                    "Outstanding orders {} would exceed maximum {}",
                    current, max
                )
            }
            RiskViolation::DailyLossLimitBreached { daily_pnl, limit } => {
                write!(f, "Daily PnL {} breaches loss limit {}", daily_pnl, limit)
            }
            RiskViolation::DrawdownLimitBreached { drawdown, limit } => {
                write!(
                    f,
                    "Drawdown {} breaches limit {}",
                    drawdown, limit
                )
            }
            RiskViolation::KillSwitchActive => write!(f, "Kill switch active"),
            RiskViolation::TradingPaused => write!(f, "Trading paused"),
            RiskViolation::InvalidTick { price, tick_size } => {
                write!(f, "Price {} not on tick size {}", price, tick_size)
            }
            RiskViolation::PriceTooFarFromMid { price, mid, max_distance_bps } => {
                write!(
                    f,
                    "Price {} too far from mid {} (max: {}bps)",
                    price, mid, max_distance_bps
                )
            }
        }
    }
}

/// Risk manager - validates signals and limits using fixed-point arithmetic
///
/// Central risk management component that:
/// - Validates signals against position/order limits
/// - Enforces daily loss and drawdown limits
/// - Manages daily PnL resets
/// - Enforces Kill Switch and Price Sanity checks
///
/// # Thread Safety
///
/// RiskManager itself is not thread-safe (mutable state for timestamp/orders),
/// but it operates on thread-safe atomic Position.
pub struct RiskManager {
    limits: RiskLimits,
    daily_reset_timestamp: u64,
    outstanding_order_count: usize,
    kill_switch: Option<KillSwitch>,
}

impl RiskManager {
    /// Create a new RiskManager with default limits (from constants)
    pub fn new() -> Self {
        Self::with_limits(RiskLimits::default())
    }

    /// Create with explicit limits
    pub fn with_limits(limits: RiskLimits) -> Self {
        info!("Initialized RiskManager with limits: {:?}", limits);

        Self {
            limits,
            daily_reset_timestamp: Self::get_day_start_timestamp(),
            outstanding_order_count: 0,
            kill_switch: None,
        }
    }

    /// Create with kill switch integration
    pub fn with_kill_switch(limits: RiskLimits, kill_switch: KillSwitch) -> Self {
        info!("Initialized RiskManager with limits: {:?} and KillSwitch", limits);

        Self {
            limits,
            daily_reset_timestamp: Self::get_day_start_timestamp(),
            outstanding_order_count: 0,
            kill_switch: Some(kill_switch),
        }
    }

    /// Get timestamp for start of current day (UTC)
    fn get_day_start_timestamp() -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_secs();

        // Round down to start of day (86400 seconds in a day)
        (now / 86400) * 86400
    }

    /// Check if we need to reset daily stats
    pub fn check_daily_reset(&mut self, position: &Position) {
        let current_day_start = Self::get_day_start_timestamp();

        if current_day_start > self.daily_reset_timestamp {
            let old_daily_pnl = position.get_daily_pnl();
            info!(
                "New trading day detected, resetting daily PnL (was: {})",
                old_daily_pnl
            );
            position.reset_daily_pnl();
            // Initialize HWM to current realized PnL (total) to start fresh day
            let total_pnl = position.get_realized_pnl();
            position.update_daily_high_water_mark(total_pnl);
            
            self.daily_reset_timestamp = current_day_start;
        }
    }

    /// Check daily loss limit
    pub fn check_daily_loss(&self, daily_total_pnl: i64) -> Result<()> {
        // MAX_DAILY_LOSS is positive (e.g. 5000). We check if pnl < -5000.
        if daily_total_pnl < -self.limits.max_daily_loss {
             warn!(
                "Daily loss limit breached: {} < -{}",
                daily_total_pnl, self.limits.max_daily_loss
            );
            return Err(anyhow!(RiskViolation::DailyLossLimitBreached {
                daily_pnl: daily_total_pnl,
                limit: self.limits.max_daily_loss
            }));
        }
        Ok(())
    }

    /// Check drawdown limit
    pub fn check_drawdown(&self, daily_total_pnl: i64, peak_pnl: i64) -> Result<()> {
        // Drawdown is peak - current.
        let drawdown = peak_pnl.saturating_sub(daily_total_pnl);
        
        if peak_pnl > 0 && self.limits.max_drawdown > 0 {
             let allowed_drawdown = ((peak_pnl as i128) * self.limits.max_drawdown as i128 / fixed_point::SCALE as i128)
                .clamp(i64::MIN as i128, i64::MAX as i128)
                as i64;

            if allowed_drawdown > 0 && drawdown > allowed_drawdown {
                error!(
                    "Drawdown limit exceeded: drawdown={} > limit={}",
                    drawdown,
                    allowed_drawdown
                );
                return Err(anyhow!(RiskViolation::DrawdownLimitBreached {
                    drawdown,
                    limit: allowed_drawdown
                }));
            }
        }
        Ok(())
    }

    /// Validate a signal before execution
    ///
    /// Checks:
    /// - Kill Switch
    /// - Order size limits
    /// - Position limits (including open exposure)
    /// - Outstanding order counts
    /// - Price Tick
    /// - Price Distance from Mid
    pub fn validate_signal(
        &self, 
        signal: &Signal, 
        position: &Position,
        open_long_exposure: i64,
        open_short_exposure: i64,
        mid_price: u64
    ) -> Result<()> {
        // No validation needed for cancel or no action
        if !signal.requires_action() || matches!(signal.action, SignalAction::CancelAll) {
            return Ok(());
        }

        // 1. Check kill switch
        if let Some(ref ks) = self.kill_switch {
            if ks.should_stop() {
                debug!("Risk check: Kill switch active");
                return Err(anyhow!(RiskViolation::KillSwitchActive));
            }
            if ks.is_paused() {
                debug!("Risk check: Trading paused");
                return Err(anyhow!(RiskViolation::TradingPaused));
            }
        }

        let size_i64 = signal.size as i64;
        // Check order size limits
        if size_i64 < self.limits.min_order_size {
             return Err(anyhow!(RiskViolation::OrderSizeTooSmall {
                size: size_i64,
                min: self.limits.min_order_size
            }));
        }

        if size_i64 > self.limits.max_order_size {
            return Err(anyhow!(RiskViolation::OrderSizeTooLarge {
                size: size_i64,
                max: self.limits.max_order_size
            }));
        }

        // Check Price Sanity (Tick and Distance)
        match signal.action {
            SignalAction::QuoteBoth => {
                self.validate_price(signal.bid_price, mid_price)?;
                self.validate_price(signal.ask_price, mid_price)?;
            },
            SignalAction::QuoteBid => {
                self.validate_price(signal.bid_price, mid_price)?;
            },
            SignalAction::QuoteAsk => {
                self.validate_price(signal.ask_price, mid_price)?;
            },
            // TakePosition usually implies market order or aggressive limit, 
            // if prices are 0 we skip price validation.
            SignalAction::TakePosition => {
                if signal.bid_price > 0 {
                     self.validate_price(signal.bid_price, mid_price)?;
                }
                if signal.ask_price > 0 {
                     self.validate_price(signal.ask_price, mid_price)?;
                }
            }
            _ => {}
        }

        // Calculate projected position including open exposure
        let current_qty = position.get_quantity();
        
        let projected_position_check = match signal.action {
            SignalAction::QuoteBoth => {
                // Check Long side: Current + Open Long + New Buy
                let long_proj = current_qty
                    .saturating_add(open_long_exposure)
                    .saturating_add(size_i64);
                
                // Check Short side: Current - Open Short - New Sell
                // Note: short exposure is positive magnitude, so subtract it
                let short_proj = current_qty
                    .saturating_sub(open_short_exposure)
                    .saturating_sub(size_i64);
                
                if long_proj > self.limits.max_position {
                     return Err(anyhow!(RiskViolation::PositionLimitExceeded {
                        projected: long_proj,
                        limit: self.limits.max_position
                    }));
                }
                if short_proj < -self.limits.max_short {
                    return Err(anyhow!(RiskViolation::ShortLimitExceeded {
                        projected: short_proj,
                        limit: self.limits.max_short
                    }));
                }
                return Ok(());
            },
            SignalAction::QuoteBid | SignalAction::TakePosition if matches!(signal.side, CoreSide::Buy) => {
                // Buying: Current + Open Long + New Buy
                current_qty
                    .saturating_add(open_long_exposure)
                    .saturating_add(size_i64)
            },
            SignalAction::QuoteAsk | SignalAction::TakePosition if matches!(signal.side, CoreSide::Sell) => {
                 // Selling: Current - Open Short - New Sell
                 current_qty
                    .saturating_sub(open_short_exposure)
                    .saturating_sub(size_i64)
            },
            SignalAction::TakePosition => {
                // Handle default case if side logic above missed something (unlikely)
                 match signal.side {
                    CoreSide::Buy => current_qty.saturating_add(open_long_exposure).saturating_add(size_i64),
                    CoreSide::Sell => current_qty.saturating_sub(open_short_exposure).saturating_sub(size_i64),
                }
            }
            _ => current_qty,
        };

        if projected_position_check > self.limits.max_position {
            return Err(anyhow!(RiskViolation::PositionLimitExceeded {
                projected: projected_position_check,
                limit: self.limits.max_position
            }));
        }

        if projected_position_check < -self.limits.max_short {
            return Err(anyhow!(RiskViolation::ShortLimitExceeded {
                projected: projected_position_check,
                limit: self.limits.max_short
            }));
        }

        // Check outstanding order count
        // If we are adding orders, we need to check.
        // Signal produces 1 or 2 orders.
        let new_orders = match signal.action {
            SignalAction::QuoteBoth => 2,
            _ => 1,
        };
        
        let new_count = self.outstanding_order_count + new_orders;
        if new_count > self.limits.max_outstanding_orders {
             return Err(anyhow!(RiskViolation::TooManyOutstandingOrders {
                current: new_count,
                max: self.limits.max_outstanding_orders
            }));
        }

        Ok(())
    }

    /// Helper to validate a single price
    fn validate_price(&self, price: u64, mid_price: u64) -> Result<()> {
        if price == 0 {
            return Ok(());
        }

        // Check tick size
        if self.limits.tick_size > 0 && price % self.limits.tick_size != 0 {
             warn!(
                "Risk check: Price {} not on tick size {}",
                price, self.limits.tick_size
            );
            return Err(anyhow!(RiskViolation::InvalidTick {
                price,
                tick_size: self.limits.tick_size
            }));
        }

        // Check distance from mid
        if mid_price > 0 {
            let distance = if price > mid_price {
                price - mid_price
            } else {
                mid_price - price
            };

            // Calculate distance in BPS using u128 to prevent overflow
            // bps = (distance * 10000) / mid_price
            let distance_bps = (distance as u128 * 10_000) / mid_price as u128;
            let distance_bps = distance_bps as u32;

            if distance_bps > self.limits.max_price_distance_bps {
                warn!(
                    "Risk check: Price too far from mid ({}bps > {}bps)",
                    distance_bps, self.limits.max_price_distance_bps
                );
                return Err(anyhow!(RiskViolation::PriceTooFarFromMid {
                    price,
                    mid: mid_price,
                    max_distance_bps: self.limits.max_price_distance_bps
                }));
            }
        }

        Ok(())
    }

    /// Increment outstanding order count
    pub fn increment_order_count(&mut self, count: usize) {
        self.outstanding_order_count += count;
    }

    /// Decrement outstanding order count
    pub fn decrement_order_count(&mut self, count: usize) {
        self.outstanding_order_count = self.outstanding_order_count.saturating_sub(count);
    }

    /// Get risk limits
    pub fn limits(&self) -> &RiskLimits {
        &self.limits
    }
}

impl Default for RiskManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Position;

    fn create_test_limits() -> RiskLimits {
        RiskLimits {
            max_position: 1_000_000_000, // 1 BTC
            max_short: 1_000_000_000, // 1 BTC
            max_order_size: 500_000_000, // 0.5 BTC
            min_order_size: 1_000_000, // 0.001 BTC
            max_outstanding_orders: 10,
            max_daily_loss: 5_000 * 1_000_000_000, // 5000 USD
            max_drawdown: 50_000_000, // 5%
            tick_size: TICK_SIZE, // Use centralized constant
            max_price_distance_bps: 50, // 0.5% from mid
        }
    }

    // Helper to create price values (9 decimals)
    fn price(amount: u64) -> u64 {
        amount * 1_000_000_000
    }

    #[test]
    fn test_validate_signal_limits() {
        let limits = create_test_limits();
        let rm = RiskManager::with_limits(limits);
        let pos = Position::new();
        let mid = price(50000);

        // Valid signal
        let signal = Signal::quote_bid(price(50000), 100_000_000); // 0.1 BTC
        assert!(rm.validate_signal(&signal, &pos, 0, 0, mid).is_ok());

        // Too large
        let signal = Signal::quote_bid(price(50000), 600_000_000); // 0.6 BTC
        assert!(rm.validate_signal(&signal, &pos, 0, 0, mid).is_err());
        
        // Too small
        let signal = Signal::quote_bid(price(50000), 100); // Tiny
        assert!(rm.validate_signal(&signal, &pos, 0, 0, mid).is_err());
    }
    
    #[test]
    fn test_position_limits() {
        let limits = create_test_limits();
        let rm = RiskManager::with_limits(limits);
        let pos = Position::new();
        let mid = price(50000);
        
        // Already have 0.9 BTC
        pos.update_quantity(900_000_000);
        
        // Buy 0.2 BTC -> 1.1 BTC > 1.0 BTC Limit
        let signal = Signal::quote_bid(price(50000), 200_000_000);
        assert!(rm.validate_signal(&signal, &pos, 0, 0, mid).is_err());
        
        // Sell 0.2 BTC -> 0.7 BTC (OK)
        let signal = Signal::quote_ask(price(50000), 200_000_000);
        assert!(rm.validate_signal(&signal, &pos, 0, 0, mid).is_ok());
    }
    
    #[test]
    fn test_daily_loss_check() {
        let limits = create_test_limits();
        let rm = RiskManager::with_limits(limits);
        
        // Loss within limit
        assert!(rm.check_daily_loss(-1000 * 1_000_000_000).is_ok());
        
        // Loss exceeds limit
        assert!(rm.check_daily_loss(-6000 * 1_000_000_000).is_err());
    }

    #[test]
    fn test_daily_reset() {
        let limits = create_test_limits();
        let mut rm = RiskManager::with_limits(limits);
        let pos = Position::new();
        
        pos.update_daily_pnl(100);
        
        // Force reset by setting last reset to past
        rm.daily_reset_timestamp = 0;
        
        rm.check_daily_reset(&pos);
        
        assert_eq!(pos.get_daily_pnl(), 0);
    }

    #[test]
    fn test_price_validation() {
        let limits = create_test_limits();
        let rm = RiskManager::with_limits(limits);
        let pos = Position::new();
        let mid = price(50000);

        // Invalid tick
        let signal = Signal::quote_bid(price(50000) + 1, 100_000_000);
        assert!(rm.validate_signal(&signal, &pos, 0, 0, mid).is_err());

        // Price too far
        let signal = Signal::quote_bid(price(55000), 100_000_000); // 10% away
        assert!(rm.validate_signal(&signal, &pos, 0, 0, mid).is_err());
    }

    #[test]
    fn test_kill_switch() {
        let limits = create_test_limits();
        let ks = KillSwitch::new();
        let rm = RiskManager::with_kill_switch(limits, ks.clone());
        let pos = Position::new();
        let mid = price(50000);

        // Normal
        let signal = Signal::quote_bid(price(50000), 100_000_000);
        assert!(rm.validate_signal(&signal, &pos, 0, 0, mid).is_ok());

        // Trip kill switch
        ks.shutdown("test");
        assert!(rm.validate_signal(&signal, &pos, 0, 0, mid).is_err());
    }
}
