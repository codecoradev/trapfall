# Configuration

## Environment Variables

All configuration is via environment variables. Copy `.env.example` to `.env` and customize.

| Variable | Default | Description |
|----------|---------|-------------|
| `TRAPFALL_DB` | `trapfall.db` | SQLite database path |
| `TRAPFALL_LISTEN` | `0.0.0.0:3000` | HTTP listen address |
| `TRAPFALL_SECURE_COOKIE` | `true` | Set `false` for HTTP local dev |
| `TRAPFALL_CORS_ORIGINS` | *(empty = allow all)* | Comma-separated origins |
| `RUST_LOG` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |

## Local Development

For local HTTP development, you must set:

```bash
TRAPFALL_SECURE_COOKIE=false
```

Otherwise browsers will reject the session cookie over HTTP.

## Production

For production with HTTPS:

```bash
TRAPFALL_SECURE_COOKIE=true
TRAPFALL_CORS_ORIGINS=https://trapfall.yourcompany.com
RUST_LOG=trapfall=info
```

## Docker Compose

The included `docker-compose.yml` is pre-configured for development:

```yaml
services:
  trapfall:
    image: ghcr.io/codecoradev/trapfall:0.0.3
    ports:
      - "3000:3000"
    volumes:
      - trapfall-data:/data
    environment:
      - RUST_LOG=trapfall=debug
      - TRAPFALL_SECURE_COOKIE=false
    command: --db /data/trapfall.db serve --listen 0.0.0.0:3000
```

## Database

TrapFall uses SQLite in WAL mode with `synchronous=NORMAL`. The database is a single file — back it up by copying the file.

No migration commands needed — the schema is auto-created on first run.
