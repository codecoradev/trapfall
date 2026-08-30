# Contributing to TrapFall

## Development Setup

### Prerequisites

- Rust 1.87+ (stable)
- Node.js 20+
- npm 9+

### Build

```bash
# Frontend
cd web && npm ci && npm run build && cd ..

# Backend
cargo build --workspace

# Run tests
cargo test --workspace
```

### Development Workflow

1. Create a branch from `develop`: `git checkout -b feat/your-feature develop`
2. Make changes + write tests
3. Run `cargo fmt` + `cargo clippy -- -D warnings`
4. Commit with conventional messages: `feat(scope): description`
5. Push + open PR to `develop`

### CI Checks (all required)

| Check | Command |
|-------|---------|
| Check | `cargo check --workspace` |
| Format | `cargo fmt --check` |
| Clippy | `cargo clippy -- -D warnings` |
| Test | `cargo test --workspace` |
| Build | `cargo build --release -p trapfalld` |
| Cora Review | AI code review (PR only) |

### Project Structure

```
crates/
├── trapfall-proto/    # Shared types (Issue, Event, Level, etc.)
├── trapfall-core/     # Store abstraction, fingerprinting (Blake3)
├── trapfall-db/       # Data layer (SQLite + Postgres, migrations)
├── trapfall-ingest/   # Envelope parser (Sentry SDK format)
├── trapfall-mcp/      # MCP server (stdio JSON-RPC)
└── trapfalld/         # Binary: HTTP server, auth, alerts, search, SPA
web/                   # SvelteKit frontend source
```

### Code Style

- **Rust**: `cargo fmt`, `cargo clippy -- -D warnings`
- **TypeScript/Svelte**: Prettier defaults
- **Commits**: Conventional (`feat:`, `fix:`, `ci:`, `docs:`)

### Branch Protection

- `develop` is the default branch — all PRs target it
- `main` is synced from `develop` via release tags
- Never push directly to `develop` or `main`
- Squash merge, delete branch after merge

## CLA

All contributions (code, docs, tests, configuration) require a signed
Contributor License Agreement before a pull request can be merged:

- 📋 **Individual?** → [Sign the Individual CLA](https://codecoradev.github.io/cla/?type=individual)
- 🏢 **Contributing on behalf of a company?** → [Sign the Corporate CLA](https://codecoradev.github.io/cla/?type=corporate)

The CLA is a license agreement, not a copyright assignment — you keep
ownership of your work. Signing takes a couple of minutes and is stored
in the [codecoradev/.github](https://github.com/codecoradev/.github)
repository; a bot checks it automatically on every pull request.

## Contributions are unpaid

Contributing to this project is **voluntary and unpaid**. There is no
compensation, payment, bounty, or financial reward of any kind for
contributions — now or in the future. You contribute on your own time,
at your own discretion, because you want to improve the project.

If any paid-contribution program is ever introduced, it will be announced
explicitly and this document will be updated. Until then, assume every
contribution is volunteer work under the Apache-2.0 license terms above.
