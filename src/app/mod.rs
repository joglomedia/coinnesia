pub mod reconciliation;
pub mod services;
pub mod shutdown;
pub mod supervisor;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use axum::http::StatusCode;
use tokio::{net::TcpListener, time};
use tokio_util::sync::CancellationToken;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};
use tracing::info;

use crate::{
    api, app::reconciliation::StartupGate, cache::Cache, config::AppConfig,
    observability::HealthRegistry, storage::Db,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub health: HealthRegistry,
    pub startup_gate: StartupGate,
    pub db: Option<Db>,
    pub cache: Option<Cache>,
}

impl AppState {
    pub async fn bootstrap(config: AppConfig) -> Result<Self> {
        let config = Arc::new(config);
        let health = HealthRegistry::new();
        let startup_gate = StartupGate::pending();
        let db = Db::connect_optional(&config.database).await?;
        let cache = Cache::connect_optional(&config.cache).await?;

        health.set_component("api", true);
        health.set_component("supervisor", false);
        health.set_component("database", db.is_some() || !config.database.enabled);
        health.set_component("cache", cache.is_some() || !config.cache.enabled);
        health.set_component("scanner", false);
        health.set_component("alert", false);
        health.set_component("trading", false);
        health.set_component("reconciliation", false);

        Ok(Self {
            config,
            health,
            startup_gate,
            db,
            cache,
        })
    }

    pub fn bind_addr(&self) -> Result<SocketAddr> {
        format!("{}:{}", self.config.server.host, self.config.server.port)
            .parse()
            .with_context(|| {
                format!(
                    "invalid server bind address {}:{}",
                    self.config.server.host, self.config.server.port
                )
            })
    }
}

pub async fn serve(config: AppConfig) -> Result<()> {
    let state = AppState::bootstrap(config).await?;
    let shutdown = CancellationToken::new();
    let supervisor = supervisor::Supervisor::new(state.clone(), shutdown.child_token());
    let addr = state.bind_addr()?;
    let router = api::router(state.clone())
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(state.config.server.request_timeout_secs),
        ));

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind Axum server on {addr}"))?;

    info!(%addr, "coinnesia service started");
    state.health.set_component("supervisor", true);
    let shutdown_timeout = Duration::from_secs(state.config.runtime.shutdown_timeout_secs);

    let server = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown::wait_for_shutdown(shutdown.clone()));
    let supervisor_task = tokio::spawn(supervisor.run());

    let server_result = server.await.context("Axum server failed");
    shutdown.cancel();

    match time::timeout(shutdown_timeout, supervisor_task).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(error))) => return Err(error).context("supervisor failed"),
        Ok(Err(error)) => return Err(error).context("supervisor task join failed"),
        Err(_) => {
            state.health.set_component("supervisor", false);
            tracing::warn!("supervisor shutdown timed out");
        }
    }

    server_result
}
