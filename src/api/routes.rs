use axum::{
    routing::{get, post},
    Router,
};

use crate::{api::handlers, app::AppState};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/ready", get(handlers::ready))
        .route("/metrics", get(handlers::metrics))
        .route("/config", get(handlers::config_summary))
        .route("/scan", post(handlers::scan_trigger_placeholder))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{api::router, app::AppState, config::AppConfig};

    async fn test_router() -> axum::Router {
        let mut config = AppConfig::from_default_toml().expect("default config parses");
        config.server.auth_token_env = "COINNESIA_TEST_API_TOKEN".to_owned();
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
        let app = test_router().await;
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
    }

    #[tokio::test]
    async fn ready_returns_component_status() {
        let app = test_router().await;
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
        let config = AppConfig::from_default_toml().expect("default config parses");
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
    async fn config_summary_is_sanitized() {
        let app = test_router().await;
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
        assert_eq!(body["symbols"], 3);
        assert_eq!(body["exchange"], "paper");
        assert!(body.get("auth_token_env").is_none());
        assert!(body.get("api_key").is_none());
        assert!(body.get("api_secret").is_none());
    }

    #[tokio::test]
    async fn scan_requires_auth() {
        std::env::set_var("COINNESIA_TEST_API_TOKEN", "secret-token");
        let app = test_router().await;
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
        std::env::remove_var("COINNESIA_TEST_API_TOKEN");
    }

    #[tokio::test]
    async fn scan_accepts_bearer_auth() {
        std::env::set_var("COINNESIA_TEST_API_TOKEN", "secret-token");
        let app = test_router().await;
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
        std::env::remove_var("COINNESIA_TEST_API_TOKEN");
    }
}
