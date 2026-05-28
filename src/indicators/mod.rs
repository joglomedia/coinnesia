pub mod adx;
pub mod atr;
pub mod candle;
pub mod cmf;
pub mod ema;
pub mod htf_bias;
pub mod liquidity;
pub mod macd;
pub mod obv;
pub mod order_block;
pub mod regime;
pub mod relative_strength;
pub mod rsi;
pub mod rvol;
pub mod smc;
pub mod support_resistance;
pub mod volume;
pub mod vwap;

use crate::Candle;

pub trait Indicator {
    type Output;

    fn name(&self) -> &'static str;
    fn calculate(&self, candles: &[Candle]) -> Self::Output;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IndicatorPoint {
    pub value: f64,
    pub ready: bool,
}

impl IndicatorPoint {
    pub const fn pending() -> Self {
        Self {
            value: f64::NAN,
            ready: false,
        }
    }

    pub const fn ready(value: f64) -> Self {
        Self { value, ready: true }
    }
}
