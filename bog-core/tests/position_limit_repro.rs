
#[cfg(test)]
mod tests {
    use bog_strategies::simple_spread::{SimpleSpread, ORDER_SIZE, SPREAD_BPS};
    use bog_core::core::{Position, SignalAction};
    use bog_core::engine::Strategy;
    use bog_core::orderbook::L2OrderBook;
    use bog_core::data::MarketSnapshot;
    use bog_core::config::MAX_POSITION;
    use huginn::shm::PADDING_SIZE;

    #[test]
    fn test_position_limit_overshoot() {
        let mut strategy = SimpleSpread::new();

        // Setup market
        let snapshot = MarketSnapshot {
            generation_start: 0,
            generation_end: 0,
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
            _padding: [0; PADDING_SIZE],
        };
        
        let mut book = L2OrderBook::new(1);
        book.sync_from_snapshot(&snapshot);

        // Setup position VERY close to limit
        // limit is typically 10 BTC (10_000_000_000)
        // ORDER_SIZE is 0.1 BTC (100_000_000)
        // Set position to MAX - 0.05 BTC
        let near_limit = MAX_POSITION as i64 - 50_000_000; 
        
        let position = Position::new();
        position.update_quantity(near_limit);
        
        // Strategy should realize that adding ORDER_SIZE would exceed limit
        // and thus should NOT quote bid (which increases position)
        let signal = strategy.calculate(&book, &position).unwrap();
        
        // If it quotes both, it's a bug because a fill would put us at MAX + 0.05
        println!("Current: {}, Max: {}, Order: {}", position.get_quantity(), MAX_POSITION, ORDER_SIZE);
        println!("Signal: {:?}", signal.action);
        
        if let SignalAction::QuoteBoth { .. } = signal.action {
            // This is the bug - we are quoting buy even though it would breach limits
            panic!("Strategy quoted BUY which would exceed position limit!");
        }
        
        if let SignalAction::QuoteBid { .. } = signal.action {
            panic!("Strategy quoted BUY which would exceed position limit!");
        }
    }
}

