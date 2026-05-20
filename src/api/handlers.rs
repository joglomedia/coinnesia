use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::{
    api::auth::ApiAuth,
    api::dto::{ConfigSummary, HealthResponse},
    app::AppState,
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

pub async fn scan_trigger_placeholder(
    State(_state): State<AppState>,
    _auth: ApiAuth,
) -> Json<Value> {
    Json(json!({
        "accepted": true,
        "reason": "scan trigger endpoint scaffolded; scanner service wiring pending"
    }))
}
