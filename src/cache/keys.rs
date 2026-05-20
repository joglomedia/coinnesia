#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBuilder {
    prefix: String,
}

impl KeyBuilder {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    pub fn build(&self, parts: &[&str]) -> String {
        build_key(&self.prefix, parts)
    }

    pub fn dedupe(&self, scope: &str, id: &str) -> String {
        self.build(&["dedupe", scope, id])
    }

    pub fn heartbeat(&self, component: &str) -> String {
        self.build(&["heartbeat", component])
    }

    pub fn lock(&self, name: &str) -> String {
        self.build(&["lock", name])
    }

    pub fn topic(&self, name: &str) -> String {
        self.build(&["topic", name])
    }

    pub fn rate_limit(&self, name: &str) -> String {
        self.build(&["rate_limit", name])
    }
}

pub fn build_key(prefix: &str, parts: &[&str]) -> String {
    let mut key = String::from(prefix);
    for part in parts {
        key.push(':');
        key.push_str(part.trim_matches(':'));
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_prefixed_key() {
        assert_eq!(
            build_key("coinnesia", &["scan", "BTCUSDT"]),
            "coinnesia:scan:BTCUSDT"
        );
    }

    #[test]
    fn builds_namespaced_keys() {
        let keys = KeyBuilder::new("coinnesia");
        assert_eq!(
            keys.dedupe("telegram", "BTCUSDT-1h"),
            "coinnesia:dedupe:telegram:BTCUSDT-1h"
        );
        assert_eq!(keys.heartbeat("scanner"), "coinnesia:heartbeat:scanner");
        assert_eq!(keys.lock("scanner"), "coinnesia:lock:scanner");
        assert_eq!(keys.topic("signals"), "coinnesia:topic:signals");
        assert_eq!(keys.rate_limit("binance"), "coinnesia:rate_limit:binance");
    }
}
