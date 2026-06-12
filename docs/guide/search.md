# Search

Issue search is integrated directly into the **Issues** page — no separate search page needed.

## Using Search

1. Go to **Issues** in the navigation
2. Type in the **Search issues...** input at the top
3. Results update automatically after 300ms debounce
4. Combine with filters: status tabs, level dropdown, and project selector

## What's Searched

Search matches against:
- **Issue title** — the error type/message
- **Culprit** — the function or module where the error occurred

## Filters

Filters work together with search:

| Filter | Options |
|--------|---------|
| **Status** | All, Unresolved, Resolved, Ignored |
| **Level** | All, Fatal, Error, Warning, Info, Debug |
| **Project** | Select from dropdown |

## URL State

All search and filter state is saved in the URL query params, so you can share filtered views:

```
/issues?project=web-app&q=TypeError&status=unresolved&level=error
```

## API

Search is also available via API:

```
GET /api/0/projects/{slug}/search?q=TypeError&status=unresolved&level=error&limit=20&page=1
```

Returns the same `ListResponse<Issue>` format as the issues endpoint.
