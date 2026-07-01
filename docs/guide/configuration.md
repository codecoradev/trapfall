# Configuration

## Environment Variables

All configuration is via environment variables. Copy `.env.example` to `.env` and customize.

| Variable | Default | Description |
|----------|---------|-------------|
| `TRAPFALL_DATABASE_URL` | `sqlite:trapfall.db` | Database URL (`sqlite:path.db` or `postgres://...`) |
| `TRAPFALL_LISTEN` | `0.0.0.0:9090` | HTTP listen address |
| `TRAPFALL_SECURE_COOKIE` | `true` | Set `false` for HTTP local dev |
| `TRAPFALL_CORS_ORIGINS` | *(empty = allow all)* | Comma-separated origins |
| `TRAPFALL_TIMEZONE` | `UTC` | IANA timezone for display only (logs + dashboard); storage stays UTC |
| `RUST_LOG` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |

## Database URL

TrapFall supports selecting a database backend via URL scheme:

```bash
# SQLite (default) — bare paths auto-prefixed with sqlite:
TRAPFALL_DATABASE_URL=trapfall.db               # → sqlite:trapfall.db
TRAPFALL_DATABASE_URL=sqlite:/data/trapfall.db   # explicit

# Postgres (built-in, no extra build flags needed)
TRAPFALL_DATABASE_URL=postgres://user:pass@host:5432/trapfall
```

The `--db` CLI flag takes precedence over the env var:

```bash
trapfall --db /custom/path.db serve
trapfall --db sqlite:/data/trapfall.db serve
```

## Local Development

For local HTTP development, you must set:

```bash
TRAPFALL_SECURE_COOKIE=false
```

Otherwise browsers will reject the session cookie over HTTP.

## Production

For production with HTTPS:

```bash
TRAPFALL_SECURE_COOKIE=true
TRAPFALL_CORS_ORIGINS=https://trapfall.yourcompany.com
RUST_LOG=trapfall=info
```

## Display Timezone

`TRAPFALL_TIMEZONE` sets the timezone for **display only** — log timestamps
and the dashboard. **All stored timestamps remain UTC** (the immutable
invariant used by Sentry SDKs, DSNs, and retention); this setting never
affects stored data, only how times are rendered.

```bash
# Jakarta time (WIB, UTC+7) for logs + dashboard
TRAPFALL_TIMEZONE=Asia/Jakarta
```

Use any [IANA timezone name](https://en.wikipedia.org/wiki/List_of_tz_database_time_zones)
(e.g. `America/New_York`, `Europe/London`, `Asia/Singapore`). Invalid values
fall back to `UTC` with a warning at startup.

## Docker Compose

The included `docker-compose.yml` is pre-configured:

```yaml
services:
  trapfall:
    image: ghcr.io/codecoradev/trapfall:0.0.5
    ports:
      - "9090:9090"
    volumes:
      - trapfall-data:/data
    environment:
      - RUST_LOG=trapfall=debug
      - TRAPFALL_SECURE_COOKIE=false
      - TRAPFALL_DATABASE_URL=sqlite:/data/trapfall.db
    command: serve --listen 0.0.0.0:9090
```

## Database

TrapFall uses SQLite in WAL mode with `synchronous=NORMAL`. The database is a single file — back it up by copying the file.

No migration commands needed — the schema is auto-created on first run.
