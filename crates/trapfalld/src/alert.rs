//! Alert engine — evaluates rules against incoming issues and dispatches webhooks.
//!
//! Runs as a background task alongside the digest task. Receives `Issue` structs
//! after digest processing, checks them against enabled alert rules, and fires
//! webhook POSTs (best-effort).

use sqlx::SqlitePool;
use tokio::sync::mpsc;
use trapfall_proto::{AlertRule, Issue};

use trapfall_core::Store;

/// Spawn the alert engine background task. Returns the sender for injecting issues.
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

    // Get enabled rules for this project
    let rules = store.get_enabled_rules_for_project(&issue.project_id).await?;

    for rule in rules {
        if !matches_rule(&rule, issue) {
            continue;
        }

        // Cooldown check: skip if same rule fired within cooldown period
        if is_cooling_down(pool, &rule.id, rule.cooldown_seconds).await? {
            tracing::debug!("Rule {} on cooldown, skipping", rule.name);
            continue;
        }

        // Record alert history
        let history_id = store.insert_alert_history(&rule.id, &issue.project_id, &issue.id).await?;

        // Dispatch based on action_type
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

/// Check if an issue matches a rule's conditions.
///
/// Conditions JSON format:
/// ```json
/// {
///   "level": ["error", "fatal"],    // match issue level
///   "count_gte": 10,               // match if issue count >= N
///   "title_contains": "panic"       // substring match in title
/// }
/// ```
fn matches_rule(rule: &AlertRule, issue: &Issue) -> bool {
    let conditions = &rule.conditions;

    // Level filter
    if let Some(levels) = conditions.get("level").and_then(|v| v.as_array()) {
        let level_str = format!("{:?}", issue.level).to_lowercase();
        let matches_level = levels
            .iter()
            .any(|l| l.as_str().map(|s| s.to_lowercase() == level_str).unwrap_or(false));
        if !matches_level {
            return false;
        }
    }

    // Count threshold
    if let Some(min_count) = conditions.get("count_gte").and_then(|v| v.as_i64()) {
        if issue.count < min_count {
            return false;
        }
    }

    // Title substring match
    if let Some(pattern) = conditions.get("title_contains").and_then(|v| v.as_str()) {
        if !issue.title.to_lowercase().contains(&pattern.to_lowercase()) {
            return false;
        }
    }

    true
}

/// Check if a rule has fired within its cooldown window.
async fn is_cooling_down(
    pool: &SqlitePool,
    rule_id: &str,
    cooldown_seconds: i64,
) -> anyhow::Result<bool> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT created_at FROM alert_history WHERE rule_id = ? AND status = 'sent' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(rule_id)
    .fetch_optional(pool)
    .await?;

    if let Some((last_fired,)) = row {
        if let Ok(fired_time) = chrono::DateTime::parse_from_rfc3339(&last_fired) {
            let elapsed = chrono::Utc::now()
                .signed_duration_since(fired_time.with_timezone(&chrono::Utc))
                .num_seconds();
            return Ok(elapsed < cooldown_seconds);
        }
    }

    Ok(false)
}

/// Best-effort webhook dispatch via reqwest.
async fn dispatch_webhook(rule: &AlertRule, issue: &Issue) -> anyhow::Result<()> {
    let url = rule
        .action_config
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("no url in action_config"))?;

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

    let client = reqwest::Client::new();
    let resp =
        client.post(url).json(&payload).timeout(std::time::Duration::from_secs(10)).send().await?;

    if resp.status().is_success() {
        tracing::info!("Webhook dispatched to {url} for rule '{}'", rule.name);
    } else {
        let status = resp.status();
        tracing::warn!("Webhook returned {status} for rule '{}'", rule.name);
    }

    Ok(())
}
