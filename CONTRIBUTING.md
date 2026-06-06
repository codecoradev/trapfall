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
├── trapfall-core/     # Store (SQLite), migrations, helpers
├── trapfall-ingest/   # Envelope parser
├── trapfall-search/   # LIKE-based search
├── trapfall-alert/    # Alert engine + webhook dispatch
├── trapfall-mcp/      # MCP server (stdio JSON-RPC)
├── trapfall-dashboard/# SvelteKit SPA (via rust-embed)
└── trapfalld/         # Main binary + HTTP server
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
