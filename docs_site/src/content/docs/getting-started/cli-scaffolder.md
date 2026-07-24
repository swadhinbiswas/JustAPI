---
title: Project Scaffolder CLI
description: How to generate complete multi-database and multi-protocol project templates with zero boilerplate.
---

JustAPI features an interactive CLI project generator (`justapi create`) to scaffold complete applications with your choice of database and API architecture.

## Interactive Mode

Run `justapi create` without arguments to launch the TTY interactive wizard:

```bash
justapi create my_app
```

The wizard will prompt you to select:
1. **Database Engine:** SQLite, PostgreSQL, MySQL, DuckDB, ClickHouse, MongoDB, or Redis.
2. **API Protocol:** REST (OpenAPI), GraphQL (GraphiQL), gRPC (Protobuf), or JSON-RPC 2.0.

## Command Line Flags

You can bypass the interactive wizard using `--db` and `--api-type` flags:

```bash
# REST API with DuckDB analytical engine
justapi create analytics_app --db duckdb --api-type rest

# GraphQL API with PostgreSQL database
justapi create graph_service --db postgres --api-type graphql

# gRPC microservice with Redis
justapi create rpc_service --db redis --api-type grpc

# JSON-RPC 2.0 service with SQLite
justapi create jsonrpc_api --db sqlite --api-type jsonrpc
```

## Generated Project Layout

```
my_app/
├── app/
│   ├── __init__.py
│   └── main.py          # Pre-configured JustAPI application
├── migrations/
│   └── 0001_initial.sql # Database schema migrations
├── docker-compose.otel.yml # OpenTelemetry & Jaeger observability stack
├── .env.example
└── README.md
```
