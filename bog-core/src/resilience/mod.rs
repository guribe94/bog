//! Resilience patterns for production deployment
//!
//! Provides robust error handling and recovery mechanisms:
//! - Gap detection in market data streams (with wraparound support)
//! - Exponential backoff for retries
//! - Automatic reconnection with backoff
//! - Circuit breaker pattern (now with typestate FSM!)
//! - Connection health monitoring
//! - Global panic handler for graceful shutdown

pub mod backoff;
pub mod circuit_breaker;
pub mod circuit_breaker_v2;
pub mod gap_detector;
pub mod health;
pub mod kill_switch;
pub mod panic;
pub mod reconnect;
pub mod stale_data;

pub use backoff::{BackoffConfig, ExponentialBackoff};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
pub use circuit_breaker_v2::CircuitBreakerV2;
pub use gap_detector::GapDetector;
pub use health::{FeedHealth, HealthConfig, HealthStatus};
pub use kill_switch::{KillSwitch, KillSwitchState};
pub use panic::install_panic_handler;
pub use reconnect::{ConnectionState, ResilientConfig, ResilientMarketFeed};
pub use stale_data::{StaleDataBreaker, StaleDataConfig, StaleDataState};
