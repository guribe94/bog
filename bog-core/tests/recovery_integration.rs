use bog_core::execution::{ProductionExecutor, ProductionExecutorConfig, Executor, Order, Side};
use bog_core::engine::{Engine, Strategy};
use bog_core::engine::executor_bridge::ExecutorBridge;
use bog_core::core::{Position, Signal};
use bog_core::data::MarketSnapshot;
use bog_core::orderbook::L2OrderBook;
use rust_decimal_macros::dec;
use tempfile::NamedTempFile;
use anyhow::Result;

// Mock Strategy for Engine
struct NoOpStrategy;
impl Strategy for NoOpStrategy {
    fn calculate(&mut self, _book: &L2OrderBook, _position: &Position) -> Option<Signal> {
        None
    }
    fn name(&self) -> &'static str { "NoOp" }
}

#[test]
fn test_engine_recovery_from_journal() -> Result<()> {
    // 1. Setup: Create a temporary journal file
    let mut temp_file = NamedTempFile::new()?;
    let journal_path = temp_file.path().to_path_buf();
    
    // Close the file handle so the executor can open it, but keep the path valid (NamedTempFile keeps it until drop)
    // Actually, NamedTempFile deletes on drop. We can clone the path. 
    // ProductionExecutor opens with append=true.
    
    // 2. Run 1 (Simulation)
    {
        let config = ProductionExecutorConfig {
            enable_journal: true,
            journal_path: journal_path.clone(),
            recover_on_startup: false, // First run
            validate_recovery: true,
            instant_fills: true, // Immediate fills for simplicity
            ..Default::default()
        };
        
        let mut executor = ProductionExecutor::new(config);
        
        // Place and fill an order (Buy 1.0 BTC @ 50,000)
        let order = Order::limit(Side::Buy, dec!(50000), dec!(1.0));
        let order_id = executor.place_order(order)?;
        
        // Verify it filled (instant_fills=true)
        let status = executor.get_order_status(&order_id);
        assert_eq!(status, Some(bog_core::execution::OrderStatus::Filled));
        
        // Executor drops here, closing file handle and flushing
    }
    
    // 3. Run 2 (Recovery)
    {
        let config = ProductionExecutorConfig {
            enable_journal: true,
            journal_path: journal_path.clone(),
            recover_on_startup: true, // Enable recovery
            validate_recovery: true,
            instant_fills: true,
            ..Default::default()
        };
        
        let executor = ProductionExecutor::new(config);
        
        // Calculate net position from recovered state
        let net_position = executor.calculate_net_position();
        println!("Recovered net position: {}", net_position);
        
        // Should be 1.0 BTC in fixed point (9 decimals) = 1_000_000_000
        assert_eq!(net_position, 1_000_000_000, "Failed to recover net position from journal");
        
        // Initialize Engine
        let strategy = NoOpStrategy;
        // Wrap executor in ExecutorBridge to adapt to Engine's Executor trait
        let bridged_executor = ExecutorBridge::new(executor);
        let engine = Engine::new(strategy, bridged_executor);
        
        // 4. Action: Update Engine position
        if net_position != 0 {
            engine.position().update_quantity(net_position);
        }
        
        // 5. Assertion: Verify Engine position
        let engine_qty = engine.position().get_quantity();
        assert_eq!(engine_qty, 1_000_000_000, "Engine position not updated correctly");
    }
    
    Ok(())
}

