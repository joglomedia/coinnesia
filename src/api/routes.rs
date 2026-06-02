use axum::{
    middleware,
    routing::{get, post},
    Router,
};

use crate::{
    api::{handlers, metrics},
    app::AppState,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/ready", get(handlers::ready))
        .route("/metrics", get(handlers::metrics))
        .route("/config", get(handlers::config_summary))
        .route("/scan", post(handlers::scan_trigger))
        // 1.7.11 — panel surfaces. `/signals/{symbol}` returns the latest
        // panel report JSON; `/panel/{symbol}` renders the same panel as an
        // HTML page for human inspection.
        .route("/signals/{symbol}", get(handlers::signal_by_symbol))
        .route("/panel/{symbol}", get(handlers::panel_by_symbol))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            metrics::record_api_metrics,
        ))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use tokio::time::{sleep, Duration};
    use tower::ServiceExt;

    use crate::{api::router, app::AppState, config::AppConfig};

    async fn test_router(auth_token_env: &str) -> axum::Router {
        let mut config = AppConfig::from_default_toml().expect("default config parses");
        config.database.enabled = false;
        config.cache.enabled = false;
        config.alerts.enabled = false;
        config.server.auth_token_env = auth_token_env.to_owned();
        config.data_sources.primary = "empty".to_owned();
        config.data_sources.fallback = "empty".to_owned();
        let state = AppState::bootstrap(config).await.expect("state boots");
        state.health.set_component("supervisor", true);
        state.health.set_component("scanner", true);
        state.health.set_component("alert", true);
        state.health.set_component("trading", true);
        state.health.set_component("reconciliation", true);
        router(state)
    }

    async fn json_response(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        serde_json::from_slice(&bytes).expect("json response")
    }

    #[tokio::test]
    async fn health_returns_component_status() {
        let app = test_router("COINNESIA_TEST_API_TOKEN_HEALTH").await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["healthy"], true);
        assert!(body["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|component| { component["name"] == "api" && component["healthy"] == true }));
        assert!(body["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|component| { component["name"] == "api" && component["stale"] == false }));
    }

    #[tokio::test]
    async fn ready_returns_component_status() {
        let app = test_router("COINNESIA_TEST_API_TOKEN_READY").await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["healthy"], true);
    }

    #[tokio::test]
    async fn ready_reports_unavailable_when_component_is_unhealthy() {
        let mut config = AppConfig::from_default_toml().expect("default config parses");
        config.database.enabled = false;
        config.cache.enabled = false;
        config.alerts.enabled = false;
        let state = AppState::bootstrap(config).await.expect("state boots");
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = json_response(response).await;
        assert_eq!(body["healthy"], false);
    }

    #[tokio::test]
    async fn ready_reports_unavailable_when_worker_heartbeat_is_stale() {
        let mut config = AppConfig::from_default_toml().expect("default config parses");
        config.database.enabled = false;
        config.cache.enabled = false;
        config.alerts.enabled = false;
        config.runtime.health_stale_after_secs = 0;
        let state = AppState::bootstrap(config).await.expect("state boots");
        state.health.set_component("supervisor", true);
        state.health.heartbeat("scanner");
        state.health.set_component("alert", true);
        state.health.set_component("trading", true);
        state.health.set_component("reconciliation", true);
        sleep(Duration::from_millis(1)).await;

        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = json_response(response).await;
        let scanner = body["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|component| component["name"] == "scanner")
            .expect("scanner component exists");
        assert_eq!(scanner["stale"], true);
        assert_eq!(scanner["healthy"], false);
    }

    #[tokio::test]
    async fn config_summary_is_sanitized() {
        let app = test_router("COINNESIA_TEST_API_TOKEN_CONFIG").await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        let expected_symbols = crate::config::AppConfig::from_default_toml()
            .expect("default config parses")
            .symbols
            .len();
        assert_eq!(body["symbols"], expected_symbols);
        assert_eq!(body["exchange"], "paper");
        assert!(body.get("auth_token_env").is_none());
        assert!(body.get("api_key").is_none());
        assert!(body.get("api_secret").is_none());
    }

    #[tokio::test]
    async fn metrics_returns_runtime_counters() {
        let app = test_router("COINNESIA_TEST_API_TOKEN_METRICS").await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let body = String::from_utf8(bytes.to_vec()).expect("utf8 body");
        assert!(body.contains("coinnesia_up 1"));
        assert!(body.contains("coinnesia_scan_cycles_total"));
        assert!(body.contains("coinnesia_api_requests_total"));
        assert!(body.contains("coinnesia_exchange_errors_total"));
    }

    #[tokio::test]
    async fn health_response_includes_heartbeat_metadata() {
        let mut config = AppConfig::from_default_toml().expect("default config parses");
        config.database.enabled = false;
        config.cache.enabled = false;
        config.alerts.enabled = false;
        config.runtime.health_stale_after_secs = 60;
        let state = AppState::bootstrap(config).await.expect("state boots");
        state.health.heartbeat("scanner");
        state.health.set_component("supervisor", true);
        state.health.set_component("alert", true);
        state.health.set_component("trading", true);
        state.health.set_component("reconciliation", true);
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        let scanner = body["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|component| component["name"] == "scanner")
            .expect("scanner component exists");
        assert_eq!(scanner["heartbeat_required"], true);
        assert!(scanner["last_heartbeat_age_secs"].is_number());
    }

    #[tokio::test]
    async fn scan_requires_auth() {
        std::env::set_var("COINNESIA_TEST_API_TOKEN_REJECT", "secret-token");
        let app = test_router("COINNESIA_TEST_API_TOKEN_REJECT").await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/scan")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        std::env::remove_var("COINNESIA_TEST_API_TOKEN_REJECT");
    }

    #[tokio::test]
    async fn scan_accepts_bearer_auth() {
        std::env::set_var("COINNESIA_TEST_API_TOKEN_ACCEPT", "secret-token");
        let app = test_router("COINNESIA_TEST_API_TOKEN_ACCEPT").await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/scan")
                    .header("authorization", "Bearer secret-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["accepted"], true);
        std::env::remove_var("COINNESIA_TEST_API_TOKEN_ACCEPT");
    }

    #[tokio::test]
    async fn signal_by_symbol_returns_404_when_no_data() {
        // Acceptance for 1.7.11: when neither the cache nor the database
        // hold a record for the requested symbol, the API returns 404 with
        // a JSON body describing the miss. With cache + db disabled in the
        // test bootstrap the handler never finds a panel.
        let app = test_router("COINNESIA_TEST_API_TOKEN_SIG_404").await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/signals/UNKNOWN")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = json_response(response).await;
        assert_eq!(body["error"], "no_signal");
        assert_eq!(body["symbol"], "UNKNOWN");
    }

    #[tokio::test]
    async fn panel_by_symbol_renders_html_404_when_no_data() {
        let app = test_router("COINNESIA_TEST_API_TOKEN_PANEL_404").await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/panel/UNKNOWN")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        assert!(
            content_type.starts_with("text/html"),
            "expected HTML content-type, got: {content_type}"
        );
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let body = String::from_utf8(bytes.to_vec()).expect("utf8");
        assert!(body.contains("No panel"));
        assert!(body.contains("UNKNOWN"));
    }
}
