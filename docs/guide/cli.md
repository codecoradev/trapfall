# CLI Reference

## `trapfall serve`

Start the HTTP server.

```bash
trapfall serve [OPTIONS]

Options:
  --listen <ADDR>    Listen address (default: 0.0.0.0:9090)
  --db <URL>         Database URL or path (default: trapfall.db)
```

The `--db` flag accepts both bare paths (`trapfall.db`) and scheme-prefixed URLs (`sqlite:trapfall.db`, `postgres://...`).

Environment variable `TRAPFALL_DATABASE_URL` overrides the default, and `--db` flag takes precedence over the env var.

## `trapfall project add`

Create a new project.

```bash
trapfall project add "Project Name" [slug]

# Examples
trapfall project add "My Web App"              # slug: my-web-app
trapfall project add "Backend API" backend     # slug: backend
```

Outputs the project name, slug, and DSN.

## `trapfall project list`

List all projects.

```bash
trapfall project list
```

## `trapfall project rotate-dsn`

Generate a new DSN key for a project. Old DSN stops working immediately.

```bash
trapfall project rotate-dsn <slug>
```

## `trapfall healthcheck`

Check if the server is running and healthy. Returns exit code 0 on success.

```bash
trapfall healthcheck
```

Used by Docker `HEALTHCHECK`.

## `trapfall mcp`

Start the MCP server for AI agent tools. Communicates via stdio JSON-RPC 2.0.

```bash
trapfall mcp
```

See [MCP Server](/guide/mcp) for available tools.
