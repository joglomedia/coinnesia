use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::{
    api::auth::ApiAuth,
    api::dto::{ConfigSummary, HealthResponse},
    app::AppState,
    data::ConfiguredMarketData,
    scanner::Scanner,
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
