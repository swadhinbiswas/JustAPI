---
title: CLI Reference
description: Complete reference for the justapi command-line tool.
---

The `justapi` CLI provides development, scaffolding, and management commands.

## Installation

```bash
cargo install justapi-cli
justapi --version
```

## Commands

### `justapi serve`

Start the development or production server.

```bash
justapi serve [OPTIONS]
```

| Flag | Default | Description |
|---|---|---|
| `--host` | `127.0.0.1` | Bind address |
| `--port` | `8000` | Bind port |
| `--workers` | CPU count | Number of worker processes |
| `--reload` | false | Enable hot-reload on file changes |
| `--timeout` | 30 | Request timeout (seconds) |
| `--unix` | — | Unix socket path |
| `--scale` | false | Enable load-based auto-scaling |
| `--min-workers` | 2 | Minimum workers (with --scale) |
| `--max-workers` | CPU count | Maximum workers (with --scale) |
| `--scale-low` | 100 | RPS to scale down |
| `--scale-high` | 1000 | RPS to scale up |
| `--scale-cooldown` | 30 | Seconds between scaling events |

### `justapi create`

Generate a new project from a template.

```bash
justapi create [OPTIONS] <project_name>
```

| Flag | Default | Description |
|---|---|---|
| `--db` | (interactive) | Database engine (sqlite, postgres, mysql, duckdb, clickhouse, mongodb, redis) |
| `--api-type` | (interactive) | API protocol (rest, graphql, grpc, jsonrpc) |

### `justapi db`

Database migration commands.

```bash
justapi db migrate   # Apply pending migrations
justapi db rollback  # Rollback last migration
justapi db list      # List migration status
justapi db seed      # Seed database with initial data
justapi db reset     # Drop and recreate all tables
justapi db inspect   # Inspect database schema
```

### `justapi routes`

List all registered routes.

```bash
justapi routes
```

Output:

```
Method  Path               Handler
GET     /                  root
GET     /items/{item_id}   read_item
POST    /items/            create_item
```

### `justapi doctor`

Run system diagnostics.

```bash
justapi doctor
```

Checks Python version, Rust availability, database connectivity, and configuration validity.

### `justapi gen`

Generate artifacts.

```bash
justapi gen openapi > openapi.json    # Generate OpenAPI spec
justapi gen client --lang typescript  # Generate TypeScript client
```

### `justapi check`

Validate configuration without starting the server.

```bash
justapi check
```

### `justapi profile`

Profile handler performance.

```bash
justapi profile --endpoint /items/42
```

### `justapi new`

Alias for `justapi create`.

## Global Flags

| Flag | Description |
|---|---|
| `--help` | Show help message |
| `--version` | Show version |

## See Also

- [Configuration Reference](/reference/configuration/) — Environment variables and app config
- [Project Scaffolder](/getting-started/cli-scaffolder/) — Project generation guide
- [Release Notes](/reference/release-notes/) — Version history
