# SQLite → Postgres Migration

TrapFall supports migrating from SQLite to Postgres using the built-in `db` CLI commands.

## Prerequisites

- Postgres 14+ running and accessible
- TrapFall binary built with `--features postgres`

## Steps

### 1. Export from SQLite

```bash
# Export all data to JSONL format
trapfall db export \
  --from sqlite:trapfall.db \
  --to /tmp/trapfall-export.jsonl
```

This exports all tables (projects, users, issues, events, sessions, auth_attempts, alert_rules, alert_history) in FK-safe order.

### 2. Import to Postgres

```bash
# Import JSONL into Postgres
trapfall db import \
  --from /tmp/trapfall-export.jsonl \
  --to postgres://user:password@localhost:5432/trapfall
```

The import automatically runs migrations on the target Postgres to create the schema if needed.

### 3. Verify

```bash
# Check row counts and health
trapfall db verify \
  --url postgres://user:password@localhost:5432/trapfall
```

Output:

```
┌──────────────────┬───────────┐
│ Table            │ Row count │
├──────────────────┼───────────┤
│ projects         │         3 │
│ users            │         1 │
│ issues           │        42 │
│ events           │       156 │
│ ...              │       ... │
├──────────────────┼───────────┤
│ TOTAL            │       202 │
└──────────────────┴───────────┘
✅ Database is healthy
```

### 4. Switch over

Update your deployment to use the Postgres URL:

```bash
TRAPFALL_DATABASE_URL=postgres://user:password@localhost:5432/trapfall \
  trapfall serve --listen 0.0.0.0:3000
```

## Notes

- **Downtime**: Export while TrapFall is stopped to avoid missing events.
- **IDs preserved**: All UUIDs are kept — projects and DSNs remain the same.
- **Large databases**: Export processes tables sequentially. For 100K+ events, expect ~30 seconds.
- **Rollback**: Keep the SQLite file as backup. To rollback, switch `TRAPFALL_DATABASE_URL` back to SQLite.
