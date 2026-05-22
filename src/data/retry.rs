use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use tokio::sync::Mutex;
use tracing::warn;

use crate::config::RetryConfig;

/// Retry an async operation with exponential backoff and deterministic jitter.
/// Permanent client errors (HTTP 4xx, except 429) are not retried.
pub async fn with_retry<F, Fut, T>(config: &RetryConfig, operation: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut delay = Duration::from_millis(config.base_delay_ms);
    let max_delay = Duration::from_millis(config.max_delay_ms);

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) if attempt == config.max_retries => return Err(error),
            Err(error) => {
                // Do not retry permanent HTTP client errors (400-series except 429)
                let is_permanent = error
                    .downcast_ref::<reqwest::Error>()
                    .and_then(|e| e.status())
                    .is_some_and(|s| s.is_client_error() && s.as_u16() != 429);
                if is_permanent {
                    return Err(error);
                }

                // Deterministic jitter: add 0–7% of delay based on attempt number
                let jitter_factor = 1.0 + (attempt as f64 * 7919.0 % 100.0) / 1000.0;
                let jittered = delay.mul_f64(jitter_factor);

                warn!(
                    attempt,
                    delay_ms = jittered.as_millis(),
                    error = %error,
                    "retrying after transient error"
                );
                tokio::time::sleep(jittered).await;
                delay = (delay * 2).min(max_delay);
            }
        }
    }
    unreachable!()
}

/// In-memory sliding-window rate limiter. Thread-safe via `Arc<Mutex<...>>`.
/// Does not depend on Valkey; suitable for per-process throttling.
#[derive(Clone)]
pub struct RateLimiter {
    capacity: u32,
    window: Duration,
    state: Arc<Mutex<RateLimiterState>>,
}

struct RateLimiterState {
    window_start: Instant,
    count: u32,
}

impl RateLimiter {
    pub fn new(capacity: u32, window: Duration) -> Self {
        Self {
            capacity,
            window,
            state: Arc::new(Mutex::new(RateLimiterState {
                window_start: Instant::now(),
                count: 0,
            })),
        }
    }

    /// Acquire a slot. Blocks until a slot is available within the current window.
    pub async fn acquire(&self) {
        loop {
            let sleep_for = {
                let mut state = self.state.lock().await;
                let now = Instant::now();
                let elapsed = now.duration_since(state.window_start);

                if elapsed >= self.window {
                    // Start a new window
                    state.window_start = now;
                    state.count = 0;
                }

                if state.count < self.capacity {
                    state.count += 1;
                    return;
                }

                // Window is full — sleep until it resets
                self.window.saturating_sub(elapsed)
            };

            tokio::time::sleep(sleep_for.max(Duration::from_millis(1))).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn rate_limiter_allows_up_to_capacity_immediately() {
        let limiter = RateLimiter::new(5, Duration::from_secs(1));
        let start = Instant::now();
        for _ in 0..5 {
            limiter.acquire().await;
        }
        assert!(start.elapsed() < Duration::from_millis(50), "first 5 should not block");
    }

    #[tokio::test]
    async fn retry_succeeds_on_first_attempt() {
        let config = RetryConfig { max_retries: 3, base_delay_ms: 10, max_delay_ms: 100 };
        let result = with_retry(&config, || async { Ok::<i32, anyhow::Error>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn retry_returns_error_after_max_attempts() {
        let config = RetryConfig { max_retries: 2, base_delay_ms: 1, max_delay_ms: 10 };
        let attempts = Arc::new(Mutex::new(0u32));
        let attempts_clone = attempts.clone();
        let result = with_retry(&config, || {
            let counter = attempts_clone.clone();
            async move {
                *counter.lock().await += 1;
                Err::<i32, anyhow::Error>(anyhow::anyhow!("transient error"))
            }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(*attempts.lock().await, 3); // initial + 2 retries
    }
}
