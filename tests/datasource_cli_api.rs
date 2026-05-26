use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use coinnesia::{api, app::AppState, config::AppConfig};
use serde_json::Value;
use tower::ServiceExt;

struct MockBinanceServer {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
    _handle: thread::JoinHandle<()>,
}

impl MockBinanceServer {
    fn start(max_requests: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock server binds");
        listener
            .set_nonblocking(true)
            .expect("mock server becomes nonblocking");
        let addr = listener.local_addr().expect("mock server local addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_requests = Arc::clone(&requests);
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut handled = 0;

            while handled < max_requests && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .expect("mock stream read timeout sets");
                        let request = read_http_request(&mut stream);
                        let request_text = String::from_utf8_lossy(&request);
                        if let Some(request_line) = request_text.lines().next() {
                            thread_requests
                                .lock()
                                .expect("request log locks")
                                .push(request_line.to_owned());
                        }
                        let body = kline_response_body();
                        let response = format!(
                            concat!(
                                "HTTP/1.1 200 OK\r\n",
                                "content-type: application/json\r\n",
                                "content-length: {}\r\n",
                                "connection: close\r\n",
                                "\r\n",
                                "{}"
                            ),
                            body.len(),
                            body
                        );
                        stream
                            .write_all(response.as_bytes())
                            .expect("mock response writes");
                        handled += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            requests,
            _handle: handle,
        }
    }

    fn request_lines(&self) -> Vec<String> {
        self.requests.lock().expect("request log locks").clone()
    }
}

#[tokio::test]
async fn api_scan_uses_configured_datasource_and_returns_scan_result() {
    let server = MockBinanceServer::start(20);
    let mut config = test_config(&server.base_url);
    config.database.enabled = false;
    config.cache.enabled = false;
    config.alerts.enabled = false;
    config.server.auth_token_env = "COINNESIA_TEST_DATASOURCE_API_TOKEN".to_owned();
    std::env::set_var(&config.server.auth_token_env, "secret-token");

    let state = AppState::bootstrap(config).await.expect("state boots");
    let app = api::router(state);
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
        .expect("scan response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_response(response).await;
    assert_eq!(body["accepted"], true);
    assert_eq!(body["scanned"], 3);
    assert_eq!(body["signals"], 3);
    assert_requested_symbols(&server.request_lines(), ["BTCUSDT", "PAXGUSDT"]);

    std::env::remove_var("COINNESIA_TEST_DATASOURCE_API_TOKEN");
}

#[test]
fn cli_scan_once_uses_configured_datasource_and_completes_cycle() {
    let server = MockBinanceServer::start(20);
    let config_path = write_test_config(&server.base_url);
    let output = Command::new(env!("CARGO_BIN_EXE_coinnesia"))
        .arg("--config")
        .arg(&config_path)
        .arg("scan-once")
        .output()
        .expect("coinnesia binary runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");

    assert!(
        output.status.success(),
        "scan-once failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(combined.contains("scan cycle completed"));
    assert!(combined.contains("scanned=3"));
    assert!(combined.contains("signals=3"));
    assert_requested_symbols(&server.request_lines(), ["BTCUSDT", "PAXGUSDT"]);

    let _ = fs::remove_file(config_path);
}

async fn json_response(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    serde_json::from_slice(&bytes).expect("json response")
}

fn test_config(base_url: &str) -> AppConfig {
    toml::from_str(&test_config_toml(base_url)).expect("test config parses")
}

fn write_test_config(base_url: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "coinnesia-datasource-test-{}.toml",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::write(&path, test_config_toml(base_url)).expect("test config writes");
    path
}

fn test_config_toml(base_url: &str) -> String {
    include_str!("../config/default.toml")
        .replace("candle_limit = 250", "candle_limit = 20")
        .replace("[database]\nenabled = true", "[database]\nenabled = false")
        .replace("[cache]\nenabled = true", "[cache]\nenabled = false")
        .replace("[alerts]\nenabled = true", "[alerts]\nenabled = false")
        .replace("max_retries = 3", "max_retries = 0")
        .replace(
            "rest_url = \"https://api.binance.com\"",
            &format!("rest_url = \"{base_url}\""),
        )
}

fn kline_response_body() -> String {
    let rows = (0..20)
        .map(|idx| {
            let ts = 1_700_000_000_000_i64 + (idx * 60_000);
            let open = 100.0 + idx as f64;
            let high = open + 1.0;
            let low = open - 1.0;
            let close = open + 0.5;
            format!("[{ts},\"{open:.2}\",\"{high:.2}\",\"{low:.2}\",\"{close:.2}\",\"1000.00\"]")
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{rows}]")
}

fn read_http_request(stream: &mut impl Read) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => request.extend_from_slice(&buffer[..read]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(_) => break,
        }
    }
    request
}

fn assert_requested_symbols<const N: usize>(request_lines: &[String], expected_symbols: [&str; N]) {
    let mut counts = BTreeMap::<String, usize>::new();
    for request_line in request_lines {
        if let Some(symbol) = query_value(request_line, "symbol") {
            *counts.entry(symbol).or_default() += 1;
        }
    }

    for expected_symbol in expected_symbols {
        assert!(
            counts.get(expected_symbol).copied().unwrap_or_default() >= 1,
            "expected {expected_symbol} request in {request_lines:#?}"
        );
    }
}

fn query_value(request_line: &str, key: &str) -> Option<String> {
    let path = request_line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (pair_key, pair_value) = pair.split_once('=')?;
        (pair_key == key).then(|| percent_decode(pair_value))
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%' && idx + 2 < bytes.len() {
            if let Ok(hex) = u8::from_str_radix(&value[idx + 1..idx + 3], 16) {
                output.push(hex);
                idx += 3;
                continue;
            }
        }
        output.push(if bytes[idx] == b'+' { b' ' } else { bytes[idx] });
        idx += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

// ── Baseline & parity tests (require live Binance — run with: cargo test -- --ignored --nocapture) ──

/// Dokumentasi output bespoke sebelum migrasi. Jalankan sekali di Step 0, simpan output
/// sebagai referensi manual untuk dibandingkan dengan rest_sdk_parity di Step 3.
#[tokio::test]
#[ignore]
async fn baseline_candle_structure() {
    use coinnesia::{
        config::AppConfig,
        data::{binance::BinanceDataSource, MarketDataSource},
        Timeframe,
    };

    let config = AppConfig::from_default_toml().unwrap();
    let source = BinanceDataSource::new(
        config.exchange.binance.clone(),
        config.exchange.rate_limit_per_second as u32,
        config.data_sources.retry.clone(),
    );
    let candles = source.candles("BTCUSDT", Timeframe::D1, 5).await.unwrap();

    assert_eq!(candles.len(), 5, "harus ada 5 candles");
    for c in &candles {
        assert!(c.open > 0.0, "open > 0");
        assert!(c.high >= c.low, "high >= low");
        assert!(c.volume > 0.0, "volume > 0");
    }
    let ascending = candles.windows(2).all(|w| w[0].ts < w[1].ts);
    assert!(ascending, "timestamps harus ascending");

    println!("BASELINE BESPOKE OUTPUT:\n{candles:#?}");
}

/// Parity test setelah REST SDK migration (Step 3). Bandingkan output secara manual
/// dengan baseline_candle_structure di atas — OHLCV D1 candles tertutup harus identik.
#[tokio::test]
#[ignore]
async fn rest_sdk_parity() {
    use coinnesia::{
        config::AppConfig,
        data::{binance::BinanceDataSource, MarketDataSource},
        Timeframe,
    };

    let config = AppConfig::from_default_toml().unwrap();
    let source = BinanceDataSource::new(
        config.exchange.binance.clone(),
        config.exchange.rate_limit_per_second as u32,
        config.data_sources.retry.clone(),
    );
    let candles = source.candles("BTCUSDT", Timeframe::D1, 5).await.unwrap();

    assert_eq!(candles.len(), 5, "harus ada 5 candles");
    for c in &candles {
        assert!(c.open > 0.0, "open > 0");
        assert!(c.high >= c.low, "high >= low");
        assert!(c.volume > 0.0, "volume > 0");
    }
    let ascending = candles.windows(2).all(|w| w[0].ts < w[1].ts);
    assert!(ascending, "timestamps harus ascending");

    println!("SDK REST PARITY OUTPUT:\n{candles:#?}");
}
