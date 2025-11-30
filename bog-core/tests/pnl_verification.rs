
use bog_core::execution::{Fill, OrderId, Side};
use bog_core::risk::{RiskLimits, RiskManager};
use rust_decimal_macros::dec;
use rust_decimal::Decimal;

fn create_test_limits() -> RiskLimits {
    RiskLimits {
        max_position: dec!(10.0),
        max_short: dec!(10.0),
        max_order_size: dec!(10.0),
        min_order_size: dec!(0.001),
        max_outstanding_orders: 100,
        max_daily_loss: dec!(10000.0),
        max_drawdown_pct: 0.50,
    }
}

#[test]
fn test_long_position_pnl_calculation() {
    let limits = create_test_limits();
    let mut rm = RiskManager::with_limits(limits);

    // 1. Buy 1 BTC at $50,000
    let buy_fill = Fill::new(OrderId::new_random(), Side::Buy, dec!(50000), dec!(1.0));
    rm.update_position(&buy_fill).unwrap();

    assert_eq!(rm.position().quantity, dec!(1.0));
    assert_eq!(rm.position().cost_basis, dec!(-50000)); // Cash flow is negative

    // 2. Sell 1 BTC at $51,000 (Profit should be $1,000)
    let sell_fill = Fill::new(OrderId::new_random(), Side::Sell, dec!(51000), dec!(1.0));
    rm.update_position(&sell_fill).unwrap();

    assert_eq!(rm.position().quantity, Decimal::ZERO);
    
    // Realized PnL should be $1,000
    // Current buggy implementation likely calculates: (51000 - (-50000)) = 101,000
    assert_eq!(rm.position().realized_pnl, dec!(1000), "PnL should be 1000, got {}", rm.position().realized_pnl);
}

#[test]
fn test_short_position_pnl_calculation() {
    let limits = create_test_limits();
    let mut rm = RiskManager::with_limits(limits);

    // 1. Sell 1 BTC at $50,000 (Short)
    let sell_fill = Fill::new(OrderId::new_random(), Side::Sell, dec!(50000), dec!(1.0));
    rm.update_position(&sell_fill).unwrap();

    assert_eq!(rm.position().quantity, dec!(-1.0));
    assert_eq!(rm.position().cost_basis, dec!(50000)); // Cash flow is positive

    // 2. Buy 1 BTC at $40,000 (Profit should be $10,000)
    let buy_fill = Fill::new(OrderId::new_random(), Side::Buy, dec!(40000), dec!(1.0));
    rm.update_position(&buy_fill).unwrap();

    assert_eq!(rm.position().quantity, Decimal::ZERO);
    
    // Realized PnL should be $10,000
    // (50000 - 40000) = 10000
    assert_eq!(rm.position().realized_pnl, dec!(10000), "PnL should be 10000, got {}", rm.position().realized_pnl);
}

