use crate::Timeframe;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeframeSet {
    pub primary: Timeframe,
    pub confirmations: Vec<Timeframe>,
}

pub fn parse_timeframe(value: &str) -> Option<Timeframe> {
    match value {
        "1m" | "M1" => Some(Timeframe::M1),
        "5m" | "M5" => Some(Timeframe::M5),
        "15m" | "M15" => Some(Timeframe::M15),
        "1h" | "H1" => Some(Timeframe::H1),
        "4h" | "H4" => Some(Timeframe::H4),
        "1d" | "D1" => Some(Timeframe::D1),
        "1w" | "W1" => Some(Timeframe::W1),
        "1mo" | "MN1" | "Mn1" => Some(Timeframe::Mn1),
        _ => None,
    }
}

pub fn threshold_for_timeframe(
    timeframe: Timeframe,
    config: &crate::config::StrategyConfig,
) -> f64 {
    match timeframe {
        Timeframe::M1 | Timeframe::M5 | Timeframe::M15 => config.min_confidence_15m,
        Timeframe::H1 => config.min_confidence_1h,
        Timeframe::H4 => config.min_confidence_4h,
        Timeframe::D1 | Timeframe::W1 | Timeframe::Mn1 => config.min_confidence_1d,
    }
}

pub fn timeframe_set(values: &[String]) -> TimeframeSet {
    let mut parsed = values
        .iter()
        .filter_map(|value| parse_timeframe(value))
        .collect::<Vec<_>>();
    if parsed.is_empty() {
        parsed.push(Timeframe::D1);
    }

    let primary = parsed[0];
    TimeframeSet {
        primary,
        confirmations: parsed.into_iter().skip(1).collect(),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::AppConfig;

    use super::*;

    #[test]
    fn maps_timeframes_to_config_thresholds() {
        let config = AppConfig::from_default_toml().expect("default config parses");
        assert_eq!(
            threshold_for_timeframe(Timeframe::M15, &config.strategy),
            config.strategy.min_confidence_15m
        );
        assert_eq!(
            threshold_for_timeframe(Timeframe::H1, &config.strategy),
            config.strategy.min_confidence_1h
        );
        assert_eq!(
            threshold_for_timeframe(Timeframe::D1, &config.strategy),
            config.strategy.min_confidence_1d
        );
    }
}
