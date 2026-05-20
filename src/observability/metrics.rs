use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

#[derive(Debug, Clone, Default)]
pub struct RuntimeMetrics {
    inner: Arc<RuntimeMetricsInner>,
}

#[derive(Debug, Default)]
struct RuntimeMetricsInner {
    scan_cycles: AtomicU64,
    symbols_scanned: AtomicU64,
    signals_generated: AtomicU64,
    api_requests: AtomicU64,
    api_latency_ms_total: AtomicU64,
    exchange_errors: AtomicU64,
    telegram_delivery_attempts: AtomicU64,
    kill_switch_events: AtomicU64,
}

impl RuntimeMetrics {
    pub fn inc_scan_cycle(&self) {
        self.inner.scan_cycles.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_symbols_scanned(&self, symbols: u64) {
        self.inner
            .symbols_scanned
            .fetch_add(symbols, Ordering::Relaxed);
    }

    pub fn add_signals_generated(&self, signals: u64) {
        self.inner
            .signals_generated
            .fetch_add(signals, Ordering::Relaxed);
    }

    pub fn record_api_request(&self, latency_ms: u64) {
        self.inner.api_requests.fetch_add(1, Ordering::Relaxed);
        self.inner
            .api_latency_ms_total
            .fetch_add(latency_ms, Ordering::Relaxed);
    }

    pub fn inc_exchange_error(&self) {
        self.inner.exchange_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_telegram_delivery_attempt(&self) {
        self.inner
            .telegram_delivery_attempts
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_kill_switch_event(&self) {
        self.inner
            .kill_switch_events
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> RuntimeMetricsSnapshot {
        RuntimeMetricsSnapshot {
            scan_cycles: self.inner.scan_cycles.load(Ordering::Relaxed),
            symbols_scanned: self.inner.symbols_scanned.load(Ordering::Relaxed),
            signals_generated: self.inner.signals_generated.load(Ordering::Relaxed),
            api_requests: self.inner.api_requests.load(Ordering::Relaxed),
            api_latency_ms_total: self.inner.api_latency_ms_total.load(Ordering::Relaxed),
            exchange_errors: self.inner.exchange_errors.load(Ordering::Relaxed),
            telegram_delivery_attempts: self
                .inner
                .telegram_delivery_attempts
                .load(Ordering::Relaxed),
            kill_switch_events: self.inner.kill_switch_events.load(Ordering::Relaxed),
        }
    }

    pub fn render_prometheus(&self, components: usize) -> String {
        let snapshot = self.snapshot();
        format!(
            concat!(
                "coinnesia_up 1\n",
                "coinnesia_components {components}\n",
                "coinnesia_scan_cycles_total {scan_cycles}\n",
                "coinnesia_symbols_scanned_total {symbols_scanned}\n",
                "coinnesia_signals_generated_total {signals_generated}\n",
                "coinnesia_api_requests_total {api_requests}\n",
                "coinnesia_api_latency_ms_total {api_latency_ms_total}\n",
                "coinnesia_exchange_errors_total {exchange_errors}\n",
                "coinnesia_telegram_delivery_attempts_total {telegram_delivery_attempts}\n",
                "coinnesia_kill_switch_events_total {kill_switch_events}\n"
            ),
            components = components,
            scan_cycles = snapshot.scan_cycles,
            symbols_scanned = snapshot.symbols_scanned,
            signals_generated = snapshot.signals_generated,
            api_requests = snapshot.api_requests,
            api_latency_ms_total = snapshot.api_latency_ms_total,
            exchange_errors = snapshot.exchange_errors,
            telegram_delivery_attempts = snapshot.telegram_delivery_attempts,
            kill_switch_events = snapshot.kill_switch_events,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeMetricsSnapshot {
    pub scan_cycles: u64,
    pub symbols_scanned: u64,
    pub signals_generated: u64,
    pub api_requests: u64,
    pub api_latency_ms_total: u64,
    pub exchange_errors: u64,
    pub telegram_delivery_attempts: u64,
    pub kill_switch_events: u64,
}
