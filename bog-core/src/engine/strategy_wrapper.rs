//! Strategy Lifecycle Wrapper - Engine-Level State Machine
//!
//! Wraps any Strategy implementation with lifecycle management (FSM) at the engine level.
//! Strategies remain Zero-Sized Types (ZSTs), while lifecycle state is managed externally.

use crate::core::strategy_fsm::{
    StrategyState, StrategyInitializing
};
use crate::core::{Position, Signal};
use crate::data::MarketSnapshot;
use super::Strategy;

/// Wrapper that adds lifecycle FSM to any Strategy
///
/// # Generic Parameters
/// * `S` - The strategy type (must implement Strategy trait)
///
/// # State Machine
/// ```text
/// INITIALIZING → start() → ACTIVE ↔ pause/resume ↔ PAUSED
///                            ↓                         ↓
///                         stop()                    stop()
///                            ↓                         ↓
///                         STOPPED (terminal)
/// ```
///
/// # Design
/// - Strategy remains ZST (0 bytes)
/// - Lifecycle state stored in wrapper
/// - Signals only generated when Active
/// - Compile-time dispatch (no vtables)
pub struct StrategyWrapper<S: Strategy> {
    /// The wrapped strategy (often 0 bytes)
    strategy: S,

    /// Lifecycle state machine
    lifecycle: StrategyState,
}

impl<S: Strategy> StrategyWrapper<S> {
    /// Create a new wrapper in Initializing state
    pub fn new(strategy: S) -> Self {
        let name = strategy.name().to_string();
        Self {
            strategy,
            lifecycle: StrategyState::Initializing(StrategyInitializing::new(name)),
        }
    }

    /// Start the strategy (Initializing → Active)
    pub fn start(&mut self) {
        // Use temporary value during transition
        let temp_state = StrategyState::Initializing(StrategyInitializing::new(String::new()));
        let old_state = std::mem::replace(&mut self.lifecycle, temp_state);

        self.lifecycle = match old_state {
            StrategyState::Initializing(state) => StrategyState::Active(state.start()),
            other => other,  // No-op if not in Initializing state
        };
    }

    /// Pause the strategy (Active → Paused)
    pub fn pause(&mut self) {
        let temp_state = StrategyState::Initializing(StrategyInitializing::new(String::new()));
        let old_state = std::mem::replace(&mut self.lifecycle, temp_state);

        self.lifecycle = match old_state {
            StrategyState::Active(state) => StrategyState::Paused(state.pause()),
            other => other,
        };
    }

    /// Resume the strategy (Paused → Active)
    pub fn resume(&mut self) {
        let temp_state = StrategyState::Initializing(StrategyInitializing::new(String::new()));
        let old_state = std::mem::replace(&mut self.lifecycle, temp_state);

        self.lifecycle = match old_state {
            StrategyState::Paused(state) => StrategyState::Active(state.resume()),
            other => other,
        };
    }

    /// Stop the strategy (any state → Stopped)
    pub fn stop(&mut self) {
        let temp_state = StrategyState::Initializing(StrategyInitializing::new(String::new()));
        let old_state = std::mem::replace(&mut self.lifecycle, temp_state);

        self.lifecycle = match old_state {
            StrategyState::Initializing(state) => StrategyState::Stopped(state.stop()),
            StrategyState::Active(state) => StrategyState::Stopped(state.stop()),
            StrategyState::Paused(state) => StrategyState::Stopped(state.stop()),
            StrategyState::Stopped(state) => StrategyState::Stopped(state),
        };
    }

    /// Calculate signal (only if Active)
    ///
    /// # Returns
    /// * `Some(signal)` - If strategy is Active and generates signal
    /// * `None` - If strategy is not Active or no signal generated
    #[inline(always)]
    pub fn calculate(&mut self, snapshot: &MarketSnapshot, position: &Position) -> Option<Signal> {
        // Only generate signals if Active
        match &self.lifecycle {
            StrategyState::Active(_) => self.strategy.calculate(snapshot, position),
            _ => None,
        }
    }

    /// Check if strategy is operational (Active state)
    pub fn is_operational(&self) -> bool {
        match &self.lifecycle {
            StrategyState::Active(state) => state.is_operational(),
            _ => false,
        }
    }

    /// Get current lifecycle state
    pub fn state(&self) -> &StrategyState {
        &self.lifecycle
    }

    /// Get strategy name
    pub fn name(&self) -> &str {
        self.strategy.name()
    }

    /// Reset strategy state
    pub fn reset(&mut self) {
        self.strategy.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::SignalAction;

    // Test strategy that always generates signals
    struct AlwaysQuoteStrategy;

    impl Strategy for AlwaysQuoteStrategy {
        fn calculate(&mut self, _snapshot: &MarketSnapshot) -> Option<Signal> {
            Some(Signal::quote_both(
                50_000_000_000_000,
                50_010_000_000_000,
                100_000_000,
            ))
        }

        fn name(&self) -> &'static str {
            "AlwaysQuote"
        }
    }

    fn create_test_snapshot() -> MarketSnapshot {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        MarketSnapshot {
            market_id: 1,
            sequence: 100,
            exchange_timestamp_ns: now,
            local_recv_ns: now,
            local_publish_ns: now,
            best_bid_price: 50_000_000_000_000,
            best_bid_size: 1_000_000_000,
            best_ask_price: 50_010_000_000_000,
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 110],
        }
    }

    #[test]
    fn test_wrapper_blocks_paused_strategy() {
        let strategy = AlwaysQuoteStrategy;
        let mut wrapper = StrategyWrapper::new(strategy);

        // Start and then pause
        wrapper.start();
        wrapper.pause();

        // Wrapper should return None when paused
        let snapshot = create_test_snapshot();
        let signal = wrapper.calculate(&snapshot);

        assert!(signal.is_none(), "Should not generate signals when paused");
        assert!(!wrapper.is_operational(), "Should not be operational when paused");
    }

    #[test]
    fn test_wrapper_allows_active_strategy() {
        let strategy = AlwaysQuoteStrategy;
        let mut wrapper = StrategyWrapper::new(strategy);

        // Start the strategy
        wrapper.start();

        // Wrapper should delegate to strategy when active
        let snapshot = create_test_snapshot();
        let signal = wrapper.calculate(&snapshot);

        assert!(signal.is_some(), "Should generate signals when active");
        assert!(wrapper.is_operational(), "Should be operational when active");

        if let Some(sig) = signal {
            assert_eq!(sig.action, SignalAction::QuoteBoth);
        }
    }

    #[test]
    fn test_wrapper_blocks_initializing_strategy() {
        let strategy = AlwaysQuoteStrategy;
        let mut wrapper = StrategyWrapper::new(strategy);

        // Don't start - should remain in Initializing state
        let snapshot = create_test_snapshot();
        let signal = wrapper.calculate(&snapshot);

        assert!(signal.is_none(), "Should not generate signals when initializing");
        assert!(!wrapper.is_operational(), "Should not be operational when initializing");
    }

    #[test]
    fn test_wrapper_blocks_stopped_strategy() {
        let strategy = AlwaysQuoteStrategy;
        let mut wrapper = StrategyWrapper::new(strategy);

        wrapper.start();
        wrapper.stop();

        let snapshot = create_test_snapshot();
        let signal = wrapper.calculate(&snapshot);

        assert!(signal.is_none(), "Should not generate signals when stopped");
        assert!(!wrapper.is_operational(), "Should not be operational when stopped");
    }

    #[test]
    fn test_wrapper_lifecycle_transitions() {
        let strategy = AlwaysQuoteStrategy;
        let mut wrapper = StrategyWrapper::new(strategy);

        // Initial state: Initializing
        assert!(!wrapper.is_operational());

        // Transition: Initializing → Active
        wrapper.start();
        assert!(wrapper.is_operational());

        // Transition: Active → Paused
        wrapper.pause();
        assert!(!wrapper.is_operational());

        // Transition: Paused → Active
        wrapper.resume();
        assert!(wrapper.is_operational());

        // Transition: Active → Stopped
        wrapper.stop();
        assert!(!wrapper.is_operational());
    }

    #[test]
    fn test_wrapper_pause_resume_cycle() {
        let strategy = AlwaysQuoteStrategy;
        let mut wrapper = StrategyWrapper::new(strategy);
        let snapshot = create_test_snapshot();

        wrapper.start();

        // Should generate signal when active
        assert!(wrapper.calculate(&snapshot).is_some());

        // Pause
        wrapper.pause();
        assert!(wrapper.calculate(&snapshot).is_none());

        // Resume
        wrapper.resume();
        assert!(wrapper.calculate(&snapshot).is_some());

        // Pause again
        wrapper.pause();
        assert!(wrapper.calculate(&snapshot).is_none());
    }

    #[test]
    fn test_wrapper_name_delegated() {
        let strategy = AlwaysQuoteStrategy;
        let wrapper = StrategyWrapper::new(strategy);

        assert_eq!(wrapper.name(), "AlwaysQuote");
    }
}
