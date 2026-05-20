use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use async_trait::async_trait;

use super::{Balance, Exchange, OrderRequest, OrderResponse};

#[derive(Debug, Default)]
pub struct PaperExchange {
    next_id: AtomicU64,
}

#[async_trait]
impl Exchange for PaperExchange {
    async fn place_order(&self, request: OrderRequest) -> Result<OrderResponse> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(OrderResponse {
            order_id: format!("paper-{}-{}", request.symbol, id),
            accepted: true,
        })
    }

    async fn cancel_order(&self, _symbol: &str, _order_id: &str) -> Result<()> {
        Ok(())
    }

    async fn balances(&self) -> Result<Vec<Balance>> {
        Ok(Vec::new())
    }
}
