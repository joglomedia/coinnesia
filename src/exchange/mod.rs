pub mod binance;
pub mod bybit;
pub mod mexc;
pub mod okx;
pub mod paper;

use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    Oco,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderResponse {
    pub order_id: String,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Balance {
    pub asset: String,
    pub available: Decimal,
    pub locked: Decimal,
}

#[async_trait]
pub trait Exchange: Send + Sync {
    async fn place_order(&self, request: OrderRequest) -> Result<OrderResponse>;
    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<()>;
    async fn balances(&self) -> Result<Vec<Balance>>;
}
