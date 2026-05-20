#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketRegime {
    Sideways,
    TrendExpansion,
    DistributionRisk,
    AccumulationRisk,
    Shock,
}

impl MarketRegime {
    pub const fn allows_signals(self) -> bool {
        matches!(self, Self::TrendExpansion)
    }
}
