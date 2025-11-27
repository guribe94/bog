//! Integration tests for depth-aware trading strategies
//!
//! NOTE: These tests are currently disabled pending API migration.
//! The Engine API has changed significantly and these tests need to be rewritten.

#[test]
#[ignore] // Test needs API migration: Engine::run() and depth handling changed
fn test_depth_aware_strategy_end_to_end() {
    // TODO: Rewrite using new Engine API with process_tick(snapshot, data_fresh: bool)
}

#[test]
#[ignore] // Test needs API migration
fn test_depth_strategy_performance() {
    // TODO: Rewrite using new Strategy::calculate(snapshot, position) signature
}
