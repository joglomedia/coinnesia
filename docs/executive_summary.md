  Ringkasan Eksekutif — coinnesia-signal-bot

  1) Tujuan Utama Proyek

  Coinnesia adalah async multi-asset trading signal scanner berbasis Rust yang dirancang sebagai layanan 24/7. Tujuan intinya: memindai ratusan simbol lintas kelas aset
  (BTC, altcoin, gold proxy, forex, saham IDX/US), menghasilkan sinyal Long/Short/Wait/Freeze yang sudah melalui filter konteks pasar, lalu mengirim peringatan ke
  Telegram serta menyimpan jejaknya ke Postgres + Valkey untuk audit dan reproduksibilitas. Crate coinnesia v0.1.0 (edition 2021, MSRV 1.80) dideskripsikan di
  Cargo.toml:6 sebagai "Async multi-asset trading signal scanner with config-driven strategy profiles".

  Filosofi yang dikodifikasikan di AGENTS.md adalah config-driven, indicator-deterministic, asset-aware: setiap kelas aset memiliki profil bobot indikator sendiri (BTC
  structure-first, altcoin anti-trap, gold proxy-driven, forex session-driven, IDX volume-guard), dan keputusan akhir lewat enam lapis evaluasi.

  2) Fungsi Fitur Kunci

  ┌───────────────────────────────────┬──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
  │                            Area                            │                                            Implementasi                                             │
  ├────────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ CLI (src/main.rs)                                          │ check-config, serve, migrate, scan-once, scan, trade (placeholder), backtest (scaffold)             │
  ├────────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ HTTP API (src/api/routes.rs)                               │ GET /health, /ready, /metrics, /config; POST /scan (dengan token auth via COINNESIA_API_TOKEN)      │
  ├────────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Service Kernel (src/app/supervisor.rs)                     │ Supervisor + JoinSet mengawasi 4 worker: scanner, alert, trading, reconciliation; shutdown via      │
  │                                                            │ CancellationToken                                                                                   │
  ├────────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Indikator (src/indicators/* — 14 modul)                    │ EMA, ATR (Wilder), RSI (RMA wajib), ADX/DMI, MACD, VWAP (anchored + daily WIB), Volume, Candle      │
  │                                                            │ shape, SMC (BOS/CHOCH), Liquidity sweeps, Order blocks, Support/Resistance, Regime                  │
  ├────────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Strategy 6-layer (src/strategy/*)                          │ confidence scoring → MTF threshold → session gate (WIB) → trap guard → entry plan ATR-based         │
  │                                                            │ (EW1/EW2/EW3 + Deep Add) → TP1/2/3 & SL engine                                                      │
  ├────────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Profil Aset (src/assets/*)                                 │ btc, altcoin, gold, forex, stocks_idx — masing-masing menyetel bobot indikator                      │
  ├────────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Data Sources (src/data/*)                                  │ Binance REST + WebSocket (tokio-tungstenite, opsional), TradingView via tvdata-rs 0.1.2, Yahoo      │
  │                                                            │ fallback, proxy simbol (XAUUSD/IHSG/DXY)                                                            │
  ├────────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Persistence (src/storage/* + migrasi                       │ Tabel: symbols, signal_evaluations, alert_jobs, alert_deliveries, orders, order_events, fills,      │
  │ 20260520000000_phase_0_4_foundation.sql)                   │ positions, balances, portfolio_snapshots, risk_events, backtest_runs                                │
  ├────────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Cache & Pub/Sub (src/cache/*)                              │ Valkey/Redis — snapshots, dedupe TTL, distributed locks, pub/sub scanner→alert, rate-limit token    │
  │                                                            │ bucket                                                                                              │
  ├────────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Alerts (src/alerts/worker.rs + telegram.rs)                │ Worker polling alert_jobs, kirim ke Telegram Bot API, dedup via Valkey TTL, retry exponential       │
  ├────────────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Observability (src/observability/*)                        │ Health registry per-komponen, RuntimeMetrics counter, /metrics endpoint                             │
  └────────────────────────────────────────────────────────────┴─────────────────────────────────────────────────────────────────────────────────────────────────────┘

  3) Cara Kerja Sistem (Garis Besar)

  ┌─────────────────────────────────────────────────────────────────────┐
  │  CLI (clap) → AppConfig::from_file(default.toml) → tokio::main      │
  └──────────────────────────────┬──────────────────────────────────────┘
                                 │
                ┌────────────────┴──────────────────┐
                │  app::serve()  →  AppState        │
                │  Axum router + Supervisor JoinSet │
                └────────────────┬──────────────────┘
                                 │
     ┌──────────┬────────────────┼────────────────┬─────────────────┐
     │ Scanner  │  Alert Worker  │  Trading WIP   │  Reconciliation │
     │ worker   │                │                │                 │
     └────┬─────┴────────┬───────┴────────────────┴─────────────────┘
          │              │
          ▼              ▼
     scan_once() ────────────────────────────────────────────────────┐
     ① Ingest:  MarketDataSource (Binance/TV/Yahoo) → Vec<Candle>    │
     ② Analyze: indicators → ConfidenceScore → SignalGenerator       │
     ③ Gate:    MTF threshold → session WIB → trap_guard             │
     ④ Plan:    EntryPlan (ATR-based EW/TP/SL)                       │
     ⑤ Persist: signal_evaluations (Postgres) + snapshot (Valkey)    │
     ⑥ Publish: pub/sub → alert_jobs row (Postgres queue)            │
                                                                     │
     AlertWorker.process_once() ◄───────────────────────────────────┘
     claim job → dedupe Valkey → send Telegram → record delivery

  Konfigurasi (config/default.toml, ~250 baris, 20+ section) menjadi single source of truth untuk periode indikator, threshold confidence per timeframe, jam sesi WIB,
  alokasi portfolio, drawdown gate, dan rate limit exchange. Konfig dimuat lewat AppConfig::from_file (src/config/mod.rs) — bisa di-override via COINNESIA_CONFIG env var
   atau flag --config.

  4) Dependensi Utama

  Dari Cargo.toml:

  ┌───────────────┬────────────────────────────────────────────────────────────┐
  │     Layer     │                       Crate (versi)                        │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ Runtime async │ tokio 1 (full features), tokio-util, futures               │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ HTTP server   │ axum 0.8, tower, tower-http                                │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ HTTP client   │ reqwest 0.12 (rustls)                                      │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ WebSocket     │ tokio-tungstenite (rustls-tls-webpki-roots)                │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ Database      │ sqlx 0.8 (postgres + runtime-tokio-rustls + chrono + uuid) │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ Cache         │ redis 0.32 (tokio-comp + connection-manager)               │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ Market data   │ tvdata-rs 0.1.2 (TradingView)                              │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ CLI           │ clap 4 (derive)                                            │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ Config        │ toml, serde, serde_json, dotenvy                           │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ Observability │ tracing, tracing-subscriber                                │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ Util          │ anyhow, thiserror, chrono, uuid, rust_decimal, async-trait │
  ├───────────────┼────────────────────────────────────────────────────────────┤
  │ Test          │ approx 0.5                                                 │
  └───────────────┴────────────────────────────────────────────────────────────┘

  Infra runtime via compose.yml: Postgres 15-alpine (port 5432), Valkey 7.2 (port 6379), service migrator (auto-run migrasi dari src/storage/migrations), Adminer (8081),
   RedisInsight (8082).

  5) Bagian yang Belum Selesai

  README.md dan docs/development_plan.md menandai Phase 0 & 1 selesai; Phase 2+ belum. Bukti konkret di kode (verified via wc -l pada modul-modul Phase 2):

  ┌───────────────────────────────────────────────────────────────────────────┬────────────────────┬─────────────────────────────────────────────────────────────────┐
  │                                   Modul                                   │       Status       │                              Bukti                              │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ src/trading/executor.rs                                                   │ Stub kosong           │ 1 baris                                                      │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ src/trading/order_manager.rs                                              │ Stub kosong           │ 7 baris                                                      │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ src/trading/position.rs                                                   │ Stub kosong           │ 8 baris                                                      │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ src/trading/mod.rs TradingEngine::handle_signal                           │ Tidak eksekusi order  │ 29 baris; let _ = &self.exchange; placeholder                │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ Command::Trade (src/main.rs:111-116)                                      │ Placeholder           │ hanya info!("trade command scaffolded; trading service       │
  │                                                                           │                       │ wiring pending")                                             │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ src/exchange/binance.rs, bybit.rs, mexc.rs, okx.rs                        │ Stub 1 baris          │ hanya paper.rs (30 baris) yang punya isi                     │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ src/backtest/{data_loader,optimizer,report,sim_exchange,sim_portfolio}.rs │ Stub kosong           │ 1 baris masing-masing; engine.rs 24 baris                    │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ src/portfolio/{rebalancer}.rs                                             │ Stub                  │ 1 baris; allocator.rs 11 baris, mod.rs 16 baris (struct      │
  │                                                                           │                       │ config-only)                                                 │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ src/risk/{correlation,drawdown,kill_switch,limits,position_sizer}.rs      │ Stub minimal (4–6     │ RiskManager::evaluate hanya cek SignalState::Freeze          │
  │                                                                           │ baris)                │                                                              │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ API read endpoints                                                        │ Belum ada             │ /signals, /orders, /positions, /portfolio belum diekspos     │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ src/risk/{correlation,drawdown,kill_switch,limits,position_sizer}.rs      │ Stub minimal (4–6     │ RiskManager::evaluate hanya cek SignalState::Freeze          │
  │                                                                           │ baris)                │                                                              │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ API read endpoints                                                        │ Belum ada             │ /signals, /orders, /positions, /portfolio belum diekspos     │
  ├───────────────────────────────────────────────────────────────────────────┼───────────────────────┼──────────────────────────────────────────────────────────────┤
  │ WebSocket Binance                                                         │ Disabled by default   │ exchange.binance.ws.enabled = false di config                │
  └───────────────────────────────────────────────────────────────────────────┴───────────────────────┴──────────────────────────────────────────────────────────────┘

  Tidak ditemukan marker eksplisit TODO/FIXME/unimplemented!() dalam kode — gap berbentuk file kosong dan placeholder log message, bukan komentar.

  Roadmap tertulis (docs/development_plan.md):
  - Phase 2: live/paper trading dengan order lifecycle, fills, reconciliation
  - Phase 3: ekspansi portfolio/risk, rebalancing, drawdown gate aktif
  - Phase 4: event-driven backtester + optimizer
  - Phase 5: benchmarking 500+ simbol

  ---
  Maturitas singkat: Infrastruktur dan jalur sinyal sudah produksi-grade dan dites (108 file .rs, ~9.2k LoC, 6 file integration test + unit test inline di seluruh modul
  indikator/strategy). Bagian execution & capital management (trading nyata, risk gate penuh, portfolio rebalancing, backtester) masih scaffolding — siap untuk diisi di
  Phase 2 ke atas tanpa perlu mengubah kontrak modul yang ada (semua sudah memakai trait Exchange, trait MarketDataSource, trait AlertSink).