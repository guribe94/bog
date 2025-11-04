use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Current position state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Current position quantity (positive = long, negative = short)
    pub quantity: Decimal,

    /// Cost basis (total cash invested)
    pub cost_basis: Decimal,

    /// Realized PnL (from closed positions)
    pub realized_pnl: Decimal,

    /// Daily PnL (resets at start of day)
    pub daily_pnl: Decimal,

    /// Daily high water mark for drawdown calculation
    pub daily_high_water_mark: Decimal,

    /// Total number of trades executed
    pub trade_count: u64,
}

impl Default for Position {
    fn default() -> Self {
        Self {
            quantity: Decimal::ZERO,
            cost_basis: Decimal::ZERO,
            realized_pnl: Decimal::ZERO,
            daily_pnl: Decimal::ZERO,
            daily_high_water_mark: Decimal::ZERO,
            trade_count: 0,
        }
    }
}

impl Position {
    /// Calculate average entry price for current position
    pub fn avg_entry_price(&self) -> Option<Decimal> {
        if self.quantity != Decimal::ZERO {
            Some(self.cost_basis / self.quantity)
        } else {
            None
        }
    }

    /// Calculate unrealized PnL given current market price
    pub fn unrealized_pnl(&self, market_price: Decimal) -> Decimal {
        if self.quantity != Decimal::ZERO {
            (market_price - self.avg_entry_price().unwrap_or(market_price)) * self.quantity
        } else {
            Decimal::ZERO
        }
    }

    /// Calculate total PnL (realized + unrealized)
    pub fn total_pnl(&self) -> Decimal {
        self.realized_pnl
    }

    /// Check if position is flat (zero)
    pub fn is_flat(&self) -> bool {
        self.quantity == Decimal::ZERO
    }

    /// Check if position is long
    pub fn is_long(&self) -> bool {
        self.quantity > Decimal::ZERO
    }

    /// Check if position is short
    pub fn is_short(&self) -> bool {
        self.quantity < Decimal::ZERO
    }
}

/// Risk limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimits {
    /// Maximum long position size
    pub max_position: Decimal,

    /// Maximum short position size (positive value)
    pub max_short: Decimal,

    /// Maximum size per order
    pub max_order_size: Decimal,

    /// Minimum size per order
    pub min_order_size: Decimal,

    /// Maximum number of outstanding orders
    pub max_outstanding_orders: usize,

    /// Maximum daily loss (circuit breaker)
    pub max_daily_loss: Decimal,

    /// Maximum drawdown percentage (0.0 to 1.0)
    pub max_drawdown_pct: f64,
}

/// Risk violation types
#[derive(Debug, Clone)]
pub enum RiskViolation {
    OrderSizeTooSmall {
        size: Decimal,
        min: Decimal,
    },
    OrderSizeTooLarge {
        size: Decimal,
        max: Decimal,
    },
    PositionLimitExceeded {
        projected: Decimal,
        limit: Decimal,
    },
    ShortLimitExceeded {
        projected: Decimal,
        limit: Decimal,
    },
    TooManyOutstandingOrders {
        current: usize,
        max: usize,
    },
    DailyLossLimitBreached {
        daily_pnl: Decimal,
        limit: Decimal,
    },
    DrawdownLimitBreached {
        drawdown_pct: f64,
        limit: f64,
    },
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
                write!(
                    f,
                    "Daily PnL {} breaches loss limit {}",
                    daily_pnl, limit
                )
            }
            RiskViolation::DrawdownLimitBreached {
                drawdown_pct,
                limit,
            } => {
                write!(
                    f,
                    "Drawdown {:.2}% breaches limit {:.2}%",
                    drawdown_pct * 100.0,
                    limit * 100.0
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_position_default() {
        let pos = Position::default();
        assert_eq!(pos.quantity, Decimal::ZERO);
        assert!(pos.is_flat());
        assert!(!pos.is_long());
        assert!(!pos.is_short());
    }

    #[test]
    fn test_position_states() {
        let mut pos = Position::default();

        pos.quantity = dec!(0.5);
        assert!(pos.is_long());
        assert!(!pos.is_flat());
        assert!(!pos.is_short());

        pos.quantity = dec!(-0.5);
        assert!(pos.is_short());
        assert!(!pos.is_flat());
        assert!(!pos.is_long());
    }

    #[test]
    fn test_avg_entry_price() {
        let mut pos = Position::default();

        pos.quantity = dec!(2.0);
        pos.cost_basis = dec!(100000); // Bought 2 BTC for $100,000

        let avg = pos.avg_entry_price();
        assert_eq!(avg, Some(dec!(50000))); // $50,000 per BTC
    }

    #[test]
    fn test_unrealized_pnl() {
        let mut pos = Position::default();

        pos.quantity = dec!(1.0);
        pos.cost_basis = dec!(50000); // Bought 1 BTC at $50,000

        let unrealized = pos.unrealized_pnl(dec!(51000)); // Price now $51,000
        assert_eq!(unrealized, dec!(1000)); // $1,000 profit
    }

    #[test]
    fn test_risk_violation_display() {
        let violation = RiskViolation::OrderSizeTooSmall {
            size: dec!(0.001),
            min: dec!(0.01),
        };

        let display = format!("{}", violation);
        assert!(display.contains("0.001"));
        assert!(display.contains("0.01"));
    }
}
