# TrapFall

> Lightweight self-hosted error capture engine. Sentry-compatible, Rust + SvelteKit.

## Quick Facts

| | |
|---|---|
| **Binary** | ~5–8 MB stripped |
| **RAM idle** | <15 MB |
| **License** | Apache-2.0 |
| **Protocol** | Sentry envelope (drop-in DSN swap) |
| **Storage** | SQLite default, Postgres via feature flag |
| **Dashboard** | Embedded SvelteKit SPA |
| **AI** | MCP server (stdio) |

## Scope: Error Capture Only

TrapFall focuses **100% on error capture** — receiving, grouping, and displaying errors.
Not APM, not logging, not tracing.

## Crate Structure

```
trapfall/
├── crates/
│   ├── trapfall-proto/     # Wire types & protocol definitions
│   ├── trapfall-core/      # Storage trait, config, auth, fingerprint
│   ├── trapfall-ingest/    # HTTP handler, envelope parser, digest loop
│   ├── trapfall-search/    # LIKE + trigram substring search
│   ├── trapfall-alert/     # Alerting rules engine & webhooks
│   ├── trapfall-mcp/       # MCP server (stdio transport)
│   ├── trapfall-dashboard/ # Embedded SvelteKit SPA
│   └── trapfalld/          # Daemon binary
└── web/                    # SvelteKit frontend source
```

## Build

```bash
cargo build --release
```

## Run

```bash
trapfall serve --bind 0.0.0.0:9090 --db ./trapfall.db
```

## Status

**Phase 0 — Repo setup.** Not yet functional.
