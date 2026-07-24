---
title: Database Integration
description: Connect to SQL and NoSQL databases with JustAPI's Rust-native connection pool — a FastAPI alternative for high-performance data access.
keywords: database integration, SQL, NoSQL, connection pool, JustAPI, FastAPI alternative, sqlx, PostgreSQL, SQLite
---

JustAPI provides a built-in database connection pool manager (`app.db`) that executes queries in Rust via `sqlx`, keeping the GIL released for maximum throughput.

## Quick Start: SQLite

```python
from justapi import JustAPIApp

app = JustAPIApp()

app.set_database(
    "sqlite:///app.db",
    init_sql="""
    CREATE TABLE IF NOT EXISTS items (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        quantity INTEGER DEFAULT 0
    )
    """,
)


@app.get("/items")
def list_items(request):
    return app.db.query("SELECT * FROM items ORDER BY id")


@app.post("/items")
def add_item(request):
    data = request.json()
    app.db.execute(
        "INSERT INTO items (name, quantity) VALUES ($1, $2)",
        [data["name"], data.get("quantity", 0)],
    )
    return {"message": "Item created"}
```

## Supported Databases

| Engine | URL Scheme | Driver | Use Case |
|---|---|---|---|
| **PostgreSQL** | `postgres://user:pass@host/db` | Rust `sqlx-postgres` | Production OLTP |
| **MySQL** | `mysql://user:pass@host/db` | Rust `sqlx-mysql` | Scalable relational |
| **SQLite** | `sqlite:///path/to/db.db` | Rust `sqlx-sqlite` | Embedded, dev, single-server |
| **DuckDB** | `duckdb:///path/to/db.duckdb` | Python `duckdb` | Analytical OLAP |
| **ClickHouse** | `clickhouse://host:9000/db` | Python `clickhouse-driver` | Column-store analytics |
| **MongoDB** | `mongodb://host:27017/db` | Python `pymongo` | Document store |
| **Redis** | `redis://host:6379/0` | Python `redis` | Caching, pub/sub |

## PostgreSQL Example

```python
app.set_database(
    "postgres://postgres:password@localhost:5432/mydb",
    init_sql="""
    CREATE TABLE IF NOT EXISTS users (
        id SERIAL PRIMARY KEY,
        name TEXT NOT NULL,
        email TEXT UNIQUE NOT NULL
    )
    """,
)


@app.post("/users")
def create_user(request):
    data = request.json()
    result = app.db.query(
        "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id",
        [data["name"], data["email"]],
    )
    return {"user_id": result[0]["id"]}
```

## Query API

The `app.db` object provides two methods:

| Method | Description | Returns |
|---|---|---|
| `app.db.query(sql, params)` | Execute a SELECT query | `list[dict]` — rows as dicts |
| `app.db.execute(sql, params)` | Execute INSERT/UPDATE/DELETE | `int` — affected rows |

## Connection Pool Configuration

```python
app.set_database(
    "postgres://localhost/mydb",
    max_connections=20,                # Pool size (default: 10)
    request_acquire_timeout=3.0,       # Seconds before 503 (default: 3.0)
    init_sql="CREATE TABLE IF NOT EXISTS ...",
)
```

### SQLite-Specific Options

```python
app.set_database(
    "sqlite:///app.db",
    wal=True,                           # Enable WAL mode for concurrent access
    pragmas=["journal_mode=WAL", "synchronous=NORMAL"],
)
```

By default, JustAPI enables `busy_timeout=5000`, WAL journal mode, and `synchronous=NORMAL` for all SQLite connections to support concurrent reads and writes.

## Pool Saturation Handling

When the connection pool is exhausted, requests get **503 Service Unavailable** instead of waiting indefinitely:

```python
# Default: request_acquire_timeout=3.0 seconds
# If no connection is available within 3s, returns 503
app.set_database("sqlite:///app.db", request_acquire_timeout=1.0)
```

## Migration System

JustAPI includes a simple migration runner:

```bash
justapi db migrate    # Apply pending migrations
justapi db rollback   # Rollback last migration
justapi db list       # List migration status
```

Migrations are SQL files in the `migrations/` directory:

```sql
-- migrations/0002_add_email.sql
ALTER TABLE users ADD COLUMN email TEXT;
```

## Using Native Drivers (Python)

For databases not supported by `sqlx`, use standard Python drivers:

```python
import duckdb


app = JustAPIApp()
conn = duckdb.connect("analytics.duckdb")


@app.get("/analytics/sales")
def sales_summary(request):
    result = conn.execute(
        "SELECT region, SUM(amount) as total FROM sales GROUP BY region"
    ).fetchall()
    return {"summary": [{"region": r[0], "total": r[1]} for r in result]}
```

## How It Works Internally

1. `app.set_database()` creates a Rust `sqlx` connection pool (`AnyPool`)
2. Queries are executed in Rust via `py.detach()` — zero GIL overhead
3. Results are returned as Python dicts via zero-copy serialization
4. Transactions are auto-managed: commit on 2xx, rollback on error
5. Pool saturation returns 503 with `Retry-After: 1` header

## Next Steps

- [Background Tasks](/tutorials/background-tasks/) — Async processing after DB writes
- [Dependency Injection](/tutorials/dependency-injection/) — DB session as a dependency
- [Performance Tuning](/advanced/performance-tuning/) — Optimize database queries
