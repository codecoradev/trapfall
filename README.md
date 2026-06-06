# TrapFall

Self-hosted error capture daemon — Rust + SvelteKit 5.

Lightweight alternative to Sentry. Capture errors from any SDK, view them in a real-time dashboard, get webhook alerts.

## Features

- **Error ingest** — Sentry-compatible envelope API
- **Real-time dashboard** — SvelteKit 5 + Tailwind v4 + shadcn-svelte
- **WebSocket updates** — Live issue feed, no refresh needed
- **Alert rules** — Condition-based webhooks with cooldown
- **MCP server** — AI agent tools via stdio JSON-RPC
- **Full-text search** — LIKE-based substring search
- **One-command deploy** — Docker Compose

## Quick Start

### Docker (recommended)

```bash
docker compose up -d
# Open http://localhost:9090 → Setup wizard
```

### From Source

```bash
# Build frontend
cd web && npm ci && npm run build && cd ..

# Build + run
cargo run --release -p trapfalld -- serve
```

## CLI Commands

```bash
trapfall serve                    # Start HTTP server (default)
trapfall project add "My App"     # Create project
trapfall project list             # List projects
trapfall project rotate-dsn app   # Rotate DSN key
trapfall project set-webhook app https://hooks.example.com/...
trapfall healthcheck              # Docker healthcheck
trapfall mcp                      # Start MCP server (stdio)
```

## Configuration

| Env | Default | Description |
|-----|---------|-------------|
| `TRAPFALL_DB` | `trapfall.db` | SQLite database path |
| `TRAPFALL_LISTEN` | `0.0.0.0:9090` | HTTP listen address |
| `RUST_LOG` | `info` | Log level |

## API Endpoints

### Ingest (public)
- `POST /api/{project_id}/envelope/` — Sentry SDK envelope

### Dashboard (auth-required)
- `GET /api/0/projects` — List projects
- `GET /api/0/projects/{slug}` — Get project
- `GET /api/0/projects/{slug}/issues` — List issues
- `GET /api/0/issues/{id}` — Get issue detail
- `POST /api/0/issues/{id}/status` — Set issue status
- `GET /api/0/issues/{id}/events` — List events
- `GET /api/0/projects/{slug}/rules` — List alert rules
- `POST /api/0/projects/{slug}/rules` — Create alert rule
- `DELETE /api/0/rules/{id}` — Delete alert rule
- `POST /api/0/rules/{id}/toggle` — Enable/disable rule
- `GET /api/0/projects/{slug}/search?q=...` — Search issues
- `GET /api/0/ws` — WebSocket real-time updates

### System
- `GET /health` — Health check
- `GET /metrics` — Prometheus metrics

## MCP Tools

12 tools for AI agents via stdio:

`list_issues`, `get_issue`, `get_event`, `set_status`, `search_issues`, `list_projects`, `get_project`, `get_project_stats`, `list_alert_rules`, `list_events`, `rotate_dsn`, `healthcheck`

## Tech Stack

| Layer | Tech |
|-------|------|
| Backend | Rust, Axum 0.8, SQLite (sqlx), tokio |
| Frontend | SvelteKit 5, Tailwind v4, shadcn-svelte |
| Build | Cargo workspace, npm, Docker multi-stage |
| CI | GitHub Actions (check, fmt, clippy, test, build, Cora Review) |

## License

MIT
