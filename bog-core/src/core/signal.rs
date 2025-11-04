//! Zero-overhead trading signals
//!
//! Signals are 64-byte stack-allocated structs that fit exactly in one cache line.
//! No heap allocations, no dynamic dispatch, no enum variants.

use super::types::Side;
use std::fmt;

/// Action to take based on strategy signal
///
/// Single byte enum to minimize size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SignalAction {
    /// No action required
    NoAction = 0,
    /// Place quotes on both sides
    QuoteBoth = 1,
    /// Place bid quote only
    QuoteBid = 2,
    /// Place ask quote only
    QuoteAsk = 3,
    /// Cancel all quotes
    CancelAll = 4,
    /// Take a market position (aggressive)
    TakePosition = 5,
}

/// Trading signal - exactly 64 bytes, cache-line aligned
///
/// All prices and sizes use fixed-point arithmetic (9 decimal places).
/// Padding ensures the struct is exactly 64 bytes to fit in one cache line.
#[derive(Clone, Copy)]
#[repr(C, align(64))]
pub struct Signal {
    /// Action to take
    pub action: SignalAction,

    /// Side for TakePosition action
    pub side: Side,

    /// Bid price (fixed-point, 9 decimals)
    pub bid_price: u64,

    /// Ask price (fixed-point, 9 decimals)
    pub ask_price: u64,

    /// Order size (fixed-point, 9 decimals)
    pub size: u64,

    /// Reserved for future use
    _reserved: [u8; 2],

    /// Padding to exactly 64 bytes
    _padding: [u8; 32],
}

impl Signal {
    /// Create a no-action signal
    #[inline(always)]
    pub const fn no_action() -> Self {
        Self {
            action: SignalAction::NoAction,
            side: Side::Buy,
            bid_price: 0,
            ask_price: 0,
            size: 0,
            _reserved: [0; 2],
            _padding: [0; 32],
        }
    }

    /// Create a quote-both signal
    #[inline(always)]
    pub const fn quote_both(bid_price: u64, ask_price: u64, size: u64) -> Self {
        Self {
            action: SignalAction::QuoteBoth,
            side: Side::Buy,
            bid_price,
            ask_price,
            size,
            _reserved: [0; 2],
            _padding: [0; 32],
        }
    }

    /// Create a quote-bid signal
    #[inline(always)]
    pub const fn quote_bid(price: u64, size: u64) -> Self {
        Self {
            action: SignalAction::QuoteBid,
            side: Side::Buy,
            bid_price: price,
            ask_price: 0,
            size,
            _reserved: [0; 2],
            _padding: [0; 32],
        }
    }

    /// Create a quote-ask signal
    #[inline(always)]
    pub const fn quote_ask(price: u64, size: u64) -> Self {
        Self {
            action: SignalAction::QuoteAsk,
            side: Side::Sell,
            bid_price: 0,
            ask_price: price,
            size,
            _reserved: [0; 2],
            _padding: [0; 32],
        }
    }

    /// Create a cancel-all signal
    #[inline(always)]
    pub const fn cancel_all() -> Self {
        Self {
            action: SignalAction::CancelAll,
            side: Side::Buy,
            bid_price: 0,
            ask_price: 0,
            size: 0,
            _reserved: [0; 2],
            _padding: [0; 32],
        }
    }

    /// Create a take-position signal
    #[inline(always)]
    pub const fn take_position(side: Side, size: u64) -> Self {
        Self {
            action: SignalAction::TakePosition,
            side,
            bid_price: 0,
            ask_price: 0,
            size,
            _reserved: [0; 2],
            _padding: [0; 32],
        }
    }

    /// Check if signal requires action
    #[inline(always)]
    pub const fn requires_action(&self) -> bool {
        !matches!(self.action, SignalAction::NoAction)
    }

    /// Get net position change from this signal
    ///
    /// Market making orders are neutral (assuming both sides fill equally).
    /// TakePosition signals have actual position impact.
    #[inline(always)]
    pub const fn net_position_change(&self) -> i64 {
        match self.action {
            SignalAction::TakePosition => match self.side {
                Side::Buy => self.size as i64,
                Side::Sell => -(self.size as i64),
            },
            _ => 0,
        }
    }

    /// Get total order size from this signal
    #[inline(always)]
    pub const fn total_size(&self) -> u64 {
        match self.action {
            SignalAction::QuoteBoth => self.size * 2, // Both sides
            SignalAction::QuoteBid | SignalAction::QuoteAsk | SignalAction::TakePosition => {
                self.size
            }
            SignalAction::CancelAll | SignalAction::NoAction => 0,
        }
    }
}

impl Default for Signal {
    #[inline(always)]
    fn default() -> Self {
        Self::no_action()
    }
}

impl fmt::Debug for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.action {
            SignalAction::NoAction => write!(f, "Signal::NoAction"),
            SignalAction::QuoteBoth => write!(
                f,
                "Signal::QuoteBoth {{ bid: {}, ask: {}, size: {} }}",
                self.bid_price, self.ask_price, self.size
            ),
            SignalAction::QuoteBid => {
                write!(f, "Signal::QuoteBid {{ price: {}, size: {} }}", self.bid_price, self.size)
            }
            SignalAction::QuoteAsk => {
                write!(f, "Signal::QuoteAsk {{ price: {}, size: {} }}", self.ask_price, self.size)
            }
            SignalAction::CancelAll => write!(f, "Signal::CancelAll"),
            SignalAction::TakePosition => write!(
                f,
                "Signal::TakePosition {{ side: {:?}, size: {} }}",
                self.side, self.size
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_size() {
        // Verify signal is exactly 64 bytes (one cache line)
        assert_eq!(std::mem::size_of::<Signal>(), 64);
        assert_eq!(std::mem::align_of::<Signal>(), 64);
    }

    #[test]
    fn test_signal_no_action() {
        let signal = Signal::no_action();
        assert_eq!(signal.action, SignalAction::NoAction);
        assert!(!signal.requires_action());
        assert_eq!(signal.net_position_change(), 0);
        assert_eq!(signal.total_size(), 0);
    }

    #[test]
    fn test_signal_quote_both() {
        let signal = Signal::quote_both(50_000_000_000_000, 50_005_000_000_000, 100_000_000);
        assert_eq!(signal.action, SignalAction::QuoteBoth);
        assert!(signal.requires_action());
        assert_eq!(signal.bid_price, 50_000_000_000_000);
        assert_eq!(signal.ask_price, 50_005_000_000_000);
        assert_eq!(signal.size, 100_000_000);
        assert_eq!(signal.net_position_change(), 0); // Market making is neutral
        assert_eq!(signal.total_size(), 200_000_000); // Both sides
    }

    #[test]
    fn test_signal_quote_bid() {
        let signal = Signal::quote_bid(50_000_000_000_000, 100_000_000);
        assert_eq!(signal.action, SignalAction::QuoteBid);
        assert!(signal.requires_action());
        assert_eq!(signal.bid_price, 50_000_000_000_000);
        assert_eq!(signal.total_size(), 100_000_000);
    }

    #[test]
    fn test_signal_quote_ask() {
        let signal = Signal::quote_ask(50_005_000_000_000, 100_000_000);
        assert_eq!(signal.action, SignalAction::QuoteAsk);
        assert!(signal.requires_action());
        assert_eq!(signal.ask_price, 50_005_000_000_000);
        assert_eq!(signal.total_size(), 100_000_000);
    }

    #[test]
    fn test_signal_cancel_all() {
        let signal = Signal::cancel_all();
        assert_eq!(signal.action, SignalAction::CancelAll);
        assert!(signal.requires_action());
        assert_eq!(signal.total_size(), 0);
    }

    #[test]
    fn test_signal_take_position_buy() {
        let signal = Signal::take_position(Side::Buy, 500_000_000);
        assert_eq!(signal.action, SignalAction::TakePosition);
        assert_eq!(signal.side, Side::Buy);
        assert!(signal.requires_action());
        assert_eq!(signal.net_position_change(), 500_000_000);
        assert_eq!(signal.total_size(), 500_000_000);
    }

    #[test]
    fn test_signal_take_position_sell() {
        let signal = Signal::take_position(Side::Sell, 300_000_000);
        assert_eq!(signal.action, SignalAction::TakePosition);
        assert_eq!(signal.side, Side::Sell);
        assert!(signal.requires_action());
        assert_eq!(signal.net_position_change(), -300_000_000);
        assert_eq!(signal.total_size(), 300_000_000);
    }

    #[test]
    fn test_signal_is_copy() {
        let signal1 = Signal::quote_both(50_000_000_000_000, 50_005_000_000_000, 100_000_000);
        let signal2 = signal1; // Should be a copy, not a move

        // Both should be usable
        assert_eq!(signal1.bid_price, signal2.bid_price);
        assert_eq!(signal1.ask_price, signal2.ask_price);
    }

    #[test]
    fn test_signal_default() {
        let signal = Signal::default();
        assert_eq!(signal.action, SignalAction::NoAction);
        assert!(!signal.requires_action());
    }
}
