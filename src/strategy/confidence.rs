use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConfidenceScore {
    pub long: f64,
    pub short: f64,
    pub directional_gap: f64,
}

impl ConfidenceScore {
    pub const fn neutral() -> Self {
        Self {
            long: 0.0,
            short: 0.0,
            directional_gap: 0.0,
        }
    }

    pub fn from_sides(long: f64, short: f64) -> Self {
        Self {
            long,
            short,
            directional_gap: (long - short).abs(),
        }
    }

    pub fn passes(self, threshold: f64, min_gap: f64) -> bool {
        self.long.max(self.short) >= threshold && self.directional_gap >= min_gap
    }

    /// Confidence value matching the chosen direction. Falls back to the
    /// max(long, short) for `Wait` so callers can still feed something sane
    /// into `f_prob_score` when the panel is being rendered without a side.
    pub fn score(self, direction: super::SignalDirection) -> f64 {
        use super::SignalDirection::*;
        match direction {
            Long => self.long,
            Short => self.short,
            Wait => self.long.max(self.short),
        }
    }
}
