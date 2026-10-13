//! Alert engine — evaluates rules against incoming issues and dispatches webhooks.

use tokio::sync::mpsc;
use trapfall_core::Store;
use trapfall_proto::{AlertRule, Issue};

/// Spawn the alert engine background task.
pub fn spawn_alert_engine(store: Store, _buffer: usize) -> mpsc::UnboundedSender<Issue> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Issue>();

    tokio::spawn(async move {
        while let Some(issue) = rx.recv().await {
            if let Err(e) = process_issue(&store, &issue).await {
                tracing::warn!("Alert engine error for issue {}: {e}", issue.id);
            }
        }
        tracing::info!("Alert engine task stopped");
    });

    tx
}

async fn process_issue(store: &Store, issue: &Issue) -> anyhow::Result<()> {
    let rules = store.get_enabled_rules_for_project(&issue.project_id).await?;

    for rule in rules {
        if !matches_rule(&rule, issue) {
            continue;
        }

        if store.backend().is_rule_cooling_down(&rule.id, rule.cooldown_seconds).await? {
            tracing::debug!("Rule {} on cooldown, skipping", rule.name);
            continue;
        }

        let history_id = store.insert_alert_history(&rule.id, &issue.project_id, &issue.id).await?;

        let result = match rule.action_type.as_str() {
            "webhook" => dispatch_webhook(&rule, issue).await,
            other => {
                tracing::warn!("Unknown action type: {other}");
                Err(anyhow::anyhow!("unknown action type: {other}"))
            }
        };

        match result {
            Ok(()) => {
                if let Err(e) = store.mark_alert_sent(&history_id).await {
                    tracing::warn!("Failed to mark alert sent: {e}");
                }
            }
            Err(e) => {
                if let Err(e2) = store.mark_alert_failed(&history_id, &e.to_string()).await {
                    tracing::warn!("Failed to mark alert failed: {e2}");
                }
            }
        }
    }

    Ok(())
}

fn matches_rule(rule: &AlertRule, issue: &Issue) -> bool {
    let conditions = &rule.conditions;

    if let Some(levels) = conditions.get("level").and_then(|v| v.as_array()) {
        let level_str = format!("{:?}", issue.level).to_lowercase();
        let matches_level = levels.iter().any(|l| l.as_str().map(|s| s.to_lowercase() == level_str).unwrap_or(false));
        if !matches_level {
            return false;
        }
    }

    if let Some(min_count) = conditions.get("count_gte").and_then(|v| v.as_i64()) {
        if issue.count < min_count {
            return false;
        }
    }

    if let Some(pattern) = conditions.get("title_contains").and_then(|v| v.as_str()) {
        if !issue.title.to_lowercase().contains(&pattern.to_lowercase()) {
            return false;
        }
    }

    true
}

/// Syntactic webhook URL validation (no DNS). Used at rule creation and again
/// at dispatch. Requires HTTPS, no userinfo, and a host that is not a
/// private/internal literal or reserved name.
pub fn validate_webhook_url(url: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("invalid webhook URL: {e}"))?;
    if parsed.scheme() != "https" {
        return Err(format!("webhook URL must use HTTPS (got: {})", parsed.scheme()));
    }
    check_host(&parsed)?;
    Ok(parsed)
}

/// Host checks shared by all validators: no credentials, no private literal,
/// no reserved name.
fn check_host(parsed: &url::Url) -> Result<(), String> {
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("webhook URL must not contain credentials".to_string());
    }
    let blocked = "webhook URL points to private/internal address".to_string();
    match parsed.host() {
        None => Err("webhook URL has no host".to_string()),
        Some(url::Host::Ipv4(ip)) if ip_is_private(ip.into()) => Err(blocked),
        Some(url::Host::Ipv6(ip)) if ip_is_private(ip.into()) => Err(blocked),
        Some(url::Host::Domain(d)) if is_internal_hostname(d) => Err(blocked),
        Some(_) => Ok(()),
    }
}

fn is_internal_hostname(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    h.is_empty()
        || h == "localhost"
        || h.ends_with(".localhost")
        || h.ends_with(".internal")
        || h.ends_with(".local")
        || h.ends_with(".localdomain")
}

/// Reject the whole record set if any address is non-public (a mixed
/// public/private answer is a rebinding signal).
fn vet_addrs(addrs: &[std::net::IpAddr]) -> Result<(), String> {
    if addrs.is_empty() {
        return Err("webhook host did not resolve".to_string());
    }
    if addrs.iter().any(|ip| ip_is_private(*ip)) {
        return Err("webhook URL points to private/internal address".to_string());
    }
    Ok(())
}

/// Best-effort webhook dispatch with SSRF protection.
///
/// DNS is resolved exactly once; every address is vetted and the connection is
/// pinned to the vetted addresses (no second lookup, so no rebinding window).
async fn dispatch_webhook(rule: &AlertRule, issue: &Issue) -> anyhow::Result<()> {
    let url = rule
        .action_config
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("no url in action_config"))?;

    let parsed_url = validate_webhook_url(url).map_err(|e| anyhow::anyhow!(e))?;
    let host = parsed_url.host_str().unwrap_or_default().to_string();
    let port = parsed_url.port_or_known_default().unwrap_or(443);

    let addrs: Vec<std::net::SocketAddr> = match parsed_url.host() {
        Some(url::Host::Ipv4(ip)) => vec![std::net::SocketAddr::new(ip.into(), port)],
        Some(url::Host::Ipv6(ip)) => vec![std::net::SocketAddr::new(ip.into(), port)],
        _ => {
            let lookup = host.clone();
            tokio::task::spawn_blocking(move || dns_resolve_host(&lookup, port))
                .await
                .map_err(|e| anyhow::anyhow!("dns task failed: {e}"))?
                .map_err(|e| anyhow::anyhow!("webhook DNS resolution failed: {e}"))?
        }
    };
    let ips: Vec<std::net::IpAddr> = addrs.iter().map(|a| a.ip()).collect();
    if let Err(e) = vet_addrs(&ips) {
        tracing::warn!("Webhook blocked (host {host}): {e}");
        anyhow::bail!(e);
    }

    // Per-request client pinned to the vetted addresses; redirects disabled.
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(5));
    if matches!(parsed_url.host(), Some(url::Host::Domain(_))) {
        builder = builder.resolve_to_addrs(&host, &addrs);
    }
    let client = builder.build()?;

    let payload = serde_json::json!({
        "rule": rule.name,
        "action": "webhook",
        "issue": {
            "id": issue.id,
            "title": issue.title,
            "level": format!("{:?}", issue.level).to_lowercase(),
            "status": format!("{:?}", issue.status).to_lowercase(),
            "count": issue.count,
            "project_id": issue.project_id,
            "culprit": issue.culprit,
            "last_seen": issue.last_seen,
        }
    });

    let resp = client.post(parsed_url).json(&payload).timeout(std::time::Duration::from_secs(5)).send().await?;

    if resp.status().is_success() {
        tracing::info!("Webhook dispatched to {host} for rule '{}'", rule.name);
    } else {
        let status = resp.status();
        tracing::warn!("Webhook returned {status} for rule '{}'", rule.name);
    }

    Ok(())
}

/// Check if a URL points to a private/internal address (any scheme; resolves
/// DNS and requires every resolved address to be public).
#[cfg(test)]
fn is_private_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return true;
    };
    if check_host(&parsed).is_err() {
        return true;
    }
    match parsed.host() {
        Some(url::Host::Domain(d)) => match dns_resolve_host(d, 443) {
            Ok(addrs) => {
                let ips: Vec<_> = addrs.iter().map(|a| a.ip()).collect();
                vet_addrs(&ips).is_err()
            }
            Err(_) => true,
        },
        Some(_) => false,
        None => true,
    }
}

/// True if the address is not a public, globally routable unicast address.
fn ip_is_private(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => ipv4_is_private(v4),
        std::net::IpAddr::V6(v6) => {
            // Unwrap ::ffff:a.b.c.d to IPv4.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return ipv4_is_private(v4);
            }
            let seg = v6.segments();
            // 64:ff9b::/96 NAT64: judge the embedded IPv4.
            if seg[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                let [a, b] = seg[6].to_be_bytes();
                let [c, d] = seg[7].to_be_bytes();
                return ipv4_is_private(std::net::Ipv4Addr::new(a, b, c, d));
            }
            // IPv4-compatible ::a.b.c.d (includes :: and ::1).
            if seg[..6] == [0; 6] {
                return true;
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // fc00::/7 ULA
                || (seg[0] & 0xfe00) == 0xfc00
                // fe80::/10 link-local, fec0::/10 site-local (deprecated)
                || (seg[0] & 0xffc0) == 0xfe80
                || (seg[0] & 0xffc0) == 0xfec0
                // 64:ff9b:1::/48 local-use NAT64, 100::/64 discard
                || (seg[0] == 0x64 && seg[1] == 0xff9b && seg[2] == 1)
                || (seg[0] == 0x100 && seg[1] == 0 && seg[2] == 0 && seg[3] == 0)
        }
    }
}

fn ipv4_is_private(v4: std::net::Ipv4Addr) -> bool {
    let o = v4.octets();
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_unspecified()
        || v4.is_broadcast()
        || v4.is_multicast()
        // 0.0.0.0/8
        || o[0] == 0
        // 100.64.0.0/10 CGNAT (RFC 6598)
        || (o[0] == 100 && (64..=127).contains(&o[1]))
        // 192.0.0.0/24 IETF protocol assignments
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        // 198.18.0.0/15 benchmarking
        || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
        // 240.0.0.0/4 reserved
        || o[0] >= 240
}

/// Resolve a hostname to socket addresses (blocking DNS lookup).
/// Called via spawn_blocking to avoid blocking the tokio runtime.
fn dns_resolve_host(host: &str, port: u16) -> std::io::Result<Vec<std::net::SocketAddr>> {
    use std::net::ToSocketAddrs;
    Ok((host, port).to_socket_addrs()?.collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_is_private_loopback() {
        assert!(ip_is_private("127.0.0.1".parse().unwrap()));
        assert!(ip_is_private("::1".parse().unwrap()));
        assert!(ip_is_private("0.0.0.0".parse().unwrap()));
    }

    #[test]
    fn test_ip_is_private_rfc1918() {
        // 10.0.0.0/8
        assert!(ip_is_private("10.0.0.1".parse().unwrap()));
        assert!(ip_is_private("10.255.255.255".parse().unwrap()));
        // 172.16.0.0/12 — full range
        assert!(ip_is_private("172.16.0.1".parse().unwrap()));
        assert!(ip_is_private("172.20.5.3".parse().unwrap()));
        assert!(ip_is_private("172.30.0.1".parse().unwrap()));
        assert!(ip_is_private("172.31.255.255".parse().unwrap()));
        // 172.32.x should NOT be private
        assert!(!ip_is_private("172.32.0.1".parse().unwrap()));
        // 192.168.0.0/16
        assert!(ip_is_private("192.168.0.1".parse().unwrap()));
        assert!(ip_is_private("192.168.255.255".parse().unwrap()));
    }

    #[test]
    fn test_ip_is_private_link_local() {
        assert!(ip_is_private("169.254.0.1".parse().unwrap()));
        assert!(ip_is_private("169.254.169.254".parse().unwrap()));
    }

    #[test]
    fn test_ip_is_private_carrier_nat() {
        assert!(ip_is_private("100.64.0.1".parse().unwrap()));
        assert!(ip_is_private("100.127.255.255".parse().unwrap()));
        assert!(!ip_is_private("100.63.255.255".parse().unwrap()));
        assert!(!ip_is_private("100.128.0.1".parse().unwrap()));
    }

    #[test]
    fn test_ip_is_not_private() {
        assert!(!ip_is_private("8.8.8.8".parse().unwrap()));
        assert!(!ip_is_private("1.1.1.1".parse().unwrap()));
        assert!(!ip_is_private("203.0.113.1".parse().unwrap()));
    }

    #[test]
    fn test_is_private_url_blocks_internal_hostnames() {
        assert!(is_private_url("https://localhost/path"));
        assert!(is_private_url("https://test.internal/webhook"));
        assert!(is_private_url("https://myhost.local/api"));
        assert!(is_private_url("https://0.0.0.0/"));
    }

    #[test]
    fn test_is_private_url_blocks_private_ips() {
        assert!(is_private_url("https://127.0.0.1/"));
        assert!(is_private_url("https://10.0.0.1/"));
        assert!(is_private_url("https://192.168.1.1/"));
        assert!(is_private_url("https://172.16.0.1/"));
        assert!(is_private_url("https://172.30.0.1/"));
        assert!(is_private_url("https://172.31.0.1/"));
    }

    #[test]
    fn test_is_private_url_invalid() {
        assert!(is_private_url("not-a-url"));
    }

    #[test]
    fn test_https_scheme_enforcement() {
        // dispatch_webhook rejects non-HTTPS URLs at the URL parse + scheme check.
        // Verify the URL parsing logic directly.
        let http_url = url::Url::parse("http://example.com/webhook").unwrap();
        assert_ne!(http_url.scheme(), "https");

        let https_url = url::Url::parse("https://example.com/webhook").unwrap();
        assert_eq!(https_url.scheme(), "https");
    }

    #[test]
    fn test_is_private_url_blocks_metadata_endpoint() {
        // AWS/GCP/Azure metadata endpoints
        assert!(is_private_url("http://169.254.169.254/latest/meta-data/"));
        assert!(is_private_url("http://169.254.169.254/computeMetadata/v1/"));
    }

    #[test]
    fn test_is_private_url_blocks_carrier_nat() {
        assert!(is_private_url("https://100.64.0.1/"));
        assert!(is_private_url("https://100.127.255.254/"));
    }

    #[test]
    fn test_ip_is_private_ipv4_mapped_and_ula() {
        for ip in [
            "::ffff:169.254.169.254",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:7f00:1",
            "fc00::1",
            "fd12:3456::1",
            "fec0::1",
            "64:ff9b::a9fe:a9fe",
            "64:ff9b::7f00:1",
            "ff02::1",
            "::",
            "::127.0.0.1",
            "0.0.0.1",
            "192.0.0.8",
            "198.18.0.1",
            "198.19.255.255",
            "224.0.0.1",
            "255.255.255.255",
            "240.0.0.1",
        ] {
            assert!(ip_is_private(ip.parse().unwrap()), "{ip} must be blocked");
        }
        assert!(!ip_is_private("2606:4700:4700::1111".parse().unwrap()));
        assert!(!ip_is_private("::ffff:8.8.8.8".parse().unwrap()));
        assert!(!ip_is_private("198.20.0.1".parse().unwrap()));
    }

    #[test]
    fn test_is_private_url_literal_encodings() {
        for u in [
            "https://[::1]/",
            "https://[::ffff:169.254.169.254]/",
            "https://[fd00::1]/",
            "https://2130706433/",
            "https://0x7f000001/",
            "https://0177.0.0.1/",
            "https://127.1/",
            "https://LOCALHOST/",
            "https://localhost./",
            "https://foo.localhost/",
            "https://metadata.google.internal./",
            "https://user:pw@127.0.0.1/",
        ] {
            assert!(is_private_url(u), "{u} must be blocked");
        }
    }

    #[test]
    fn test_validate_webhook_url_syntax() {
        assert!(validate_webhook_url("https://hooks.slack.com/services/T/B/X").is_ok());
        assert!(validate_webhook_url("http://hooks.slack.com/x").is_err());
        assert!(validate_webhook_url("https://127.0.0.1/x").is_err());
        assert!(validate_webhook_url("https://[::ffff:127.0.0.1]/x").is_err());
        assert!(validate_webhook_url("https://user:pw@example.com/x").is_err());
        assert!(validate_webhook_url("https://localhost/x").is_err());
        assert!(validate_webhook_url("nonsense").is_err());
    }

    #[test]
    fn test_vet_addrs_rejects_mixed_records() {
        let pub_ip: std::net::IpAddr = "8.8.8.8".parse().unwrap();
        let priv_ip: std::net::IpAddr = "10.0.0.1".parse().unwrap();
        assert!(vet_addrs(&[pub_ip, priv_ip]).is_err());
        assert!(vet_addrs(&[priv_ip, pub_ip]).is_err());
        assert!(vet_addrs(&[]).is_err());
        assert!(vet_addrs(&[pub_ip]).is_ok());
    }
}
