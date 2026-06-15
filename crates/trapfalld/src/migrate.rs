//! Database migration tool — export/import/verify.
//!
//! Phase 5 (#170): migrate data between SQLite and Postgres backends.
//!
//! ## Usage
//!
//! ```bash
//! # Export SQLite → JSONL
//! trapfall db export --from sqlite:trapfall.db --to dump.jsonl
//!
//! # Import JSONL → Postgres
//! trapfall db import --from dump.jsonl --to postgres://user@host/db
//!
//! # Verify row counts
//! trapfall db verify --url postgres://user@host/db
//! ```

use anyhow::{Context, Result};
use serde_json::json;
use sqlx::Row;

/// Tables to export/import, in FK-safe order (parents before children).
const TABLES: &[&str] =
    &["projects", "users", "issues", "events", "sessions", "auth_attempts", "alert_rules", "alert_history"];

/// Column lists per table (must match schema exactly).
const COLUMNS: &[(&str, &[&str])] = &[
    ("projects", &["id", "slug", "name", "dsn_key", "dsn", "webhook_url", "archived_at", "created_at"]),
    ("users", &["id", "email", "name", "password_hash", "role", "created_at"]),
    (
        "issues",
        &[
            "id",
            "project_id",
            "fingerprint",
            "title",
            "culprit",
            "status",
            "level",
            "count",
            "user_count",
            "first_seen",
            "last_seen",
        ],
    ),
    ("events", &["id", "issue_id", "project_id", "data", "received_at"]),
    ("sessions", &["id", "user_id", "token", "expires_at", "created_at"]),
    ("auth_attempts", &["id", "email", "ip", "success", "created_at"]),
    (
        "alert_rules",
        &[
            "id",
            "project_id",
            "name",
            "enabled",
            "conditions",
            "action_type",
            "action_config",
            "cooldown_seconds",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "alert_history",
        &["id", "rule_id", "project_id", "issue_id", "status", "attempts", "last_error", "created_at", "sent_at"],
    ),
];

// ── Export ────────────────────────────────────────────────────────────

/// Export all tables from a database to a JSONL file.
///
/// Each line is `{"table": "<name>", "row": {<columns>}}`.
/// Tables are exported in FK-safe order.
pub async fn export_database(from_url: &str, to_path: &str) -> Result<()> {
    let url = trapfall_db::normalise_url(from_url);
    let backend = trapfall_db::open_database(&url).await?;
    let pool = backend.sqlite_pool().context("Export currently only supports SQLite sources")?;

    let mut output = String::new();
    let mut total_rows = 0;

    for (table, columns) in COLUMNS {
        let col_list = columns.join(", ");
        let sql = format!("SELECT {col_list} FROM {table}");
        let rows = sqlx::query(&sql).fetch_all(pool).await?;

        let n = rows.len();
        for row in rows {
            let mut record = serde_json::Map::new();
            for col in *columns {
                let val: Option<String> = row.try_get(col).unwrap_or(None);
                record.insert(col.to_string(), json!(val));
            }
            let line = json!({ "table": table, "row": record });
            output.push_str(&line.to_string());
            output.push('\n');
            total_rows += 1;
        }
        tracing::info!("Exported {table}: {} rows", n);
    }

    std::fs::write(to_path, &output)?;
    println!("✅ Exported {total_rows} rows to {to_path}");
    Ok(())
}

// ── Import ────────────────────────────────────────────────────────────

/// Import JSONL data into a Postgres database.
///
/// Reads each line, determines the table, and inserts the row.
/// Preserves UUIDs for referential integrity.
#[cfg(feature = "postgres")]
pub async fn import_database(from_path: &str, to_url: &str) -> Result<()> {
    let content = std::fs::read_to_string(from_path).with_context(|| format!("Failed to read {from_path}"))?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(to_url)
        .await
        .context("Failed to connect to target Postgres")?;

    // Run migrations to ensure schema exists
    sqlx::query(include_str!("../../trapfall-db/migrations/postgres/001_initial.sql")).execute(&pool).await?;
    sqlx::query(include_str!("../../trapfall-db/migrations/postgres/002_alert_rules.sql")).execute(&pool).await?;

    // Build column map for validation
    let col_map: std::collections::HashMap<&str, &[&str]> = COLUMNS.iter().copied().collect();

    let mut total = 0;
    let mut errors = 0;

    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let record: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Skipping line {} (parse error): {e}", i + 1);
                errors += 1;
                continue;
            }
        };

        let table = record["table"].as_str().context("Missing 'table' field")?;
        let row = &record["row"];

        let columns = col_map.get(table).with_context(|| format!("Unknown table: {table}"))?;

        // Build INSERT with $N params
        let col_list = columns.join(", ");
        let placeholders: Vec<String> = (1..=columns.len()).map(|n| format!("${n}")).collect();
        let sql = format!("INSERT INTO {table} ({col_list}) VALUES ({})", placeholders.join(", "));

        let mut q = sqlx::query(&sql);
        for col in *columns {
            let val = row[*col].as_str().unwrap_or(NULL_SENTINEL);
            if val == NULL_SENTINEL {
                q = q.bind(None::<String>);
            } else {
                q = q.bind(val);
            }
        }

        if let Err(e) = q.execute(&pool).await {
            tracing::warn!("Insert failed for {table} (line {}): {e}", i + 1);
            errors += 1;
        } else {
            total += 1;
        }
    }

    println!("✅ Imported {total} rows ({errors} errors)");
    Ok(())
}

#[cfg(feature = "postgres")]
const NULL_SENTINEL: &str = "\0__NULL__\0";

#[cfg(not(feature = "postgres"))]
pub async fn import_database(_from_path: &str, _to_url: &str) -> Result<()> {
    anyhow::bail!("Import requires `postgres` Cargo feature. Build with `--features postgres`.")
}

// ── Verify ────────────────────────────────────────────────────────────

/// Verify database integrity by counting rows per table.
pub async fn verify_database(url: &str) -> Result<()> {
    let url = trapfall_db::normalise_url(url);
    let backend = trapfall_db::open_database(&url).await?;

    println!("┌──────────────────┬───────────┐");
    println!("│ Table            │ Row count │");
    println!("├──────────────────┼───────────┤");

    let mut grand_total = 0i64;
    for table in TABLES {
        let count = backend.count_table(table).await.unwrap_or(0);
        grand_total += count;
        println!("│ {:<16} │ {:>9} │", table, count);
    }

    println!("├──────────────────┼───────────┤");
    println!("│ {:<16} │ {:>9} │", "TOTAL", grand_total);
    println!("└──────────────────┴───────────┘");

    // Ping check
    let healthy = backend.ping().await?;
    if healthy {
        println!("✅ Database is healthy");
    } else {
        println!("❌ Database ping failed");
    }

    Ok(())
}
