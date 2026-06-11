# Multi-Project

TrapFall supports multiple projects, each with its own DSN and isolated error data. This lets you track errors separately for your web app, mobile app, backend API, etc.

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
#         DSN: https://abc123@localhost:3000/1

trapfall project add "Mobile App" mobile-app
# With custom slug
```

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

## Rotating DSN Keys

If a DSN key is compromised:

```bash
trapfall project rotate-dsn my-web-app
```

This generates a new DSN. Update your SDK configuration with the new DSN.
