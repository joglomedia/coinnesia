use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::{
    api::auth::ApiAuth,
    api::dto::{ConfigSummary, HealthResponse},
    app::AppState,
};

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(
        HealthResponse::from_snapshot(state.health.snapshot())
            .with_reconciliation(state.startup_gate.status()),
    )
}

pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let response = HealthResponse::from_snapshot(state.health.snapshot())
        .with_reconciliation(state.startup_gate.status());
    let status = if response.healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(response))
}

pub async fn metrics(State(state): State<AppState>) -> String {
    let snapshot = state.health.snapshot();
    format!(
        "coinnesia_up 1\ncoinnesia_components {}\n",
        snapshot.components.len()
    )
}

pub async fn config_summary(State(state): State<AppState>) -> Json<ConfigSummary> {
    Json(ConfigSummary::from_config(&state.config))
}

pub async fn scan_trigger_placeholder(_auth: ApiAuth) -> Json<Value> {
    Json(json!({
        "accepted": true,
        "reason": "scan trigger endpoint scaffolded; scanner service wiring pending"
    }))
}
