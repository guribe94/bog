//! Resilience patterns for production deployment
//!
//! Provides robust error handling and recovery mechanisms:
//! - Exponential backoff for retries
//! - Automatic reconnection with backoff
//! - Circuit breaker pattern
//! - Connection health monitoring

pub mod backoff;
pub mod reconnect;
pub mod circuit_breaker;

pub use backoff::{ExponentialBackoff, BackoffConfig};
pub use reconnect::{ResilientMarketFeed, ResilientConfig, ConnectionState};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
