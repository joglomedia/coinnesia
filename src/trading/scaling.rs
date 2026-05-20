use crate::config::ScalingConfig;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScalingPlan {
    pub ew1_pct: f64,
    pub ew2_pct: f64,
    pub ew3_pct: f64,
    pub deep_add_pct: f64,
}

impl ScalingPlan {
    pub fn from_config(config: &ScalingConfig) -> Self {
        Self {
            ew1_pct: config.ew1_pct,
            ew2_pct: config.ew2_pct,
            ew3_pct: config.ew3_pct,
            deep_add_pct: config.deep_add_pct,
        }
    }

    pub fn total_pct(self) -> f64 {
        self.ew1_pct + self.ew2_pct + self.ew3_pct + self.deep_add_pct
    }
}

#[cfg(test)]
mod tests {
    use crate::config::AppConfig;

    use super::*;

    #[test]
    fn default_scaling_does_not_exceed_full_position() {
        let config = AppConfig::from_default_toml().unwrap();
        assert_eq!(
            ScalingPlan::from_config(&config.trading.scaling).total_pct(),
            100.0
        );
    }
}
