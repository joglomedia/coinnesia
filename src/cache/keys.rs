pub fn build_key(prefix: &str, parts: &[&str]) -> String {
    let mut key = String::from(prefix);
    for part in parts {
        key.push(':');
        key.push_str(part);
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
}
