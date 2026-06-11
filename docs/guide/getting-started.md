# Getting Started

## Prerequisites

- Docker (recommended) **or** Rust 1.85+ with Node.js 22+

## Docker Setup (recommended)

```bash
# Pull the image
docker pull ghcr.io/codecoradev/trapfall:0.0.3

# Run with persistent data
docker run -d \
  --name trapfall \
  -p 3000:3000 \
  -v trapfall-data:/data \
  -e TRAPFALL_SECURE_COOKIE=false \
  ghcr.io/codecoradev/trapfall:0.0.3 \
  --db /data/trapfall.db serve --listen 0.0.0.0:3000
```

Or with Docker Compose:

```bash
git clone https://github.com/codecoradev/trapfall.git
cd trapfall
docker compose up -d
```

## Setup Wizard

1. Open **http://localhost:3000** in your browser
2. The setup wizard appears on first run
3. Create your admin account (email, name, password)
4. A default project is created automatically with a DSN
5. **Copy the DSN** — you'll need it for your SDK

## Integrate with Your App

Use the DSN with any Sentry SDK:

```js
// JavaScript / Node.js
Sentry.init({ dsn: "https://<key>@your-server:3000/1" });
```

```python
# Python
import sentry_sdk
sentry_sdk.init(dsn="https://<key>@your-server:3000/1")
```

```rust
// Rust
sentry::init(("https://<key>@your-server:3000/1", sentry::ClientOptions::default()));
```

```dart
// Flutter / Dart
await SentryFlutter.init((options) => {
  options.dsn = "https://<key>@your-server:3000/1",
});
```

## Verify

Trigger a test error in your app, then check the TrapFall dashboard — the error appears in real-time on the Issues page.

## From Source

```bash
# Build frontend
cd web
npm ci
npm run build
cd ..

# Run
cargo run --release -p trapfalld -- --db trapfall.db serve --listen 0.0.0.0:3000
```

## Next Steps

- [Create additional projects](/guide/multi-project) for different apps/services
- [Configure alerts](/guide/alerts) for webhook notifications
- [Secure your deployment](/guide/security) for production
