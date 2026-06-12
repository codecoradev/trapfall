# Docker

## Quick Start

```bash
docker pull ghcr.io/codecoradev/trapfall:latest
docker run -d -p 3000:3000 -v trapfall-data:/data ghcr.io/codecoradev/trapfall:latest
```

## Image Details

The Docker image is built on a **scratch** base with a statically-linked MUSL binary — total image size is **~6MB**.

| Component | Size |
|-----------|------|
| Binary (static MUSL) | ~5MB |
| Frontend (SPA) | ~400KB |
| Base image | 0 (scratch) |

Key technical details:
- **TLS**: Uses rustls (pure Rust) — no OpenSSL dependency
- **Builder**: `rust:1.86-alpine` with MUSL for fully static binary
- **Runtime**: `FROM scratch` — zero OS overhead, minimal attack surface
- **Multi-arch**: Supports `linux/amd64` and `linux/arm64`

## Docker Compose

```yaml
services:
  trapfall:
    image: ghcr.io/codecoradev/trapfall:latest
    # For local development, uncomment to build from source:
    # build:
    #   context: .
    #   dockerfile: Dockerfile
    container_name: trapfall
    restart: unless-stopped
    ports:
      - "3000:3000"
    volumes:
      - trapfall-data:/data
    environment:
      - RUST_LOG=trapfall=info
      - TRAPFALL_SECURE_COOKIE=false  # Set true for HTTPS
    command: >
      --db /data/trapfall.db
      serve
      --listen 0.0.0.0:3000
    healthcheck:
      test: ["CMD", "/trapfall", "--db", "/data/trapfall.db", "healthcheck"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s

volumes:
  trapfall-data:
```

## Data Persistence

The SQLite database is stored at `/data/trapfall.db` inside the container. Use a Docker volume to persist data across restarts.

## Health Check

The container includes a built-in healthcheck:

```bash
/trapfall --db /data/trapfall.db healthcheck
```

Returns exit code 0 if the server is healthy. Note: the binary is at `/trapfall` (scratch image has no PATH).

## Reverse Proxy (Production)

For production with HTTPS, put TrapFall behind a reverse proxy:

### Nginx

```nginx
server {
    listen 443 ssl;
    server_name trapfall.example.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket support
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

When using HTTPS, set `TRAPFALL_SECURE_COOKIE=true`.
