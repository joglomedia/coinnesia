use tokio_util::sync::CancellationToken;
use tracing::info;

pub async fn wait_for_shutdown(token: CancellationToken) {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("shutdown signal received");
        }
        _ = token.cancelled() => {
            info!("shutdown token cancelled");
        }
    }
}
