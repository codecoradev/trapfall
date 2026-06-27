# Changelog

All notable changes to TrapFall are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-06-27

### Added — Phase 1: Transaction (Performance Tracing)

- **Transaction + Span wire types** (#235): `Transaction`, `Span`,
  `SpanStatus` proto types with JSON parsing and envelope item encoding.
- **Transaction envelope parser** (#236): extracts transaction items from
  Sentry envelopes, including nested span arrays and duration fields.
- **Transactions + transaction_spans DB tables** (#237): full schema with
  queries for listing, filtering by project/release, and detail retrieval
  (SQLite + Postgres).
- **Transaction ingest handler** (#238): processes incoming transactions,
  persists to DB, and broadcasts via WebSocket for real-time dashboard updates.
- **Performance dashboard tab** (#239): transaction list with filters,
  detail view, and waterfall chart for span visualization.

### Added — Phase 2: Session (Release Health)

- **Session wire types** (#240): `SessionStatus`, `SessionUpdate`,
  `SessionAggregates` proto types with JSON parsing.
- **Session envelope parser** (#241): parses `session` and `sessions`
  item types from Sentry envelopes, including aggregate session data.
- **Release health DB table** (#242): stores session data with queries for
  crash rate calculation, release comparison, and time-series aggregation.
- **Release health dashboard** (#243): dedicated UI tab showing crash-free
  rates, session counts, and release-level health metrics.

### Added — Phase 3: Attachment (Binary Upload)

- **Attachment wire type** (#244): `Attachment` proto type with filename,
  size, content_type, and SHA1 hash fields.
- **Binary-safe envelope parser** (#245): handles base64-encoded binary
  attachments in envelope items, validates magic bytes, and decodes content.
- **Attachments table + disk storage** (#246): database schema with
  metadata tracking, filesystem storage with size limits, and CRUD queries.
- **Attachment API + event detail UI** (#247): `GET /api/attachments/{id}`
  endpoint, attachment viewer in event detail page.

### Fixed

- **quinn-proto RUSTSEC-2026-0185** (#254): bumped `quinn-proto`
  0.11.14 → 0.11.15 to resolve medium-severity advisory.
- **Cora review config** (#254): switched to `freemodel` (gpt-5.4) as
  default Cora provider.

### Changed

- Proto layer extended with Transaction, Span, and Session types —
  `ParsedEnvelope` now supports 5 envelope item types (event, transaction,
  session, sessions, attachment).
- WebSocket broadcasts now include transaction events alongside error events.

## [0.1.4] - 2026-06-19

### Added

- **Expanded unit test coverage for `trapfall-db`** (#221): +43 new test
  functions covering error paths (FK violations, non-existent IDs, empty
  results), `delete_project` cascade atomicity, `upsert_issue`
  idempotency, and pagination edge cases (`page=0`, `page` beyond total,
  `per_page=0`, large `per_page`).
- **Expanded test coverage for `trapfalld`** (#221): +17 new test
  functions covering auth middleware (expired/tampered/invalid session
  tokens), handler response shapes (404 vs 400 vs 401 vs 403), rate
  limiter edge cases (fractional cost, hard cap, refill ceiling), and
  DSN masking edge cases.

### Fixed

- **Cookie advisory GHSA-pxg6-pf52-xh8x** (#220): `npm audit` in `web/`
  no longer reports 7 low-severity vulnerabilities from transitive
  `cookie < 0.7.0` dependency. Resolved via npm `overrides` forcing
  `cookie@^0.7.2` (SvelteKit 3.0 stable not yet released).

### Test count

- Workspace total: 125 → **195+** tests (+70).

## [0.1.3] - 2026-06-19

### Added

- **Configurable public URL for DSN generation** (#210): new
  `TRAPFALL_PUBLIC_URL` env var (legacy alias `TRAPFALL_DSN_HOST`) lets
  operators pin the host used when minting project DSNs, instead of
  trusting the per-request `Host` header. Config loading centralized in
  `Config::from_env()`.

### Fixed

- **Postgres startup path** (#213): `main.rs` no longer assumes SQLite
  when running migrations. New `Database::run_migrations()` trait method
  dispatches to the correct backend — `postgres://` URLs now actually
  boot the server (v0.1.2 advertised built-in Postgres but the migration
  step crashed).
- **DSN secret leak in project list** (#214): `GET /api/0/projects` now
  masks the DSN secret key (`https://3167cebd...366b@host/id`). Full DSN
  is still returned by create / rotate / single-project GET for admin
  copy.
- **MCP schema drift** (#215): `Level` enum now includes `trace`, and
  `IssueStatus` now includes `regression`, matching the proto types.
  Unknown slugs in `search_issues` now surface a JSON-RPC error instead
  of silently searching across all projects.
- **Silent DB error masking** (#216): `count_issues` / `count_events` /
  `count_search_issues` failures now emit a `tracing::warn!` instead of
  silently returning 0.
- **OpenAPI content-type** (#216): `/api/docs/openapi.yaml` now serves
  `application/yaml` instead of `text/html` so OpenAPI tooling can parse
  it.
- **Per-request HTML allocation** (#216): SPA `index.html` fallback is
  cached in a `OnceLock` instead of being re-decoded per request.
- **Atomic project deletion** (#209): `delete_project` now wraps its 5
  cascading DELETEs in a single transaction (SQLite + Postgres). A
  mid-sequence failure no longer leaves orphaned rows.
- **Lost-update race in `upsert_issue`** (#209): SQLite implementation
  replaced SELECT-then-UPDATE with an atomic
  `INSERT ... ON CONFLICT DO UPDATE`, so concurrent ingest no longer
  drops event counts.
- **Pagination underflow on `?page=0`** (#211): `page` is now clamped to
  a minimum of 1 across issue + event listing endpoints, preventing a
  `u32` wrap-around that produced invalid offsets.
- **Hardcoded `db_path`** (#210): `Config.db_path` now reflects the
  actual resolved database URL instead of always `"trapfall.db"`.

## [0.1.2] - 2026-06-16

### Fixed

- **Postgres support built-in by default** (#207): Docker image dan binary release
  sekarang include Postgres driver. Tidak perlu `--features postgres` lagi.
  User bisa langsung set `TRAPFALL_DATABASE_URL=postgres://...` tanpa build dari source.

## [0.1.1] - 2026-06-16

### Fixed

- **Rate limiter panic** (#196): `Mutex::lock().unwrap()` replaced with
  poison-recovery pattern `unwrap_or_else(|e| e.into_inner())`
- **Blocking DNS in SSRF check** (#202): `is_private_url` now runs via
  `tokio::task::spawn_blocking` to avoid blocking async runtime
- **IP spoofing via X-Forwarded-For** (#197): Extracted IP now validated
  as `IpAddr` and only first entry taken from comma-separated list
- **Webhook timeout** (#198): Reduced from 10s to 5s
- **EventRow silent data loss** (#201): JSON parse failures now logged
  via `tracing::warn` before falling back to null

## [0.1.0] - 2026-06-16

### Added

- **Postgres backend** (#168): full `PostgresBackend` implementation of `Database` trait
  - `crates/trapfall-db/src/postgres.rs` — all 40+ trait methods
  - Postgres migrations: `migrations/postgres/001_initial.sql`, `002_alert_rules.sql`
  - `open_database("postgres://...")` now instantiates `PostgresBackend`
  - `run_postgres_migrations()` for schema setup
  - Dialect: `$N` params, `ILIKE`, `BOOLEAN`, `ON CONFLICT DO UPDATE` upsert
  - Shared row types and helpers extracted to `common.rs` (DRY with SQLite)
  - Build: `cargo build --features postgres` compiles both backends
- **Shared test suite** (#169): backend-agnostic tests covering all major operations
  - 10 shared test functions in `tests/common.rs`
  - SQLite runner: 11/11 tests pass on in-memory SQLite
  - Postgres runner: gated behind `TEST_POSTGRES_URL` env var
- **SQLite → Postgres migration tool** (#170): `trapfall db export/import/verify`
  - Export all tables to JSONL format
  - Import JSONL to Postgres with automatic schema setup
  - Verify row counts and health check
  - Migration guide in `docs/guide/migration.md`
- **TRAPFALL_LISTEN env var**: `Serve` command now reads from env

### Fixed

- **Axum routing bug**: `.nest()` + `.merge()` caused API routes to return SPA HTML.
  Replaced with flat routing + `.route_layer()`.
- **Search pagination bug**: Search used 0-indexed page offset, frontend sends 1-indexed.
  Fixed to `(page - 1) * per_page`.
- **require_auth whitelist**: Public routes (setup, login, health, metrics) now bypass auth.
- **db verify CLI**: Opens correct database via `--url` flag (early-return for Db subcommands).

### Changed

- SqlitePool purge: all core crates route through `dyn Database` (37 → 7 leak points)
- Core library crates (proto, db, core, ingest, search) are backend-agnostic
- Ready for Postgres Phase 3 and future crates.io publish

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

[unreleased]: https://github.com/codecoradev/trapfall/compare/v0.2.0...develop
[0.2.0]: https://github.com/codecoradev/trapfall/compare/v0.1.4...v0.2.0
[0.1.4]: https://github.com/codecoradev/trapfall/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/codecoradev/trapfall/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/codecoradev/trapfall/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/codecoradev/trapfall/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/codecoradev/trapfall/compare/v0.0.5...v0.1.0
[0.0.5]: https://github.com/codecoradev/trapfall/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/codecoradev/trapfall/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/codecoradev/trapfall/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/codecoradev/trapfall/releases/tag/v0.0.2
