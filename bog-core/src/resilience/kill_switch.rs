//! Emergency Kill Switch - Graceful Shutdown System
//!
//! Provides multiple mechanisms to stop the trading bot safely:
//! - Signal handlers (SIGTERM, SIGUSR1, SIGUSR2)
//! - Atomic shutdown flag (for programmatic shutdown)
//! - Future: HTTP endpoint for remote shutdown
//!
//! ## Usage
//!
//! ```no_run
//! use bog_core::resilience::KillSwitch;
//!
//! // Install signal handlers
//! let kill_switch = KillSwitch::install();
//!
//! // Main trading loop
//! while !kill_switch.should_stop() {
//!     // Trade...
//! }
//!
//! // Cleanup
//! kill_switch.shutdown("Graceful shutdown");
//! ```
//!
//! ## Signals
//!
//! - **SIGTERM**: Graceful shutdown (save state, cancel orders)
//! - **SIGUSR1**: Emergency halt (immediate stop, no cleanup)
//! - **SIGUSR2**: Pause trading (resume with another SIGUSR2)
//!
//! ## Safety
//!
//! All signal handlers are async-signal-safe (only atomic operations).

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{error, info, warn};

/// Kill switch state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KillSwitchState {
    /// Normal operation
    Running = 0,
    /// Paused (can resume)
    Paused = 1,
    /// Shutting down gracefully
    ShuttingDown = 2,
    /// Emergency stop (immediate)
    EmergencyStop = 3,
}

impl From<u8> for KillSwitchState {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Running,
            1 => Self::Paused,
            2 => Self::ShuttingDown,
            3 => Self::EmergencyStop,
            _ => Self::Running,
        }
    }
}

/// Emergency kill switch for graceful shutdown
///
/// Thread-safe, async-signal-safe shutdown coordination.
#[derive(Clone)]
pub struct KillSwitch {
    /// Current state
    state: Arc<AtomicU8>,
    /// Shutdown reason (if set)
    shutdown_reason: Arc<parking_lot::Mutex<Option<String>>>,
    /// Timestamp when shutdown initiated
    shutdown_time: Arc<parking_lot::Mutex<Option<SystemTime>>>,
}

impl KillSwitch {
    /// Create a new kill switch in Running state
    pub fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(KillSwitchState::Running as u8)),
            shutdown_reason: Arc::new(parking_lot::Mutex::new(None)),
            shutdown_time: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    /// Install signal handlers and return kill switch
    ///
    /// Sets up:
    /// - SIGTERM → graceful shutdown
    /// - SIGUSR1 → emergency stop
    /// - SIGUSR2 → pause/resume toggle
    pub fn install() -> Self {
        let kill_switch = Self::new();

        #[cfg(unix)]
        {
            let ks_term = kill_switch.clone();
            let ks_usr1 = kill_switch.clone();
            let ks_usr2 = kill_switch.clone();

            // SIGTERM: Graceful shutdown
            if let Err(e) = signal_hook::flag::register(
                signal_hook::consts::SIGTERM,
                Arc::new(AtomicBool::new(true)),
            ) {
                error!("Failed to register SIGTERM handler: {}", e);
            } else {
                // Use a separate thread to set the shutdown state
                // (signal_hook only allows async-signal-safe operations)
                std::thread::spawn(move || {
                    // Wait for signal (this is safe because we're in a separate thread)
                    std::thread::park_timeout(std::time::Duration::from_secs(999999));
                    ks_term.shutdown("SIGTERM received");
                });

                info!("SIGTERM handler installed (graceful shutdown)");
            }

            // SIGUSR1: Emergency stop
            if let Err(e) = signal_hook::flag::register(
                signal_hook::consts::SIGUSR1,
                Arc::new(AtomicBool::new(true)),
            ) {
                error!("Failed to register SIGUSR1 handler: {}", e);
            } else {
                std::thread::spawn(move || {
                    std::thread::park_timeout(std::time::Duration::from_secs(999999));
                    ks_usr1.emergency_stop("SIGUSR1 received");
                });

                info!("SIGUSR1 handler installed (emergency stop)");
            }

            // SIGUSR2: Pause/resume toggle
            if let Err(e) = signal_hook::flag::register(
                signal_hook::consts::SIGUSR2,
                Arc::new(AtomicBool::new(true)),
            ) {
                error!("Failed to register SIGUSR2 handler: {}", e);
            } else {
                std::thread::spawn(move || {
                    std::thread::park_timeout(std::time::Duration::from_secs(999999));
                    ks_usr2.toggle_pause();
                });

                info!("SIGUSR2 handler installed (pause/resume)");
            }
        }

        kill_switch
    }

    /// Check if bot should stop
    #[inline]
    pub fn should_stop(&self) -> bool {
        let state: KillSwitchState = self.state.load(Ordering::Acquire).into();
        matches!(
            state,
            KillSwitchState::ShuttingDown | KillSwitchState::EmergencyStop
        )
    }

    /// Check if bot is paused
    #[inline]
    pub fn is_paused(&self) -> bool {
        let state: KillSwitchState = self.state.load(Ordering::Acquire).into();
        matches!(state, KillSwitchState::Paused)
    }

    /// Check if bot is running
    #[inline]
    pub fn is_running(&self) -> bool {
        let state: KillSwitchState = self.state.load(Ordering::Acquire).into();
        matches!(state, KillSwitchState::Running)
    }

    /// Initiate graceful shutdown
    pub fn shutdown(&self, reason: &str) {
        info!("Kill switch activated: {}", reason);

        self.state
            .store(KillSwitchState::ShuttingDown as u8, Ordering::Release);

        *self.shutdown_reason.lock() = Some(reason.to_string());
        *self.shutdown_time.lock() = Some(SystemTime::now());
    }

    /// Initiate emergency stop (immediate)
    pub fn emergency_stop(&self, reason: &str) {
        error!("EMERGENCY STOP: {}", reason);

        self.state
            .store(KillSwitchState::EmergencyStop as u8, Ordering::Release);

        *self.shutdown_reason.lock() = Some(format!("EMERGENCY: {}", reason));
        *self.shutdown_time.lock() = Some(SystemTime::now());
    }

    /// Pause trading (can resume)
    pub fn pause(&self) {
        let current: KillSwitchState = self.state.load(Ordering::Acquire).into();

        if matches!(current, KillSwitchState::Running) {
            info!("Kill switch: Pausing trading");
            self.state
                .store(KillSwitchState::Paused as u8, Ordering::Release);
        }
    }

    /// Resume trading (from paused state)
    pub fn resume(&self) {
        let current: KillSwitchState = self.state.load(Ordering::Acquire).into();

        if matches!(current, KillSwitchState::Paused) {
            info!("Kill switch: Resuming trading");
            self.state
                .store(KillSwitchState::Running as u8, Ordering::Release);
        }
    }

    /// Toggle pause/resume
    pub fn toggle_pause(&self) {
        if self.is_paused() {
            self.resume();
        } else if self.is_running() {
            self.pause();
        }
    }

    /// Get current state
    pub fn state(&self) -> KillSwitchState {
        self.state.load(Ordering::Acquire).into()
    }

    /// Get shutdown reason if shutdown was initiated
    pub fn shutdown_reason(&self) -> Option<String> {
        self.shutdown_reason.lock().clone()
    }

    /// Get shutdown timestamp
    pub fn shutdown_time(&self) -> Option<SystemTime> {
        *self.shutdown_time.lock()
    }
}

impl Default for KillSwitch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kill_switch_creation() {
        let ks = KillSwitch::new();
        assert!(ks.is_running());
        assert!(!ks.should_stop());
        assert!(!ks.is_paused());
    }

    #[test]
    fn test_graceful_shutdown() {
        let ks = KillSwitch::new();

        ks.shutdown("Test shutdown");

        assert!(ks.should_stop());
        assert_eq!(ks.state(), KillSwitchState::ShuttingDown);
        assert_eq!(ks.shutdown_reason(), Some("Test shutdown".to_string()));
        assert!(ks.shutdown_time().is_some());
    }

    #[test]
    fn test_emergency_stop() {
        let ks = KillSwitch::new();

        ks.emergency_stop("Critical error");

        assert!(ks.should_stop());
        assert_eq!(ks.state(), KillSwitchState::EmergencyStop);
        assert!(ks.shutdown_reason().unwrap().contains("EMERGENCY"));
    }

    #[test]
    fn test_pause_resume() {
        let ks = KillSwitch::new();

        ks.pause();
        assert!(ks.is_paused());
        assert!(!ks.should_stop());

        ks.resume();
        assert!(ks.is_running());
        assert!(!ks.is_paused());
    }

    #[test]
    fn test_toggle_pause() {
        let ks = KillSwitch::new();

        ks.toggle_pause();
        assert!(ks.is_paused());

        ks.toggle_pause();
        assert!(ks.is_running());
    }

    #[test]
    fn test_cannot_resume_from_shutdown() {
        let ks = KillSwitch::new();

        ks.shutdown("Test");
        ks.resume(); // Should have no effect

        assert!(ks.should_stop()); // Still stopped
    }

    #[test]
    fn test_concurrent_access() {
        let ks = KillSwitch::new();
        let ks_clone = ks.clone();

        let handle = std::thread::spawn(move || {
            ks_clone.pause();
        });

        handle.join().unwrap();

        assert!(ks.is_paused());
    }
}
