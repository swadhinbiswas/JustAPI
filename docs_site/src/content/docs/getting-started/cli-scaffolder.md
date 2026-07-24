---
title: Project Scaffolder CLI
description: How to generate complete multi-database and multi-protocol project templates with zero boilerplate.
---

JustAPI features an interactive CLI project generator (`justapi create`) to scaffold complete applications with your choice of database and API architecture.

## Installation

Ensure you have the CLI installed:

```bash
cargo install justapi-cli
justapi --version
```

## Interactive Mode

Run `justapi create` without arguments to launch the TTY interactive wizard:

```bash
justapi create my_app
```

The wizard will prompt you to select:

1. **Database Engine:** SQLite, PostgreSQL, MySQL, DuckDB, ClickHouse, MongoDB, or Redis.
2. **API Protocol:** REST (OpenAPI), GraphQL (GraphiQL), gRPC (Protobuf), or JSON-RPC 2.0.

## Command Line Flags

Bypass the interactive wizard for scripting or quick scaffolding:

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
│   └── main.py               # Pre-configured JustAPI application
├── migrations/
│   └── 0001_initial.sql       # Database schema migration
├── docker-compose.otel.yml    # OpenTelemetry, Jaeger, Prometheus, Grafana
├── .env.example               # Environment variable template
└── README.md                  # Project documentation
```

### `main.py` (Generated Example)

When you select REST + PostgreSQL, the generated `main.py` looks like:

```python
from justapi import JustAPIApp

app = JustAPIApp(title="my_app", version="0.1.0")

app.set_database("postgres://user:pass@localhost:5432/my_app")


@app.get("/")
def root(request):
    return {"service": "my_app", "status": "running"}


@app.get("/health")
def health(request):
    return {"status": "ok"}
```

## Protocol Selection Guide

| Protocol | Best For | Docs URL |
|---|---|---|
| **REST** | General-purpose APIs, CRUD, microservices | `/docs` (Swagger) |
| **GraphQL** | Complex data queries, real-time subscriptions | `/graphql` (GraphiQL) |
| **gRPC** | High-performance internal services, streaming RPC | `protobuf` binary protocol |
| **JSON-RPC 2.0** | Lightweight RPC, wallet/web3 APIs | `/jsonrpc` |

## Database Selection Guide

| Database | Best For | Driver |
|---|---|---|
| **PostgreSQL** | Production OLTP, complex queries | Rust `sqlx-postgres` |
| **MySQL** | Scalable relational workloads | Rust `sqlx-mysql` |
| **SQLite** | Development, embedded, single-server | Rust `sqlx-sqlite` |
| **DuckDB** | Analytical queries, Parquet, CSV | Python `duckdb` |
| **ClickHouse** | High-volume column-store analytics | Python `clickhouse-driver` |
| **MongoDB** | Document storage, flexible schemas | Python `pymongo` |
| **Redis** | Caching, rate-limiting, pub/sub | Python `redis` |

## Development Server

After scaffolding, start the dev server with hot reload:

```bash
cd my_app
justapi serve --reload
```

## Next Steps

- [First Steps](/getting-started/first-steps/) — Build your first API endpoint
- [Database Integration](/tutorials/database-integration/) — Connect and query databases
- [Multi-Protocol APIs](/advanced/multi-protocol-apis/) — REST, GraphQL, gRPC, JSON-RPC
