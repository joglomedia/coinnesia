use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::{
    alerts::AlertSink,
    config::TelegramAlertConfig,
    strategy::{
        panel::{PanelAssetExtras, PanelReport},
        SignalResult, SignalState,
    },
    AssetClass,
};

#[derive(Debug, Clone)]
pub struct TelegramAlertSink {
    client: Client,
    bot_token: String,
    chat_id: String,
    api_base_url: String,
    parse_mode: String,
    disable_web_page_preview: bool,
}

impl TelegramAlertSink {
    pub fn from_config(config: &TelegramAlertConfig) -> Result<Option<Self>> {
        if !config.enabled {
            return Ok(None);
        }

        let bot_token = std::env::var(&config.bot_token_env).with_context(|| {
            format!(
                "{} must be set when Telegram alerts are enabled",
                config.bot_token_env
            )
        })?;
        let chat_id = std::env::var(&config.chat_id_env).with_context(|| {
            format!(
                "{} must be set when Telegram alerts are enabled",
                config.chat_id_env
            )
        })?;

        Ok(Some(Self {
            client: Client::new(),
            bot_token,
            chat_id,
            api_base_url: config.api_base_url.trim_end_matches('/').to_owned(),
            parse_mode: config.parse_mode.clone(),
            disable_web_page_preview: config.disable_web_page_preview,
        }))
    }

    pub fn new(
        bot_token: impl Into<String>,
        chat_id: impl Into<String>,
        api_base_url: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            bot_token: bot_token.into(),
            chat_id: chat_id.into(),
            api_base_url: api_base_url.into(),
            parse_mode: "HTML".to_owned(),
            disable_web_page_preview: true,
        }
    }
}

#[async_trait]
impl AlertSink for TelegramAlertSink {
    async fn send(&self, message: &str) -> Result<()> {
        let url = format!("{}/bot{}/sendMessage", self.api_base_url, self.bot_token);
        let response = self
            .client
            .post(url)
            .json(&serde_json::json!({
                "chat_id": self.chat_id,
                "text": message,
                "parse_mode": self.parse_mode,
                "disable_web_page_preview": self.disable_web_page_preview,
            }))
            .send()
            .await
            .context("failed to send Telegram message")?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read Telegram response")?;

        if !status.is_success() {
            anyhow::bail!("Telegram sendMessage returned {status}: {body}");
        }

        let response: TelegramResponse =
            serde_json::from_str(&body).context("failed to parse Telegram response")?;
        if !response.ok {
            anyhow::bail!(
                "Telegram sendMessage rejected message: {}",
                response
                    .description
                    .unwrap_or_else(|| "unknown error".to_owned())
            );
        }

        Ok(())
    }
}

/// Render a `SignalResult` as a Telegram HTML message.
///
/// When the signal carries a [`PanelReport`] (1.7.10) we emit the full
/// reference panel with PUTUSAN / TRADE SCORE / BIAS / CONF / SESI / FLOW /
/// EW1-3 / DEEP RISK / TRAP GATE / SL / TP1-3 (price + probability + ETA) /
/// RECLAIM rows, plus per-asset extras: IDX adds RVOL/CMF/OBV/RS rows, Gold
/// adds the XAU proxy bias row, Forex adds the H4+D1 HTF aggregate row. When
/// the panel is absent (legacy callers or test fixtures) we fall back to a
/// compact one-liner so the dedupe pipeline keeps working.
pub fn format_signal(signal: &SignalResult) -> String {
    match &signal.panel {
        Some(panel) => format_panel(&signal.symbol, signal, panel),
        None => format_legacy(signal),
    }
}

fn format_legacy(signal: &SignalResult) -> String {
    let entry_plan = signal.entry_plan.as_ref();
    let tp3_note = entry_plan
        .map(|plan| format!("TP3 optional: {:.8}", plan.take_profits.tp3_optional))
        .unwrap_or_else(|| "TP3 optional: no entry plan".to_owned());
    format!(
        concat!(
            "<b>{}</b> {:?}\n",
            "Confidence L:{:.1} S:{:.1} gap:{:.1}\n",
            "{}\n",
            "Reason: {}"
        ),
        escape_html(&signal.symbol),
        signal.state,
        signal.confidence.long,
        signal.confidence.short,
        signal.confidence.directional_gap,
        escape_html(&tp3_note),
        escape_html(&signal.reason)
    )
}

fn format_panel(symbol: &str, signal: &SignalResult, panel: &PanelReport) -> String {
    let header_emoji = match signal.state {
        SignalState::Long => "✅",
        SignalState::Short => "❌",
        SignalState::Wait => "⚠️",
        SignalState::Freeze => "🧊",
    };
    let asset_label = asset_class_label(panel.asset_class);
    let mut rows: Vec<String> = Vec::with_capacity(28);
    rows.push(format!(
        "{} <b>{}</b> · {} · {}",
        header_emoji,
        escape_html(symbol),
        asset_label,
        escape_html(&panel.putusan_text),
    ));
    rows.push(format!(
        "<b>TRADE SCORE</b> {}/100 · {}",
        panel.trade_score,
        escape_html(panel.trade_score_status.label()),
    ));
    if let Some(window) = panel.next_trade_window {
        rows.push(format!(
            "<b>NEXT TRADE</b> {} → {}",
            escape_html(&window.from.to_rfc3339()),
            escape_html(&window.until.to_rfc3339()),
        ));
    }
    rows.push(format!("<b>BIAS</b> {}", escape_html(&panel.bias_text)));
    rows.push(format!("<b>CONF</b> {}", escape_html(&panel.conf_text)));
    rows.push(format!("<b>SESI</b> {}", escape_html(&panel.session_text)));
    rows.push(format!(
        "<b>FLOW</b> {} · {}",
        escape_html(panel.flow_state.label()),
        escape_html(&panel.flow_text),
    ));

    // EW rows
    rows.push(format!(
        "<b>EW1</b> {} {}",
        escape_html(panel.ew1_status.label()),
        ew_emoji(panel.ew1_status),
    ));
    rows.push(format!(
        "<b>EW2</b> {} {}",
        escape_html(panel.ew2_status.label()),
        ew_emoji(panel.ew2_status),
    ));
    rows.push(format!(
        "<b>EW3</b> {} {}",
        escape_html(panel.ew3_status.label()),
        ew_emoji(panel.ew3_status),
    ));
    rows.push(format!(
        "<b>DEEP RISK</b> {} {}",
        escape_html(panel.deep_status.label()),
        deep_emoji(panel.deep_status),
    ));
    rows.push(format!(
        "<b>TRAP GATE</b> {}",
        escape_html(&panel.trap_gate_text),
    ));

    if let Some(entry_ideal) = panel.entry_ideal {
        rows.push(format!("<b>ENTRY IDEAL</b> {:.4}", entry_ideal));
    }
    if let Some(window) = panel.waktu_entry {
        rows.push(format!(
            "<b>WAKTU ENTRY</b> {} → {}",
            escape_html(&window.from.to_rfc3339()),
            escape_html(&window.until.to_rfc3339()),
        ));
    }

    if let (Some(width), Some(dist)) = (panel.sl_wide_label, panel.sl_atr_distance) {
        rows.push(format!(
            "<b>SL</b> {} · {:.2} ATR",
            escape_html(sl_width_label(width)),
            dist,
        ));
    }

    if let (Some(plan), Some(p1)) = (signal.entry_plan.as_ref(), panel.tp1_prob) {
        rows.push(format!(
            "<b>TP1</b> {:.4} · {}% {}",
            plan.take_profits.tp1,
            p1,
            panel
                .tp1_label
                .map(|l| escape_html(l.as_str()))
                .unwrap_or_default(),
        ));
    }
    if let (Some(plan), Some(p2)) = (signal.entry_plan.as_ref(), panel.tp2_prob) {
        rows.push(format!(
            "<b>TP2</b> {:.4} · {}% {}",
            plan.take_profits.tp2,
            p2,
            panel
                .tp2_label
                .map(|l| escape_html(l.as_str()))
                .unwrap_or_default(),
        ));
    }
    if let (Some(plan), Some(p3)) = (signal.entry_plan.as_ref(), panel.tp3_prob) {
        rows.push(format!(
            "<b>TP3</b> {:.4} · {}% {} (optional)",
            plan.take_profits.tp3_optional,
            p3,
            panel
                .tp3_label
                .map(|l| escape_html(l.as_str()))
                .unwrap_or_default(),
        ));
    }
    if let Some(eta) = panel.eta_tp1 {
        rows.push(format!(
            "<b>ETA TP1</b> {} → {}",
            escape_html(&eta.from.to_rfc3339()),
            escape_html(&eta.until.to_rfc3339()),
        ));
    }
    if let Some(eta) = panel.eta_tp2 {
        rows.push(format!(
            "<b>ETA TP2</b> {} → {}",
            escape_html(&eta.from.to_rfc3339()),
            escape_html(&eta.until.to_rfc3339()),
        ));
    }
    if let Some(eta) = panel.eta_tp3 {
        rows.push(format!(
            "<b>ETA TP3</b> {} → {}",
            escape_html(&eta.from.to_rfc3339()),
            escape_html(&eta.until.to_rfc3339()),
        ));
    }
    if let Some(price) = panel.reclaim_price {
        rows.push(format!("<b>RECLAIM</b> {:.4}", price));
    }

    append_asset_extras(&mut rows, panel.asset_class, &panel.extras);

    rows.join("\n")
}

fn append_asset_extras(
    rows: &mut Vec<String>,
    asset_class: AssetClass,
    extras: &PanelAssetExtras,
) {
    match asset_class {
        AssetClass::Gold => {
            if let Some(bias) = extras.xauusd_bias {
                rows.push(format!("<b>XAU PROXY</b> {:?}", bias));
            }
        }
        AssetClass::Forex => {
            if let (Some(long), Some(short), Some(max)) = (
                extras.htf_long_score,
                extras.htf_short_score,
                extras.htf_max_score,
            ) {
                let suffix = match (extras.htf_block_long, extras.htf_block_short) {
                    (Some(true), _) => " · BLOCK LONG",
                    (_, Some(true)) => " · BLOCK SHORT",
                    _ => "",
                };
                rows.push(format!(
                    "<b>HTF BIAS</b> long={}/{} short={}/{}{}",
                    long, max, short, max, suffix,
                ));
            }
        }
        AssetClass::StocksIdx => {
            if let Some(rvol) = extras.rvol_value {
                rows.push(format!("<b>RVOL</b> {:.2}", rvol));
            }
            if let Some(cmf) = extras.cmf_value {
                rows.push(format!("<b>CMF</b> {:.3}", cmf));
            }
            if let Some(obv) = extras.obv_slope_value {
                rows.push(format!("<b>OBV SLOPE</b> {:.2}", obv));
            }
            if let Some(rs) = extras.rs_vs_ihsg_value {
                rows.push(format!("<b>RS vs IHSG</b> {:.3}", rs));
            }
        }
        AssetClass::Btc | AssetClass::Altcoin | AssetClass::StocksUs => {}
    }
}

fn asset_class_label(asset_class: AssetClass) -> &'static str {
    match asset_class {
        AssetClass::Btc => "BTC",
        AssetClass::Altcoin => "ALT",
        AssetClass::Gold => "GOLD",
        AssetClass::Forex => "FX",
        AssetClass::StocksIdx => "IDX",
        AssetClass::StocksUs => "US",
    }
}

fn ew_emoji(status: crate::strategy::panel::EwStatus) -> &'static str {
    use crate::strategy::panel::EwStatus::*;
    match status {
        Touched => "✅",
        Valid => "✅",
        Watch => "⚠️",
        Map => "🔵",
        DeepReclaim => "🔁",
    }
}

fn deep_emoji(status: crate::strategy::panel::DeepStatus) -> &'static str {
    use crate::strategy::panel::DeepStatus::*;
    match status {
        Map => "🔵",
        Danger => "❌",
        ReclaimOk => "✅",
        Invalid => "🚫",
    }
}

fn sl_width_label(width: crate::strategy::sl_engine::SlWidth) -> &'static str {
    use crate::strategy::sl_engine::SlWidth::*;
    match width {
        Normal => "NORMAL",
        Wide => "WIDE",
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[derive(Debug, Deserialize)]
struct TelegramResponse {
    ok: bool,
    description: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::{
        config::AppConfig,
        strategy::{
            confidence::ConfidenceScore,
            panel::{
                DeepStatus, EwStatus, PanelAssetExtras, PanelReport, TradeScoreStatus,
            },
            plan_context::FlowState,
            SignalResult, SignalState,
        },
        AssetClass,
    };

    fn dummy_panel(asset_class: AssetClass) -> PanelReport {
        PanelReport {
            asset_class,
            putusan_text: "LONG".to_owned(),
            trade_score: 72,
            trade_score_status: TradeScoreStatus::Strong,
            next_trade_window: None,
            next_trade_expires_at: None,
            bias_text: "BELI 70%".to_owned(),
            conf_text: "BELI 70% | MIN 50%".to_owned(),
            session_text: "USA | 20:30-02:59 WIB".to_owned(),
            flow_text: "Normal/sedang, TP harus realistis".to_owned(),
            flow_state: FlowState::Mid,
            ew1_status: EwStatus::Valid,
            ew2_status: EwStatus::Watch,
            ew3_status: EwStatus::Map,
            deep_status: DeepStatus::Map,
            trap_gate_text: "BERSIH".to_owned(),
            entry_ideal: Some(100.12345),
            waktu_entry: None,
            waktu_entry_expires_at: None,
            sl_wide_label: None,
            sl_atr_distance: None,
            tp1_prob: None,
            tp2_prob: None,
            tp3_prob: None,
            tp1_label: None,
            tp2_label: None,
            tp3_label: None,
            eta_tp1: None,
            eta_tp2: None,
            eta_tp3: None,
            reclaim_price: None,
            extras: PanelAssetExtras::default(),
        }
    }

    #[test]
    fn alert_format_marks_tp3_optional() {
        let config = AppConfig::from_default_toml().expect("default config parses");
        let signal = SignalResult {
            symbol: "BTCUSDT".to_owned(),
            state: SignalState::Long,
            confidence: ConfidenceScore::from_sides(82.0, 20.0),
            reason: "test_signal".to_owned(),
            entry_plan: None,
            panel: None,
        }
        .with_entry_plan(&config, 100.0, 10.0);

        let message = super::format_signal(&signal);

        assert!(message.contains("TP3 optional"));
    }

    #[test]
    fn alert_format_marks_tp3_optional_without_entry_plan() {
        let signal = SignalResult {
            symbol: "BTCUSDT".to_owned(),
            state: SignalState::Wait,
            confidence: ConfidenceScore::neutral(),
            reason: "test_wait".to_owned(),
            entry_plan: None,
            panel: None,
        };

        let message = super::format_signal(&signal);

        assert!(message.contains("TP3 optional"));
    }

    #[test]
    fn panel_renders_full_row_set_with_asset_label() {
        // Acceptance for 1.7.11: when the panel is present, format_signal
        // emits the full Pine-equivalent row set with the asset-class label
        // and the core PUTUSAN/TRADE SCORE/BIAS/CONF/SESI/FLOW/EW1-3/DEEP
        // RISK/TRAP GATE headings.
        let panel = dummy_panel(AssetClass::Btc);
        let signal = SignalResult {
            symbol: "BTCUSDT".to_owned(),
            state: SignalState::Long,
            confidence: ConfidenceScore::from_sides(70.0, 20.0),
            reason: "six_layer_pass".to_owned(),
            entry_plan: None,
            panel: Some(panel),
        };
        let message = super::format_signal(&signal);

        for needle in [
            "BTCUSDT", "BTC", "TRADE SCORE", "BIAS", "CONF", "SESI", "FLOW", "EW1", "EW2",
            "EW3", "DEEP RISK", "TRAP GATE",
        ] {
            assert!(
                message.contains(needle),
                "panel must contain `{needle}` row; got:\n{message}"
            );
        }
    }

    #[test]
    fn idx_variant_adds_rvol_cmf_obv_rs_rows() {
        let mut panel = dummy_panel(AssetClass::StocksIdx);
        panel.extras.rvol_value = Some(1.42);
        panel.extras.cmf_value = Some(0.18);
        panel.extras.obv_slope_value = Some(120.5);
        panel.extras.rs_vs_ihsg_value = Some(0.034);
        let signal = SignalResult {
            symbol: "BBCA".to_owned(),
            state: SignalState::Wait,
            confidence: ConfidenceScore::neutral(),
            reason: "idx_test".to_owned(),
            entry_plan: None,
            panel: Some(panel),
        };
        let message = super::format_signal(&signal);
        for needle in ["RVOL", "CMF", "OBV SLOPE", "RS vs IHSG", "IDX"] {
            assert!(
                message.contains(needle),
                "IDX panel must contain `{needle}` row; got:\n{message}"
            );
        }
    }

    #[test]
    fn gold_variant_adds_xau_proxy_row() {
        let mut panel = dummy_panel(AssetClass::Gold);
        panel.extras.xauusd_bias = Some(crate::strategy::SignalDirection::Long);
        let signal = SignalResult {
            symbol: "PAXG".to_owned(),
            state: SignalState::Wait,
            confidence: ConfidenceScore::neutral(),
            reason: "gold_test".to_owned(),
            entry_plan: None,
            panel: Some(panel),
        };
        let message = super::format_signal(&signal);
        assert!(message.contains("XAU PROXY"));
        assert!(message.contains("GOLD"));
    }

    #[test]
    fn forex_variant_adds_htf_bias_row() {
        let mut panel = dummy_panel(AssetClass::Forex);
        panel.extras.htf_long_score = Some(0);
        panel.extras.htf_short_score = Some(2);
        panel.extras.htf_max_score = Some(2);
        panel.extras.htf_block_long = Some(true);
        panel.extras.htf_block_short = Some(false);
        let signal = SignalResult {
            symbol: "EURUSD".to_owned(),
            state: SignalState::Wait,
            confidence: ConfidenceScore::neutral(),
            reason: "forex_test".to_owned(),
            entry_plan: None,
            panel: Some(panel),
        };
        let message = super::format_signal(&signal);
        assert!(message.contains("HTF BIAS"));
        assert!(message.contains("BLOCK LONG"));
        assert!(message.contains("FX"));
    }
}
