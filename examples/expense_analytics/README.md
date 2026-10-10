# Expense Tracker with Analytics

A complete, working JustAPI application: SQLite persistence, full CRUD,
spending analytics, a live SSE feed, background audit logging, and health
checks — plus an end-to-end test suite that runs against the real pipeline.

This is the "does the framework actually work" example: every endpoint below
is exercised by `test_app.py` and was verified against a live server.

## Layout

```
examples/expense_analytics/
  app.py        # the application (routes, models, analytics, SSE)
  seed.py       # deterministic demo data (~6 months, ~300 expenses)
  test_app.py   # 20 end-to-end tests (CRUD, validation, analytics, SSE, health)
  README.md     # this file
```

## Run it

```bash
cd examples/expense_analytics

# Optional: fill the DB with 6 months of demo data (deterministic)
python seed.py            # --force to wipe and reseed

# Start the server (SQLite file: ./expenses.db by default)
python app.py             # http://127.0.0.1:8000, docs at /docs

# Or point at another database file / port
EXPENSES_DB=/tmp/demo.db PORT=9000 python app.py
```

## Test it

```bash
cd examples/expense_analytics
pytest test_app.py -v
```

The tests use `JustAPITestClient` (in-process, native middleware + validation)
with a throwaway SQLite file, so they need no server and no network.

## Endpoints

| Method | Path | What it does |
|---|---|---|
| GET | `/` | Service metadata |
| POST | `/expenses` | Create (201; Pydantic-validated body) |
| GET | `/expenses` | List + filter: `category`, `min_amount`, `max_amount`, `from_date`, `to_date`, `limit` (1–200), `offset` |
| GET | `/expenses/{id}` | Fetch one (404 when missing) |
| PATCH | `/expenses/{id}` | Partial update (400 on empty body) |
| DELETE | `/expenses/{id}` | Delete (204) |
| GET | `/analytics/summary` | Count, total, average, min/max, date range (optional `category`) |
| GET | `/analytics/by-category` | Grouped totals with percentage shares |
| GET | `/analytics/monthly` | Monthly spend series |
| GET | `/analytics/trend?days=N` | Daily spend, last N days (1–365) |
| GET | `/analytics/top?n=N` | Top N expenses by amount (1–20) |
| GET | `/stream/summary` | Server-Sent Events: live summary snapshots |
| GET | `/health`, `/ready` | Liveness / readiness (DB probe registered) |

## What it demonstrates

- **SQLite through the Rust-native pool** — `app.set_database("sqlite:...")`
  with `init_sql` applying the schema idempotently on every pooled connection,
  WAL + `synchronous=NORMAL` pragmas, and parameterised SQL everywhere
  (`app.db.query/execute`).
- **Pydantic request models** — `def create_expense(item: ExpenseCreate)` with
  `Field` constraints; violations come back as FastAPI-shaped 422s.
- **Query parameters with constraints** — `Query(50, ge=1, le=200)`,
  `Query(None, pattern=r"^\d{4}-\d{2}-\d{2}$")`, enforced before your handler.
- **Status codes** — `status_code=201` on the decorator is applied to the
  response (FastAPI parity), while `{"status": 204}` envelopes and
  `JSONResponse` objects keep their own status.
- **Background tasks** — audit rows (`create`/`update`/`delete`) are written
  *after* the response via `background_tasks.add_task(...)`.
- **SSE** — `@app.sse(...)` streams live snapshots; handlers may take a
  `request` argument or none.
- **Health checks** — `app.register_health_check("database", ...)` powers
  `/ready`.
- **Secure headers + timeout** — `enable_secure_headers()`, `enable_timeout(30)`.

## Example requests

```bash
# Create (201)
curl -s -X POST http://127.0.0.1:8000/expenses \
  -H 'Content-Type: application/json' \
  -d '{"description":"Team lunch","amount":64.20,"category":"Food","spent_on":"2026-10-09"}'

# Filtered list
curl -s "http://127.0.0.1:8000/expenses?category=food&min_amount=50&limit=5"

# Analytics
curl -s http://127.0.0.1:8000/analytics/summary
curl -s http://127.0.0.1:8000/analytics/by-category
curl -s "http://127.0.0.1:8000/analytics/trend?days=30"

# Live feed (Ctrl-C to stop)
curl -N http://127.0.0.1:8000/stream/summary
```

## Notes

- SQLite is perfect for this demo; for production traffic point
  `set_database` at Postgres (`postgres://...`) — the same `app.db` API and
  parameter placeholders (`?`) work across drivers.
- The `expenses.db` file (plus `-wal`/`-shm`) is created in this directory;
  delete it to start fresh.
