use super::order_bridge::OrderStateWrapper;
use super::{ExecutionMode, Executor, Fill, Order, OrderId, OrderStatus};
use anyhow::{anyhow, Result};
use rust_decimal::prelude::ToPrimitive;
use std::collections::HashMap;
use tracing::{info, warn};

/// Lighter DEX executor stub
/// This is a placeholder that logs API calls without making real requests
/// TODO Phase 8: Replace with real Lighter DEX SDK integration
///
/// Now uses OrderStateWrapper internally for compile-time state validation!
pub struct LighterExecutor {
    /// Orders stored as state machine wrappers for type-safe transitions
    orders: HashMap<OrderId, OrderStateWrapper>,
    pending_fills: Vec<Fill>,
    mode: ExecutionMode,
    api_url: String,
    /// WebSocket URL - reserved for Phase 7 live integration with Lighter SDK
    #[allow(dead_code)]
    ws_url: String,
}

impl LighterExecutor {
    pub fn new(api_url: String, ws_url: String) -> Self {
        warn!("LighterExecutor is a STUB implementation - no real orders will be placed!");
        info!("API URL: {}", api_url);
        info!("WebSocket URL: {}", ws_url);

        Self {
            orders: HashMap::new(),
            pending_fills: Vec::new(),
            mode: ExecutionMode::Live,
            api_url,
            ws_url,
        }
    }

    // TODO: Remove from_config in favor of const-based configuration
    /*
    pub fn from_config(config: &crate::config::LighterConfig) -> Result<Self> {
        Ok(Self::new(
            config.api_url.clone(),
            config.ws_url.clone(),
        ))
    }
    */
}

impl Executor for LighterExecutor {
    fn place_order(&mut self, order: Order) -> Result<OrderId> {
        info!(
            "STUB: Would place order on Lighter DEX: {} {} @ {} (size: {})",
            order.side, order.id, order.price, order.size
        );
        info!("  API endpoint: POST {}/orders", self.api_url);
        info!(
            "  Request body: {{\"side\": \"{}\", \"price\": {}, \"size\": {}}}",
            order.side, order.price, order.size
        );

        let order_id = order.id.clone();

        // Create state machine wrapper from legacy order (WITH VALIDATION!)
        let mut order_wrapper = OrderStateWrapper::from_legacy(&order)
            .map_err(|e| anyhow!("Invalid order ID: {}", e))?;

        // Acknowledge the order (Pending → Open) using type-safe transition
        if let Err(e) = order_wrapper.acknowledge() {
            return Err(anyhow!("Failed to acknowledge order: {}", e));
        }

        // Store the state machine wrapper
        self.orders.insert(order_id.clone(), order_wrapper);

        warn!("STUB: Order {} logged but NOT sent to exchange", order_id);

        Ok(order_id)
    }

    fn cancel_order(&mut self, order_id: &OrderId) -> Result<()> {
        info!("STUB: Would cancel order {} on Lighter DEX", order_id);
        info!(
            "  API endpoint: DELETE {}/orders/{}",
            self.api_url, order_id
        );

        if let Some(order_wrapper) = self.orders.get_mut(order_id) {
            if order_wrapper.is_active() {
                // Use state machine to cancel (type-safe!)
                if let Err(e) = order_wrapper.cancel() {
                    return Err(anyhow!("Failed to cancel order {}: {}", order_id, e));
                }

                warn!(
                    "STUB: Order {} marked as cancelled but NOT sent to exchange",
                    order_id
                );
                Ok(())
            } else {
                Err(anyhow!("Order {} is not active", order_id))
            }
        } else {
            Err(anyhow!("Order {} not found", order_id))
        }
    }

    fn get_fills(&mut self) -> Vec<Fill> {
        // STUB: No real fills since we're not executing
        warn!("STUB: No fills available (not connected to real exchange)");
        std::mem::take(&mut self.pending_fills)
    }

    fn get_order_status(&self, order_id: &OrderId) -> Option<OrderStatus> {
        info!("STUB: Would query order status for {}", order_id);
        info!("  API endpoint: GET {}/orders/{}", self.api_url, order_id);

        // Use state machine to get status (type-safe!)
        self.orders.get(order_id).map(|wrapper| wrapper.status())
    }

    fn get_active_orders(&self) -> Vec<&Order> {
        info!("STUB: Would query active orders");
        info!("  API endpoint: GET {}/orders?status=active", self.api_url);

        // Compute legacy view on-demand (saves memory and hot-path latency)
        self.orders
            .values()
            .filter(|wrapper| wrapper.is_active())
            .map(|wrapper| wrapper.to_legacy())
            .collect::<Vec<Order>>()
            .leak()
            .iter()
            .collect()
    }

    fn execution_mode(&self) -> ExecutionMode {
        self.mode
    }

    fn get_open_exposure(&self) -> (i64, i64) {
        let mut long_exposure = 0i64;
        let mut short_exposure = 0i64;

        for wrapper in self.orders.values() {
            if wrapper.is_active() {
                let order = wrapper.to_legacy();
                let remaining = order.remaining_size();

                // Convert to fixed-point i64
                let remaining_u64 = (remaining * rust_decimal::Decimal::from(1_000_000_000))
                    .to_u64()
                    .unwrap_or(0);

                match order.side {
                    super::Side::Buy => long_exposure += remaining_u64 as i64,
                    super::Side::Sell => short_exposure += remaining_u64 as i64,
                }
            }
        }
        (long_exposure, short_exposure)
    }
}

// Future: Real implementation structure
//
// pub struct RealLighterExecutor {
//     client: LighterClient,  // HTTP client
//     ws: WebSocketStream,     // WebSocket for fills
//     orders: HashMap<OrderId, Order>,
//     api_key: String,
//     private_key: PrivateKey,
// }
//
// impl RealLighterExecutor {
//     pub async fn new(config: &LighterConfig) -> Result<Self> {
//         // Connect to Lighter DEX API
//         let client = LighterClient::new(&config.api_url, &config.api_key)?;
//
//         // Connect WebSocket for order updates
//         let ws = connect_async(&config.ws_url).await?;
//
//         // Load private key for signing
//         let private_key = load_private_key(&config.private_key_path)?;
//
//         Ok(Self {
//             client,
//             ws,
//             orders: HashMap::new(),
//             api_key: config.api_key.clone(),
//             private_key,
//         })
//     }
//
//     async fn sign_order(&self, order: &Order) -> Result<Signature> {
//         // Sign order with private key
//         todo!()
//     }
//
//     pub async fn place_order_async(&mut self, order: Order) -> Result<OrderId> {
//         // Sign the order
//         let signature = self.sign_order(&order).await?;
//
//         // Send to API
//         let response = self.client
//             .post("/orders")
//             .json(&OrderRequest {
//                 side: order.side,
//                 price: order.price,
//                 size: order.size,
//                 signature,
//             })
//             .send()
//             .await?;
//
//         let order_id = response.json::<OrderResponse>()?.order_id;
//         Ok(order_id)
//     }
//
//     pub async fn process_ws_messages(&mut self) -> Result<()> {
//         // Listen to WebSocket for fill updates
//         while let Some(msg) = self.ws.next().await {
//             match msg? {
//                 Message::Fill(fill) => {
//                     self.pending_fills.push(fill);
//                 }
//                 Message::OrderUpdate(update) => {
//                     if let Some(order) = self.orders.get_mut(&update.order_id) {
//                         order.status = update.status;
//                         order.filled_size = update.filled_size;
//                     }
//                 }
//                 _ => {}
//             }
//         }
//         Ok(())
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::Side;
    use rust_decimal_macros::dec;

    #[test]
    fn test_lighter_stub_creation() {
        let executor = LighterExecutor::new(
            "https://api.lighter.xyz".to_string(),
            "wss://ws.lighter.xyz".to_string(),
        );

        assert_eq!(executor.execution_mode(), ExecutionMode::Live);
        assert_eq!(executor.api_url, "https://api.lighter.xyz");
    }

    #[test]
    fn test_stub_place_order() {
        let mut executor = LighterExecutor::new(
            "https://api.lighter.xyz".to_string(),
            "wss://ws.lighter.xyz".to_string(),
        );

        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        let result = executor.place_order(order);

        assert!(result.is_ok());
        let order_id = result.unwrap();

        // Verify order is tracked
        assert!(executor.get_order_status(&order_id).is_some());
        assert_eq!(
            executor.get_order_status(&order_id),
            Some(OrderStatus::Open)
        );
    }

    #[test]
    fn test_stub_cancel_order() {
        let mut executor = LighterExecutor::new(
            "https://api.lighter.xyz".to_string(),
            "wss://ws.lighter.xyz".to_string(),
        );

        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));
        let order_id = executor.place_order(order).unwrap();

        let result = executor.cancel_order(&order_id);
        assert!(result.is_ok());

        assert_eq!(
            executor.get_order_status(&order_id),
            Some(OrderStatus::Cancelled)
        );
    }

    #[test]
    fn test_stub_no_fills() {
        let mut executor = LighterExecutor::new(
            "https://api.lighter.xyz".to_string(),
            "wss://ws.lighter.xyz".to_string(),
        );

        // STUB returns no fills
        let fills = executor.get_fills();
        assert_eq!(fills.len(), 0);
    }
}
