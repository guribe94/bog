//! Unit tests for position update logic
//!
//! Tests the correctness of fill processing and position tracking

use bog_core::core::Position;
use bog_core::execution::{Fill, OrderId};
use rust_decimal::Decimal;

fn create_test_fill(side: bog_core::execution::types::Side, price: u64, size: u64) -> Fill {
    Fill::new(
        OrderId::new_random(),
        side,
        Decimal::new(price as i64, 9),  // 9 decimal places
        Decimal::new(size as i64, 9),
    )
}

#[test]
fn test_single_fill_updates_position() {
    // Given: Initial position = 0, one buy fill of 0.1 BTC
    let position = Position::new();
    let fill = create_test_fill(bog_core::execution::types::Side::Buy, 50_000_000_000_000, 100_000_000);
    
    // When: Process the fill
    position.process_fill(&fill).unwrap();
    
    // Then: Position should be +0.1 BTC
    assert_eq!(position.get_quantity(), 100_000_000);
    assert_eq!(position.get_trade_count(), 1);
}

#[test]
fn test_multiple_fills_accumulate_position() {
    // Given: Initial position = 0
    let position = Position::new();
    let fills = vec![
        create_test_fill(bog_core::execution::types::Side::Buy, 50_000_000_000_000, 100_000_000),  // +0.1
        create_test_fill(bog_core::execution::types::Side::Buy, 50_010_000_000_000, 100_000_000),  // +0.1
        create_test_fill(bog_core::execution::types::Side::Sell, 50_005_000_000_000, 150_000_000), // -0.15
    ];
    
    // When: Process all fills
    for fill in fills {
        position.process_fill(&fill).unwrap();
    }
    
    // Then: Position should be +0.05 BTC (0.1 + 0.1 - 0.15)
    assert_eq!(position.get_quantity(), 50_000_000);
    assert_eq!(position.get_trade_count(), 3);
}

#[test]
fn test_buy_fill_negative_cash_flow() {
    // Given: Buy fill at $50,000 for 0.1 BTC
    let fill = create_test_fill(bog_core::execution::types::Side::Buy, 50_000_000_000_000, 100_000_000);
    let position = Position::new();
    
    // When: Process fill
    position.process_fill(&fill).unwrap();
    
    // Then: Cash flow should be negative (spent money - tracked in PnL)
    assert!(position.get_realized_pnl() < 0);
    // -$5,000 in fixed-point but prorated based on current implementation
}

#[test]
fn test_sell_fill_positive_cash_flow() {
    // Given: Sell fill at $50,000 for 0.1 BTC
    let fill = create_test_fill(bog_core::execution::types::Side::Sell, 50_000_000_000_000, 100_000_000);
    let position = Position::new();
    
    // When: Process fill
    position.process_fill(&fill).unwrap();
    
    // Then: Cash flow should be positive (received money - tracked in PnL)
    assert!(position.get_realized_pnl() > 0);
}

#[test]
fn test_position_pnl_updates() {
    // Given: Sequence of profitable fills
    let position = Position::new();
    
    // Buy at $50,000, sell at $50,100 = $100 profit - fees
    let buy_fill = create_test_fill(bog_core::execution::types::Side::Buy, 50_000_000_000_000, 100_000_000);
    position.process_fill(&buy_fill).unwrap();
    
    let sell_fill = create_test_fill(bog_core::execution::types::Side::Sell, 50_100_000_000_000, 100_000_000);
    position.process_fill(&sell_fill).unwrap();
    
    // Then: PnL should reflect profit
    assert!(position.get_realized_pnl() > 0);
}

#[test]
fn test_short_position_tracking() {
    // Given: Initial position = 0
    let position = Position::new();
    
    // Sell 0.1 BTC (short position)
    let fill = create_test_fill(bog_core::execution::types::Side::Sell, 50_000_000_000_000, 100_000_000);
    position.process_fill(&fill).unwrap();
    
    // Then: Position should be -0.1 BTC
    assert_eq!(position.get_quantity(), -100_000_000);
}

#[test]
fn test_overflow_protection() {
    // Given: Position near i64::MAX
    let position = Position::new();
    position.update_quantity(i64::MAX - 1000);
    
    // When: Try to process fill that would overflow
    let fill = create_test_fill(bog_core::execution::types::Side::Buy, 50_000_000_000_000, 10_000_000_000);
    let result = position.process_fill(&fill);
    
    // Then: Should return overflow error, not panic
    assert!(result.is_err());
    // The exact error type will be PositionError::Overflow
}

#[test]
fn test_position_has_process_fill() {
    // This will fail to compile if process_fill doesn't exist
    let position = Position::new();
    let fill = create_test_fill(bog_core::execution::types::Side::Buy, 50_000_000_000_000, 100_000_000);
    
    // Just check that the method exists and compiles
    let _result = position.process_fill(&fill);
    
    // If we get here, the method exists
    assert!(true);
}
