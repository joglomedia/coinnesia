pub mod alerts;
pub mod api;
pub mod app;
pub mod assets;
pub mod backtest;
pub mod cache;
pub mod config;
pub mod data;
pub mod exchange;
pub mod indicators;
pub mod observability;
pub mod portfolio;
pub mod risk;
pub mod scanner;
pub mod storage;
pub mod strategy;
pub mod trading;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Btc,
    Altcoin,
    Gold,
    Forex,
    StocksIdx,
    StocksUs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Timeframe {
    M1,
    M5,
    M15,
    H1,
    H4,
    D1,
    W1,
    Mn1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candle {
    pub ts: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Candle {
    pub fn true_range(&self, prev_close: Option<f64>) -> f64 {
        match prev_close {
            Some(prev_close) => {
                let high_low = self.high - self.low;
                let high_close = (self.high - prev_close).abs();
                let low_close = (self.low - prev_close).abs();
                high_low.max(high_close).max(low_close)
            }
            None => self.high - self.low,
        }
    }

    pub fn body(&self) -> f64 {
        (self.close - self.open).abs()
    }

    pub fn direction(&self) -> strategy::SignalDirection {
        if self.close > self.open {
            strategy::SignalDirection::Long
        } else if self.close < self.open {
            strategy::SignalDirection::Short
        } else {
            strategy::SignalDirection::Wait
        }
    }
}
