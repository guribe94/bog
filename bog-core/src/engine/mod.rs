//! Trading Engine
//!
//! This module contains the trading engine implementations:
//! - `generic`: Const generic zero-overhead engine (NEW - HFT optimized)
//! - `simulated`: Zero-overhead simulated executor with object pools
//! - `risk`: Const-based risk validation with zero overhead
//! - `position_reconciliation`: Position drift detection and correction
//! - `gap_recovery`: Automatic gap recovery for data feed issues
//! - Legacy dynamic dispatch engine (deprecated, commented out)

// New const generic engine (HFT optimized)
pub mod alert_manager;
pub mod executor_bridge;
pub mod gap_recovery;
pub mod generic;
pub mod position_reconciliation;
pub mod risk;
pub mod simulated;
pub mod strategy_wrapper;
pub mod traits;

// Re-export new engine types
pub use alert_manager::{Alert, AlertConfig, AlertManager, AlertSeverity, AlertStats, AlertType};
pub use gap_recovery::{GapRecoveryConfig, GapRecoveryManager, GapRecoveryStats};
pub use generic::{Engine, EngineStats, Executor, Strategy};
pub use position_reconciliation::{PositionReconciler, ReconciliationConfig, ReconciliationStats};
pub use risk::{validate_signal, RiskViolation};
pub use simulated::SimulatedExecutor;
pub use strategy_wrapper::StrategyWrapper;
