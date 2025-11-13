//! Resilience patterns for production deployment
//!
//! Provides robust error handling and recovery mechanisms:
//! - Exponential backoff for retries
//! - Automatic reconnection with backoff
//! - Circuit breaker pattern (now with typestate FSM!)
//! - Connection health monitoring
//! - Global panic handler for graceful shutdown

pub mod backoff;
pub mod reconnect;
pub mod circuit_breaker;
pub mod circuit_breaker_v2;
pub mod panic;
pub mod kill_switch;

pub use backoff::{ExponentialBackoff, BackoffConfig};
pub use reconnect::{ResilientMarketFeed, ResilientConfig, ConnectionState};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
pub use circuit_breaker_v2::CircuitBreakerV2;
pub use panic::install_panic_handler;
pub use kill_switch::{KillSwitch, KillSwitchState};
