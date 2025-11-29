//! Bridge between execution::Executor and engine::Executor traits
//!
//! This module provides an adapter that allows execution::SimulatedExecutor
//! (which has realistic fill simulation) to be used with the generic Engine.

use super::{Executor as EngineExecutor};
use crate::core::{Position, Signal, SignalAction};
use crate::execution::{Executor as ExecExecutor, Fill, Order, Side};
use anyhow::Result;
use rust_decimal::Decimal;

/// Adapter that bridges execution::Executor to engine::Executor
pub struct ExecutorBridge<E: ExecExecutor> {
    executor: E,
}

impl<E: ExecExecutor> ExecutorBridge<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }
}

impl<E: ExecExecutor> EngineExecutor for ExecutorBridge<E> {
    fn execute(&mut self, signal: Signal, _position: &Position) -> Result<()> {
        match signal.action {
            SignalAction::QuoteBoth => {
                // Convert u64 fixed-point to Decimal
                let bid_price = Decimal::from(signal.bid_price) / Decimal::from(1_000_000_000);
                let ask_price = Decimal::from(signal.ask_price) / Decimal::from(1_000_000_000);
                let size = Decimal::from(signal.size) / Decimal::from(1_000_000_000);

                // Place bid order
                let bid_order = Order::limit(Side::Buy, bid_price, size);
                self.executor.place_order(bid_order)?;

                // Place ask order
                let ask_order = Order::limit(Side::Sell, ask_price, size);
                self.executor.place_order(ask_order)?;
            }
            SignalAction::QuoteBid => {
                let bid_price = Decimal::from(signal.bid_price) / Decimal::from(1_000_000_000);
                let size = Decimal::from(signal.size) / Decimal::from(1_000_000_000);

                let bid_order = Order::limit(Side::Buy, bid_price, size);
                self.executor.place_order(bid_order)?;
            }
            SignalAction::QuoteAsk => {
                let ask_price = Decimal::from(signal.ask_price) / Decimal::from(1_000_000_000);
                let size = Decimal::from(signal.size) / Decimal::from(1_000_000_000);

                let ask_order = Order::limit(Side::Sell, ask_price, size);
                self.executor.place_order(ask_order)?;
            }
            SignalAction::TakePosition => {
                // Market order - use bid for buy, ask for sell
                let (price, exec_side) = match signal.side {
                    crate::core::Side::Buy => (signal.ask_price, Side::Buy),   // Hit the ask
                    crate::core::Side::Sell => (signal.bid_price, Side::Sell), // Hit the bid
                };
                let price_decimal = Decimal::from(price) / Decimal::from(1_000_000_000);
                let size = Decimal::from(signal.size) / Decimal::from(1_000_000_000);

                let order = Order::limit(exec_side, price_decimal, size);
                self.executor.place_order(order)?;
            }
            SignalAction::CancelAll => {
                // Cancel all active orders
                let active_orders: Vec<_> = self.executor.get_active_orders()
                    .iter()
                    .map(|o| o.id.clone())
                    .collect();

                for order_id in active_orders {
                    let _ = self.executor.cancel_order(&order_id);
                }
            }
            SignalAction::NoAction => {
                // Do nothing
            }
        }
        Ok(())
    }

    fn cancel_all(&mut self) -> Result<()> {
        // Get all active orders and cancel them
        let active_orders: Vec<_> = self.executor.get_active_orders()
            .iter()
            .map(|o| o.id.clone())
            .collect();

        for order_id in active_orders {
            // Ignore errors when cancelling (order may already be filled/cancelled)
            let _ = self.executor.cancel_order(&order_id);
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "BridgedSimulatedExecutor"
    }

    fn get_fills(&mut self) -> Vec<Fill> {
        self.executor.get_fills()
    }

    fn dropped_fill_count(&self) -> u64 {
        self.executor.dropped_fill_count()
    }

    fn get_open_exposure(&self) -> (i64, i64) {
        self.executor.get_open_exposure()
    }
}
