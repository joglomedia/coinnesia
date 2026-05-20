#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepKind {
    High,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiquiditySweep {
    pub kind: SweepKind,
    pub level: f64,
    pub reclaimed: bool,
}
