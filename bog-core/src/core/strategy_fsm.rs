//! Strategy Lifecycle State Machine - Typestate Pattern
//!
//! Implements compile-time verified state machine for strategy lifecycle.
//!
//! # State Diagram
//!
//! ```text
//!      INITIALIZING
//!           │
//!         start()
//!           ▼
//!        ACTIVE ◄──────► PAUSED
//!           │              │
//!         stop()         stop()
//!           ▼              ▼
//!        STOPPED ───────► STOPPED
//!       (terminal)       (terminal)
//! ```
//!
//! **Key Invariants (enforced at compile time):**
//! - Cannot generate signals in Initializing or Paused state
//! - Cannot resume from Stopped state (terminal)
//! - Cannot pause if not Active
//! - Cannot start if already Active
//!
//! # Usage
//!
//! ```
//! use bog_core::core::strategy_fsm::*;
//!
//! // Create strategy in Initializing state
//! let strategy = StrategyInitializing::new("SimpleSpread".to_string());
//!
//! // Start the strategy (Initializing → Active)
//! let strategy = strategy.start();
//!
//! // Now we can generate signals (only Active has this method)
//! // let signal = strategy.generate_signal(&market_data);
//!
//! // Pause trading
//! let strategy = strategy.pause();
//!
//! // Resume trading
//! let strategy = strategy.resume();
//!
//! // Stop strategy (terminal state)
//! let strategy = strategy.stop();
//! // strategy.resume(); // COMPILE ERROR - Stopped has no resume()
//! ```

use std::time::{Duration, SystemTime};

// ============================================================================
// Strategy Data (shared by all states)
// ============================================================================

/// Core strategy data shared by all states
#[derive(Debug, Clone)]
pub struct StrategyData {
    /// Strategy name/identifier
    pub name: String,
    /// Timestamp when strategy was created
    pub created_at: SystemTime,
    /// Timestamp when strategy started (if applicable)
    pub started_at: Option<SystemTime>,
    /// Timestamp when strategy was paused (if applicable)
    pub paused_at: Option<SystemTime>,
    /// Timestamp when strategy stopped (if applicable)
    pub stopped_at: Option<SystemTime>,
    /// Total runtime (excluding paused time)
    pub total_runtime: Duration,
    /// Number of times paused
    pub pause_count: u64,
}

impl StrategyData {
    pub fn new(name: String) -> Self {
        Self {
            name,
            created_at: SystemTime::now(),
            started_at: None,
            paused_at: None,
            stopped_at: None,
            total_runtime: Duration::ZERO,
            pause_count: 0,
        }
    }
}

// ============================================================================
// State: Initializing
// ============================================================================

/// Strategy in Initializing state
///
/// Strategy has been created but not yet started.
///
/// **Valid Transitions:**
/// - `start()` → StrategyActive
#[derive(Debug, Clone)]
pub struct StrategyInitializing {
    data: StrategyData,
}

impl StrategyInitializing {
    /// Create a new strategy in Initializing state
    pub fn new(name: String) -> Self {
        Self {
            data: StrategyData::new(name),
        }
    }

    /// Access the underlying data
    pub fn data(&self) -> &StrategyData {
        &self.data
    }

    /// Transition: Initializing → Active
    pub fn start(mut self) -> StrategyActive {
        let now = SystemTime::now();
        self.data.started_at = Some(now);
        StrategyActive { data: self.data }
    }

    /// Transition: Initializing → Stopped (abort before starting)
    pub fn stop(mut self) -> StrategyStopped {
        let now = SystemTime::now();
        self.data.stopped_at = Some(now);
        StrategyStopped { data: self.data }
    }
}

// ============================================================================
// State: Active
// ============================================================================

/// Strategy in Active state
///
/// Strategy is running and can generate trading signals.
///
/// **Valid Transitions:**
/// - `pause()` → StrategyPaused
/// - `stop()` → StrategyStopped
#[derive(Debug, Clone)]
pub struct StrategyActive {
    data: StrategyData,
}

impl StrategyActive {
    /// Access the underlying data
    pub fn data(&self) -> &StrategyData {
        &self.data
    }

    /// Transition: Active → Paused
    pub fn pause(mut self) -> StrategyPaused {
        let now = SystemTime::now();
        self.data.paused_at = Some(now);
        self.data.pause_count += 1;

        // Calculate runtime so far
        if let Some(started) = self.data.started_at {
            if let Ok(duration) = now.duration_since(started) {
                self.data.total_runtime += duration;
            }
        }

        StrategyPaused { data: self.data }
    }

    /// Transition: Active → Stopped
    pub fn stop(mut self) -> StrategyStopped {
        let now = SystemTime::now();
        self.data.stopped_at = Some(now);

        // Calculate final runtime
        if let Some(started) = self.data.started_at {
            if let Ok(duration) = now.duration_since(started) {
                self.data.total_runtime += duration;
            }
        }

        StrategyStopped { data: self.data }
    }

    /// Check if strategy is operational (always true for Active)
    pub fn is_operational(&self) -> bool {
        true
    }
}

// ============================================================================
// State: Paused
// ============================================================================

/// Strategy in Paused state
///
/// Strategy is paused and NOT generating trading signals.
///
/// **Valid Transitions:**
/// - `resume()` → StrategyActive
/// - `stop()` → StrategyStopped
#[derive(Debug, Clone)]
pub struct StrategyPaused {
    data: StrategyData,
}

impl StrategyPaused {
    /// Access the underlying data
    pub fn data(&self) -> &StrategyData {
        &self.data
    }

    /// Transition: Paused → Active
    pub fn resume(mut self) -> StrategyActive {
        // Update started_at to mark resume time (for runtime calculation)
        self.data.started_at = Some(SystemTime::now());
        StrategyActive { data: self.data }
    }

    /// Transition: Paused → Stopped
    pub fn stop(mut self) -> StrategyStopped {
        let now = SystemTime::now();
        self.data.stopped_at = Some(now);
        StrategyStopped { data: self.data }
    }

    /// Check if strategy is operational (always false for Paused)
    pub fn is_operational(&self) -> bool {
        false
    }
}

// ============================================================================
// State: Stopped (Terminal)
// ============================================================================

/// Strategy in Stopped state (terminal)
///
/// Strategy has been stopped and cannot be restarted.
/// This is a terminal state.
///
/// **Valid Transitions:** None (terminal state)
#[derive(Debug, Clone)]
pub struct StrategyStopped {
    data: StrategyData,
}

impl StrategyStopped {
    /// Access the underlying data
    pub fn data(&self) -> &StrategyData {
        &self.data
    }

    /// Get total runtime
    pub fn total_runtime(&self) -> Duration {
        self.data.total_runtime
    }

    /// Get number of times paused
    pub fn pause_count(&self) -> u64 {
        self.data.pause_count
    }

    /// Check if strategy is operational (always false for Stopped)
    pub fn is_operational(&self) -> bool {
        false
    }
}

// ============================================================================
// Enum wrapper (for storage/serialization)
// ============================================================================

/// Type-erased strategy state
#[derive(Debug, Clone)]
pub enum StrategyState {
    Initializing(StrategyInitializing),
    Active(StrategyActive),
    Paused(StrategyPaused),
    Stopped(StrategyStopped),
}

impl StrategyState {
    /// Check if strategy is operational (can generate signals)
    pub fn is_operational(&self) -> bool {
        matches!(self, StrategyState::Active(_))
    }

    /// Check if strategy is stopped
    pub fn is_stopped(&self) -> bool {
        matches!(self, StrategyState::Stopped(_))
    }

    /// Get state name
    pub fn state_name(&self) -> &'static str {
        match self {
            StrategyState::Initializing(_) => "Initializing",
            StrategyState::Active(_) => "Active",
            StrategyState::Paused(_) => "Paused",
            StrategyState::Stopped(_) => "Stopped",
        }
    }
}

// Conversions from typed states to enum
impl From<StrategyInitializing> for StrategyState {
    fn from(s: StrategyInitializing) -> Self {
        StrategyState::Initializing(s)
    }
}

impl From<StrategyActive> for StrategyState {
    fn from(s: StrategyActive) -> Self {
        StrategyState::Active(s)
    }
}

impl From<StrategyPaused> for StrategyState {
    fn from(s: StrategyPaused) -> Self {
        StrategyState::Paused(s)
    }
}

impl From<StrategyStopped> for StrategyState {
    fn from(s: StrategyStopped) -> Self {
        StrategyState::Stopped(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_initializing_to_active() {
        let strategy = StrategyInitializing::new("TestStrategy".to_string());
        assert_eq!(strategy.data().name, "TestStrategy");
        assert!(strategy.data().started_at.is_none());

        let strategy = strategy.start();
        assert!(strategy.data().started_at.is_some());
        assert!(strategy.is_operational());
    }

    #[test]
    fn test_active_to_paused_to_active() {
        let strategy = StrategyInitializing::new("TestStrategy".to_string()).start();

        // Small delay to accumulate runtime
        thread::sleep(Duration::from_millis(10));

        let strategy = strategy.pause();
        assert!(!strategy.is_operational());
        assert_eq!(strategy.data().pause_count, 1);

        let strategy = strategy.resume();
        assert!(strategy.is_operational());
    }

    #[test]
    fn test_active_to_stopped() {
        let strategy = StrategyInitializing::new("TestStrategy".to_string()).start();

        thread::sleep(Duration::from_millis(10));

        let strategy = strategy.stop();
        assert!(!strategy.is_operational());
        assert!(strategy.data().stopped_at.is_some());
        assert!(strategy.total_runtime() > Duration::ZERO);
    }

    #[test]
    fn test_paused_to_stopped() {
        let strategy = StrategyInitializing::new("TestStrategy".to_string())
            .start()
            .pause();

        let strategy = strategy.stop();
        assert!(!strategy.is_operational());
        assert_eq!(strategy.pause_count(), 1);
    }

    #[test]
    fn test_initializing_abort() {
        let strategy = StrategyInitializing::new("TestStrategy".to_string());

        // Can stop before starting
        let strategy = strategy.stop();
        assert!(!strategy.is_operational());
        assert!(strategy.data().started_at.is_none());
        assert_eq!(strategy.total_runtime(), Duration::ZERO);
    }

    #[test]
    fn test_multiple_pause_resume_cycles() {
        let mut strategy = StrategyInitializing::new("TestStrategy".to_string()).start();

        for i in 0..3 {
            thread::sleep(Duration::from_millis(5));
            let paused = strategy.pause();
            assert_eq!(paused.data().pause_count, i + 1);

            thread::sleep(Duration::from_millis(2));
            strategy = paused.resume();
        }

        let stopped = strategy.stop();
        assert_eq!(stopped.pause_count(), 3);
        assert!(stopped.total_runtime() >= Duration::from_millis(15));
    }

    #[test]
    fn test_strategy_state_enum() {
        let strategy = StrategyInitializing::new("TestStrategy".to_string());
        let state: StrategyState = strategy.into();

        assert!(!state.is_operational());
        assert!(!state.is_stopped());
        assert_eq!(state.state_name(), "Initializing");
    }

    // ========================================================================
    // Compile-Time Safety Demonstrations
    // ========================================================================

    // Uncomment to see compile errors:

    // #[test]
    // fn test_cannot_resume_from_stopped() {
    //     let strategy = StrategyInitializing::new("Test".to_string())
    //         .start()
    //         .stop();
    //     // This won't compile - Stopped has no resume()
    //     // strategy.resume();
    // }

    // #[test]
    // fn test_cannot_pause_from_paused() {
    //     let strategy = StrategyInitializing::new("Test".to_string())
    //         .start()
    //         .pause();
    //     // This won't compile - Paused has no pause()
    //     // strategy.pause();
    // }

    // #[test]
    // fn test_cannot_start_from_active() {
    //     let strategy = StrategyInitializing::new("Test".to_string())
    //         .start();
    //     // This won't compile - Active has no start()
    //     // strategy.start();
    // }
}
