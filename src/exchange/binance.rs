use anyhow::Result;
use async_trait::async_trait;

use crate::{
    config::BinanceConfig,
    exchange::{Balance, Exchange, OrderRequest, OrderResponse},
};

pub struct BinanceExchange {
    #[allow(dead_code)]
    config: BinanceConfig,
}

impl BinanceExchange {
    pub fn new(config: BinanceConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Exchange for BinanceExchange {
    async fn place_order(&self, _request: OrderRequest) -> Result<OrderResponse> {
        anyhow::bail!("BinanceExchange::place_order not yet implemented (Phase 2)")
    }

    async fn cancel_order(&self, _symbol: &str, _order_id: &str) -> Result<()> {
        anyhow::bail!("BinanceExchange::cancel_order not yet implemented (Phase 2)")
    }

    async fn balances(&self) -> Result<Vec<Balance>> {
        anyhow::bail!("BinanceExchange::balances not yet implemented (Phase 2)")
    }
}
