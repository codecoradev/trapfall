# TrapFall

Self-hosted error capture engine — Rust + SvelteKit 5. Sentry SDK compatible (drop-in DSN swap).

Lightweight alternative to Sentry. Capture errors from any Sentry SDK, view them in a real-time dashboard, get webhook alerts. Single binary, SQLite or Postgres, **6MB Docker image**.

## Features

- **Sentry-compatible ingest** — Drop-in DSN swap, works with any Sentry SDK (Rust, Python, JS, Flutter, etc.)
- **Multi-project support** — Web, mobile, API — each with its own DSN and isolated errors
- **Project management** — Archive, unarchive, rename, rotate DSN, permanent delete
- **Real-time dashboard** — SvelteKit 5 + Tailwind v4 + shadcn-svelte
- **Issue search + filters** — Search by title/culprit, filter by status and level, pagination
- **WebSocket updates** — Live issue feed, no refresh needed
- **Blake3 fingerprinting** — Automatic error grouping and deduplication
- **Alert rules** — Per-project condition-based webhooks with cooldown
- **Full-text search** — LIKE + sqlite_trigram substring search
- **OpenAPI docs** — Swagger UI at `/api/docs`
- **MCP server** — 12 AI agent tools via stdio JSON-RPC
- **Tiny Docker image** — 5.75MB (scratch + MUSL static binary + rustls)
- **Multi-backend database** — SQLite (default, zero-config) or Postgres (built-in, no extra flags needed)
- **One-command deploy** — Docker Compose with persistent volume

## Quick Start

### Docker (recommended)

```bash
docker pull ghcr.io/codecoradev/trapfall:latest
docker run -p 3000:3000 -v trapfall-data:/data ghcr.io/codecoradev/trapfall:latest
```

Or with Docker Compose:

```bash
# Clone and run
git clone https://github.com/codecoradev/trapfall.git
cd trapfall
docker compose up -d

# Open http://localhost:3000 → Setup wizard
```

### From Binary

Download from [GitHub Releases](https://github.com/codecoradev/trapfall/releases/latest):

```bash
# Linux (x86_64)
tar xzf trapfall-x86_64-unknown-linux-gnu-v0.0.5.tar.gz
./trapfall --db trapfall.db serve --listen 0.0.0.0:3000
```

### From Source

```bash
# Build frontend
cd web && npm ci && npm run build && cd ..

# Build + run
cargo run --release -p trapfalld -- --db trapfall.db serve --listen 0.0.0.0:3000
```

## Setup Wizard

1. Open `http://localhost:3000` → First run shows setup wizard
2. Create admin account (email, name, password)
3. Default project created automatically with DSN
4. Copy DSN → integrate with your app

## Multi-Project Setup

Create separate projects for each app/service:

1. Dashboard → **Projects** → **"+ Add Project"**
2. Name it (e.g., "Web App", "Mobile App", "Backend API")
3. Each project gets a unique DSN
4. Point each SDK to its own DSN — errors are isolated per project

### Managing Projects

- **Rename**: Click ⋮ menu → Rename
- **Rotate DSN**: Click ⋮ menu → Rotate DSN (old key revoked immediately)
- **Archive**: Click ⋮ menu → Archive (hides from main view, preserves data)
- **Delete**: Archive first, then Delete permanently from Archived tab
- **Unarchive**: Restore archived projects from the Archived tab

## SDK Integration

Works with any Sentry SDK — just swap the DSN to point to your TrapFall server:

### Rust

```rust
sentry::init(("https://<key>@your-server:3000/<project_id>", sentry::ClientOptions::default()));
```

### Python

```python
import sentry_sdk
sentry_sdk.init(dsn="https://<key>@your-server:3000/<project_id>")
```

### JavaScript / Node.js

```js
Sentry.init({ dsn: "https://<key>@your-server:3000/<project_id>" });
```

### Flutter / Dart

```dart
await SentryFlutter.init((options) => {
  options.dsn = "https://<key>@your-server:3000/<project_id>",
});
```

## CLI Commands

```bash
trapfall serve                        # Start HTTP server (default)
trapfall project add "My App"         # Create project (CLI)
trapfall project add "Mobile" mobile  # Create project with custom slug
trapfall project list                 # List projects
trapfall project rotate-dsn app       # Rotate DSN key
trapfall healthcheck                  # Docker healthcheck
trapfall mcp                          # Start MCP server (stdio)
```

## Configuration

| Env | Default | Description |
|-----|---------|-------------|
| `TRAPFALL_DATABASE_URL` | `sqlite:trapfall.db` | Database URL (`sqlite:` or `postgres://`, falls back to `--db`) |
| `TRAPFALL_LISTEN` | `0.0.0.0:9090` | HTTP listen address (Docker uses 3000) |
| `TRAPFALL_SECURE_COOKIE` | `true` | Set `false` for HTTP local dev |
| `TRAPFALL_CORS_ORIGINS` | *(empty = allow all)* | Comma-separated origins for production |
| `RUST_LOG` | `info` | Log level (`debug` for verbose) |

See [.env.example](.env.example) for full reference.

## API Endpoints

### Ingest (public, DSN key auth)
- `POST /api/{project_id}/envelope/` — Sentry SDK envelope

### Dashboard (cookie auth)
- `POST /api/0/setup` — First-run setup wizard
- `POST /api/0/auth/login` — Login
- `POST /api/0/auth/logout` — Logout
- `GET /api/0/auth/me` — Current user
- `POST /api/0/auth/change-password` — Change password
- `GET /api/0/projects` — List projects
- `POST /api/0/projects` — Create project
- `GET /api/0/projects/{slug}` — Get project
- `PATCH /api/0/projects/{slug}` — Rename project
- `DELETE /api/0/projects/{slug}` — Delete project (archived only)
- `POST /api/0/projects/{slug}/archive` — Archive project
- `DELETE /api/0/projects/{slug}/archive` — Unarchive project
- `POST /api/0/projects/{slug}/rotate-dsn` — Rotate DSN key
- `GET /api/0/projects/{slug}/issues` — List issues (with filters + pagination)
- `GET /api/0/projects/{slug}/search?q=...` — Search issues
- `GET /api/0/issues/{id}` — Get issue detail
- `POST /api/0/issues/{id}/status` — Set issue status (resolved/unresolved/ignored)
- `GET /api/0/issues/{id}/events` — List events
- `GET /api/0/ws` — WebSocket real-time updates
- `GET /api/0/projects/{slug}/rules` — Alert rules
- `POST /api/0/projects/{slug}/rules` — Create alert rule
- `DELETE /api/0/rules/{id}` — Delete alert rule
- `POST /api/0/rules/{id}/toggle` — Enable/disable rule
- `GET /api/docs` — Swagger UI
- `GET /api/docs/openapi.yaml` — OpenAPI spec

### System
- `GET /health` — Health check
- `GET /metrics` — Prometheus metrics

## MCP Tools

12 tools for AI agents via stdio JSON-RPC 2.0:

`list_issues`, `get_issue`, `get_event`, `set_status`, `search_issues`, `list_projects`, `get_project`, `get_project_stats`, `list_alert_rules`, `list_events`, `rotate_dsn`, `healthcheck`

## Tech Stack

| Layer | Tech |
|-------|------|
| Backend | Rust, Axum 0.8, SQLite + Postgres (sqlx), tokio |
| Frontend | SvelteKit 5, Tailwind v4, shadcn-svelte |
| TLS | rustls (pure Rust, no OpenSSL) |
| Fingerprinting | blake3 |
| Build | Cargo workspace, npm, Docker multi-stage (scratch + MUSL) |
| CI | GitHub Actions (10 checks including Cora Review, Trivy, Cargo Audit) |

## Architecture

```
HTTP → mpsc(256) → digest(batch=16) → broadcast(64) + webhook(64)
                          ↓
              SQLite (WAL, single-writer)
```

Single binary with embedded SPA via `rust-embed`. Single-writer SQLite in WAL mode with `synchronous=NORMAL`. Docker image built on `scratch` with statically-linked MUSL binary — no OS layer, minimal attack surface.

## Documentation

Full documentation at [docs/](docs/) (VitePress):

- [Getting Started](docs/guide/getting-started.md)
- [Docker Deployment](docs/guide/docker.md)
- [Multi-Project Management](docs/guide/multi-project.md)
- [SDK Integration](docs/guide/sdk-integration.md)
- [Search & Filters](docs/guide/search.md)
- [Alert Rules](docs/guide/alerts.md)
- [API Reference](docs/guide/api.md)
- [Configuration](docs/guide/configuration.md)
- [Security](docs/guide/security.md)
- [MCP Server](docs/guide/mcp.md)

## License

Apache-2.0
