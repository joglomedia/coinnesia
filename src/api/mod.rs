pub mod auth;
pub mod dto;
pub mod errors;
pub mod handlers;
pub mod routes;

use axum::Router;

use crate::app::AppState;

pub fn router(state: AppState) -> Router {
    routes::router(state)
}
