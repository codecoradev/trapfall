# CLAUDE.md — TrapFall Project Conventions

## Project Overview

TrapFall is a lightweight, self-hosted error capture engine written in Rust with an embedded SvelteKit SPA dashboard. It is Sentry SDK compatible (drop-in DSN swap). Apache-2.0 licensed.

**Scope:** Error capture only. No APM, no logging, no tracing, no OpenTelemetry.

## Architecture

- **Single binary** with embedded SPA via `rust-embed`
- **Single-writer SQLite** (WAL mode, `synchronous=NORMAL`) — single-threaded tokio (`current_thread`)
- **Channel pipeline:** HTTP → mpsc(256) → digest(batch=16) → broadcast(64) + webhook(64)
- **Fingerprinting:** blake3 (deterministic, stable)
- **Search:** LIKE + sqlite_trigram (no FTS5)
- **MCP:** stdio transport only (no TCP)
- **Multi-tenant:** Schema ready but Solo mode MVP — TEAMS deferred to post-MVP

## Crate Naming

| Crate | Purpose |
|-------|---------|
| `trapfall-proto` | Wire types (Event, Issue, Fingerprint) |
| `trapfall-core` | Storage trait, config, auth, fingerprint |
| `trapfall-ingest` | HTTP handler, envelope parser, digest loop |
| `trapfall-search` | LIKE + trigram search module |
| `trapfall-alert` | Configurable alerting rules engine |
| `trapfall-mcp` | MCP server via stdio (JSON-RPC 2.0) |
| `trapfall-dashboard` | Embedded SPA (SvelteKit) |
| `trapfalld` | Daemon binary (CLI: `trapfall`) |

## Rust Style

- Edition 2024, MSRV 1.85
- `thiserror` for library errors, `anyhow` for application errors
- `tracing` for all logging (no `println!` in library code)
- No `unwrap()` in production code — use `?` or explicit error handling
- `serde` derive on all wire types
- Tests alongside source files (`#[cfg(test)]` modules)
- Release profile: `opt-level=z`, `lto=fat`, `panic=abort`, `strip=true`

## Git Workflow

- **Default branch:** `develop`
- **Release branch:** `main` (mirror, auto-synced via tag)
- **Versioning:** Stay in `0.x.x` indefinitely. Minor per feature batch, patch per fix
- **NEVER push to main directly** — always PR to develop
- **CHANGELOG.md** — version section per release
- Commit messages: conventional format (`feat:`, `fix:`, `chore:`, `docs:`)

## Key Design Decisions

| Decision | Status |
|----------|--------|
| FTS5 dropped → LIKE + trigram | Decided |
| MCP stdio only (no TCP) | Decided |
| OpenTelemetry dropped | Decided |
| Solo mode first, TEAMS deferred | Decided |
| blake3 fingerprint | Decided |
| Apache-2.0 license | Decided |
| Community Edition first | Decided |
| Timeline ~3 months | Decided |

## What TrapFall is NOT

- ❌ APM / performance monitoring
- ❌ Log aggregation
- ❌ Distributed tracing
- ❌ Session replay
- ❌ OpenTelemetry / OTLP
- ❌ Profiling
