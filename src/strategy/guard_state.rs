//! Per-symbol stateful guard counters for V61.6–V61.8 protective layers:
//! shock freeze, adaptive trap cooldown, deep-add reclaim, SMC trend state,
//! and trend-flip confirmation. The state is recomputed once per evaluation
//! and persisted between cycles via [`GuardStateStore`].

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    config::TrapGuardConfig,
    indicators::regime::MarketRegime,
    Candle,
};

use super::SignalDirection;

/// Persistent SMC trend state machine. Pine V61.9 keeps a similar variable
/// (`smcTrendState`) that records the latest validated structural shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmcTrendState {
    #[default]
    Idle,
    BullishExpansion,
    BearishExpansion,
    BullishChoch,
    BearishChoch,
}

impl SmcTrendState {
    pub fn direction(self) -> SignalDirection {
        match self {
            Self::BullishExpansion | Self::BullishChoch => SignalDirection::Long,
            Self::BearishExpansion | Self::BearishChoch => SignalDirection::Short,
            Self::Idle => SignalDirection::Wait,
        }
    }
}

/// State carried between scan cycles for a single symbol.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GuardState {
    /// Bars remaining where signals are blocked because a recent trap fired.
    pub trap_cooldown_bars: usize,
    /// Bars remaining where the signal is held Frozen because a Shock candle fired.
    pub shock_freeze_bars: usize,
    /// V61.4 kill-switch — bars since the last EW3/Deep-Add break without
    /// reclaim. Reset to zero when price reclaims the deep-add zone.
    pub deep_reclaim_bars: usize,
    /// Bars since the SMC trend state last flipped — feeds V61.7
    /// `flipConfirmBars`.
    pub since_flip_bars: usize,
    /// Latest validated SMC trend state.
    pub smc_trend: SmcTrendState,
    /// Last swing high used for `min_swing_distance_atr` validation.
    pub last_swing_high: Option<f64>,
    /// Last swing low used for `min_swing_distance_atr` validation.
    pub last_swing_low: Option<f64>,
    /// V61.4 last computed deep-add level for a long candidate. `None` until
    /// the entry-plan engine first surfaces a long deep-add. The scanner
    /// updates this after each emission so the next bar can detect break /
    /// reclaim against the same anchor.
    #[serde(default)]
    pub last_deep_add_long: Option<f64>,
    /// Mirror of `last_deep_add_long` for short candidates.
    #[serde(default)]
    pub last_deep_add_short: Option<f64>,
}

/// Inputs the strategy needs to advance the state at the close of a bar.
pub struct GuardAdvanceInput<'a> {
    pub regime: MarketRegime,
    /// Raw trap score from `evaluate_trap_guard` (0–100+). Values ≥
    /// `trap_score_threshold` set the adaptive cooldown.
    pub trap_score: f64,
    pub candles: &'a [Candle],
    pub atr: f64,
    /// Most recent swing high inside the structure-detection window.
    pub current_swing_high: Option<f64>,
    /// Most recent swing low inside the structure-detection window.
    pub current_swing_low: Option<f64>,
    /// Latest structural event from `detect_structure`.
    pub structure_event: crate::indicators::smc::StructureEvent,
    /// True when the latest close is below the active deep-add zone (long side)
    /// or above it (short side). Until the entry-plan rewrite in 1.7.5 ships,
    /// the scanner passes `false` and the counter stays inert.
    pub deep_add_broken: bool,
    /// True when the latest close has reclaimed the deep-add zone.
    pub deep_add_reclaimed: bool,
}

impl GuardState {
    /// Advance the state by one bar using `input`. Returns the new state.
    ///
    /// Ordering:
    /// 1. Decrement bar counters (cooldown, shock freeze, deep reclaim).
    /// 2. If `regime == Shock`, prime `shock_freeze_bars` to `config.shock_freeze_bars`.
    /// 3. If `trap_score ≥ trap_score_threshold`, set `trap_cooldown_bars`
    ///    to the adaptive band (low / mid / high) selected by score.
    /// 4. Maintain `deep_reclaim_bars` from `deep_add_broken` / `deep_add_reclaimed`.
    /// 5. Run the SMC trend state machine; transitions require the swing
    ///    delta to clear `min_swing_distance_atr * ATR`.
    pub fn advance(&self, config: &TrapGuardConfig, input: &GuardAdvanceInput<'_>) -> Self {
        let mut next = self.clone();

        // Decrement counters first. This means a freshly armed shock_freeze of
        // 5 actually blocks 5 future bars; on the bar that arms it we return
        // 5, and the next bar starts the decrement.
        next.trap_cooldown_bars = next.trap_cooldown_bars.saturating_sub(1);
        next.shock_freeze_bars = next.shock_freeze_bars.saturating_sub(1);

        // Shock detection arms the freeze counter for `shock_freeze_bars` bars.
        if input.regime == MarketRegime::Shock {
            next.shock_freeze_bars = config.shock_freeze_bars;
        }

        // Adaptive trap cooldown band selection. The bands are additive offsets
        // above `trap_score_threshold`; default thresholds put scores 60–74 at
        // 4 bars, 75–89 at 6 bars, ≥ 90 at 7 bars (Pine V61.7 `useAdaptiveTrapCooldown`).
        if input.trap_score >= config.trap_score_threshold {
            let band_bars = if input.trap_score >= config.cooldown_band_high_score {
                config.cooldown_band_high_bars
            } else if input.trap_score >= config.cooldown_band_mid_score {
                config.cooldown_band_mid_bars
            } else {
                config.cooldown_band_low_bars
            };
            // Prime to band_bars rather than max(prev, band_bars) — a fresh
            // trap re-arms the cooldown even if a smaller cooldown was active.
            next.trap_cooldown_bars = band_bars;
        }

        // V61.4 deep-add reclaim accounting. The break/reclaim booleans are
        // computed by the entry-plan engine; until 1.7.5 lands they remain
        // false and this branch is inert.
        if input.deep_add_broken && !input.deep_add_reclaimed {
            next.deep_reclaim_bars = next.deep_reclaim_bars.saturating_add(1);
        } else if input.deep_add_reclaimed {
            next.deep_reclaim_bars = 0;
        }

        // SMC trend state machine with swing-distance validation.
        let prior_state = next.smc_trend;
        let prior_high = next.last_swing_high;
        let prior_low = next.last_swing_low;
        let min_dist = (config.min_swing_distance_atr * input.atr).max(0.0);
        let bullish_valid = matches!(
            input.structure_event,
            crate::indicators::smc::StructureEvent::BullishBos
                | crate::indicators::smc::StructureEvent::BullishChoch
        ) && swing_delta_passes(prior_high, input.current_swing_high, min_dist);
        let bearish_valid = matches!(
            input.structure_event,
            crate::indicators::smc::StructureEvent::BearishBos
                | crate::indicators::smc::StructureEvent::BearishChoch
        ) && swing_delta_passes(prior_low, input.current_swing_low, min_dist);

        let next_state = if bullish_valid {
            match input.structure_event {
                crate::indicators::smc::StructureEvent::BullishBos => SmcTrendState::BullishExpansion,
                crate::indicators::smc::StructureEvent::BullishChoch => SmcTrendState::BullishChoch,
                _ => prior_state,
            }
        } else if bearish_valid {
            match input.structure_event {
                crate::indicators::smc::StructureEvent::BearishBos => SmcTrendState::BearishExpansion,
                crate::indicators::smc::StructureEvent::BearishChoch => SmcTrendState::BearishChoch,
                _ => prior_state,
            }
        } else {
            prior_state
        };

        if next_state != prior_state {
            next.since_flip_bars = 0;
        } else {
            next.since_flip_bars = next.since_flip_bars.saturating_add(1);
        }
        next.smc_trend = next_state;

        // Always remember the latest observed swings so future deltas can be
        // measured against them, even when the structural event did not flip
        // the state.
        if input.current_swing_high.is_some() {
            next.last_swing_high = input.current_swing_high;
        }
        if input.current_swing_low.is_some() {
            next.last_swing_low = input.current_swing_low;
        }

        next
    }

    /// True when either freeze or cooldown counters indicate emission should
    /// be suppressed at the start of the evaluator.
    pub fn is_frozen(&self) -> bool {
        self.shock_freeze_bars > 0
    }

    pub fn is_trap_cooldown_active(&self) -> bool {
        self.trap_cooldown_bars > 0
    }
}

fn swing_delta_passes(prior: Option<f64>, current: Option<f64>, min_dist: f64) -> bool {
    match (prior, current) {
        (Some(prior), Some(current)) => (current - prior).abs() >= min_dist,
        // Without a prior reference, the first observation is always valid.
        (None, Some(_)) => true,
        _ => false,
    }
}

/// Locate the maximum high and minimum low over the most recent `lookback`
/// candles. Used as the swing reference for SMC trend state transitions.
pub fn latest_swings(candles: &[Candle], lookback: usize) -> (Option<f64>, Option<f64>) {
    if candles.is_empty() {
        return (None, None);
    }
    let start = candles.len().saturating_sub(lookback.max(2));
    let window = &candles[start..];
    let mut high: Option<f64> = None;
    let mut low: Option<f64> = None;
    for c in window {
        high = Some(high.map_or(c.high, |h: f64| h.max(c.high)));
        low = Some(low.map_or(c.low, |l: f64| l.min(c.low)));
    }
    (high, low)
}

/// Persistence interface for `GuardState`. Implementations cover both
/// in-memory (tests, single-process) and Valkey-backed (multi-process)
/// deployments.
#[async_trait]
pub trait GuardStateStore: Send + Sync {
    async fn load(&self, symbol: &str) -> GuardState;
    async fn save(&self, symbol: &str, state: GuardState);
}

/// Default in-memory implementation. Survives across scan cycles within a
/// single process but is lost on restart; Valkey-backed equivalents persist
/// across restarts.
#[derive(Debug, Clone, Default)]
pub struct InMemoryGuardStore {
    inner: Arc<Mutex<HashMap<String, GuardState>>>,
}

impl InMemoryGuardStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl GuardStateStore for InMemoryGuardStore {
    async fn load(&self, symbol: &str) -> GuardState {
        self.inner
            .lock()
            .unwrap()
            .get(symbol)
            .cloned()
            .unwrap_or_default()
    }

    async fn save(&self, symbol: &str, state: GuardState) {
        self.inner.lock().unwrap().insert(symbol.to_owned(), state);
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use crate::config::AppConfig;
    use crate::indicators::smc::StructureEvent;

    use super::*;

    fn config() -> TrapGuardConfig {
        AppConfig::from_default_toml()
            .expect("default config parses")
            .trap_guard
    }

    fn flat_candles(len: usize) -> Vec<Candle> {
        let now = Utc::now();
        (0..len)
            .map(|i| Candle {
                ts: now + Duration::minutes(i as i64),
                open: 100.0,
                high: 100.5,
                low: 99.5,
                close: 100.0,
                volume: 1_000.0,
            })
            .collect()
    }

    #[test]
    fn shock_arms_freeze_counter() {
        let cfg = config();
        let state = GuardState::default();
        let input = GuardAdvanceInput {
            regime: MarketRegime::Shock,
            trap_score: 0.0,
            candles: &flat_candles(20),
            atr: 1.0,
            current_swing_high: Some(101.0),
            current_swing_low: Some(99.0),
            structure_event: StructureEvent::None,
            deep_add_broken: false,
            deep_add_reclaimed: false,
        };
        let next = state.advance(&cfg, &input);
        assert_eq!(next.shock_freeze_bars, cfg.shock_freeze_bars);
        assert!(next.is_frozen());
    }

    #[test]
    fn shock_freeze_decrements_each_bar() {
        let cfg = config();
        let mut state = GuardState {
            shock_freeze_bars: cfg.shock_freeze_bars,
            ..Default::default()
        };
        let benign_input = GuardAdvanceInput {
            regime: MarketRegime::Sideways,
            trap_score: 0.0,
            candles: &flat_candles(20),
            atr: 1.0,
            current_swing_high: Some(101.0),
            current_swing_low: Some(99.0),
            structure_event: StructureEvent::None,
            deep_add_broken: false,
            deep_add_reclaimed: false,
        };
        for _ in 0..cfg.shock_freeze_bars {
            state = state.advance(&cfg, &benign_input);
        }
        assert_eq!(state.shock_freeze_bars, 0);
        assert!(!state.is_frozen());
    }

    #[test]
    fn adaptive_cooldown_uses_high_band_for_high_score() {
        let cfg = config();
        let state = GuardState::default();
        let input = GuardAdvanceInput {
            regime: MarketRegime::Sideways,
            trap_score: cfg.cooldown_band_high_score,
            candles: &flat_candles(20),
            atr: 1.0,
            current_swing_high: Some(101.0),
            current_swing_low: Some(99.0),
            structure_event: StructureEvent::None,
            deep_add_broken: false,
            deep_add_reclaimed: false,
        };
        let next = state.advance(&cfg, &input);
        assert_eq!(next.trap_cooldown_bars, cfg.cooldown_band_high_bars);
    }

    #[test]
    fn smc_trend_state_requires_swing_distance() {
        let mut cfg = config();
        cfg.min_swing_distance_atr = 0.45;
        let state = GuardState {
            last_swing_high: Some(100.0),
            last_swing_low: Some(99.0),
            ..Default::default()
        };
        // Swing delta below the gate must NOT flip the state.
        let tiny_input = GuardAdvanceInput {
            regime: MarketRegime::TrendExpansion,
            trap_score: 0.0,
            candles: &flat_candles(20),
            atr: 1.0,
            current_swing_high: Some(100.1), // delta 0.1 < 0.45
            current_swing_low: Some(99.0),
            structure_event: StructureEvent::BullishBos,
            deep_add_broken: false,
            deep_add_reclaimed: false,
        };
        let next = state.advance(&cfg, &tiny_input);
        assert_eq!(next.smc_trend, SmcTrendState::Idle);

        // Larger delta clears the gate and flips to BullishExpansion.
        let big_input = GuardAdvanceInput {
            current_swing_high: Some(100.6), // delta 0.6 >= 0.45
            ..tiny_input
        };
        let next = state.advance(&cfg, &big_input);
        assert_eq!(next.smc_trend, SmcTrendState::BullishExpansion);
        assert_eq!(next.since_flip_bars, 0);
    }

    #[tokio::test]
    async fn in_memory_store_round_trips_state() {
        let store = InMemoryGuardStore::new();
        let mut state = GuardState::default();
        state.shock_freeze_bars = 3;
        store.save("BTCUSDT", state.clone()).await;
        let loaded = store.load("BTCUSDT").await;
        assert_eq!(loaded, state);
        // Unknown symbol returns default.
        assert_eq!(store.load("MISSING").await, GuardState::default());
    }
}
