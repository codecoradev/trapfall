# Changelog

All notable changes to TrapFall are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.5] - 2026-06-15

### Added

- **Database trait abstraction** (#186): extract `Database` trait into `trapfall-db` crate
  - All SQL moved from `Store` into `SqliteBackend` implementations
  - `Store` becomes a thin facade over `dyn Database`
  - Foundation for multi-backend support (Postgres in Phase 3)
- **Database connection factory** (#167): `open_database(url)` detects URL scheme
  (`sqlite:` or `postgres:`) and instantiates the correct backend
  - `TRAPFALL_DATABASE_URL` env var support (falls back to `--db` flag)
  - Cargo feature flags: `default = ["sqlite"]`, optional `postgres`
  - `normalise_url()` helper: bare paths auto-prefixed with `sqlite:`
  - Docker: `DATABASE_URL` / `TRAPFALL_DATABASE_URL` env var support
  - Graceful error if `postgres:` URL used without `postgres` feature

## [0.0.4] - 2026-06-12

### Added

- **Project CRUD**: archive, unarchive, permanent delete, rename, rotate DSN (#152)
  - Active/Archived tabs on Projects page
  - Kebab menu (⋮) per project card with Rename, Rotate DSN, Archive actions
  - Archived projects: Unarchive, Delete permanently
  - `PATCH /api/0/projects/{slug}` — rename project
  - `DELETE /api/0/projects/{slug}` — permanent delete (archived only)
  - `POST /api/0/projects/{slug}/archive` — archive project
  - `DELETE /api/0/projects/{slug}/archive` — unarchive project
  - `POST /api/0/projects/{slug}/rotate-dsn` — regenerate DSN key
- **Issue search**: search bar on Issues page with debounce (#153)
  - Merged Search page into Issues — one unified view
  - Search by title/culprit with status/level filters combined
  - Removed separate Search nav link (4 items: Issues, Projects, Rules, Settings)
- **Issue filters**: status tabs (All/Unresolved/Resolved/Ignored) + level dropdown (#149)
- **Issue pagination**: page numbers + showing X–Y of Z (#149)
- **Project selector**: Issues and Rules pages now have project dropdown (#148, #154)
- **Back navigation**: issue detail page has Back button + ESC keyboard shortcut (#147)
- **VitePress docs**: 9 screenshots added to all guide pages
- **CF Pages**: auto-deploy docs to Cloudflare Pages on release

### Fixed

- **Search UX**: Enter to search + 1.5s debounce (was 300ms per-keystroke). Empty state shows project name hint (#158)

- **DSN bug**: `generate_dsn_with()` used hardcoded `/1` instead of project UUID — Sentry SDKs POSTed to wrong URL, all events silently dropped (#151)
- **Rules page**: hardcoded to `projects[0]` — could not manage rules for other projects (#154)

### Changed

- **Docker image**: shrunk from **112MB to 5.75MB** (-95%)
  - Switched reqwest TLS from native-tls (OpenSSL) to rustls (pure Rust)
  - Builder: `debian` → `alpine` (MUSL static binary)
  - Runtime: `debian-slim` → `scratch` (zero OS overhead)
- **Migration**: `project_archive` made idempotent via `pragma_table_info` check

## [0.0.3] - 2026-06-11

### Fixed

- **Critical**: Ingest pipeline broken — handler looked up project by slug but received UUID from URL path. Events silently dropped (#135)
- **Critical**: Envelope parser only supported 3-line format. Added support for 2-line bare event envelopes (#135)
- **Critical**: FK constraint — ingest used URL slug instead of project UUID, causing all events to fail (#134)
- WebSocket 401 after login — moved handler outside auth middleware, validates cookie directly (#132)
- API double `/0/` path — `API_BASE + "/0/projects"` → `API_BASE + "/projects"` (#132)
- DSN hardcoded `localhost:9090` — now uses request `Host` header (#132)
- Setup page code examples caused Svelte build error — escaped curly braces in template literals (#133)
- Secure cookie hardcoded `true` — browser rejected cookies via HTTP in local dev (#134)
- Search page missing padding — inconsistent with other dashboard pages (#132)

### Added

- **Multi-project support**: "+ Add Project" button on Projects page (#132)
- `POST /api/0/projects` endpoint for creating projects from dashboard (#132)
- `create_project_with_host()` — generates DSN using request Host header (#132)
- Setup page shows DSN usage examples (Rust, Python, JS, Flutter) (#132)
- `get_project_by_id()` store method for UUID lookups (#135)
- Diagnostic logging throughout ingest pipeline (#135)
- `AGENTS.md` — rules for AI agents working on TrapFall (#134)
- Cora Review CI step made non-blocking on API errors (#136)

### Changed

- Docker compose dev defaults: `RUST_LOG=debug`, `SECURE_COOKIE=false` (#134)
- Digest flush log level promoted from trace to info (#135)
- Integration tests updated to use UUID for ingest URL path (#135)

## [0.0.2] - 2026-06-08

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
- **Release workflow**: Cross-compile 4 platforms, SHA256 checksums, auto sync `main` branch

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

[unreleased]: https://github.com/codecoradev/trapfall/compare/v0.0.5...develop
[0.0.5]: https://github.com/codecoradev/trapfall/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/codecoradev/trapfall/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/codecoradev/trapfall/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/codecoradev/trapfall/releases/tag/v0.0.2
