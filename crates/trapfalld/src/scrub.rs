//! PII scrubbing pipeline — redact sensitive data before persistence.
//!
//! Motivation: Sentry SDKs capture user data (emails, IPs, tokens) in
//! `extra`, `tags`, `contexts`, and breadcrumb messages. Under
//! **UU PDP (Law No. 27/2022)** TrapFall as data processor must not retain
//! raw PII without explicit consent.
//!
//! Strategy:
//! 1. Compile-once `RegexSet` for known PII patterns.
//! 2. Recursively walk `serde_json::Value` trees, replacing matches.
//! 3. Detect sensitive **keys** in JSON objects and redact their values.
//! 4. Anonymize IP addresses (IPv4 last octet zeroed).

use std::sync::LazyLock;

use regex::Regex;
use regex::RegexSet;

// ── Pattern Definitions ────────────────────────────────────────────────

/// Sentinel inserted in place of scrubbed PII.
const REDACTED_EMAIL: &str = "[REDACTED:email]";
const REDACTED_CC: &str = "[REDACTED:cc]";
const REDACTED_TOKEN: &str = "[REDACTED:token]";
const REDACTED_PHONE: &str = "[REDACTED:phone]";
const REDACTED_VALUE: &str = "[REDACTED]";

/// Compiled regex set — matches in order of `PATTERNS`.
static PII_SET: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        // 0: email
        r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b",
        // 1: credit card (13-19 digits, optional spaces/dashes, Luhn not checked)
        r"\b(?:\d[ -]*?){13,19}\b",
        // 2: API key / token prefixes
        r"(?:sk[-_]?(?:test[-_]?)?[A-Za-z0-9]{20,}|ghp_[A-Za-z0-9]{36,}|AKIA[A-Z0-9]{16}|Bearer\s+[A-Za-z0-9._\-]+|glpat-[A-Za-z0-9\-]{20,}|xox[bpoa]-[A-Za-z0-9\-]+)",
        // 3: Indonesian phone (08xx or +62xxx, 10-15 digits)
        r"(?:\+?62|0)8[1-9]\d{6,13}",
    ])
    .expect("PII regex set must compile")
});

/// Individual regexes for replacement (indexed same as PII_SET).
static PII_REGEXES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    PII_SET.patterns().iter().map(|p| Regex::new(p).expect("individual PII regex must compile")).collect()
});

/// Variable names whose **values** should be redacted regardless of content.
static SENSITIVE_VAR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(password|passwd|secret|token|api[_-]?key|auth|credential|private[_-]?key|access[_-]?token|refresh[_-]?token|session[_-]?id|cookie|ssn|npwp)")
        .expect("sensitive var regex must compile")
});

/// Variable names related to IP addresses — apply IP anonymization.
static IP_VAR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(ip|ipaddr|ip_addr|remote_addr|client_ip|x-forwarded-for|forwarded)")
        .expect("IP var regex must compile")
});

/// IPv4 pattern for standalone anonymization.
static IPV4_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b").expect("IPv4 regex must compile"));

// ── Public API ─────────────────────────────────────────────────────────

/// Scrub PII from a Sentry event in-place.
///
/// Operates on `message`, `tags`, `extra`, `contexts`, `exception`
/// values, and `breadcrumbs`.
pub fn scrub_event(event: &mut trapfall_proto::Event) {
    if let Some(msg) = event.message.take() {
        event.message = Some(scrub_string(&msg));
    }

    scrub_json(&mut event.tags);
    scrub_json(&mut event.extra);
    scrub_json(&mut event.contexts);

    // Scrub exception values
    if let Some(ex_vals) = event.exception.as_mut() {
        for ex in ex_vals.values.iter_mut() {
            if let Some(val) = ex.value.take() {
                ex.value = Some(scrub_string(&val));
            }
        }
    }

    // Scrub breadcrumb messages and data
    for bc in event.breadcrumbs.values.iter_mut() {
        if let Some(msg) = bc.message.take() {
            bc.message = Some(scrub_string(&msg));
        }
        if let Some(data) = bc.data.take() {
            let mut d = data;
            scrub_json(&mut d);
            bc.data = Some(d);
        }
    }
}

/// Scrub PII from a transaction's contexts/tags/extra in-place.
pub fn scrub_transaction(txn: &mut trapfall_proto::Transaction) {
    if let Some(ctx) = txn.contexts.take() {
        let mut c = ctx;
        scrub_json(&mut c);
        txn.contexts = Some(c);
    }
    if let Some(tags) = txn.tags.take() {
        let mut t = tags;
        scrub_json(&mut t);
        txn.tags = Some(t);
    }
    if let Some(extra) = txn.extra.take() {
        let mut e = extra;
        scrub_json(&mut e);
        txn.extra = Some(e);
    }
    if let Some(req) = txn.request.take() {
        let mut r = req;
        scrub_json(&mut r);
        txn.request = Some(r);
    }
}

/// Scrub a raw JSON value tree recursively.
pub fn scrub_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            *s = scrub_string(s);
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                scrub_json(v);
            }
        }
        serde_json::Value::Object(map) => {
            // Check for sensitive keys first
            let sensitive_keys: Vec<String> = map.keys().filter(|k| is_sensitive_key(k)).cloned().collect();
            for key in sensitive_keys {
                if let Some(val) = map.get_mut(&key) {
                    if is_ip_key(&key) {
                        if let Some(s) = val.as_str() {
                            *val = serde_json::Value::String(anonymize_ip(s));
                        }
                    } else {
                        scrub_json_value(val);
                    }
                }
            }
            // Then recurse into all values for embedded PII
            for (_, v) in map.iter_mut() {
                scrub_json(v);
            }
        }
        _ => {}
    }
}

// ── Internal Helpers ───────────────────────────────────────────────────

/// Apply all PII regex replacements to a single string.
fn scrub_string(input: &str) -> String {
    let mut result = input.to_string();

    // Find all matches and their pattern indices
    for (idx, regex) in PII_REGEXES.iter().enumerate() {
        let replacement = match idx {
            0 => REDACTED_EMAIL,
            1 => REDACTED_CC,
            2 => REDACTED_TOKEN,
            3 => REDACTED_PHONE,
            _ => REDACTED_VALUE,
        };
        result = regex.replace_all(&result, replacement).to_string();
    }

    // IP anonymization (standalone IPs not in sensitive-key context)
    result = IPV4_REGEX
        .replace_all(&result, |caps: &regex::Captures| {
            let octets: Vec<u8> = (1..=4).filter_map(|i| caps[i].parse::<u8>().ok()).collect();
            if octets.len() == 4 { format!("{}.{}.{}.0", octets[0], octets[1], octets[2]) } else { caps[0].to_string() }
        })
        .to_string();

    result
}

/// Scrub a value that's under a sensitive key — redact entirely.
fn scrub_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(_) => {
            *value = serde_json::Value::String(REDACTED_VALUE.to_string());
        }
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
            *value = serde_json::Value::String(REDACTED_VALUE.to_string());
        }
        _ => {}
    }
}

/// Check if a key name suggests sensitive data.
fn is_sensitive_key(key: &str) -> bool {
    SENSITIVE_VAR_REGEX.is_match(key)
}

/// Check if a key name relates to IP addresses.
fn is_ip_key(key: &str) -> bool {
    IP_VAR_REGEX.is_match(key)
}

/// Anonymize an IP address (IPv4: zero last octet, IPv6: truncate).
fn anonymize_ip(input: &str) -> String {
    let trimmed = input.trim();

    // IPv4
    if let Some(anon) = anonymize_ipv4(trimmed) {
        return anon;
    }

    // IPv6 — truncate to /48 prefix (zero last 80 bits)
    if trimmed.contains(':') {
        let parts: Vec<&str> = trimmed.split(':').collect();
        if parts.len() >= 4 {
            return format!("{}:{}:{}:0000:0000:0000:0000:0000", parts[0], parts[1], parts[2]);
        }
    }

    // Not an IP — return as-is (will be caught by general scrub)
    trimmed.to_string()
}

/// Anonymize IPv4 by zeroing the last octet.
fn anonymize_ipv4(input: &str) -> Option<String> {
    let parts: Vec<&str> = input.split('.').collect();
    if parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok()) {
        Some(format!("{}.{}.{}.0", parts[0], parts[1], parts[2]))
    } else {
        None
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_email() {
        let input = "Contact user@example.com for details";
        let result = scrub_string(input);
        assert!(result.contains(REDACTED_EMAIL));
        assert!(!result.contains("user@example.com"));
    }

    #[test]
    fn scrub_multiple_emails() {
        let input = "admin@corp.io and test@mail.org both received alerts";
        let result = scrub_string(input);
        assert!(!result.contains("admin@corp.io"));
        assert!(!result.contains("test@mail.org"));
    }

    #[test]
    fn scrub_credit_card() {
        let input = "Card: 4111-1111-1111-1111";
        let result = scrub_string(input);
        assert!(result.contains(REDACTED_CC));
    }

    #[test]
    fn scrub_stripe_token() {
        // Construct via concat to avoid secret scanner false positives.
        let key = concat!("sk_test_", "abcdefghij1234567890XYZ");
        let input = format!("Payment key: {key}");
        let result = scrub_string(input.as_str());
        assert!(result.contains(REDACTED_TOKEN));
        assert!(!result.contains(key));
    }

    #[test]
    fn scrub_github_pat() {
        let pat = concat!("ghp_", "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
        let input = format!("Token: {pat}");
        let result = scrub_string(input.as_str());
        assert!(result.contains(REDACTED_TOKEN));
    }

    #[test]
    fn scrub_aws_key() {
        let key = concat!("AKIA", "IOSFODNN7EXAMPLE");
        let input = format!("AWS: {key}");
        let result = scrub_string(input.as_str());
        assert!(result.contains(REDACTED_TOKEN));
    }

    #[test]
    fn scrub_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let result = scrub_string(input);
        assert!(result.contains(REDACTED_TOKEN));
    }

    #[test]
    fn scrub_indonesian_phone() {
        let input = "Hubungi: 081234567890 atau +6281234567890";
        let result = scrub_string(input);
        assert!(result.contains(REDACTED_PHONE));
        assert!(!result.contains("081234567890"));
        assert!(!result.contains("+6281234567890"));
    }

    #[test]
    fn anonymize_ipv4_standalone() {
        let result = scrub_string("Request from 192.168.1.42");
        assert!(result.contains("192.168.1.0"));
        assert!(!result.contains("192.168.1.42"));
    }

    #[test]
    fn scrub_json_object_with_sensitive_keys() {
        let mut json = serde_json::json!({
            "password": "my-secret-123",
            "username": "john",
            "email": "john@test.com"
        });
        scrub_json(&mut json);
        assert_eq!(json["password"], REDACTED_VALUE);
        assert_eq!(json["username"], "john"); // not sensitive
        assert_eq!(json["email"], REDACTED_EMAIL);
    }

    #[test]
    fn scrub_json_nested() {
        let pat = concat!("ghp_", "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
        let pat_str = format!("token: {pat}");
        let mut json = serde_json::json!({
            "user": {
                "email": "deep@nested.io",
                "data": [pat_str.as_str()]
            }
        });
        scrub_json(&mut json);
        let scrubbed = json.to_string();
        assert!(!scrubbed.contains("deep@nested.io"));
        assert!(!scrubbed.contains("ghp_"));
    }

    #[test]
    fn scrub_json_array_of_strings() {
        let mut json = serde_json::json!(["admin@x.com", "user@y.com", "plain text"]);
        scrub_json(&mut json);
        let scrubbed = json.to_string();
        assert!(scrubbed.contains(REDACTED_EMAIL));
        assert!(!scrubbed.contains("admin@x.com"));
    }

    #[test]
    fn no_false_positive_on_plain_text() {
        let input = "Error: undefined variable in function call at line 42";
        let result = scrub_string(input);
        assert_eq!(input, result);
    }

    #[test]
    fn ip_anonymization_function() {
        assert_eq!(anonymize_ip("10.0.0.1"), "10.0.0.0");
        assert_eq!(anonymize_ip("172.16.254.1"), "172.16.254.0");
    }

    #[test]
    fn is_sensitive_key_detection() {
        assert!(is_sensitive_key("password"));
        assert!(is_sensitive_key("api_key"));
        assert!(is_sensitive_key("API-KEY"));
        assert!(is_sensitive_key("accessToken"));
        assert!(!is_sensitive_key("username"));
        assert!(!is_sensitive_key("message"));
    }

    #[test]
    fn json_with_ip_key_uses_anonymize() {
        let mut json = serde_json::json!({
            "client_ip": "203.142.84.77",
            "user_agent": "Mozilla/5.0"
        });
        scrub_json(&mut json);
        assert_eq!(json["client_ip"], "203.142.84.0");
        assert_eq!(json["user_agent"], "Mozilla/5.0");
    }

    #[test]
    fn combined_pii_in_one_string() {
        let key = concat!("sk_test_", "abcdefghij1234567890XYZ");
        let input = format!("Email: admin@x.com, Token: {key}, IP: 10.0.0.5");
        let result = scrub_string(input.as_str());
        assert!(result.contains(REDACTED_EMAIL));
        assert!(result.contains(REDACTED_TOKEN));
        assert!(result.contains("10.0.0.0"));
        assert!(!result.contains("admin@x.com"));
        assert!(!result.contains(key));
    }
}
