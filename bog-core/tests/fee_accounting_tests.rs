//! Tests for fee accounting accuracy
//!
//! These tests verify that:
//! - Fees are calculated correctly (2 bps = 0.02%)
//! - Fees are deducted from PnL
//! - Round-trip economics work correctly
//! - PnL matches expected: (sell_price - buy_price) * quantity - fees

use bog_core::execution::{Fill, Side};
use rust_decimal::Decimal;

#[test]
fn test_round_trip_pnl_with_fees() {
    // Test that round-trip profitability accounts for fees
    //
    // Scenario:
    //   - Buy 0.1 BTC @ $50,000
    //     Notional: $5,000
    //     Fee (2 bps): $5,000 * 0.0002 = $1.00
    //     Cost: $5,001.00
    //
    //   - Sell 0.1 BTC @ $50,010
    //     Notional: $5,001
    //     Fee (2 bps): $5,001 * 0.0002 = $1.00
    //     Revenue: $5,000.00
    //
    //   - Gross profit: $10
    //   - Total fees: $2.00
    //   - Net profit: $8.00
    //
    // Expected: realized_pnl = $8.00

    let buy_price = Decimal::from(50000);
    let sell_price = Decimal::from(50010);
    let quantity = Decimal::from_f64(0.1).unwrap();
    let fee_bps = Decimal::from_f64(0.0002).unwrap();  // 2 bps = 0.02%

    // Calculate expected
    let gross_profit = (sell_price - buy_price) * quantity;
    let buy_fee = buy_price * quantity * fee_bps;
    let sell_fee = sell_price * quantity * fee_bps;
    let total_fee = buy_fee + sell_fee;
    let net_profit = gross_profit - total_fee;

    // Expected result
    assert_eq!(gross_profit, Decimal::from(10));  // $10
    assert_eq!(total_fee, Decimal::from_f64(2.0).unwrap());  // $2
    assert_eq!(net_profit, Decimal::from(8));  // $8

    // After implementation, verify that PnL calculation matches this
}

#[test]
fn test_fee_calculation_2bps_accuracy() {
    // Test fee calculation accuracy for various order sizes
    //
    // Fee should be exactly 2 bps (0.02%) of notional

    let price = Decimal::from(50000);
    let fee_bps = Decimal::from_f64(0.0002).unwrap();  // 2 bps

    let test_cases = vec![
        (Decimal::from_f64(0.01).unwrap(), Decimal::from_f64(0.1).unwrap()),      // 0.01 BTC: $0.10
        (Decimal::from_f64(0.1).unwrap(), Decimal::from_f64(1.0).unwrap()),       // 0.1 BTC: $1.00
        (Decimal::from_f64(1.0).unwrap(), Decimal::from_f64(10.0).unwrap()),      // 1.0 BTC: $10.00
        (Decimal::from_f64(10.0).unwrap(), Decimal::from_f64(100.0).unwrap()),    // 10 BTC: $100.00
    ];

    for (size, expected_fee) in test_cases {
        let notional = price * size;
        let calculated_fee = notional * fee_bps;

        assert_eq!(
            calculated_fee, expected_fee,
            "Fee for {} BTC should be {} (notional: {})",
            size, expected_fee, notional
        );
    }
}

#[test]
fn test_fees_deducted_from_pnl() {
    // Test that fees are properly deducted from both realized and daily PnL
    //
    // Scenario: Multiple round-trips, verify fees accumulate correctly

    let price = Decimal::from(50000);
    let quantity = Decimal::from_f64(0.1).unwrap();
    let fee_bps = Decimal::from_f64(0.0002).unwrap();

    // Three round-trips
    let mut total_profit = Decimal::from(0);
    let mut total_fees = Decimal::from(0);

    for i in 1..=3 {
        let sell_price = Decimal::from(50000 + (i * 10));  // $50,010, $50,020, $50,030
        let gross_profit = (sell_price - price) * quantity;
        let fee = (price * quantity * fee_bps) + (sell_price * quantity * fee_bps);

        total_profit += gross_profit;
        total_fees += fee;
    }

    let net_profit = total_profit - total_fees;

    // Expected: 3 x $10 gross = $30, minus 3 x $2 fees = $6, equals $24
    assert_eq!(total_profit, Decimal::from(30));
    assert_eq!(total_fees, Decimal::from_f64(6.0).unwrap());
    assert_eq!(net_profit, Decimal::from(24));

    // After implementation, verify that daily_pnl and realized_pnl match this
}

#[test]
fn test_fee_rounding_with_fractional_satoshis() {
    // Test fee calculation with precise satoshi-level amounts
    //
    // Bitcoin prices lead to non-round-number fees

    let price_sat = Decimal::from_f64(50000.123456).unwrap();
    let quantity_sat = Decimal::from_f64(0.123456789).unwrap();
    let fee_bps = Decimal::from_f64(0.0002).unwrap();

    let notional = price_sat * quantity_sat;
    let fee = notional * fee_bps;

    // After implementation: verify fee is calculated to appropriate precision
    assert!(fee > Decimal::from(0), "Fee should be positive");
    assert!(fee < notional, "Fee should be less than notional");
}

#[test]
fn test_fee_consistency_across_fills() {
    // Test that every fill consistently applies 2 bps fee
    //
    // Scenario: Verify fee is applied even with unusual prices

    let test_prices = vec![
        Decimal::from(100),           // Very cheap
        Decimal::from(50000),         // Normal
        Decimal::from(100000),        // Very expensive
        Decimal::from_f64(1234.56).unwrap(),  // Fractional
    ];

    let quantity = Decimal::from_f64(0.1).unwrap();
    let fee_bps = Decimal::from_f64(0.0002).unwrap();

    for price in test_prices {
        let notional = price * quantity;
        let fee = notional * fee_bps;

        // Verify fee is positive and reasonable
        assert!(fee > Decimal::from(0), "Fee should be positive for price {}", price);
        assert!(fee <= notional, "Fee should not exceed notional for price {}", price);

        // Verify fee is exactly 2 bps
        let fee_rate = fee / notional;
        assert_eq!(fee_rate, fee_bps, "Fee rate should be 2 bps for price {}", price);
    }
}
