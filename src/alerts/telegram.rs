use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::{alerts::AlertSink, config::TelegramAlertConfig, strategy::SignalResult};

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

pub fn format_signal(signal: &SignalResult) -> String {
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
        strategy::{confidence::ConfidenceScore, SignalResult, SignalState},
    };

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
}
