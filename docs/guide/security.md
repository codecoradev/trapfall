# Security

## Authentication

- **Admin account** created during setup wizard
- **Session cookies** — `HttpOnly`, `SameSite=Strict`
- **Password hashing** — Argon2id
- **Brute-force protection** — rate limited login attempts

## Secure Cookies

Set `TRAPFALL_SECURE_COOKIE=true` (default) for HTTPS deployments.

For local HTTP development:
```bash
TRAPFALL_SECURE_COOKIE=false
```

## CORS

Set allowed origins for production:
```bash
TRAPFALL_CORS_ORIGINS=https://trapfall.yourcompany.com
```

Empty = allow all origins (**development only**).

## DSN Key Auth

Ingest endpoints authenticate via DSN key:
- `X-Sentry-Auth: Sentry sentry_key=<key>, sentry_version=7`
- Or `Authorization: Bearer <key>`

DSN keys are UUIDs — not guessable. Rotate with `trapfall project rotate-dsn <slug>`.

## Rate Limiting

Ingest endpoints are rate-limited per project ID to prevent abuse. Default: generous burst with steady refill.

## SQL Injection

All queries use parameterized statements. No string concatenation in SQL.

## DSN Key Lookup

DSN keys are looked up with exact match (`=`) on an indexed column, not LIKE.

## Production Checklist

- [ ] HTTPS via reverse proxy (Nginx, Caddy, Cloudflare Tunnel)
- [ ] `TRAPFALL_SECURE_COOKIE=true`
- [ ] `TRAPFALL_CORS_ORIGINS` set to your domain
- [ ] `RUST_LOG=trapfall=info` (not debug)
- [ ] Docker volume for persistent data
- [ ] Regular database backups (copy the SQLite file)
- [ ] Rotate DSN keys if compromised
