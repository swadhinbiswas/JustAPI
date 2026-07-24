---
title: Database Integration (SQL & NoSQL)
description: Connect to PostgreSQL, MySQL, SQLite, DuckDB, ClickHouse, Mongo, and Redis in JustAPI.
---

JustAPI provides a Rust-native connection pool manager (`app.db`) that executes SQL queries in Rust with zero GIL lock.

## PostgreSQL Integration

```python
from justapi import JustAPIApp

app = JustAPIApp()
app.set_database(
    "postgres://postgres:password@localhost:5432/mydb",
    init_sql="CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY, name TEXT);"
)

@app.get("/users")
def get_users(request):
    return app.db.query("SELECT * FROM users ORDER BY id")
```

## Analytical Databases (DuckDB & ClickHouse)

```python
import duckdb
from justapi import JustAPIApp

app = JustAPIApp()
conn = duckdb.connect("analytics.duckdb")

@app.get("/analytics/sales")
def sales_summary(request):
    res = conn.execute("SELECT region, SUM(amount) FROM sales GROUP BY region").fetchall()
    return {"summary": res}
```
