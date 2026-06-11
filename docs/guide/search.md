# Search

TrapFall uses LIKE + sqlite_trigram for substring search across issue titles.

## Using Search

1. Go to **Search** page in the dashboard
2. Enter your search query
3. Results show matching issues from all projects

## API

```bash
curl -b cookie "http://localhost:3000/api/0/projects/my-app/search?q=TypeError&page=1&per_page=20"
```

**Response:**
```json
{
  "data": [{ "id": "...", "title": "TypeError: Cannot read property 'x'", "level": "error" }],
  "total": 1,
  "page": 1,
  "per_page": 20
}
```

## How It Works

- **LIKE** — standard SQL substring matching
- **sqlite_trigram** — trigram index for faster LIKE queries on large datasets
- Searches across issue **titles** only
- Case-insensitive

## Performance

For typical use (< 100K issues), search is fast. For larger datasets, the trigram index keeps queries performant without the overhead of FTS5.

## What TrapFall Search is NOT

- ❌ Full-text search across event bodies, stack traces, or breadcrumbs
- ❌ Faceted search (by tag, user, browser, etc.)
- ❌ Regex or structured queries

These may be added in future versions.
