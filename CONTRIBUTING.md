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
