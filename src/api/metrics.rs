use std::time::Instant;

use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};

use crate::app::AppState;

pub async fn record_api_metrics(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let started_at = Instant::now();
    let response = next.run(request).await;
    let latency_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
    state.metrics.record_api_request(latency_ms);
    response
}
