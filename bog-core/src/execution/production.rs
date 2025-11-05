//! Production-grade execution layer with comprehensive observability
//!
//! This executor provides:
//! - Thread-safe order state tracking
//! - Complete order lifecycle logging
//! - Execution metrics for monitoring
//! - Persistent execution journal
//! - Configurable fill simulation for testing
//!
//! **NOTE**: This is a production-quality STUB for testing and development.
//! Real exchange integration should be added later while maintaining this interface.

use super::{Executor, ExecutionMode, Fill, Order, OrderId, OrderStatus};
use anyhow::{anyhow, Result};
use dashmap::DashMap;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write as IoWrite};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info, warn};

/// Order state with full lifecycle tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OrderState {
    order: Order,
    submitted_at: SystemTime,
    acknowledged_at: Option<SystemTime>,
    fills: Vec<Fill>,
}

impl OrderState {
    fn new(order: Order) -> Self {
        Self {
            submitted_at: SystemTime::now(),
            acknowledged_at: None,
            fills: Vec::new(),
            order,
        }
    }

    fn acknowledge(&mut self) {
        self.acknowledged_at = Some(SystemTime::now());
    }

    fn add_fill(&mut self, fill: Fill) {
        self.fills.push(fill);
    }

    /// Calculate total filled volume
    fn total_fill_volume(&self) -> Decimal {
        self.fills.iter().map(|f| f.notional()).sum()
    }

    /// Validate order state consistency
    fn is_valid(&self) -> bool {
        // Check fills don't exceed order size
        let total_filled: Decimal = self.fills.iter().map(|f| f.size).sum();
        if total_filled > self.order.size {
            return false;
        }

        // Check filled_size matches fills
        if (self.order.filled_size - total_filled).abs() > Decimal::from_str("0.00000001").unwrap() {
            return false;
        }

        // Check status consistency
        match self.order.status {
            OrderStatus::Filled => total_filled >= self.order.size,
            OrderStatus::PartiallyFilled => total_filled > Decimal::ZERO && total_filled < self.order.size,
            _ => true,
        }
    }
}

/// Journal event for persistence and recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
enum JournalEvent {
    OrderSubmit(Order),
    OrderAck(OrderId),
    Fill(Fill),
    OrderCancel(OrderId),
}

#[derive(Debug, Serialize, Deserialize)]
struct JournalEntry {
    timestamp: u64,
    #[serde(flatten)]
    event: JournalEvent,
}

impl JournalEntry {
    fn new(event: JournalEvent) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            event,
        }
    }
}

/// Recovery statistics
#[derive(Debug, Default)]
pub struct RecoveryStats {
    pub orders_recovered: usize,
    pub fills_recovered: usize,
    pub errors: usize,
    pub journal_entries: usize,
}

/// Execution metrics for monitoring
#[derive(Debug, Default)]
pub struct ExecutionMetrics {
    /// Total orders submitted
    pub orders_submitted: AtomicU64,
    /// Total orders acknowledged
    pub orders_acknowledged: AtomicU64,
    /// Total fills received
    pub fills_received: AtomicU64,
    /// Total orders cancelled
    pub orders_cancelled: AtomicU64,
    /// Total orders rejected
    pub orders_rejected: AtomicU64,
    /// Total notional volume (BTC * price)
    pub total_volume: AtomicU64, // Stored as integer cents for atomic operations
}

impl ExecutionMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_order_submitted(&self) {
        self.orders_submitted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_order_acknowledged(&self) {
        self.orders_acknowledged.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_fill(&self, notional: Decimal) {
        self.fills_received.fetch_add(1, Ordering::Relaxed);
        // Convert to cents and store (avoiding float atomics)
        let notional_cents = (notional * Decimal::from(100)).to_u64().unwrap_or(0);
        self.total_volume.fetch_add(notional_cents, Ordering::Relaxed);
    }

    pub fn record_order_cancelled(&self) {
        self.orders_cancelled.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_order_rejected(&self) {
        self.orders_rejected.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total volume in dollars
    pub fn total_volume_usd(&self) -> f64 {
        self.total_volume.load(Ordering::Relaxed) as f64 / 100.0
    }

    /// Get fill rate (fills / orders submitted)
    pub fn fill_rate(&self) -> f64 {
        let submitted = self.orders_submitted.load(Ordering::Relaxed);
        if submitted == 0 {
            return 0.0;
        }
        let fills = self.fills_received.load(Ordering::Relaxed);
        fills as f64 / submitted as f64
    }

    /// Get acknowledgement rate
    pub fn ack_rate(&self) -> f64 {
        let submitted = self.orders_submitted.load(Ordering::Relaxed);
        if submitted == 0 {
            return 0.0;
        }
        let acked = self.orders_acknowledged.load(Ordering::Relaxed);
        acked as f64 / submitted as f64
    }
}

/// Configuration for production executor
#[derive(Debug, Clone)]
pub struct ProductionExecutorConfig {
    /// Enable execution journal (persists orders/fills to disk)
    pub enable_journal: bool,
    /// Journal file path
    pub journal_path: PathBuf,
    /// Recover state from journal on startup
    pub recover_on_startup: bool,
    /// Validate recovered state for consistency
    pub validate_recovery: bool,
    /// Simulated fill delay (0 = instant, >0 = realistic delay in ms)
    pub fill_delay_ms: u64,
    /// Fill probability for limit orders (0.0 to 1.0)
    pub fill_probability: f64,
    /// Enable instant fills for testing
    pub instant_fills: bool,
}

impl Default for ProductionExecutorConfig {
    fn default() -> Self {
        Self {
            enable_journal: true,
            journal_path: PathBuf::from("./data/execution_journal.jsonl"),
            recover_on_startup: true,
            validate_recovery: true,
            fill_delay_ms: 0,
            fill_probability: 1.0,
            instant_fills: true,
        }
    }
}

/// Production-grade executor with full observability
///
/// This executor provides a production-quality interface for order execution
/// with comprehensive logging, metrics, and persistence. Currently operates
/// as a high-quality stub for testing and development.
///
/// ## Features
///
/// - **Thread-safe**: Uses DashMap for concurrent order tracking
/// - **Observable**: Complete order lifecycle logging
/// - **Persistent**: Optional execution journal to disk
/// - **Metrics**: Built-in execution metrics for monitoring
/// - **Configurable**: Adjustable fill behavior for testing
///
/// ## Example
///
/// ```rust
/// use bog_core::execution::ProductionExecutor;
///
/// let config = ProductionExecutorConfig::default();
/// let executor = ProductionExecutor::new(config);
/// ```
pub struct ProductionExecutor {
    /// Thread-safe order state tracking
    orders: Arc<DashMap<OrderId, OrderState>>,
    /// Pending fills waiting to be consumed
    pending_fills: Arc<parking_lot::Mutex<Vec<Fill>>>,
    /// Execution metrics
    metrics: Arc<ExecutionMetrics>,
    /// Configuration
    config: ProductionExecutorConfig,
    /// Execution mode
    mode: ExecutionMode,
}

impl ProductionExecutor {
    /// Create a new production executor
    pub fn new(config: ProductionExecutorConfig) -> Self {
        info!(
            "Initializing ProductionExecutor (journal={}, recover={}, fill_delay={}ms)",
            config.enable_journal, config.recover_on_startup, config.fill_delay_ms
        );

        // Create journal directory if needed
        if config.enable_journal {
            if let Some(parent) = config.journal_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
        }

        let mut executor = Self {
            orders: Arc::new(DashMap::new()),
            pending_fills: Arc::new(parking_lot::Mutex::new(Vec::new())),
            metrics: Arc::new(ExecutionMetrics::new()),
            config,
            mode: ExecutionMode::Simulated,
        };

        // Recover from journal if enabled
        if executor.config.recover_on_startup && executor.config.journal_path.exists() {
            match executor.recover_from_journal() {
                Ok(stats) => {
                    info!(
                        "Recovery complete: {} orders, {} fills recovered from {} journal entries ({} errors)",
                        stats.orders_recovered, stats.fills_recovered, stats.journal_entries, stats.errors
                    );
                }
                Err(e) => {
                    error!("Failed to recover from journal: {}", e);
                }
            }
        }

        executor
    }

    /// Create with default configuration
    pub fn new_default() -> Self {
        Self::new(ProductionExecutorConfig::default())
    }

    /// Get execution metrics
    pub fn metrics(&self) -> &ExecutionMetrics {
        &self.metrics
    }

    /// Write to execution journal
    fn journal_event(&self, event: JournalEvent) {
        if !self.config.enable_journal {
            return;
        }

        let mut file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.journal_path)
        {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open journal file: {}", e);
                return;
            }
        };

        let entry = JournalEntry::new(event);

        if let Ok(json) = serde_json::to_string(&entry) {
            writeln!(file, "{}", json).ok();
        }
    }

    /// Recover state from journal file
    fn recover_from_journal(&mut self) -> Result<RecoveryStats> {
        let file = File::open(&self.config.journal_path)?;
        let reader = BufReader::new(file);

        let mut stats = RecoveryStats::default();
        let mut order_states: std::collections::HashMap<OrderId, OrderState> = std::collections::HashMap::new();

        for line in reader.lines() {
            stats.journal_entries += 1;

            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    stats.errors += 1;
                    warn!("Failed to read journal line: {}", e);
                    continue;
                }
            };

            let entry: JournalEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(e) => {
                    stats.errors += 1;
                    warn!("Failed to parse journal entry: {}", e);
                    continue;
                }
            };

            // Replay journal event
            match entry.event {
                JournalEvent::OrderSubmit(order) => {
                    let order_id = order.id.clone();
                    order_states.insert(order_id, OrderState::new(order));
                    stats.orders_recovered += 1;
                }
                JournalEvent::OrderAck(order_id) => {
                    if let Some(state) = order_states.get_mut(&order_id) {
                        state.acknowledge();
                    }
                }
                JournalEvent::Fill(fill) => {
                    if let Some(state) = order_states.get_mut(&fill.order_id) {
                        state.add_fill(fill.clone());
                        state.order.filled_size += fill.size;

                        // Update status
                        if state.order.filled_size >= state.order.size {
                            state.order.status = OrderStatus::Filled;
                        } else if state.order.filled_size > Decimal::ZERO {
                            state.order.status = OrderStatus::PartiallyFilled;
                        }

                        stats.fills_recovered += 1;
                    }
                }
                JournalEvent::OrderCancel(order_id) => {
                    if let Some(state) = order_states.get_mut(&order_id) {
                        state.order.status = OrderStatus::Cancelled;
                    }
                }
            }
        }

        // Validate and load recovered state
        for (order_id, state) in order_states {
            if self.config.validate_recovery && !state.is_valid() {
                warn!("Invalid order state for {}, skipping", order_id);
                stats.errors += 1;
                continue;
            }

            // Update metrics from recovered state
            self.metrics.record_order_submitted();
            if state.acknowledged_at.is_some() {
                self.metrics.record_order_acknowledged();
            }
            for fill in &state.fills {
                self.metrics.record_fill(fill.notional());
            }
            if matches!(state.order.status, OrderStatus::Cancelled) {
                self.metrics.record_order_cancelled();
            }

            self.orders.insert(order_id, state);
        }

        Ok(stats)
    }

    /// Simulate a fill for an order
    fn simulate_fill(&self, order_state: &mut OrderState) -> Option<Fill> {
        // Check fill probability for limit orders
        if !self.config.instant_fills {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            if rng.gen::<f64>() > self.config.fill_probability {
                debug!(
                    "Order {} did not fill (probability check)",
                    order_state.order.id
                );
                return None;
            }
        }

        let order = &mut order_state.order;
        let fill_size = order.remaining_size();
        let fill_price = order.price;

        // Update order
        order.filled_size += fill_size;
        order.status = if order.is_filled() {
            OrderStatus::Filled
        } else {
            OrderStatus::PartiallyFilled
        };
        order.updated_at = SystemTime::now();

        // Calculate average fill price
        order.avg_fill_price = Some(fill_price);

        // Create fill
        let fill = Fill::new(order.id.clone(), order.side, fill_price, fill_size);

        // Record metrics
        self.metrics.record_fill(fill.notional());

        // Journal fill
        self.journal_event(JournalEvent::Fill(fill.clone()));

        debug!(
            "Simulated fill: {} {} @ {} (size: {}, notional: {})",
            fill.side,
            fill.order_id,
            fill.price,
            fill.size,
            fill.notional()
        );

        Some(fill)
    }

    /// Get order statistics for logging
    pub fn order_stats(&self) -> String {
        format!(
            "Orders: {}/{} submitted/acked, Fills: {}, Cancelled: {}, Volume: ${:.2}",
            self.metrics.orders_submitted.load(Ordering::Relaxed),
            self.metrics.orders_acknowledged.load(Ordering::Relaxed),
            self.metrics.fills_received.load(Ordering::Relaxed),
            self.metrics.orders_cancelled.load(Ordering::Relaxed),
            self.metrics.total_volume_usd(),
        )
    }
}

impl Executor for ProductionExecutor {
    fn place_order(&mut self, mut order: Order) -> Result<OrderId> {
        info!(
            "PRODUCTION: Placing order {} {} @ {} (size: {})",
            order.side, order.id, order.price, order.size
        );

        // Validate order
        if order.size <= Decimal::ZERO {
            self.metrics.record_order_rejected();
            error!("Order rejected: size must be positive");
            return Err(anyhow!("Order size must be positive"));
        }

        if order.price < Decimal::ZERO {
            self.metrics.record_order_rejected();
            error!("Order rejected: price cannot be negative");
            return Err(anyhow!("Order price cannot be negative"));
        }

        // Record submission
        self.metrics.record_order_submitted();

        // Update order status
        order.status = OrderStatus::Pending;
        order.updated_at = SystemTime::now();

        let order_id = order.id.clone();
        let mut order_state = OrderState::new(order.clone());

        // Journal order submission
        self.journal_event(JournalEvent::OrderSubmit(order));

        // Simulate acknowledgement delay (if configured)
        if self.config.fill_delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.config.fill_delay_ms));
        }

        // Acknowledge order
        order_state.acknowledge();
        order_state.order.status = OrderStatus::Open;
        self.metrics.record_order_acknowledged();

        debug!("Order {} acknowledged", order_id);
        self.journal_event(JournalEvent::OrderAck(order_id.clone()));

        // Simulate fill (if instant fills enabled)
        if self.config.instant_fills {
            if let Some(fill) = self.simulate_fill(&mut order_state) {
                order_state.add_fill(fill.clone());
                self.pending_fills.lock().push(fill);
            }
        }

        // Store order state
        self.orders.insert(order_id.clone(), order_state);

        Ok(order_id)
    }

    fn cancel_order(&mut self, order_id: &OrderId) -> Result<()> {
        info!("PRODUCTION: Cancelling order {}", order_id);

        if let Some(mut entry) = self.orders.get_mut(order_id) {
            let order_state = entry.value_mut();

            if order_state.order.is_active() {
                order_state.order.status = OrderStatus::Cancelled;
                order_state.order.updated_at = SystemTime::now();

                self.metrics.record_order_cancelled();
                self.journal_event(JournalEvent::OrderCancel(order_id.clone()));

                debug!("Order {} cancelled", order_id);
                Ok(())
            } else {
                warn!("Cannot cancel order {} - not active (status: {:?})", order_id, order_state.order.status);
                Err(anyhow!("Order {} is not active", order_id))
            }
        } else {
            error!("Cannot cancel order {} - not found", order_id);
            Err(anyhow!("Order {} not found", order_id))
        }
    }

    fn get_fills(&mut self) -> Vec<Fill> {
        let mut fills = self.pending_fills.lock();
        std::mem::take(&mut *fills)
    }

    fn get_order_status(&self, order_id: &OrderId) -> Option<OrderStatus> {
        self.orders.get(order_id).map(|entry| entry.value().order.status)
    }

    fn get_active_orders(&self) -> Vec<&Order> {
        // Note: DashMap doesn't allow returning references easily
        // For now, we return an empty vec. This would need refactoring
        // to return owned Orders or use a different approach.
        // This is acceptable for the stub implementation.
        Vec::new()
    }

    fn execution_mode(&self) -> ExecutionMode {
        self.mode
    }
}

impl Drop for ProductionExecutor {
    fn drop(&mut self) {
        info!(
            "Shutting down ProductionExecutor. Final stats: {}",
            self.order_stats()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::Side;
    use rust_decimal_macros::dec;

    #[test]
    fn test_production_executor_creation() {
        let config = ProductionExecutorConfig {
            enable_journal: false,
            ..Default::default()
        };
        let executor = ProductionExecutor::new(config);
        assert_eq!(executor.execution_mode(), ExecutionMode::Simulated);
    }

    #[test]
    fn test_place_and_fill_order() {
        let config = ProductionExecutorConfig {
            enable_journal: false,
            instant_fills: true,
            ..Default::default()
        };
        let mut executor = ProductionExecutor::new(config);

        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        let order_id = executor.place_order(order).unwrap();

        // Check order was placed
        assert!(executor.get_order_status(&order_id).is_some());
        assert_eq!(
            executor.get_order_status(&order_id).unwrap(),
            OrderStatus::Filled
        );

        // Check metrics
        assert_eq!(executor.metrics.orders_submitted.load(Ordering::Relaxed), 1);
        assert_eq!(executor.metrics.orders_acknowledged.load(Ordering::Relaxed), 1);
        assert_eq!(executor.metrics.fills_received.load(Ordering::Relaxed), 1);

        // Check fill was created
        let fills = executor.get_fills();
        assert_eq!(fills.len(), 1);

        let fill = &fills[0];
        assert_eq!(fill.side, Side::Buy);
        assert_eq!(fill.price, dec!(50000));
        assert_eq!(fill.size, dec!(0.1));
    }

    #[test]
    fn test_cancel_order() {
        let config = ProductionExecutorConfig {
            enable_journal: false,
            instant_fills: false, // Don't auto-fill so we can cancel
            ..Default::default()
        };
        let mut executor = ProductionExecutor::new(config);

        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        let order_id = executor.place_order(order).unwrap();

        // Order should be open
        assert_eq!(
            executor.get_order_status(&order_id).unwrap(),
            OrderStatus::Open
        );

        // Cancel order
        executor.cancel_order(&order_id).unwrap();

        // Check status
        assert_eq!(
            executor.get_order_status(&order_id).unwrap(),
            OrderStatus::Cancelled
        );

        // Check metrics
        assert_eq!(executor.metrics.orders_cancelled.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_invalid_order_size() {
        let config = ProductionExecutorConfig {
            enable_journal: false,
            ..Default::default()
        };
        let mut executor = ProductionExecutor::new(config);

        let order = Order::limit(Side::Buy, dec!(50000), dec!(0));
        let result = executor.place_order(order);

        assert!(result.is_err());
        assert_eq!(executor.metrics.orders_rejected.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_execution_metrics() {
        let config = ProductionExecutorConfig {
            enable_journal: false,
            instant_fills: true,
            ..Default::default()
        };
        let mut executor = ProductionExecutor::new(config);

        // Place multiple orders
        for _ in 0..5 {
            let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
            executor.place_order(order).unwrap();
        }

        // Check metrics
        let metrics = executor.metrics();
        assert_eq!(metrics.orders_submitted.load(Ordering::Relaxed), 5);
        assert_eq!(metrics.fills_received.load(Ordering::Relaxed), 5);
        assert_eq!(metrics.fill_rate(), 1.0);
        assert_eq!(metrics.ack_rate(), 1.0);

        // Volume should be 5 * 50000 * 0.1 = 25000
        assert!((metrics.total_volume_usd() - 25000.0).abs() < 1.0);
    }

    #[test]
    fn test_order_stats_string() {
        let config = ProductionExecutorConfig {
            enable_journal: false,
            instant_fills: true,
            ..Default::default()
        };
        let mut executor = ProductionExecutor::new(config);

        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        executor.place_order(order).unwrap();

        let stats = executor.order_stats();
        assert!(stats.contains("Orders: 1/1"));
        assert!(stats.contains("Fills: 1"));
    }

    #[test]
    fn test_order_state_validation() {
        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        let order_id = order.id.clone();
        let mut state = OrderState::new(order);

        // Valid initial state
        assert!(state.is_valid());

        // Add valid fill
        let fill = Fill::new(order_id.clone(), Side::Buy, dec!(50000), dec!(0.05));
        state.order.filled_size = dec!(0.05);
        state.add_fill(fill);
        assert!(state.is_valid());

        // Invalid: fills exceed order size
        let fill2 = Fill::new(order_id.clone(), Side::Buy, dec!(50000), dec!(0.1));
        state.add_fill(fill2);
        // Don't update filled_size - creates inconsistency
        assert!(!state.is_valid());
    }

    #[test]
    fn test_journal_recovery() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a temporary journal file
        let mut temp_file = NamedTempFile::new().unwrap();
        let journal_path = temp_file.path().to_path_buf();

        // Write journal entries
        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        let order_id = order.id.clone();

        let entry1 = JournalEntry::new(JournalEvent::OrderSubmit(order.clone()));
        writeln!(temp_file, "{}", serde_json::to_string(&entry1).unwrap()).unwrap();

        let entry2 = JournalEntry::new(JournalEvent::OrderAck(order_id.clone()));
        writeln!(temp_file, "{}", serde_json::to_string(&entry2).unwrap()).unwrap();

        let fill = Fill::new(order_id.clone(), Side::Buy, dec!(50000), dec!(0.1));
        let entry3 = JournalEntry::new(JournalEvent::Fill(fill));
        writeln!(temp_file, "{}", serde_json::to_string(&entry3).unwrap()).unwrap();

        temp_file.flush().unwrap();

        // Create executor with recovery enabled
        let config = ProductionExecutorConfig {
            enable_journal: false,  // Don't write during recovery
            journal_path: journal_path.clone(),
            recover_on_startup: true,
            validate_recovery: true,
            instant_fills: false,
            ..Default::default()
        };

        let executor = ProductionExecutor::new(config);

        // Verify recovered state
        assert_eq!(executor.metrics.orders_submitted.load(Ordering::Relaxed), 1);
        assert_eq!(executor.metrics.fills_received.load(Ordering::Relaxed), 1);

        // Check order exists
        assert!(executor.get_order_status(&order_id).is_some());
        assert_eq!(executor.get_order_status(&order_id).unwrap(), OrderStatus::Filled);
    }

    #[test]
    fn test_recovery_with_cancelled_order() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        let journal_path = temp_file.path().to_path_buf();

        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        let order_id = order.id.clone();

        // Submit and cancel
        let entry1 = JournalEntry::new(JournalEvent::OrderSubmit(order));
        writeln!(temp_file, "{}", serde_json::to_string(&entry1).unwrap()).unwrap();

        let entry2 = JournalEntry::new(JournalEvent::OrderAck(order_id.clone()));
        writeln!(temp_file, "{}", serde_json::to_string(&entry2).unwrap()).unwrap();

        let entry3 = JournalEntry::new(JournalEvent::OrderCancel(order_id.clone()));
        writeln!(temp_file, "{}", serde_json::to_string(&entry3).unwrap()).unwrap();

        temp_file.flush().unwrap();

        let config = ProductionExecutorConfig {
            enable_journal: false,
            journal_path: journal_path.clone(),
            recover_on_startup: true,
            validate_recovery: true,
            instant_fills: false,
            ..Default::default()
        };

        let executor = ProductionExecutor::new(config);

        // Verify cancelled state
        assert_eq!(executor.get_order_status(&order_id).unwrap(), OrderStatus::Cancelled);
        assert_eq!(executor.metrics.orders_cancelled.load(Ordering::Relaxed), 1);
    }
}
