//! Shared helpers for the V61.x Pine parity test harness (sub-phase 1.7.13).
//!
//! Each asset-class suite (BTC V61.9, Altcoin V62, Gold V1, Forex V58, IDX V5)
//! ships a deterministic candle generator plus a captured `PanelReport` JSON
//! fixture. The harness runs `SignalGenerator::evaluate` on the candle series
//! and asserts the live `PanelReport` matches the captured fixture.
//!
//! ## Regenerating fixtures
//!
//! When the Pine reference panel changes (or a Rust panel field is added),
//! regenerate the captured fixtures with:
//!
//! ```bash
//! UPDATE_PARITY_FIXTURES=1 cargo test --test parity
//! ```
//!
//! The harness will overwrite each `tests/fixtures/parity/<asset>.panel.json`
//! file with the current live output instead of asserting. Commit the diff
//! after manually diffing against the Pine reference panel.

#![allow(dead_code)]

use chrono::{DateTime, Duration, TimeZone, Utc};
use coinnesia::{
    config::{AppConfig, SymbolConfig},
    strategy::{
        panel::PanelReport,
        signals::{SignalGenerator, SignalResult},
    },
    AssetClass, Candle,
};
use std::{fs, path::PathBuf};

/// The well-known anchor timestamp shared by every parity fixture. Keeping it
/// stable across regenerations means the on-disk JSON snapshots only change
/// when the panel structure changes.
pub fn anchor() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 6, 0, 0, 0).unwrap()
}

/// Resolve the fixture path for a given asset suite. The file is the
/// authoritative captured panel — load it to compare, write it to regenerate.
pub fn fixture_path(stem: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push("parity");
    p.push(format!("{stem}.panel.json"));
    p
}

/// Generate a deterministic OHLCV series that satisfies the strategy's
/// indicator warm-ups and produces a meaningful directional move at the tail.
///
/// `direction` controls whether the tail break is bullish or bearish.
/// `volatility` scales the ATR amplitude — higher = wider candles.
/// `volume_base` sets the average volume; the tail break gets a 5x volume burst
/// so the breakout-volume gate fires.
pub fn synthesize_candles(
    count: usize,
    direction: TrendDirection,
    base_price: f64,
    volatility: f64,
    volume_base: f64,
    step: Duration,
) -> Vec<Candle> {
    assert!(count >= 80, "parity fixtures need at least 80 bars");
    let start = anchor() - step * (count as i32 - 1);
    let mut candles = Vec::with_capacity(count);
    for idx in 0..count {
        let breakout = idx + 1 == count;
        let trend_mag = match direction {
            TrendDirection::Long => idx as f64 * volatility * 0.6,
            TrendDirection::Short => -(idx as f64) * volatility * 0.6,
            TrendDirection::Sideways => 0.0,
        };
        let base = base_price + trend_mag;
        let body = volatility * if breakout { 4.0 } else { 1.0 };
        let (open, close) = match direction {
            TrendDirection::Long => (base, base + body),
            TrendDirection::Short => (base, base - body),
            TrendDirection::Sideways => (base, base),
        };
        let wick = volatility * 0.4;
        let (high, low) = if close >= open {
            (close + wick, open - wick)
        } else {
            (open + wick, close - wick)
        };
        let volume = if breakout {
            volume_base * 5.0
        } else {
            volume_base * (1.0 + (idx as f64 / count as f64) * 0.4)
        };
        candles.push(Candle {
            ts: start + step * (idx as i32),
            open,
            high,
            low,
            close,
            volume,
        });
    }
    candles
}

#[derive(Debug, Clone, Copy)]
pub enum TrendDirection {
    Long,
    Short,
    Sideways,
}

/// Build an `AppConfig` from the embedded default, then register the asset
/// under test as `symbol` with the requested class. Parity fixtures cover
/// asset-class differences, so a single symbol per fixture is enough.
pub fn config_with_symbol(symbol: &str, class: AssetClass) -> AppConfig {
    let mut cfg = AppConfig::from_default_toml().expect("default config parses");
    cfg.symbols = vec![SymbolConfig {
        symbol: symbol.to_owned(),
        asset_class: class,
        exchange: String::new(),
        timeframes: vec!["1d".into()],
        data_source: None,
    }];
    cfg
}

/// Run the strategy and pull out the panel. Every parity fixture asserts that
/// the panel was populated — even Wait/Freeze paths emit one per sub-phase
/// 1.7.10's contract.
pub fn evaluate(cfg: &AppConfig, symbol: &str, candles: &[Candle]) -> (SignalResult, PanelReport) {
    let result = SignalGenerator::new(cfg).evaluate(symbol, candles);
    let panel = result
        .panel
        .clone()
        .expect("strategy must emit a panel for every evaluation path");
    (result, panel)
}

/// Compare the live panel against the captured fixture.
///
/// When `UPDATE_PARITY_FIXTURES=1` is set the fixture is overwritten with the
/// live JSON instead — used to regenerate after a Pine update. The function
/// then returns early so the test still passes.
pub fn assert_panel_matches(stem: &str, panel: &PanelReport) {
    let path = fixture_path(stem);
    // Serialize via `Value` first so the on-disk JSON has sorted keys (the
    // default `serde_json::Map` is a BTreeMap), keeping the fixture diff
    // stable across struct-field reorderings.
    let live_value = serde_json::to_value(panel).expect("panel serializes");
    let live_pretty = serde_json::to_string_pretty(&live_value).expect("pretty json");

    if std::env::var_os("UPDATE_PARITY_FIXTURES").is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture dir");
        }
        fs::write(&path, &live_pretty).expect("write fixture");
        eprintln!("regenerated parity fixture: {}", path.display());
        return;
    }

    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing parity fixture {} ({e}). Run with UPDATE_PARITY_FIXTURES=1 to bootstrap.",
            path.display()
        )
    });

    // Compare the pretty-printed JSON text rather than parsed `Value`s.
    // serde_json's `Value` deserializer parses floats with a different
    // rounding algorithm than `f64::from_str`, so a value like
    // `65100.114285714284` round-trips through `Value` to a 1-ULP-different
    // f64 and re-serializes as `65100.11428571429` — producing spurious
    // "drift" against a fixture that was written from the same panel.
    // Text comparison sidesteps the lossy Value roundtrip entirely.
    if raw.trim_end() != live_pretty.trim_end() {
        panic!(
            "parity drift in {}\n=== captured ===\n{}\n=== live ===\n{}",
            path.display(),
            raw,
            live_pretty,
        );
    }
}
