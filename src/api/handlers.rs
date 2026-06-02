use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use crate::{
    alerts::telegram::format_signal,
    api::auth::ApiAuth,
    api::dto::{ConfigSummary, HealthResponse},
    app::AppState,
    cache::snapshots::SignalSnapshot,
    data::ConfiguredMarketData,
    scanner::Scanner,
    strategy::SignalResult,
};

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(
        HealthResponse::from_snapshot(state.health.snapshot_with_staleness(
            std::time::Duration::from_secs(state.config.runtime.health_stale_after_secs),
        ))
        .with_reconciliation(state.startup_gate.status()),
    )
}

pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let response = HealthResponse::from_snapshot(state.health.snapshot_with_staleness(
        std::time::Duration::from_secs(state.config.runtime.health_stale_after_secs),
    ))
    .with_reconciliation(state.startup_gate.status());
    let status = if response.healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(response))
}

pub async fn metrics(State(state): State<AppState>) -> String {
    let snapshot = state
        .health
        .snapshot_with_staleness(std::time::Duration::from_secs(
            state.config.runtime.health_stale_after_secs,
        ));
    state.metrics.render_prometheus(snapshot.components.len())
}

pub async fn config_summary(State(state): State<AppState>) -> Json<ConfigSummary> {
    Json(ConfigSummary::from_config(&state.config))
}

pub async fn scan_trigger(
    State(state): State<AppState>,
    _auth: ApiAuth,
) -> (StatusCode, Json<Value>) {
    let scanner = Scanner::with_resources(
        (*state.config).clone(),
        Arc::new(ConfiguredMarketData::from_config(&state.config)),
        state.db.clone(),
        state.cache.clone(),
    );
    match scanner.scan_once().await {
        Ok(report) => {
            state.metrics.inc_scan_cycle();
            state.metrics.add_symbols_scanned(report.scanned as u64);
            state
                .metrics
                .add_signals_generated(report.signals.len() as u64);
            (
                StatusCode::OK,
                Json(json!({
                    "accepted": true,
                    "cycle_id": report.cycle_id,
                    "scanned": report.scanned,
                    "signals": report.signals.len()
                })),
            )
        }
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "accepted": false,
                "reason": format!("scan failed: {error}")
            })),
        ),
    }
}

/// `GET /signals/:symbol` — returns the latest panel report JSON for the
/// requested symbol. Reads from the cache snapshot first (fast path used by
/// the scanner publisher); falls back to the latest DB row when the cache
/// is empty or disabled. Returns 404 when neither path has a record.
pub async fn signal_by_symbol(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Response {
    match load_latest_signal(&state, &symbol).await {
        Some(signal) => (StatusCode::OK, Json(signal)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "no_signal",
                "symbol": symbol,
                "reason": "no cached or persisted signal evaluation for this symbol"
            })),
        )
            .into_response(),
    }
}

/// `GET /panel/:symbol` — returns a human-friendly HTML rendering of the
/// latest panel report. Uses the same `format_signal` helper as the Telegram
/// alert sender so the panel layout stays in lock-step across surfaces.
/// Returns a 404 HTML page when no panel is available.
pub async fn panel_by_symbol(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Response {
    match load_latest_signal(&state, &symbol).await {
        Some(signal) => {
            let body = render_panel_page(&signal);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                body,
            )
                .into_response()
        }
        None => {
            let body = format!(
                "<!doctype html><meta charset=\"utf-8\"><title>{}</title>\
                 <body><h1>No panel</h1><p>No cached or persisted signal for \
                 <code>{}</code>.</p></body>",
                escape_html(&symbol),
                escape_html(&symbol),
            );
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                body,
            )
                .into_response()
        }
    }
}

async fn load_latest_signal(state: &AppState, symbol: &str) -> Option<SignalResult> {
    // Cache first: the scanner publisher writes a `SignalSnapshot` under
    // `snapshot:signal:<symbol>` each cycle, so a fresh cache hit always
    // beats a Postgres round-trip.
    if let Some(cache) = &state.cache {
        let key = cache.key(&["snapshot", "signal", symbol]);
        if let Ok(Some(snapshot)) = cache.get_json::<SignalSnapshot>(&key).await {
            return Some(snapshot.signal);
        }
    }
    // Fallback: hydrate from Postgres when the cache missed (or is disabled).
    if let Some(db) = &state.db {
        if let Ok(Some(record)) = db.signals().latest_for_symbol(symbol).await {
            return signal_from_record(&record);
        }
    }
    None
}

fn signal_from_record(record: &crate::storage::signals::SignalRecord) -> Option<SignalResult> {
    // Reconstruct the minimal `SignalResult` shape from the persisted record.
    // We rebuild `panel` and `entry_plan` from their JSONB blobs so callers
    // see the same payload they would have received live.
    let state = match record.state.to_uppercase().as_str() {
        "LONG" => crate::strategy::SignalState::Long,
        "SHORT" => crate::strategy::SignalState::Short,
        "FREEZE" => crate::strategy::SignalState::Freeze,
        _ => crate::strategy::SignalState::Wait,
    };
    let confidence = crate::strategy::confidence::ConfidenceScore::from_sides(
        record
            .confidence
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0),
        record
            .confidence
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0),
    );
    let entry_plan = record
        .entry_plan
        .as_ref()
        .and_then(|raw| serde_json::from_value(raw.clone()).ok());
    let panel = record
        .panel_report
        .as_ref()
        .and_then(|raw| serde_json::from_value(raw.clone()).ok());
    Some(SignalResult {
        symbol: record.symbol.clone(),
        state,
        confidence,
        reason: record.reason.clone(),
        entry_plan,
        panel,
    })
}

fn render_panel_page(signal: &SignalResult) -> String {
    // `format_signal` produces a Telegram-flavoured HTML fragment; we wrap
    // it in a minimal `<pre>`-style page so the rows align in a browser.
    let body = format_signal(signal);
    format!(
        "<!doctype html><meta charset=\"utf-8\">\
         <title>{} panel</title>\
         <style>body{{font-family:ui-monospace,Menlo,Consolas,monospace;line-height:1.5;padding:1rem;max-width:60rem;}}b{{display:inline-block;min-width:9rem;}}</style>\
         <body><pre style=\"white-space:pre-wrap;\">{}</pre></body>",
        escape_html(&signal.symbol),
        body,
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
