pub mod types;

pub use types::{Position, RiskLimits, RiskViolation};

// Removed: moving to compile-time config
// use crate::config::RiskConfig;
use crate::execution::{Fill, Side};
use crate::strategy::Signal;
use anyhow::{anyhow, Result};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

/// Risk manager - validates signals and tracks positions
pub struct RiskManager {
    position: Position,
    limits: RiskLimits,
    daily_reset_timestamp: u64,
    outstanding_order_count: usize,
}

impl RiskManager {
    // TODO: Remove runtime config, use const generics
    // pub fn new(config: &RiskConfig) -> Self { ... }

    /// Create with explicit limits (for testing or const-based initialization)
    pub fn with_limits(limits: RiskLimits) -> Self {
        info!("Initialized RiskManager with limits: {:?}", limits);

        Self {
            position: Position::default(),
            limits,
            daily_reset_timestamp: Self::get_day_start_timestamp(),
            outstanding_order_count: 0,
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
    fn check_daily_reset(&mut self) {
        let current_day_start = Self::get_day_start_timestamp();

        if current_day_start > self.daily_reset_timestamp {
            info!(
                "New trading day detected, resetting daily PnL (was: {})",
                self.position.daily_pnl
            );
            self.position.daily_pnl = Decimal::ZERO;
            self.position.daily_high_water_mark = self.position.total_pnl();
            self.daily_reset_timestamp = current_day_start;
        }
    }

    /// Validate a signal before execution
    pub fn validate_signal(&self, signal: &Signal) -> Result<()> {
        // No validation needed for cancel or no action
        if matches!(signal, Signal::CancelAll | Signal::NoAction) {
            return Ok(());
        }

        // Get orders from signal
        let orders = signal.to_orders();

        for order in &orders {
            // Check order size limits
            if order.size < self.limits.min_order_size {
                return Err(anyhow!(
                    RiskViolation::OrderSizeTooSmall {
                        size: order.size,
                        min: self.limits.min_order_size
                    }
                    .to_string()
                ));
            }

            if order.size > self.limits.max_order_size {
                return Err(anyhow!(
                    RiskViolation::OrderSizeTooLarge {
                        size: order.size,
                        max: self.limits.max_order_size
                    }
                    .to_string()
                ));
            }

            // Check position limits (projected position after order fills)
            let projected_position = match order.side {
                Side::Buy => self.position.quantity + order.size,
                Side::Sell => self.position.quantity - order.size,
            };

            if projected_position > self.limits.max_position {
                return Err(anyhow!(
                    RiskViolation::PositionLimitExceeded {
                        projected: projected_position,
                        limit: self.limits.max_position
                    }
                    .to_string()
                ));
            }

            if projected_position < -self.limits.max_short {
                return Err(anyhow!(
                    RiskViolation::ShortLimitExceeded {
                        projected: projected_position,
                        limit: self.limits.max_short
                    }
                    .to_string()
                ));
            }
        }

        // Check outstanding order count
        let new_order_count = self.outstanding_order_count + orders.len();
        if new_order_count > self.limits.max_outstanding_orders {
            return Err(anyhow!(
                RiskViolation::TooManyOutstandingOrders {
                    current: new_order_count,
                    max: self.limits.max_outstanding_orders
                }
                .to_string()
            ));
        }

        // Check daily loss limit
        if self.position.daily_pnl < -self.limits.max_daily_loss {
            error!(
                "Daily loss limit breached: {} < -{}",
                self.position.daily_pnl, self.limits.max_daily_loss
            );
            return Err(anyhow!(
                RiskViolation::DailyLossLimitBreached {
                    daily_pnl: self.position.daily_pnl,
                    limit: self.limits.max_daily_loss
                }
                .to_string()
            ));
        }

        // Check drawdown
        let current_pnl = self.position.total_pnl();
        let drawdown = self.position.daily_high_water_mark - current_pnl;
        let drawdown_pct = if self.position.daily_high_water_mark > Decimal::ZERO {
            (drawdown / self.position.daily_high_water_mark)
                .to_f64()
                .unwrap_or(0.0)
        } else {
            0.0
        };

        if drawdown_pct > self.limits.max_drawdown_pct {
            error!(
                "Drawdown limit breached: {:.2}% > {:.2}%",
                drawdown_pct * 100.0,
                self.limits.max_drawdown_pct * 100.0
            );
            return Err(anyhow!(
                RiskViolation::DrawdownLimitBreached {
                    drawdown_pct,
                    limit: self.limits.max_drawdown_pct
                }
                .to_string()
            ));
        }

        Ok(())
    }

    /// Update position from a fill
    pub fn update_position(&mut self, fill: &Fill) {
        self.check_daily_reset();

        // Update position
        let old_quantity = self.position.quantity;
        self.position.quantity += fill.position_change();

        // Calculate PnL
        let pnl = match fill.side {
            Side::Buy => {
                // Buying increases position, costs cash
                if old_quantity < Decimal::ZERO {
                    // Covering a short - realize PnL
                    let cover_size = fill.size.min(-old_quantity);
                    let avg_short_price = if old_quantity != Decimal::ZERO {
                        -self.position.cost_basis / old_quantity
                    } else {
                        fill.price
                    };
                    (avg_short_price - fill.price) * cover_size
                } else {
                    Decimal::ZERO // Adding to long position
                }
            }
            Side::Sell => {
                // Selling decreases position, adds cash
                if old_quantity > Decimal::ZERO {
                    // Selling a long - realize PnL
                    let sell_size = fill.size.min(old_quantity);
                    let avg_long_price = if old_quantity != Decimal::ZERO {
                        self.position.cost_basis / old_quantity
                    } else {
                        fill.price
                    };
                    (fill.price - avg_long_price) * sell_size
                } else {
                    Decimal::ZERO // Adding to short position
                }
            }
        };

        self.position.realized_pnl += pnl;
        self.position.daily_pnl += pnl;

        // Update cost basis
        self.position.cost_basis += fill.cash_flow();

        // Update high water mark
        let total_pnl = self.position.total_pnl();
        if total_pnl > self.position.daily_high_water_mark {
            self.position.daily_high_water_mark = total_pnl;
        }

        // Update trade count
        self.position.trade_count += 1;

        info!(
            "Position updated: quantity={}, cost_basis={}, realized_pnl={}, daily_pnl={}",
            self.position.quantity,
            self.position.cost_basis,
            self.position.realized_pnl,
            self.position.daily_pnl
        );

        if pnl != Decimal::ZERO {
            info!("Realized PnL from fill: {}", pnl);
        }
    }

    /// Increment outstanding order count
    pub fn increment_order_count(&mut self) {
        self.outstanding_order_count += 1;
    }

    /// Decrement outstanding order count
    pub fn decrement_order_count(&mut self) {
        if self.outstanding_order_count > 0 {
            self.outstanding_order_count -= 1;
        }
    }

    /// Get current position
    pub fn position(&self) -> &Position {
        &self.position
    }

    /// Get risk limits
    pub fn limits(&self) -> &RiskLimits {
        &self.limits
    }

    /// Check if trading should be halted
    pub fn should_halt_trading(&self) -> bool {
        // Check daily loss
        if self.position.daily_pnl < -self.limits.max_daily_loss {
            return true;
        }

        // Check drawdown
        let current_pnl = self.position.total_pnl();
        let drawdown = self.position.daily_high_water_mark - current_pnl;
        let drawdown_pct = if self.position.daily_high_water_mark > Decimal::ZERO {
            (drawdown / self.position.daily_high_water_mark)
                .to_f64()
                .unwrap_or(0.0)
        } else {
            0.0
        };

        drawdown_pct > self.limits.max_drawdown_pct
    }

    /// Log risk status
    pub fn log_status(&self) {
        info!("=== Risk Status ===");
        info!("Position: {}", self.position.quantity);
        info!("Cost Basis: {}", self.position.cost_basis);
        info!("Realized PnL: {}", self.position.realized_pnl);
        info!("Daily PnL: {}", self.position.daily_pnl);
        info!("Trade Count: {}", self.position.trade_count);
        info!("Outstanding Orders: {}", self.outstanding_order_count);

        let position_util = if self.position.quantity > Decimal::ZERO {
            (self.position.quantity / self.limits.max_position * Decimal::from(100))
                .to_f64()
                .unwrap_or(0.0)
        } else {
            (-self.position.quantity / self.limits.max_short * Decimal::from(100))
                .to_f64()
                .unwrap_or(0.0)
        };

        info!("Position Utilization: {:.1}%", position_util);

        let daily_loss_util = if self.limits.max_daily_loss > Decimal::ZERO {
            (-self.position.daily_pnl / self.limits.max_daily_loss * Decimal::from(100))
                .to_f64()
                .unwrap_or(0.0)
        } else {
            0.0
        };

        info!("Daily Loss Utilization: {:.1}%", daily_loss_util);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::OrderId;
    use rust_decimal_macros::dec;

    fn create_test_config() -> RiskConfig {
        RiskConfig {
            max_position: dec!(1.0),
            max_short: dec!(1.0),
            max_order_size: dec!(0.5),
            min_order_size: dec!(0.01),
            max_outstanding_orders: 10,
            max_daily_loss: dec!(1000.0),
            max_drawdown_pct: 0.20,
        }
    }

    #[test]
    fn test_risk_manager_creation() {
        let config = create_test_config();
        let rm = RiskManager::new(&config);

        assert_eq!(rm.position().quantity, Decimal::ZERO);
        assert_eq!(rm.limits().max_position, dec!(1.0));
    }

    #[test]
    fn test_validate_signal_order_size() {
        let config = create_test_config();
        let rm = RiskManager::new(&config);

        // Too small
        let signal = Signal::QuoteBid {
            price: dec!(50000),
            size: dec!(0.001),
        };
        assert!(rm.validate_signal(&signal).is_err());

        // Too large
        let signal = Signal::QuoteBid {
            price: dec!(50000),
            size: dec!(1.0),
        };
        assert!(rm.validate_signal(&signal).is_err());

        // Valid
        let signal = Signal::QuoteBid {
            price: dec!(50000),
            size: dec!(0.1),
        };
        assert!(rm.validate_signal(&signal).is_ok());
    }

    #[test]
    fn test_position_limits() {
        let config = create_test_config();
        let rm = RiskManager::new(&config);

        // Exceeds max long position
        let signal = Signal::TakePosition {
            side: Side::Buy,
            size: dec!(1.5),
        };
        assert!(rm.validate_signal(&signal).is_err());

        // Exceeds max short position
        let signal = Signal::TakePosition {
            side: Side::Sell,
            size: dec!(1.5),
        };
        assert!(rm.validate_signal(&signal).is_err());
    }

    #[test]
    fn test_update_position() {
        let config = create_test_config();
        let mut rm = RiskManager::new(&config);

        // Buy
        let fill = Fill::new(
            OrderId::new_random(),
            Side::Buy,
            dec!(50000),
            dec!(0.1),
        );
        rm.update_position(&fill);

        assert_eq!(rm.position().quantity, dec!(0.1));
        assert_eq!(rm.position().trade_count, 1);

        // Sell
        let fill = Fill::new(
            OrderId::new_random(),
            Side::Sell,
            dec!(50100),
            dec!(0.1),
        );
        rm.update_position(&fill);

        assert_eq!(rm.position().quantity, Decimal::ZERO);
        assert_eq!(rm.position().trade_count, 2);

        // Should have positive PnL (bought at 50000, sold at 50100)
        assert!(rm.position().realized_pnl > Decimal::ZERO);
    }

    #[test]
    fn test_no_action_validation() {
        let config = create_test_config();
        let rm = RiskManager::new(&config);

        // NoAction and CancelAll should always be valid
        assert!(rm.validate_signal(&Signal::NoAction).is_ok());
        assert!(rm.validate_signal(&Signal::CancelAll).is_ok());
    }
}
