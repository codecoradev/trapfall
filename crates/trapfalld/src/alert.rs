//! Alert engine — evaluates rules against incoming issues and dispatches webhooks.

use sqlx::SqlitePool;
use std::sync::LazyLock;
use tokio::sync::mpsc;
use trapfall_proto::{AlertRule, Issue};

use trapfall_core::Store;

/// Shared HTTP client for webhook dispatch — connection pooling.
static REQWEST_CLIENT: std::sync::LazyLock<reqwest::Client> =
    std::sync::LazyLock::new(|| reqwest::Client::builder().pool_max_idle_per_host(4).build().unwrap_or_default());

/// Spawn the alert engine background task.
pub fn spawn_alert_engine(pool: SqlitePool, _buffer: usize) -> mpsc::UnboundedSender<Issue> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Issue>();

    tokio::spawn(async move {
        while let Some(issue) = rx.recv().await {
            if let Err(e) = process_issue(&pool, &issue).await {
                tracing::warn!("Alert engine error for issue {}: {e}", issue.id);
            }
        }
        tracing::info!("Alert engine task stopped");
    });

    tx
}

async fn process_issue(pool: &SqlitePool, issue: &Issue) -> anyhow::Result<()> {
    let store = Store::new(pool.clone());

    let rules = store.get_enabled_rules_for_project(&issue.project_id).await?;

    for rule in rules {
        if !matches_rule(&rule, issue) {
            continue;
        }

        if is_cooling_down(pool, &rule.id, rule.cooldown_seconds).await? {
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

async fn is_cooling_down(pool: &SqlitePool, rule_id: &str, cooldown_seconds: i64) -> anyhow::Result<bool> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT created_at FROM alert_history WHERE rule_id = ? AND status = 'sent' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(rule_id)
    .fetch_optional(pool)
    .await?;

    if let Some((last_fired,)) = row {
        if let Ok(fired_time) = chrono::DateTime::parse_from_rfc3339(&last_fired) {
            let elapsed =
                chrono::Utc::now().signed_duration_since(fired_time.with_timezone(&chrono::Utc)).num_seconds();
            return Ok(elapsed < cooldown_seconds);
        }
    }

    Ok(false)
}

/// Best-effort webhook dispatch with SSRF protection.
async fn dispatch_webhook(rule: &AlertRule, issue: &Issue) -> anyhow::Result<()> {
    let url = rule
        .action_config
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("no url in action_config"))?;

    // SSRF protection: block internal/private IPs
    if is_private_url(url) {
        tracing::warn!("Webhook URL blocked (private/internal IP): {url}");
        anyhow::bail!("webhook URL points to private/internal address");
    }

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

    let resp = REQWEST_CLIENT.post(url).json(&payload).timeout(std::time::Duration::from_secs(10)).send().await?;

    if resp.status().is_success() {
        tracing::info!("Webhook dispatched to {url} for rule '{}'", rule.name);
    } else {
        let status = resp.status();
        tracing::warn!("Webhook returned {status} for rule '{}'", rule.name);
    }

    Ok(())
}

/// Check if a URL points to a private/internal IP address (SSRF protection).
fn is_private_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return true; // invalid URL = block
    };
    let Some(host) = parsed.host_str() else {
        return true;
    };

    // Block obvious internal patterns
    matches!(host, "localhost" | "127.0.0.1" | "0.0.0.0" | "::1")
        || host.starts_with("192.168.")
        || host.starts_with("10.")
        || host.starts_with("172.16.")
        || host.starts_with("172.17.")
        || host.starts_with("172.18.")
        || host.starts_with("172.19.")
        || host.starts_with("172.2")
        || host.ends_with(".internal")
        || host.ends_with(".local")
}
