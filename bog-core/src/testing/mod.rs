//! Testing utilities and mocks for integration tests
//!
//! Provides mock implementations and test helpers for:
//! - MockHuginnFeed: Programmable market data feed
//! - Test data builders (snapshots, positions, signals)
//! - Performance assertion utilities
//! - Metrics collection helpers

pub mod mock_huginn;
pub mod helpers;

pub use mock_huginn::MockHuginnFeed;
pub use helpers::*;
