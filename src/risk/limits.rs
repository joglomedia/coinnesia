#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyTradeLimit {
    pub max_trades: usize,
    pub used: usize,
}
