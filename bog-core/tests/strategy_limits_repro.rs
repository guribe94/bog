#[cfg(test)]
mod tests {
    use bog_core::config::MAX_POSITION;
    use bog_core::core::{Position, SignalAction};
    use bog_core::data::MarketSnapshot;
    use bog_core::engine::Strategy;
    use bog_strategies::simple_spread::SimpleSpread;

    // Helper to create snapshot
    fn create_snapshot() -> MarketSnapshot {
        MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000,
            best_bid_size: 1_000_000_000,
            best_ask_price: 50_010_000_000_000,
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        }
    }

    #[test]
    fn test_max_position_halts_quoting() {
        let mut strategy = SimpleSpread::new();
        let position = Position::new();

        // 1. Set position to MAX_POSITION (Long)
        // MAX_POSITION is 1_000_000_000 (1.0 BTC) by default
        // We need to simulate this by adding fills
        // Note: Position quantity is atomic, but we can update it directly for test via update_quantity
        position.update_quantity(MAX_POSITION);

        assert_eq!(position.get_quantity(), MAX_POSITION);

        // 2. Generate signal
        let snapshot = create_snapshot();
        let signal = strategy
            .calculate(&snapshot, &position)
            .expect("Should return a signal");

        // 3. Check signal
        // Since we are Long MAX, we should NOT be quoting Bid (which would increase position).
        // We should ONLY be quoting Ask (to reduce position).

        match signal.action {
            SignalAction::QuoteBoth => {
                panic!("CRITICAL: Strategy is quoting BOTH sides even at MAX_POSITION. This risks breaching limits!");
            }
            SignalAction::QuoteAsk => {
                // This is what we expect - only asking to reduce long position
                assert!(signal.ask_price > 0);
                assert_eq!(signal.bid_price, 0);
            }
            SignalAction::QuoteBid => {
                panic!("CRITICAL: Strategy is quoting BID at MAX_POSITION (Long). This is the opposite of what it should do!");
            }
            _ => {}
        }
    }

    #[test]
    fn test_max_short_halts_quoting() {
        let mut strategy = SimpleSpread::new();
        let position = Position::new();

        // 1. Set position to -MAX_POSITION (Short)
        // We assume MAX_SHORT is same magnitude as MAX_POSITION for this test
        // (Check config if not)
        position.update_quantity(-MAX_POSITION);

        assert_eq!(position.get_quantity(), -MAX_POSITION);

        // 2. Generate signal
        let snapshot = create_snapshot();
        let signal = strategy
            .calculate(&snapshot, &position)
            .expect("Should return a signal");

        // 3. Check signal
        // Since we are Short MAX, we should NOT be quoting Ask (which would increase short).
        // We should ONLY be quoting Bid (to reduce short).

        match signal.action {
            SignalAction::QuoteBoth => {
                panic!("CRITICAL: Strategy is quoting BOTH sides even at MAX_SHORT. This risks breaching limits!");
            }
            SignalAction::QuoteBid => {
                // This is what we expect - only bidding to reduce short position
                assert!(signal.bid_price > 0);
                assert_eq!(signal.ask_price, 0);
            }
            SignalAction::QuoteAsk => {
                panic!("CRITICAL: Strategy is quoting ASK at MAX_SHORT. This is the opposite of what it should do!");
            }
            _ => {}
        }
    }
}
