#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrapDecision {
    pub blocks_signal: bool,
    pub penalty: f64,
}

impl TrapDecision {
    pub const fn allow() -> Self {
        Self {
            blocks_signal: false,
            penalty: 0.0,
        }
    }

    pub const fn block(penalty: f64) -> Self {
        Self {
            blocks_signal: true,
            penalty,
        }
    }
}
