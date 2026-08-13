# TrapFall Architecture

> **Version:** 0.2.0 · **Edition:** 2024 · **MSRV:** 1.86 · **License:** Apache-2.0

## Overview

TrapFall is a self-hosted error capture engine built in Rust with an embedded SvelteKit SPA dashboard. It is **Sentry SDK compatible** — swap the DSN URL and existing Sentry SDKs work without modification.

```
┌─────────────────────────────────────────────────────────────────┐
│                        Sentry SDKs                               │
│         (Browser, Node, Python, Go, Rust, Flutter…)              │
└────────────┬────────────────────────────────────┬───────────────┘
             │ HTTP POST envelope                 │ MCP (stdio)
             ▼                                    ▼
┌──────────────────────────────┐   ┌──────────────────────────────┐
│        trapfalld (daemon)     │   │     trapfall-mcp              │
│                               │   │   (JSON-RPC 2.0 over stdio)   │
│  ┌─────────┐  ┌───────────┐  │   └──────────────┬───────────────┘
│  │  Axum   │→ │ Ingest    │  │                  │
│  │ Router  │  │ Pipeline  │  │                  │
│  └────┬────┘  └─────┬─────┘  │                  │
│       │             │         │                  │
│  ┌────▼────┐  ┌─────▼─────┐  │                  │
│  │  Auth   │  │ Scrub PII │  │                  │
│  │  + CORS │  │ (UU PDP)  │  │                  │
│  └─────────┘  └─────┬─────┘  │                  │
│                      │         │                  │
│               ┌──────▼──────┐  │                  │
│               │  Store      │←┼──────────────────┘
│               │  (trait)    │  │
│               └──────┬──────┘  │
│                      │         │
│               ┌──────▼──────┐  │
│               │ SQLite / PG │  │
│               └─────────────┘  │
│                               │
│  ┌─────────────────────────┐  │
│  │ Background Tasks         │  │
│  │ • Digest (batch=16)      │  │
│  │ • Webhook alerts         │  │
│  │ • Retention purge        │  │
│  │ • WebSocket fan-out      │  │
│  └─────────────────────────┘  │
│                               │
│  ┌─────────────────────────┐  │
│  │ Embedded SPA             │  │
│  │ (rust-embed, SvelteKit)  │  │
│  └─────────────────────────┘  │
└───────────────────────────────┘
```

## Crate Dependency Graph

```
trapfall-proto   ←── wire types (no deps)
       ↑
trapfall-db      ←── SQLite + Postgres implementations
       ↑
trapfall-core    ←── Store trait, auth, fingerprinting
       ↑
trapfall-ingest  ←── Sentry envelope parser
       ↑
trapfall-mcp     ←── MCP server (JSON-RPC)
       ↑
trapfalld        ←── daemon binary (pulls all above)
```

| Crate | Role | Depends On |
|-------|------|------------|
| `trapfall-proto` | Wire types: `Event`, `Issue`, `Transaction`, `Breadcrumb`, `StackFrame` | — |
| `trapfall-db` | Data layer: `Store` impl for SQLite + Postgres, migrations | `proto` |
| `trapfall-core` | Business logic: `Store` trait, auth (argon2), fingerprinting (blake3) | `proto`, `db` |
| `trapfall-ingest` | Sentry SDK envelope parser (multi-part, gzip/deflate) | `proto`, `core` |
| `trapfall-mcp` | MCP tool server via stdio (12 tools, JSON-RPC 2.0) | `proto`, `core`, `db` |
| `trapfalld` | Daemon binary: Axum HTTP server, auth, alerts, retention, WebSocket, SPA | all |

## Data Flow: Error Ingest

```
Sentry SDK ──POST /api/{project_id}/envelope/──→ trapfalld
                                                      │
                                           ┌──────────▼──────────┐
                                           │ 1. Rate limit check  │
                                           │ 2. Auth (DSN key)    │
                                           │ 3. Body size limit   │
                                           │    (2MB ingest)      │
                                           │ 4. Parse envelope    │
                                           │    (gzip/deflate)    │
                                           │ 5. Scrub PII         │
                                           │    (regex pipeline)  │
                                           │ 6. Fingerprint       │
                                           │    (blake3 hash)     │
                                           │ 7. Dedup + group     │
                                           └──────────┬──────────┘
                                                      │
                                           ┌──────────▼──────────┐
                                           │    mpsc(256)         │
                                           └──────────┬──────────┘
                                                      │
                              ┌───────────────────────┼───────────────────┐
                              │                       │                   │
                    ┌─────────▼─────────┐  ┌─────────▼─────────┐ ┌───────▼───────┐
                    │ Digest pipeline    │  │ Webhook dispatcher│ │ WebSocket     │
                    │ (batch=16)         │  │ (HTTPS-only,      │ │ fan-out       │
                    │ → DB insert        │  │  no redirects,    │ │ (broadcast)   │
                    │                    │  │  SSRF-guarded)    │ │               │
                    └────────────────────┘  └───────────────────┘ └───────────────┘
```

## Key Design Decisions

| Decision | Rationale | Status |
|----------|-----------|--------|
| **blake3** for fingerprinting | Deterministic, fast, no collision in practice | Decided |
| **LIKE + trigram** instead of FTS5 | Simpler, fewer moving parts, good enough for error search | Decided |
| **Single-writer SQLite** (WAL, `synchronous=NORMAL`) | Zero-config, single-file DB, sufficient for solo/small-team | Decided |
| **Postgres** as optional backend | Scale path for larger deployments (`features = ["postgres"]`) | Decided |
| **MCP stdio only** (no TCP) | Simpler, secure — agent spawns process, no network exposure | Decided |
| **Scratch + MUSL** Docker image | 5.75 MB, static binary, no libc dependency | Decided |
| **rust-embed** for SPA | Single binary deploy, no external file serving needed | Decided |
| **Channel pipeline** (mpsc → digest → broadcast) | Decouples ingest from persistence, backpressure-aware | Decided |
| **PII scrubbing on ingest** (UU PDP compliance) | Redact IPs, emails, API keys, credit cards before persistence | Shipped v0.2.0 |
| **SSRF hardening on webhooks** | HTTPS-only, no redirect following, private IP blocking | Shipped v0.2.0 |
| **Configurable retention** (`TRAPFALL_RETENTION_DAYS`) | Auto-purge old data, default 90 days | Shipped v0.2.0 |

## API Surface

### Sentry SDK Compatibility (Ingest)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/{project_id}/envelope/` | POST | Sentry envelope ingestion (errors, transactions, attachments) |

### Management API (30 routes)

| Area | Routes |
|------|--------|
| **Health** | `GET /health`, `GET /metrics` |
| **Auth** | `POST /api/0/setup`, `POST /api/0/auth/login`, `POST /api/0/auth/logout`, `GET /api/0/auth/me`, `POST /api/0/auth/change-password` |
| **Projects** | CRUD on `/api/0/projects`, archive, DSN rotation |
| **Issues** | List, get, set status, list events |
| **Search** | Full-text on issue title/culprit |
| **Alerts** | CRUD on rules, toggle, webhook dispatch |
| **Performance** | Transactions, slowest, crash rate, release health |
| **Attachments** | List, download |
| **Real-time** | `GET /api/0/ws` (WebSocket) |

### MCP Tools (12)

| Tool | Purpose |
|------|---------|
| `list_projects` | List all registered projects |
| `get_project` | Get project details by slug |
| `get_project_stats` | Issue count statistics |
| `list_issues` | List/filter issues by status & level |
| `get_issue` | Get specific issue by ID |
| `get_event` | Get full event with stacktrace |
| `set_status` | Resolve, ignore, unresolve issue |
| `search_issues` | Full-text search issues |
| `list_events` | List events for an issue |
| `list_alert_rules` | List alert rules for a project |
| `rotate_dsn` | Rotate project DSN key |
| `healthcheck` | Check server health |

## Security Architecture

### Defense in Depth

```
┌─────────────────────────────────────────────────────────┐
│ Layer 1: Network  — TLS termination (reverse proxy)     │
├─────────────────────────────────────────────────────────┤
│ Layer 2: HTTP     — CORS, body size limits (2MB/10MB)   │
├─────────────────────────────────────────────────────────┤
│ Layer 3: Auth     — Cookie session (argon2 hash), DSN   │
│                     key auth on ingest                   │
├─────────────────────────────────────────────────────────┤
│ Layer 4: Input    — PII scrubbing (regex pipeline):     │
│                     IP, email, API keys, credit cards    │
├─────────────────────────────────────────────────────────┤
│ Layer 5: Output   — SSRF protection on webhooks:        │
│                     HTTPS-only, no redirects, private    │
│                     IP blocking, DNS rebinding guard     │
├─────────────────────────────────────────────────────────┤
│ Layer 6: Data     — Retention auto-purge (default 90d)  │
└─────────────────────────────────────────────────────────┘
```

### PII Scrubbing Pipeline

Operates on parsed proto structs **after** `parse_envelope`, **before** DB insert:

| Pattern | Regex | Replacement |
|---------|-------|-------------|
| IPv4 | `\b\d{1,3}(\.\d{1,3}){3}\b` | `[ip:v4]` |
| IPv6 | `[0-9a-fA-F:]{2,}::?[0-9a-fA-F:]*` | `[ip:v6]` |
| Email | `\b[\w.+-]+@[\w-]+\.[\w.-]+\b` | `[email]` |
| Bearer token | `(?i)bearer\s+[A-Za-z0-9._-]+` | `[bearer]` |
| API key (Stripe) | `sk[-_]?(?:test[-_]?)?[A-Za-z0-9]{20,}` | `[api_key]` |
| GitHub token | `ghp_[A-Za-z0-9]{36,}` | `[api_key]` |
| AWS key | `AKIA[0-9A-Z]{16}` | `[api_key]` |
| GitLab token | `glpat-[A-Za-z0-9_-]{20}` | `[api_key]` |
| Slack token | `xox[bpoa]-[A-Za-z0-9-]+` | `[api_key]` |
| Credit card | `\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b` | `[cc]` |

## Background Tasks

| Task | Interval | Purpose |
|------|----------|---------|
| **Digest pipeline** | Continuous (batch=16) | Batch insert events into DB via mpsc channel |
| **Webhook dispatcher** | Continuous (broadcast=64) | Fire alert webhooks on new issues |
| **WebSocket fan-out** | Continuous (broadcast=64) | Push real-time updates to connected dashboards |
| **Retention purge** | Hourly | Delete events older than `TRAPFALL_RETENTION_DAYS` |

## Deployment

### Single Binary (recommended)

```bash
# Build from source
cargo build --release

# Run (SQLite, single-file)
./trapfall serve --db-path ./trapfall.db
```

### Docker (5.75 MB)

```dockerfile
FROM scratch
# Static MUSL binary + embedded SPA
# No libc, no shell, no layers
EXPOSE 9090
ENTRYPOINT ["/trapfall"]
CMD ["serve"]
```

### Configuration (Environment Variables)

| Variable | Default | Description |
|----------|---------|-------------|
| `TRAPFALL_DB_PATH` | `./trapfall.db` | SQLite database path |
| `TRAPFALL_LISTEN_ADDR` | `0.0.0.0:9090` | Bind address |
| `TRAPFALL_CORS_ORIGINS` | `*` (warn) | Comma-separated allowed origins |
| `TRAPFALL_PUBLIC_URL` | — | Public URL for DSN generation |
| `TRAPFALL_TIMEZONE` | `UTC` | Display timezone (e.g., `Asia/Jakarta`) |
| `TRAPFALL_SECURE_COOKIE` | `true` | Set Secure flag on auth cookies |
| `TRAPFALL_MAX_INGEST_BODY_MB` | `2` | Max body size for ingest endpoint |
| `TRAPFALL_MAX_BODY_MB` | `10` | Max body size for general API |
| `TRAPFALL_RETENTION_DAYS` | `90` | Event retention period (auto-purge) |

## Testing

- **255 tests** across 6 crates (unit + integration)
- Tests live alongside source (`#[cfg(test)] mod tests`)
- Integration tests in `crates/trapfalld/tests/integration.rs`
- Pre-commit hook: `cargo fmt` → `cargo clippy -D warnings` → `cora review`

## What TrapFall is NOT

- ❌ APM / performance monitoring (transactions are captured but lightweight)
- ❌ Log aggregation
- ❌ Distributed tracing / OpenTelemetry
- ❌ Session replay
- ❌ Profiling
- ❌ SSO / OIDC (deferred to post-v1)
- ❌ Multi-team / org model (schema ready, UI deferred)
