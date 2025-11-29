//! Connection State Machine - Typestate Pattern
//!
//! Implements compile-time verified state machine for connection lifecycle.
//!
//! # State Diagram
//!
//! ```text
//!     DISCONNECTED
//!           │
//!       connect()
//!           ▼
//!      CONNECTED ─────disconnect()────→ DISCONNECTED
//!           │                                 │
//!     disconnect()                         retry()
//!           │                                 │
//!           ▼                                 ▼
//!     RECONNECTING ◄───fail───┐       RECONNECTING
//!           │                 │              │
//!       success()        max retries      success()
//!           │                 │              │
//!           ▼                 ▼              ▼
//!      CONNECTED          FAILED ◄──────────┘
//!                            │
//!                        manual_retry()
//!                            │
//!                            ▼
//!                       RECONNECTING
//! ```
//!
//! **Key Features:**
//! - Failed state can be recovered via manual_retry()
//! - Reconnecting tracks attempt count
//! - Connected state can disconnect and auto-reconnect
//! - Compile-time prevention of invalid transitions
//!
//! # Usage
//!
//! ```
//! use bog_core::core::connection_fsm::*;
//!
//! // Start disconnected
//! let conn = ConnectionDisconnected::new("exchange-ws".to_string());
//!
//! // Connect
//! let conn = conn.connect();
//!
//! // Lose connection
//! let conn = conn.disconnect();
//!
//! // Retry connection
//! match conn.retry(3) {
//!     RetryResult::Reconnecting(reconnecting) => {
//!         // Attempting to reconnect
//!         match reconnecting.attempt_succeeded() {
//!             ReconnectResult::Connected(connected) => {
//!                 // Back online!
//!             }
//!             ReconnectResult::Failed(failed) => {
//!                 // Max retries exceeded
//!                 let reconnecting = failed.manual_retry(3);
//!                 // Try again...
//!             }
//!             ReconnectResult::Reconnecting(_) => {
//!                 // Still attempting to recover
//!             }
//!         }
//!     }
//!     RetryResult::Failed(failed) => {
//!         // Immediate fail (max retries already exceeded)
//!     }
//! }
//! ```

use std::time::SystemTime;

// ============================================================================
// Connection Data (shared by all states)
// ============================================================================

/// Core connection data shared by all states
#[derive(Debug, Clone)]
pub struct ConnectionData {
    /// Connection name/identifier
    pub name: String,
    /// Timestamp when connection object was created
    pub created_at: SystemTime,
    /// Timestamp when last connected
    pub last_connected_at: Option<SystemTime>,
    /// Timestamp when last disconnected
    pub last_disconnected_at: Option<SystemTime>,
    /// Total number of disconnections
    pub disconnect_count: u64,
    /// Total number of reconnection attempts
    pub reconnect_attempts: u64,
    /// Maximum reconnection attempts allowed
    pub max_reconnect_attempts: u32,
    /// Current reconnection attempt number
    pub current_attempt: u32,
}

impl ConnectionData {
    pub fn new(name: String) -> Self {
        Self {
            name,
            created_at: SystemTime::now(),
            last_connected_at: None,
            last_disconnected_at: None,
            disconnect_count: 0,
            reconnect_attempts: 0,
            max_reconnect_attempts: 10, // Default
            current_attempt: 0,
        }
    }
}

// ============================================================================
// State: Disconnected
// ============================================================================

/// Connection in Disconnected state
///
/// Not currently connected.
///
/// **Valid Transitions:**
/// - `connect()` → ConnectionConnected
/// - `retry(max_attempts)` → ConnectionReconnecting | ConnectionFailed
#[derive(Debug, Clone)]
pub struct ConnectionDisconnected {
    data: ConnectionData,
}

impl ConnectionDisconnected {
    /// Create a new connection in Disconnected state
    pub fn new(name: String) -> Self {
        Self {
            data: ConnectionData::new(name),
        }
    }

    /// Access the underlying data
    pub fn data(&self) -> &ConnectionData {
        &self.data
    }

    /// Transition: Disconnected → Connected
    pub fn connect(mut self) -> ConnectionConnected {
        let now = SystemTime::now();
        self.data.last_connected_at = Some(now);
        ConnectionConnected { data: self.data }
    }

    /// Transition: Disconnected → Reconnecting (or Failed if max attempts exceeded)
    pub fn retry(mut self, max_attempts: u32) -> RetryResult {
        self.data.max_reconnect_attempts = max_attempts;
        self.data.current_attempt = 0;

        if max_attempts == 0 {
            // Immediate failure if no retries allowed
            return RetryResult::Failed(ConnectionFailed { data: self.data });
        }

        self.data.current_attempt = 1;
        self.data.reconnect_attempts += 1;
        RetryResult::Reconnecting(ConnectionReconnecting { data: self.data })
    }

    /// Check if operational (always false for Disconnected)
    pub fn is_operational(&self) -> bool {
        false
    }
}

// ============================================================================
// State: Connected
// ============================================================================

/// Connection in Connected state
///
/// Currently connected and operational.
///
/// **Valid Transitions:**
/// - `disconnect()` → ConnectionDisconnected
#[derive(Debug, Clone)]
pub struct ConnectionConnected {
    data: ConnectionData,
}

impl ConnectionConnected {
    /// Access the underlying data
    pub fn data(&self) -> &ConnectionData {
        &self.data
    }

    /// Transition: Connected → Disconnected
    pub fn disconnect(mut self) -> ConnectionDisconnected {
        let now = SystemTime::now();
        self.data.last_disconnected_at = Some(now);
        self.data.disconnect_count += 1;
        ConnectionDisconnected { data: self.data }
    }

    /// Check if operational (always true for Connected)
    pub fn is_operational(&self) -> bool {
        true
    }
}

// ============================================================================
// State: Reconnecting
// ============================================================================

/// Connection in Reconnecting state
///
/// Attempting to reconnect after a disconnection.
///
/// **Valid Transitions:**
/// - `attempt_succeeded()` → ConnectionConnected | ConnectionFailed
/// - `attempt_failed()` → ConnectionReconnecting | ConnectionFailed
#[derive(Debug, Clone)]
pub struct ConnectionReconnecting {
    data: ConnectionData,
}

impl ConnectionReconnecting {
    /// Access the underlying data
    pub fn data(&self) -> &ConnectionData {
        &self.data
    }

    /// Get current attempt number
    pub fn current_attempt(&self) -> u32 {
        self.data.current_attempt
    }

    /// Get max attempts
    pub fn max_attempts(&self) -> u32 {
        self.data.max_reconnect_attempts
    }

    /// Transition: Reconnecting → Connected (attempt succeeded)
    pub fn attempt_succeeded(mut self) -> ReconnectResult {
        let now = SystemTime::now();
        self.data.last_connected_at = Some(now);
        self.data.current_attempt = 0; // Reset attempt counter
        ReconnectResult::Connected(ConnectionConnected { data: self.data })
    }

    /// Transition: Reconnecting → Reconnecting | Failed (attempt failed)
    pub fn attempt_failed(mut self) -> ReconnectResult {
        if self.data.current_attempt >= self.data.max_reconnect_attempts {
            // Max retries exceeded
            ReconnectResult::Failed(ConnectionFailed { data: self.data })
        } else {
            // Try again
            self.data.current_attempt += 1;
            self.data.reconnect_attempts += 1;
            ReconnectResult::Reconnecting(self)
        }
    }

    /// Check if operational (false - still reconnecting)
    pub fn is_operational(&self) -> bool {
        false
    }
}

// ============================================================================
// State: Failed (Terminal until manual retry)
// ============================================================================

/// Connection in Failed state
///
/// Connection failed after max retry attempts.
/// Requires manual intervention.
///
/// **Valid Transitions:**
/// - `manual_retry()` → ConnectionReconnecting
#[derive(Debug, Clone)]
pub struct ConnectionFailed {
    data: ConnectionData,
}

impl ConnectionFailed {
    /// Access the underlying data
    pub fn data(&self) -> &ConnectionData {
        &self.data
    }

    /// Transition: Failed → Reconnecting (manual retry)
    pub fn manual_retry(mut self, max_attempts: u32) -> ConnectionReconnecting {
        self.data.max_reconnect_attempts = max_attempts;
        self.data.current_attempt = 1;
        self.data.reconnect_attempts += 1;
        ConnectionReconnecting { data: self.data }
    }

    /// Check if operational (always false for Failed)
    pub fn is_operational(&self) -> bool {
        false
    }
}

// ============================================================================
// Result types for state transitions
// ============================================================================

/// Result of retry() on Disconnected state
pub enum RetryResult {
    Reconnecting(ConnectionReconnecting),
    Failed(ConnectionFailed),
}

/// Result of attempt_succeeded() or attempt_failed() on Reconnecting state
pub enum ReconnectResult {
    Connected(ConnectionConnected),
    Reconnecting(ConnectionReconnecting),
    Failed(ConnectionFailed),
}

impl ReconnectResult {
    pub fn is_connected(&self) -> bool {
        matches!(self, ReconnectResult::Connected(_))
    }
}

// ============================================================================
// Enum wrapper
// ============================================================================

/// Type-erased connection state
#[derive(Debug, Clone)]
pub enum ConnectionState {
    Disconnected(ConnectionDisconnected),
    Connected(ConnectionConnected),
    Reconnecting(ConnectionReconnecting),
    Failed(ConnectionFailed),
}

impl ConnectionState {
    /// Check if operational
    pub fn is_operational(&self) -> bool {
        matches!(self, ConnectionState::Connected(_))
    }

    /// Check if failed
    pub fn is_failed(&self) -> bool {
        matches!(self, ConnectionState::Failed(_))
    }

    /// Get state name
    pub fn state_name(&self) -> &'static str {
        match self {
            ConnectionState::Disconnected(_) => "Disconnected",
            ConnectionState::Connected(_) => "Connected",
            ConnectionState::Reconnecting(_) => "Reconnecting",
            ConnectionState::Failed(_) => "Failed",
        }
    }
}

// Conversions
impl From<ConnectionDisconnected> for ConnectionState {
    fn from(c: ConnectionDisconnected) -> Self {
        ConnectionState::Disconnected(c)
    }
}

impl From<ConnectionConnected> for ConnectionState {
    fn from(c: ConnectionConnected) -> Self {
        ConnectionState::Connected(c)
    }
}

impl From<ConnectionReconnecting> for ConnectionState {
    fn from(c: ConnectionReconnecting) -> Self {
        ConnectionState::Reconnecting(c)
    }
}

impl From<ConnectionFailed> for ConnectionState {
    fn from(c: ConnectionFailed) -> Self {
        ConnectionState::Failed(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disconnected_to_connected() {
        let conn = ConnectionDisconnected::new("test-ws".to_string());
        assert!(!conn.is_operational());

        let conn = conn.connect();
        assert!(conn.is_operational());
        assert!(conn.data().last_connected_at.is_some());
    }

    #[test]
    fn test_connected_to_disconnected() {
        let conn = ConnectionDisconnected::new("test-ws".to_string()).connect();

        let conn = conn.disconnect();
        assert!(!conn.is_operational());
        assert_eq!(conn.data().disconnect_count, 1);
    }

    #[test]
    fn test_retry_sequence_success() {
        let conn = ConnectionDisconnected::new("test-ws".to_string());

        match conn.retry(3) {
            RetryResult::Reconnecting(reconnecting) => {
                assert_eq!(reconnecting.current_attempt(), 1);
                assert_eq!(reconnecting.max_attempts(), 3);

                match reconnecting.attempt_succeeded() {
                    ReconnectResult::Connected(connected) => {
                        assert!(connected.is_operational());
                    }
                    _ => panic!("Should be connected"),
                }
            }
            _ => panic!("Should be reconnecting"),
        }
    }

    #[test]
    fn test_retry_sequence_exhaust_attempts() {
        let conn = ConnectionDisconnected::new("test-ws".to_string());

        let mut reconnecting = match conn.retry(3) {
            RetryResult::Reconnecting(r) => r,
            _ => panic!(),
        };

        // Fail attempt 1
        reconnecting = match reconnecting.attempt_failed() {
            ReconnectResult::Reconnecting(r) => {
                assert_eq!(r.current_attempt(), 2);
                r
            }
            _ => panic!("Should still be reconnecting"),
        };

        // Fail attempt 2
        reconnecting = match reconnecting.attempt_failed() {
            ReconnectResult::Reconnecting(r) => {
                assert_eq!(r.current_attempt(), 3);
                r
            }
            _ => panic!("Should still be reconnecting"),
        };

        // Fail attempt 3 (max) → Failed
        match reconnecting.attempt_failed() {
            ReconnectResult::Failed(failed) => {
                assert!(!failed.is_operational());
            }
            _ => panic!("Should be failed"),
        }
    }

    #[test]
    fn test_manual_retry_from_failed() {
        let conn = ConnectionDisconnected::new("test-ws".to_string());

        let failed = match conn.retry(1) {
            RetryResult::Reconnecting(r) => match r.attempt_failed() {
                ReconnectResult::Failed(f) => f,
                _ => panic!(),
            },
            _ => panic!(),
        };

        // Manual retry from failed state
        let reconnecting = failed.manual_retry(5);
        assert_eq!(reconnecting.current_attempt(), 1);
        assert_eq!(reconnecting.max_attempts(), 5);
    }

    #[test]
    fn test_connection_state_enum() {
        let conn = ConnectionDisconnected::new("test".to_string());
        let state: ConnectionState = conn.into();

        assert!(!state.is_operational());
        assert!(!state.is_failed());
        assert_eq!(state.state_name(), "Disconnected");
    }

    #[test]
    fn test_zero_max_attempts() {
        let conn = ConnectionDisconnected::new("test".to_string());

        match conn.retry(0) {
            RetryResult::Failed(failed) => {
                assert!(!failed.is_operational());
            }
            _ => panic!("Should fail immediately with 0 max attempts"),
        }
    }
}
