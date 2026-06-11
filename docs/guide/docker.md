# Docker

## Quick Start

```bash
docker pull ghcr.io/codecoradev/trapfall:0.0.3
docker run -d -p 3000:3000 -v trapfall-data:/data ghcr.io/codecoradev/trapfall:0.0.3
```

## Docker Compose

```yaml
services:
  trapfall:
    image: ghcr.io/codecoradev/trapfall:0.0.3
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
      test: ["CMD", "trapfall", "--db", "/data/trapfall.db", "healthcheck"]
      interval: 30s
      timeout: 5s
      retries: 3

volumes:
  trapfall-data:
```

## Data Persistence

The SQLite database is stored at `/data/trapfall.db` inside the container. Use a Docker volume to persist data across restarts.

## Health Check

The container includes a built-in healthcheck:

```bash
trapfall --db /data/trapfall.db healthcheck
```

Returns exit code 0 if the server is healthy.

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
