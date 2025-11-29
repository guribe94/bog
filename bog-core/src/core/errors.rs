//! Domain-specific error types for HFT core operations
//!
//! These error types provide precise information about failures in critical
//! trading operations, enabling proper error handling and alerting.

use std::fmt;

/// Errors that can occur during arithmetic operations on Position
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverflowError {
    /// Overflow when updating position quantity
    QuantityOverflow {
        /// Current quantity before update
        old: i64,
        /// Delta that would cause overflow
        delta: i64,
    },

    /// Overflow when updating realized PnL
    RealizedPnlOverflow {
        /// Current PnL before update
        old: i64,
        /// Delta that would cause overflow
        delta: i64,
    },

    /// Overflow when updating daily PnL
    DailyPnlOverflow {
        /// Current daily PnL before update
        old: i64,
        /// Delta that would cause overflow
        delta: i64,
    },

    /// Overflow in trade count (rare, after 4 billion trades)
    TradeCountOverflow {
        /// Current count before increment
        old: u32,
    },
}

impl fmt::Display for OverflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OverflowError::QuantityOverflow { old, delta } => {
                write!(
                    f,
                    "Position quantity overflow: {} + {} would exceed i64 limits",
                    old, delta
                )
            }
            OverflowError::RealizedPnlOverflow { old, delta } => {
                write!(
                    f,
                    "Realized PnL overflow: {} + {} would exceed i64 limits",
                    old, delta
                )
            }
            OverflowError::DailyPnlOverflow { old, delta } => {
                write!(
                    f,
                    "Daily PnL overflow: {} + {} would exceed i64 limits",
                    old, delta
                )
            }
            OverflowError::TradeCountOverflow { old } => {
                write!(f, "Trade count overflow: {} trades (limit: u32::MAX)", old)
            }
        }
    }
}

impl std::error::Error for OverflowError {}

/// Errors that can occur during fixed-point conversions
#[derive(Debug, Clone, PartialEq)]
pub enum ConversionError {
    /// Value is too large to represent in fixed-point
    OutOfRange {
        /// The value that couldn't be converted
        value: f64,
    },

    /// Value is NaN (not a valid price)
    NotANumber,

    /// Value is infinite (not a valid price)
    Infinite {
        /// Whether it's positive or negative infinity
        positive: bool,
    },

    /// Precision loss would be too significant
    PrecisionLoss {
        /// Original value
        original: f64,
        /// Converted value
        converted: f64,
        /// Difference in basis points
        error_bps: u64,
    },
}

impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConversionError::OutOfRange { value } => {
                write!(
                    f,
                    "Value {} is out of range for fixed-point representation (max: ~9.2 quadrillion)",
                    value
                )
            }
            ConversionError::NotANumber => {
                write!(f, "Cannot convert NaN to fixed-point")
            }
            ConversionError::Infinite { positive } => {
                write!(
                    f,
                    "Cannot convert {} infinity to fixed-point",
                    if *positive { "positive" } else { "negative" }
                )
            }
            ConversionError::PrecisionLoss {
                original,
                converted,
                error_bps,
            } => {
                write!(
                    f,
                    "Precision loss too high: {} → {} (error: {} bps)",
                    original, converted, error_bps
                )
            }
        }
    }
}

impl std::error::Error for ConversionError {}

/// Errors related to position state management
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionError {
    /// Position state is inconsistent (shouldn't happen)
    InconsistentState {
        /// Description of the inconsistency
        reason: String,
    },

    /// Position is locked for maintenance
    Locked,

    /// Overflow occurred (wraps OverflowError)
    Overflow(OverflowError),
}

impl fmt::Display for PositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PositionError::InconsistentState { reason } => {
                write!(f, "Position state inconsistent: {}", reason)
            }
            PositionError::Locked => {
                write!(f, "Position is locked for maintenance")
            }
            PositionError::Overflow(e) => {
                write!(f, "Position overflow: {}", e)
            }
        }
    }
}

impl std::error::Error for PositionError {}

impl From<OverflowError> for PositionError {
    fn from(e: OverflowError) -> Self {
        PositionError::Overflow(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overflow_error_display() {
        let err = OverflowError::QuantityOverflow {
            old: i64::MAX - 100,
            delta: 200,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("overflow"));
        assert!(msg.contains("i64 limits"));
    }

    #[test]
    fn test_conversion_error_display() {
        let err = ConversionError::OutOfRange { value: 1e20 };
        let msg = format!("{}", err);
        assert!(msg.contains("out of range"));
    }

    #[test]
    fn test_position_error_from_overflow() {
        let overflow = OverflowError::QuantityOverflow {
            old: 100,
            delta: 200,
        };
        let pos_err: PositionError = overflow.into();

        match pos_err {
            PositionError::Overflow(_) => {}
            _ => panic!("Expected Overflow variant"),
        }
    }
}
