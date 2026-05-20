use crate::Timeframe;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeframeSet {
    pub primary: Timeframe,
    pub confirmations: Vec<Timeframe>,
}
