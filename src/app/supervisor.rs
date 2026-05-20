use std::time::Duration;

use anyhow::{bail, Result};
use tokio::{task::JoinSet, time};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::app::{reconciliation::can_unlock_live_trading, AppState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerHandle {
    pub name: &'static str,
    pub status: WorkerStatus,
}

impl WorkerHandle {
    pub const fn running(name: &'static str) -> Self {
        Self {
            name,
            status: WorkerStatus::Running,
        }
    }
}

pub struct Supervisor {
    state: AppState,
    shutdown: CancellationToken,
    workers: Vec<&'static str>,
}

impl Supervisor {
    pub fn new(state: AppState, shutdown: CancellationToken) -> Self {
        Self {
            state,
            shutdown,
            workers: vec!["scanner", "alert", "trading", "reconciliation"],
        }
    }

    pub async fn run(self) -> Result<()> {
        let mut join_set = JoinSet::new();

        for worker in self.workers {
            let state = self.state.clone();
            let shutdown = self.shutdown.child_token();
            join_set.spawn(async move { run_worker(worker, state, shutdown).await });
        }

        loop {
            tokio::select! {
                _ = self.shutdown.cancelled() => {
                    info!("supervisor received shutdown");
                    break;
                }
                result = join_set.join_next() => {
                    match result {
                        Some(Ok(Ok(()))) => {}
                        Some(Ok(Err(error))) => {
                            error!(?error, "worker returned error");
                            self.state.health.set_component("supervisor", false);
                        }
                        Some(Err(error)) => {
                            error!(?error, "worker join failed");
                            self.state.health.set_component("supervisor", false);
                        }
                        None => break,
                    }
                }
            }
        }

        self.shutdown.cancel();
        while let Some(result) = join_set.join_next().await {
            if let Err(error) = result {
                error!(?error, "worker join failed during shutdown");
            }
        }
        self.state.health.set_component("supervisor", false);
        Ok(())
    }
}

async fn run_worker(
    name: &'static str,
    state: AppState,
    shutdown: CancellationToken,
) -> Result<()> {
    if name == "reconciliation" {
        return run_reconciliation_worker(state, shutdown).await;
    }

    if name == "trading" {
        return run_trading_worker(state, shutdown).await;
    }

    run_placeholder_loop(name, state, shutdown).await
}

async fn run_reconciliation_worker(state: AppState, shutdown: CancellationToken) -> Result<()> {
    state.health.set_component("reconciliation", false);
    info!(worker = "reconciliation", "startup reconciliation started");

    tokio::select! {
        _ = shutdown.cancelled() => {
            state.startup_gate.mark_failed("shutdown_before_reconciliation");
            state.health.set_component("reconciliation", false);
            info!(worker = "reconciliation", "worker stopped");
            Ok(())
        }
        _ = time::sleep(Duration::from_millis(100)) => {
            state.startup_gate.mark_passed(can_unlock_live_trading(&state.config));
            state.health.heartbeat("reconciliation");
            info!(
                worker = "reconciliation",
                live_trading_unlocked = state.startup_gate.live_trading_unlocked(),
                "startup reconciliation passed"
            );
            run_placeholder_loop("reconciliation", state, shutdown).await
        }
    }
}

async fn run_trading_worker(state: AppState, shutdown: CancellationToken) -> Result<()> {
    if state.config.trading.enabled
        && state.config.trading.mode == "live"
        && !state.startup_gate.live_trading_unlocked()
    {
        state.health.set_component("trading", false);
        info!(
            worker = "trading",
            "waiting for startup reconciliation gate"
        );
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    state.health.set_component("trading", false);
                    info!(worker = "trading", "worker stopped");
                    return Ok(());
                }
                _ = time::sleep(Duration::from_millis(50)) => {
                    if state.startup_gate.live_trading_unlocked() {
                        break;
                    }

                    if state.startup_gate.status().completed {
                        bail!("live trading blocked by startup reconciliation gate");
                    }
                }
            }
        }
    }

    run_placeholder_loop("trading", state, shutdown).await
}

async fn run_placeholder_loop(
    name: &'static str,
    state: AppState,
    shutdown: CancellationToken,
) -> Result<()> {
    state.health.heartbeat(name);
    info!(worker = name, "worker started");
    let heartbeat_secs = (state.config.runtime.health_stale_after_secs / 3).clamp(1, 30);
    let mut heartbeat = time::interval(Duration::from_secs(heartbeat_secs));

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                state.health.set_component(name, false);
                info!(worker = name, "worker stopped");
                return Ok(());
            }
            _ = heartbeat.tick() => {
                state.health.heartbeat(name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time;
    use tokio_util::sync::CancellationToken;

    use crate::{app::supervisor::Supervisor, config::AppConfig};

    #[tokio::test]
    async fn supervisor_reconciliation_allows_paper_trading_worker() {
        let mut config = AppConfig::from_default_toml().expect("default config parses");
        config.trading.enabled = true;
        config.trading.mode = "paper".to_owned();
        let state = crate::app::AppState::bootstrap(config)
            .await
            .expect("state boots");
        let shutdown = CancellationToken::new();
        let supervisor = Supervisor::new(state.clone(), shutdown.child_token());
        let task = tokio::spawn(supervisor.run());

        time::timeout(Duration::from_secs(1), async {
            loop {
                if state.health.is_healthy("trading") && state.startup_gate.status().completed {
                    break;
                }
                time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("trading worker should become healthy after reconciliation");

        assert!(state.startup_gate.status().completed);
        assert!(!state.startup_gate.live_trading_unlocked());

        shutdown.cancel();
        task.await
            .expect("supervisor task joins")
            .expect("supervisor exits cleanly");
        assert!(!state.health.is_healthy("supervisor"));
    }

    #[tokio::test]
    async fn supervisor_stops_workers_on_cancellation() {
        let config = AppConfig::from_default_toml().expect("default config parses");
        let state = crate::app::AppState::bootstrap(config)
            .await
            .expect("state boots");
        let shutdown = CancellationToken::new();
        let supervisor = Supervisor::new(state.clone(), shutdown.child_token());
        let task = tokio::spawn(supervisor.run());

        time::timeout(Duration::from_secs(1), async {
            loop {
                if state.health.is_healthy("scanner") {
                    break;
                }
                time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("scanner worker should start");

        shutdown.cancel();
        task.await
            .expect("supervisor task joins")
            .expect("supervisor exits cleanly");

        assert!(!state.health.is_healthy("scanner"));
        assert!(!state.health.is_healthy("alert"));
        assert!(!state.health.is_healthy("trading"));
        assert!(!state.health.is_healthy("reconciliation"));
    }

    #[tokio::test]
    async fn supervisor_keeps_live_trading_locked_without_runtime_dependencies() {
        let mut config = AppConfig::from_default_toml().expect("default config parses");
        config.trading.enabled = true;
        config.trading.mode = "live".to_owned();
        let state = crate::app::AppState::bootstrap(config)
            .await
            .expect("state boots");
        let shutdown = CancellationToken::new();
        let supervisor = Supervisor::new(state.clone(), shutdown.child_token());
        let task = tokio::spawn(supervisor.run());

        time::timeout(Duration::from_secs(1), async {
            loop {
                if state.startup_gate.status().completed && !state.health.is_healthy("supervisor") {
                    break;
                }
                time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("live trading gate should fail closed");

        assert!(!state.startup_gate.live_trading_unlocked());
        assert!(!state.health.is_healthy("trading"));

        shutdown.cancel();
        task.await
            .expect("supervisor task joins")
            .expect("supervisor exits cleanly");
    }

    #[tokio::test]
    async fn health_snapshot_marks_stale_components_unhealthy() {
        let config = AppConfig::from_default_toml().expect("default config parses");
        let state = crate::app::AppState::bootstrap(config)
            .await
            .expect("state boots");

        state.health.heartbeat("scanner");
        let snapshot = state.health.snapshot_with_staleness(Duration::from_secs(0));
        let component = snapshot
            .components
            .iter()
            .find(|component| component.name == "scanner")
            .expect("scanner component exists");

        assert!(component.stale);
        assert!(!component.healthy);
        assert!(!snapshot.healthy);
    }
}
