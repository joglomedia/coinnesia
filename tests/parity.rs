//! V61.x Pine reference panel parity harness (sub-phase 1.7.13).
//!
//! Runs the Rust `SignalGenerator` against a deterministic, captured candle
//! series for each supported asset class (BTC V61.9, Altcoin V62, Gold V1,
//! Forex V58, IDX V5) and asserts the resulting `PanelReport` matches the
//! committed reference panel JSON.
//!
//! Each sub-module is a self-contained `#[test]` case. Run the whole suite:
//!
//! ```bash
//! cargo test --test parity
//! ```
//!
//! See `tests/parity/common.rs` for the fixture format and regeneration flag.

#[path = "parity/common.rs"]
mod common;

#[path = "parity/altcoin_v62.rs"]
mod altcoin_v62;
#[path = "parity/btc_v619.rs"]
mod btc_v619;
#[path = "parity/forex_v58.rs"]
mod forex_v58;
#[path = "parity/gold_v1.rs"]
mod gold_v1;
#[path = "parity/idx_v5.rs"]
mod idx_v5;
