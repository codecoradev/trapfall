# Multi-Project

TrapFall supports multiple projects, each with its own DSN and isolated error data. This lets you track errors separately for your web app, mobile app, backend API, etc.

![Projects page](/images/docs-05-projects.png)

## Creating Projects

### From Dashboard

1. Login to the dashboard
2. Go to **Projects** page
3. Click **"+ Add Project"**
4. Enter a name (slug is auto-generated)
5. Click **Create Project**
6. Copy the DSN for your SDK

### From CLI

```bash
trapfall project add "My Web App"
# Output: Project created: My Web App (my-web-app)

trapfall project add "Mobile App" mobile-app
# With custom slug
```

## Managing Projects

### Rename

Click the **⋮** menu on a project card → **Rename**. Enter the new name and save.

### Rotate DSN

If a DSN key is compromised:

1. Click **⋮** → **Rotate DSN**
2. The old DSN key is immediately revoked
3. Copy the new DSN and update your SDK configuration

### Archive

Archiving hides a project from the main view and stops it from appearing in project selectors:

1. Click **⋮** → **Archive**
2. The project moves to the **Archived** tab
3. Ingest still works but the project is out of sight
4. To restore: go to **Archived** tab → **⋮** → **Unarchive**

### Delete Permanently

Deletion is permanent and irreversible. Only archived projects can be deleted:

![Project actions menu](/images/docs-06-project-menu.png)

1. Archive the project first
2. Go to the **Archived** tab
3. Click **⋮** → **Delete permanently**
4. Confirm the deletion

This removes the project and **all** its data: issues, events, alert rules, and alert history.

## Project Isolation

Each project has its own:
- **DSN** — unique key for SDK authentication
- **Issues** — error groups, independent of other projects
- **Events** — individual error occurrences
- **Alert rules** — per-project webhook configuration
- **Search** — scoped to project

## Typical Setup

| Project | SDK | Platform |
|---------|-----|----------|
| Web App | `@sentry/browser` or `@sentry/node` | JavaScript |
| Mobile App | `sentry_flutter` | Dart |
| Backend API | `sentry-sdk` or `sentry` crate | Python / Rust |
| Worker Service | `sentry-sdk` | Python |

Each service points to its own DSN. All errors flow into the same TrapFall instance but are isolated per project.

## API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/0/projects` | List all projects |
| `POST` | `/api/0/projects` | Create project |
| `GET` | `/api/0/projects/{slug}` | Get project |
| `PATCH` | `/api/0/projects/{slug}` | Rename project (`{ "name": "..." }`) |
| `DELETE` | `/api/0/projects/{slug}` | Delete permanently (archived only) |
| `POST` | `/api/0/projects/{slug}/archive` | Archive project |
| `DELETE` | `/api/0/projects/{slug}/archive` | Unarchive project |
| `POST` | `/api/0/projects/{slug}/rotate-dsn` | Regenerate DSN key |
