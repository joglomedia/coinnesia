#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BacktestMetrics {
    pub win_rate: f64,
    pub max_drawdown_pct: f64,
    pub profit_factor: f64,
}
