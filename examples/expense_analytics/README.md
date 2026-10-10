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

# Development: single process, auto-restart on *.py changes
justapi serve --addr 127.0.0.1:8000 --reload

# Production: 4 prefork workers sharing the port (SO_REUSEPORT)
justapi serve --addr 0.0.0.0:8000 --workers 4

# Or point at another database file / port
EXPENSES_DB=/tmp/demo.db PORT=9000 python app.py
```

## Deploy it (Docker)

```bash
cd examples/expense_analytics

# docker compose (recommended)
docker compose up --build          # http://127.0.0.1:8000
docker compose logs -f api
docker compose down                # keeps the SQLite volume

# or plain docker
docker build -t expense-analytics .
docker run --rm -p 8000:8000 -v expense-data:/data expense-analytics
```

The image runs as a non-root user, serves with `WORKERS=4` prefork workers,
stores the SQLite database in the `/data` volume, and ships a `/live`
healthcheck. `docker compose` reads `JUSTAPI_SPEC` from the environment, so a
local wheel works before the release is on PyPI:

```bash
docker compose build --build-arg JUSTAPI_SPEC=./justapi-2.1.0-cp311-abi3-manylinux_2_34_x86_64.whl
```

## Observability

- **`GET /metrics`** — Prometheus text format (requests/errors/bytes counters,
  latency), ready to scrape from Prometheus or a Grafana agent.
- **Structured logs** — `LOG_FORMAT=json` emits one JSON object per event
  (`timestamp`, `level`, `http.method`, `http.path`, `http.status_code`,
  `latency_ms`) for Loki/CloudWatch/Datadog; the default is readable text.
  `LOG_LEVEL=debug` turns up verbosity.
- **Health** — `GET /health` (liveness) and `GET /ready` (readiness; probes
  the database via `register_health_check`).
- **Tracing** — `justapi.logging.init_otlp_tracing(endpoint, service_name)`
  before `app.run()` exports spans over OTLP gRPC (OpenTelemetry).

## Measured performance

`oha -z 10s -c 50` against the real app (SQLite-backed handlers), same
machine as the load generator — relative numbers, not capacity claims:

| Endpoint | 1 worker | 4 workers |
|---|---|---|
| `GET /analytics/summary` | 2,453 rps · p99 25 ms | 8,007 rps · p99 9 ms |
| `GET /expenses?limit=10` | 1,535 rps · p99 43 ms | 4,910 rps · p99 14 ms |
| `GET /analytics/by-category` | 1,834 rps · p99 33 ms | 6,845 rps · p99 12 ms |

Also verified: 40 concurrent writes across 4 workers → 40×201 with every row
and background audit row persisted; worker `SIGTERM` drains in-flight
requests and is respawned; supervisor `SIGTERM` stops all workers cleanly.
Full details in `BENCHMARKS.md`.

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

## Postgres

The same app runs on Postgres — `app.db` and `?` placeholders are identical
across engines (the pool pins each placeholder to the bound value's type for
Postgres). Point `EXPENSES_DB` at a Postgres URL:

```bash
docker run -d --name pg -e POSTGRES_PASSWORD=pass -p 5433:5432 postgres:16-alpine
EXPENSES_DB=postgres://postgres:pass@127.0.0.1:5433/postgres python seed.py
EXPENSES_DB=postgres://postgres:pass@127.0.0.1:5433/postgres justapi serve --workers 4
```

Dialect notes (handled in `app.py`): Postgres uses `SERIAL` + `TEXT`
timestamps (`now()::text` — the driver cannot decode `timestamptz`), and all
rounding happens in Python (`ROUND(double precision, int)` does not exist in
Postgres). A `NULL` bound parameter is typed as text, so rows without an id
omit the column instead of binding `None`.

## Notes

- SQLite is perfect for this demo; for production traffic point
  `set_database` at Postgres (see above) — the same `app.db` API works across
  drivers.
- The `expenses.db` file (plus `-wal`/`-shm`) is created in this directory;
  delete it to start fresh.
