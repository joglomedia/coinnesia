use std::time::Duration;

use coinnesia::{
    cache::{rate_limit::RateLimitBucket, Cache},
    config::CacheConfig,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn test_cache_config() -> Option<CacheConfig> {
    std::env::var("VALKEY_URL").ok()?;
    Some(CacheConfig {
        enabled: true,
        url_env: "VALKEY_URL".to_owned(),
        key_prefix: format!("coinnesia_test:{}", Uuid::new_v4()),
        pool_size: 1,
        ttl_seconds: 60,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CachedValue {
    symbol: String,
    state: String,
}

#[tokio::test]
async fn valkey_cache_helpers_work_when_valkey_url_is_set() {
    let Some(config) = test_cache_config() else {
        return;
    };

    let cache = Cache::connect_optional(&config)
        .await
        .expect("Valkey connects")
        .expect("cache enabled");

    let key = cache.key(&["ttl", "string"]);
    cache
        .set_ttl(&key, "ok", Duration::from_secs(5))
        .await
        .expect("ttl set works");
    assert_eq!(
        cache.get_string(&key).await.expect("ttl get works"),
        Some("ok".to_owned())
    );

    let json_key = cache.key(&["ttl", "json"]);
    let value = CachedValue {
        symbol: "BTCUSDT".to_owned(),
        state: "WAIT".to_owned(),
    };
    cache
        .set_json(&json_key, &value, Duration::from_secs(5))
        .await
        .expect("json set works");
    let stored: CachedValue = cache
        .get_json(&json_key)
        .await
        .expect("json get works")
        .expect("json value exists");
    assert_eq!(stored, value);

    assert!(cache
        .mark_dedupe("telegram", "signal-1", Duration::from_secs(5))
        .await
        .expect("first dedupe succeeds"));
    assert!(!cache
        .mark_dedupe("telegram", "signal-1", Duration::from_secs(5))
        .await
        .expect("second dedupe is blocked"));

    let lock = cache
        .acquire_lock("scanner", Duration::from_secs(5))
        .await
        .expect("lock request works")
        .expect("first lock acquired");
    assert!(cache
        .acquire_lock("scanner", Duration::from_secs(5))
        .await
        .expect("second lock request works")
        .is_none());
    assert!(cache.release_lock(&lock).await.expect("lock releases"));

    let heartbeat = cache
        .heartbeat("scanner", Duration::from_secs(5))
        .await
        .expect("heartbeat writes");
    let stored_heartbeat = cache
        .get_heartbeat("scanner")
        .await
        .expect("heartbeat reads")
        .expect("heartbeat exists");
    assert_eq!(stored_heartbeat.timestamp(), heartbeat.timestamp());

    let subscribers = cache
        .publish_json("signals", &value)
        .await
        .expect("publish works without subscribers");
    assert_eq!(subscribers, 0);

    let bucket = RateLimitBucket::new(2, Duration::from_secs(5));
    let first = cache
        .check_rate_limit("binance", bucket)
        .await
        .expect("first rate check works");
    let second = cache
        .check_rate_limit("binance", bucket)
        .await
        .expect("second rate check works");
    let third = cache
        .check_rate_limit("binance", bucket)
        .await
        .expect("third rate check works");

    assert!(first.allowed);
    assert!(second.allowed);
    assert!(!third.allowed);
    assert_eq!(third.remaining, 0);
}
