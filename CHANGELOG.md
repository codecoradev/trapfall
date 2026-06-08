# Changelog

All notable changes to TrapFall will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-06-08

### Added

- **Core engine**: blake3 fingerprinting, single-writer SQLite (WAL mode), channel pipeline
- **Ingest**: HTTP handler, Sentry-compatible envelope parser, gzip support, batch digest loop
- **Search**: LIKE + sqlite_trigram search module
- **Alerting**: Configurable alert rules engine with webhook support
- **MCP**: stdio JSON-RPC 2.0 server with 12 tool functions
- **Dashboard**: SvelteKit 5 SPA embedded via rust-embed — login, setup wizard, issue list/detail, project management, settings, search
- **WebSocket**: Real-time event broadcast hub with auto-reconnect client
- **Auth**: Argon2id hashing, session management, brute-force lockout, setup wizard
- **CLI**: `trapfall` daemon binary with config, serve, migrate subcommands
- **Docker**: Multi-stage build, GHCR publishing
- **CI/CD**: GitHub Actions — Build, Check, Clippy, Format, Test, Cora Review, Cargo Audit, Trivy Filesystem/Secrets, npm Audit
- **Security**: Rate limiting, CORS configuration, configurable secure cookies, DSN key masking

### Security

- Input validation on all ingest endpoints
- Parameterized SQL queries (no string concatenation)
- CORS origins configurable, secure cookie flag configurable
- DSN key exact-match lookup (no LIKE injection)
- API keys table removed (unused dead schema)
- Trivy filesystem + secrets scanning in CI

### Changed

- MCP `call_tool` refactored from 200-line monolith to 12 dedicated tool functions
- Frontend shared utilities extracted (`$lib/utils.ts`) for consistent badge colors and time formatting
- WebSocket client now has `destroy()` method, cleaned up on logout

### Fixed

- 24 audit findings addressed across 6 batch PRs (#121–#126)
