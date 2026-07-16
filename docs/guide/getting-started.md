# Getting Started

## Prerequisites

- **Docker** (recommended) — zero install deps
- **Or** Rust 1.86+ + Node.js 22+ (build from source)

## Docker

```bash
# Pull image
docker pull ghcr.io/codecoradev/trapfall:latest

# Run (SQLite, zero-config)
docker run -d \
  --name trapfall \
  -p 9090:9090 \
  -v trapfall-data:/data \
  ghcr.io/codecoradev/trapfall:latest
```

Open `http://localhost:9090` → setup wizard.

### Docker Compose

```bash
git clone https://github.com/codecoradev/trapfall.git
cd trapfall

# Production config
cp .env.production .env

# Start (SQLite by default, Postgres optional)
docker compose -f docker-compose.prod.yml up -d
```

See [VPS Deployment](/guide/vps-deployment) for full production setup.

## Download Binary

Download from [GitHub Releases](https://github.com/codecoradev/trapfall/releases/latest):

```bash
# Linux (x86_64)
curl -fsSL https://github.com/codecoradev/trapfall/releases/latest/download/trapfall-x86_64-unknown-linux-gnu-v0.1.1.tar.gz -o trapfall.tar.gz
tar xzf trapfall.tar.gz
chmod +x trapfall

# macOS (Apple Silicon)
curl -fsSL https://github.com/codecoradev/trapfall/releases/latest/download/trapfall-aarch64-apple-darwin-v0.1.1.tar.gz -o trapfall.tar.gz
tar xzf trapfall.tar.gz
chmod +x trapfall
```

```bash
# Start server
./trapfall serve --listen 0.0.0.0:9090

# Or with custom database path
./trapfall --db /var/lib/trapfall.db serve --listen 0.0.0.0:9090
```

## From Source

```bash
git clone https://github.com/codecoradev/trapfall.git
cd trapfall

# Build frontend
cd web && npm ci && npm run build && cd ..

# Build + run
cargo run --release -p trapfalld -- serve --listen 0.0.0.0:9090
```

## Quick Start

1. **Start the server** (any method above)

2. **Open `http://localhost:9090`** → setup wizard appears on first run

3. **Create admin account** (email, name, password)

   ![Login page](/images/docs-09-login.png)

4. **Default project created** automatically with a DSN — copy it

5. **Integrate with your app** using any Sentry SDK:

   ```js
   // JavaScript / Node.js
   Sentry.init({ dsn: "https://<key>@localhost:9090/<project_id>" });
   ```

   ```python
   # Python
   import sentry_sdk
   sentry_sdk.init(dsn: "https://<key>@localhost:9090/<project_id>")
   ```

   ```rust
   // Rust
   sentry::init(("https://<key>@localhost:9090/<project_id>", sentry::ClientOptions::default()));
   ```

   ```dart
   // Flutter / Dart
   await SentryFlutter.init((options) => {
     options.dsn = "https://<key>@localhost:9090/<project_id>",
   });
   ```

6. **Trigger a test error** → it appears in real-time on the dashboard

   ![Issues list](/images/docs-01-issues-list.png)

## Next Steps

- [Configuration](/guide/configuration) — all ENV variables
- [Docker Guide](/guide/docker) — Docker Compose + Postgres setup
- [VPS Deployment](/guide/vps-deployment) — production deployment guide
- [Multi-Project](/guide/multi-project) — manage multiple apps
- [Alerts](/guide/alerts) — webhook notifications
- [SQLite → Postgres Migration](/guide/migration) — switch backends
