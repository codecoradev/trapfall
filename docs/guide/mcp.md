# MCP Server

TrapFall includes a Model Context Protocol (MCP) server for AI agent integration. Communicates via stdio JSON-RPC 2.0.

## Starting

```bash
trapfall mcp --db trapfall.db
```

Configure your AI client to run this command as an MCP server.

## Available Tools

| Tool | Description |
|------|-------------|
| `list_issues` | List issues for a project (with filters) |
| `get_issue` | Get issue detail by ID |
| `get_event` | Get event detail by ID |
| `set_status` | Set issue status (resolved/unresolved/ignored) |
| `search_issues` | Search issues by query string |
| `list_projects` | List all projects |
| `get_project` | Get project detail by slug |
| `get_project_stats` | Get issue/event counts for a project |
| `list_alert_rules` | List alert rules for a project |
| `list_events` | List events for an issue |
| `rotate_dsn` | Generate new DSN key for a project |
| `healthcheck` | Check server health |

## Example: AI Agent Usage

An AI agent connected via MCP can:

1. **Monitor errors** — `list_issues` to check for new errors
2. **Investigate** — `get_issue` + `list_events` to see full stack traces
3. **Take action** — `set_status` to resolve or ignore issues
4. **Search** — `search_issues` to find specific errors
5. **Manage projects** — `list_projects`, `get_project_stats`

## Transport

stdio only. No TCP. This is by design — MCP servers are launched as child processes by the AI client.
