---
title: Database API
description: Reference for the Rust-native database connection pool and query interface.
---

## `app.set_database()`

Configure the database connection pool:

```python
app.set_database(
    url="postgres://user:pass@localhost:5432/mydb",
    init_sql="CREATE TABLE IF NOT EXISTS ...",
    max_connections=10,
    request_acquire_timeout=3.0,
    wal=False,
    pragmas=None,
)
```

### Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `url` | `str` | required | Database connection URL |
| `init_sql` | `str` | `None` | SQL to run on startup (for schema creation) |
| `max_connections` | `int` | `10` | Maximum pool connections |
| `request_acquire_timeout` | `float` | `3.0` | Seconds before pool saturation returns 503 |
| `wal` | `bool` | `False` | Enable SQLite WAL mode |
| `pragmas` | `list[str]` | `None` | Additional SQLite pragmas |

## `app.db` Object

### Methods

| Method | Returns | Description |
|---|---|---|
| `app.db.query(sql, params)` | `list[dict]` | Execute SELECT and return rows |
| `app.db.execute(sql, params)` | `int` | Execute INSERT/UPDATE/DELETE, return affected rows |

#### `query()`

```python
rows = app.db.query(
    "SELECT id, name, price FROM products WHERE price > $1 ORDER BY id",
    [100.0],
)
# Returns: [{"id": 1, "name": "Widget", "price": 150.0}, ...]
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `sql` | `str` | required | SQL query with `$1`, `$2`, ... placeholders |
| `params` | `list` | `[]` | Query parameters |

#### `execute()`

```python
affected = app.db.execute(
    "INSERT INTO products (name, price) VALUES ($1, $2)",
    ["Widget", 150.0],
)
# Returns: 1
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `sql` | `str` | required | SQL with `$1`, `$2`, ... placeholders |
| `params` | `list` | `[]` | Query parameters |

## Connection URLs

| Database | URL Format | Example |
|---|---|---|
| PostgreSQL | `postgres://user:pass@host:port/db` | `postgres://postgres@localhost/mydb` |
| MySQL | `mysql://user:pass@host:port/db` | `mysql://root@localhost/mydb` |
| SQLite | `sqlite:///path/to/db.db` | `sqlite:///app.db` |
| SQLite (memory) | `sqlite://:memory:` | `sqlite://:memory:` |

## Pool Saturation

When all connections are in use, new requests wait up to `request_acquire_timeout` seconds, then receive **503 Service Unavailable** with a `Retry-After: 1` header.

## SQLite Defaults

For SQLite connections, JustAPI automatically enables:
- `PRAGMA busy_timeout=5000` — Wait up to 5 seconds for busy connections
- `PRAGMA journal_mode=WAL` — Write-Ahead Logging for concurrent access
- `PRAGMA synchronous=NORMAL` — Balanced durability/performance

## See Also

- [Database Integration Tutorial](/tutorials/database-integration/) — Usage guide
- [JustAPIApp](/api-reference/justapiapp/) — `set_database()` method
- [Performance Tuning](/advanced/performance-tuning/) — Database optimization
