#[cfg(test)]
mod tests {
    use bog_core::core::Position;
    use bog_core::core::fixed_point::SCALE;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_position_flip_entry_price_bug() {
        let position = Position::new();

        // 1. Open Long: Buy 1.0 BTC @ $50,000
        // Fixed point: 50_000 * 10^9
        let price_1 = 50_000 * SCALE as u64;
        let size_1 = 1 * SCALE as u64; // 1.0 BTC
        
        position.process_fill_fixed_with_fee(0, price_1, size_1, 0).unwrap();
        
        assert_eq!(position.get_quantity(), 1_000_000_000);
        assert_eq!(position.get_entry_price(), 50_000 * SCALE as u64);

        // 2. Flip to Short: Sell 2.0 BTC @ $60,000
        // This should close the 1.0 Long (Profit $10,000)
        // And open a 1.0 Short @ $60,000
        let price_2 = 60_000 * SCALE as u64;
        let size_2 = 2 * SCALE as u64; // 2.0 BTC

        position.process_fill_fixed_with_fee(1, price_2, size_2, 0).unwrap();

        // Check resulting position
        assert_eq!(position.get_quantity(), -1_000_000_000, "Should be Short 1.0 BTC");
        
        // CRITICAL CHECK: Entry price of the NEW short position should be the sell price ($60,000)
        // The bug causes it to blend the old entry price ($50,000) with the new one
        let entry_price = position.get_entry_price();
        let expected_entry = 60_000 * SCALE as u64;
        
        assert_eq!(entry_price, expected_entry, 
            "Entry price after flip should be execution price. Got: {}, Expected: {}", 
            entry_price as f64 / SCALE as f64, 
            expected_entry as f64 / SCALE as f64
        );
        
        // Check PnL: Should be (60k - 50k) * 1.0 = $10,000
        let pnl = position.get_realized_pnl();
        let expected_pnl = 10_000 * SCALE;
        assert_eq!(pnl, expected_pnl, "PnL should be $10,000");
    }
}

