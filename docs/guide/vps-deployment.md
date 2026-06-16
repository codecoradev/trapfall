# VPS Deployment Guide

Panduan lengkap untuk deploy TrapFall ke VPS (DigitalOcean, Hetzner, AWS EC2, dll).

## Prerequisites

- VPS dengan Docker + Docker Compose terinstall
- Minimal 512MB RAM (SQLite) atau 1GB RAM (Postgres)
- Domain (opsional, untuk HTTPS)

## Option 1: SQLite (Simplest — recommended for single-server)

### 1. Clone & Configure

```bash
git clone https://github.com/codecoradev/trapfall.git
cd trapfall

# Copy production config
cp .env.production .env

# Edit sesuai kebutuhan
nano .env
```

Yang perlu diubah di `.env`:
```bash
TRAPFALL_PORT=3000                    # port yang dibuka
TRAPFALL_DATABASE_URL=sqlite:/data/trapfall.db
TRAPFALL_SECURE_COOKIE=false          # false jika belum HTTPS, true jika sudah
TRAPFALL_CORS_ORIGINS=                # kosongkan jika single-server
```

### 2. Start

```bash
docker compose -f docker-compose.prod.yml up -d
```

### 3. Setup Wizard

Buka `http://VPS_IP:3000` → buat admin account → selesai.

### 4. Cek status

```bash
# Container status
docker compose -f docker-compose.prod.yml ps

# Logs
docker compose -f docker-compose.prod.yml logs -f

# Health check
curl http://localhost:3000/health
```

---

## Option 2: Postgres (Production — recommended for scale)

### 1. Clone & Configure

```bash
git clone https://github.com/codecoradev/trapfall.git
cd trapfall
cp .env.production .env
```

Edit `.env`:
```bash
# Generate strong password
POSTGRES_PASSWORD=$(openssl rand -hex 24)
echo "POSTGRES_PASSWORD=$POSTGRES_PASSWORD"

# Set di .env:
TRAPFALL_DATABASE_URL=postgres://trapfall:PASSWORD_ANDA@postgres:5432/trapfall
POSTGRES_PASSWORD=PASSWORD_ANDA
TRAPFALL_SECURE_COOKIE=true
```

### 2. Start (with Postgres)

```bash
docker compose -f docker-compose.prod.yml up -d
```

Postgres container otomatis start bersama TrapFall.

### 3. Verify

```bash
# Check Postgres healthy
docker compose -f docker-compose.prod.yml ps postgres

# Check metrics
curl http://localhost:3000/metrics
```

---

## Option 3: HTTPS dengan Caddy (Auto-SSL)

Tambahkan ini ke `docker-compose.prod.yml`:

```yaml
  caddy:
    image: caddy:2-alpine
    container_name: trapfall-caddy
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile
      - caddy-data:/data
      - caddy-config:/config
    depends_on:
      - trapfall
```

Buat `Caddyfile`:

```text
trapfall.yourdomain.com {
    reverse_proxy trapfall:3000
}
```

Set di `.env`:
```bash
TRAPFALL_SECURE_COOKIE=true
TRAPFALL_CORS_ORIGINS=https://trapfall.yourdomain.com
```

Start:
```bash
docker compose -f docker-compose.prod.yml up -d
```

Caddy akan otomatis obtain SSL certificate dari Let's Encrypt.

---

## Backup & Restore

### Backup SQLite

```bash
# Stop TrapFall first
docker compose -f docker-compose.prod.yml stop trapfall

# Copy database file
docker cp trapfall:/data/trapfall.db ./backup-$(date +%Y%m%d).db

# Restart
docker compose -f docker-compose.prod.yml start trapfall
```

### Backup Postgres

```bash
docker compose -f docker-compose.prod.yml exec postgres \
  pg_dump -U trapfall trapfall > backup-$(date +%Y%m%d).sql
```

### Restore

```bash
# SQLite
docker cp ./backup-20260101.db trapfall:/data/trapfall.db

# Postgres
cat backup-20260101.sql | docker compose -f docker-compose.prod.yml exec -T postgres \
  psql -U trapfall trapfall
```

---

## Migration: SQLite → Postgres

Jika sudah pakai SQLite dan ingin pindah ke Postgres:

```bash
# 1. Export data dari SQLite
docker exec trapfall /trapfall db export \
  --from sqlite:/data/trapfall.db \
  --to /tmp/export.jsonl

# 2. Copy export file keluar container
docker cp trapfall:/tmp/export.jsonl ./export.jsonl

# 3. Start Postgres
# (Pastikan postgres service sudah running)

# 4. Import ke Postgres
docker exec trapfall /trapfall db import \
  --from /tmp/export.jsonl \
  --to postgres://trapfall:PASSWORD@postgres:5432/trapfall

# 5. Verify
docker exec trapfall /trapfall db verify \
  --url postgres://trapfall:PASSWORD@postgres:5432/trapfall
```

---

## Common Commands

```bash
# Update ke versi terbaru
docker compose -f docker-compose.prod.yml pull
docker compose -f docker-compose.prod.yml up -d

# View logs
docker compose -f docker-compose.prod.yml logs -f trapfall

# Restart
docker compose -f docker-compose.prod.yml restart trapfall

# Stop all
docker compose -f docker-compose.prod.yml down

# Stop & delete volumes (⚠️ DATA LOSS)
docker compose -f docker-compose.prod.yml down -v
```

---

## Troubleshooting

### Container tidak start
```bash
docker compose -f docker-compose.prod.yml logs trapfall
```

### Health check failed
```bash
docker exec trapfall /trapfall healthcheck
```

### Database locked (SQLite)
Jika ada multiple writers, pindah ke Postgres.

### Port sudah digunakan
Ubah `TRAPFALL_PORT` di `.env` ke port lain (mis. 8080).

### Lupa password admin
```bash
# Connect ke container dan create user baru via CLI
docker exec -it trapfall /trapfall --db sqlite:/data/trapfall.db project-list
```
