use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};

use crate::app::AppState;

pub struct ApiAuth;

impl FromRequestParts<AppState> for ApiAuth {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let expected = std::env::var(&state.config.server.auth_token_env).ok();
        let Some(expected) = expected else {
            return Err(StatusCode::UNAUTHORIZED);
        };
        let actual = parts
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));

        if actual == Some(expected.as_str()) {
            Ok(Self)
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
