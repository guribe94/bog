//! Bridge between execution::Executor and engine::Executor traits
//!
//! This module provides an adapter that allows execution::SimulatedExecutor
//! (which has realistic fill simulation) to be used with the generic Engine.

use super::Executor as EngineExecutor;
use crate::core::{fixed_point, Position, Signal, SignalAction};
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
        let scale = Decimal::from(fixed_point::SCALE);

        // Identify existing orders by Side to enable atomic amend
        // We need to collect IDs first to avoid holding references to executor
        let (existing_buy, existing_sell, extra_orders) = {
            let active_orders = self.executor.get_active_orders();
            let mut buy = None;
            let mut sell = None;
            let mut extras = Vec::new();

            for order in active_orders {
                match order.side {
                    Side::Buy => {
                        if buy.is_none() {
                            buy = Some(order.id.clone());
                        } else {
                            extras.push(order.id.clone());
                        }
                    }
                    Side::Sell => {
                        if sell.is_none() {
                            sell = Some(order.id.clone());
                        } else {
                            extras.push(order.id.clone());
                        }
                    }
                }
            }
            (buy, sell, extras)
        };

        // Cancel extra orders (e.g. duplicates)
        for id in extra_orders {
            self.executor.cancel_order(&id)?;
        }

        // Helper to place or amend order
        // We use a macro or just inline logic because closures interacting with mutable self are tricky
        // Logic: If id exists -> amend. Else -> place.
        
        match signal.action {
            SignalAction::QuoteBoth => {
                // Bid
                let bid_price = Decimal::from(signal.bid_price) / scale;
                let size = Decimal::from(signal.size) / scale;
                let bid_order = Order::limit(Side::Buy, bid_price, size);
                
                if let Some(id) = existing_buy {
                    self.executor.amend_order(&id, bid_order)?;
                } else {
                    self.executor.place_order(bid_order)?;
                }

                // Ask
                let ask_price = Decimal::from(signal.ask_price) / scale;
                let ask_order = Order::limit(Side::Sell, ask_price, size);
                
                if let Some(id) = existing_sell {
                    self.executor.amend_order(&id, ask_order)?;
                } else {
                    self.executor.place_order(ask_order)?;
                }
            }
            SignalAction::QuoteBid => {
                // Bid
                let bid_price = Decimal::from(signal.bid_price) / scale;
                let size = Decimal::from(signal.size) / scale;
                let bid_order = Order::limit(Side::Buy, bid_price, size);
                
                if let Some(id) = existing_buy {
                    self.executor.amend_order(&id, bid_order)?;
                } else {
                    self.executor.place_order(bid_order)?;
                }
                
                // Cancel Ask if exists
                if let Some(id) = existing_sell {
                    self.executor.cancel_order(&id)?;
                }
            }
            SignalAction::QuoteAsk => {
                // Cancel Bid if exists
                if let Some(id) = existing_buy {
                    self.executor.cancel_order(&id)?;
                }

                // Ask
                let ask_price = Decimal::from(signal.ask_price) / scale;
                let size = Decimal::from(signal.size) / scale;
                let ask_order = Order::limit(Side::Sell, ask_price, size);
                
                if let Some(id) = existing_sell {
                    self.executor.amend_order(&id, ask_order)?;
                } else {
                    self.executor.place_order(ask_order)?;
                }
            }
            SignalAction::TakePosition => {
                // Cancel Quotes before taking position
                if let Some(id) = existing_buy { self.executor.cancel_order(&id)?; }
                if let Some(id) = existing_sell { self.executor.cancel_order(&id)?; }

                // Market order - use bid for buy, ask for sell
                let (price, exec_side) = match signal.side {
                    crate::core::Side::Buy => (signal.ask_price, Side::Buy), // Hit the ask
                    crate::core::Side::Sell => (signal.bid_price, Side::Sell), // Hit the bid
                };
                let price_decimal = Decimal::from(price) / scale;
                let size = Decimal::from(signal.size) / scale;

                let order = Order::limit(exec_side, price_decimal, size);
                self.executor.place_order(order)?;
            }
            SignalAction::CancelAll => {
                if let Some(id) = existing_buy { self.executor.cancel_order(&id)?; }
                if let Some(id) = existing_sell { self.executor.cancel_order(&id)?; }
            }
            SignalAction::NoAction => {
                // Do nothing
            }
        }
        Ok(())
    }

    fn cancel_all(&mut self) -> Result<()> {
        self.executor.cancel_all_orders()
    }

    fn name(&self) -> &'static str {
        "BridgedSimulatedExecutor"
    }

    fn get_fills(&mut self, fills: &mut Vec<Fill>) {
        self.executor.get_fills(fills)
    }

    fn dropped_fill_count(&self) -> u64 {
        self.executor.dropped_fill_count()
    }

    fn get_open_exposure(&self) -> (i64, i64) {
        self.executor.get_open_exposure()
    }
}
