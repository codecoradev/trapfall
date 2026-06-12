# API Reference

All dashboard API routes are under `/api/0/`. Authentication is via `trapfall_session` cookie.

## Interactive Docs

TrapFall includes **Swagger UI** for exploring the API interactively at `/api/docs`:

![Swagger UI](/images/docs-08-swagger.png)

OpenAPI spec available at `/api/docs/openapi.yaml`.

## Auth

### `POST /api/0/setup`

First-run setup wizard. Creates admin user + default project. Only available when no users exist.

**Request:**
```json
{
  "email": "admin@example.com",
  "name": "Admin",
  "password": "secure-password"
}
```

**Response:** `201 Created`
```json
{
  "user": { "id": "...", "email": "...", "name": "...", "role": "admin" },
  "project_slug": "default",
  "dsn": "https://<key>@host/<project_id>"
}
```

### `GET /api/0/setup`

Check if setup is needed.

**Response:** `200 OK`
```json
{ "needs_setup": true }
```

### `POST /api/0/auth/login`

**Request:** `{ "email": "...", "password": "..." }`

**Response:** `200 OK` + `Set-Cookie: trapfall_session=<uuid>`

### `POST /api/0/auth/logout`

**Response:** `200 OK` + clears cookie

### `GET /api/0/auth/me`

**Response:** Current user info (requires auth)

### `POST /api/0/auth/change-password`

**Request:** `{ "current_password": "...", "new_password": "..." }`

## Projects

### `GET /api/0/projects`

List all projects (active + archived). **Auth required.**

**Response field:** `archived_at` is `null` for active projects, ISO timestamp for archived.

### `POST /api/0/projects`

Create a project. **Auth required.**

**Request:** `{ "name": "My App", "slug": "my-app" }` (slug optional)

**Response:** `201 Created`

### `GET /api/0/projects/{slug}`

Get project detail including DSN.

### `PATCH /api/0/projects/{slug}`

Rename project. **Auth required.**

**Request:** `{ "name": "New Name" }`

### `DELETE /api/0/projects/{slug}`

Permanently delete a project. Only works for **archived** projects. Returns `409` if project is still active. Cascades to all related data (events, issues, alert rules, history).

### `POST /api/0/projects/{slug}/archive`

Archive a project (soft-delete). Hides from main list, still ingests events.

### `DELETE /api/0/projects/{slug}/archive`

Unarchive a project. Restores to active list.

### `POST /api/0/projects/{slug}/rotate-dsn`

Regenerate DSN key. Old key is immediately revoked. Returns updated project.

## Issues

### `GET /api/0/projects/{slug}/issues?page=1&per_page=20`

List issues for a project. Supports `status` and `level` filters.

**Response:**
```json
{
  "data": [{ "id": "...", "title": "...", "level": "error", "count": 5, "status": "unresolved" }],
  "total": 42,
  "page": 1,
  "per_page": 20
}
```

### `GET /api/0/issues/{id}`

Get issue detail.

### `POST /api/0/issues/{id}/status`

Set issue status. **Request:** `{ "status": "resolved" }`

Valid statuses: `unresolved`, `resolved`, `ignored`

### `GET /api/0/issues/{id}/events?page=1&per_page=20`

List events for an issue.

## Search

### `GET /api/0/projects/{slug}/search?q=query&page=1&per_page=20`

Substring search across issue titles. Uses LIKE + sqlite_trigram.

## Ingest

### `POST /api/{project_id}/envelope/`

Sentry-compatible envelope endpoint. **DSN key auth** via `X-Sentry-Auth` header or `Authorization: Bearer <key>`.

Content types: `application/x-sentry-envelope`, supports gzip.

## WebSocket

### `GET /api/0/ws`

Real-time updates. **Cookie auth.** Sends `IssueCreated` and `IssueUpdated` messages.

## Alert Rules

### `GET /api/0/projects/{slug}/rules`

### `POST /api/0/projects/{slug}/rules`

**Request:**
```json
{
  "name": "Alert on Fatal",
  "conditions": { "level": "fatal" },
  "action_type": "webhook",
  "action_config": { "url": "https://hooks.example.com/..." }
}
```

### `DELETE /api/0/rules/{id}`

### `POST /api/0/rules/{id}/toggle`

## System

### `GET /health` → `200 OK` (plain text "ok")

### `GET /metrics` → Prometheus metrics
