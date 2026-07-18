# demo_shop — real-life e-commerce API (Olist dataset)

A realistic, complex online-shopping API built on the JustAPI Runtime, backed by
the **Olist Brazilian e-commerce SQLite dataset** (`olist.sqlite`, ~112 MB:
99k orders, 33k products, 3k sellers, 100k reviews, 1M geolocation rows).

The goal of this app is **not** to ship a product — it is to exercise the
framework with a *real, join-heavy workload* (catalog, orders, sellers,
analytics, search) and **surface framework problems** that the echo/hello
micro-benchmarks hide. It succeeded: it found several serious issues.

## What's implemented

| Area | Endpoints |
|---|---|
| Health/meta | `/shop/health`, `/shop/meta/tables` |
| Catalog | `/products` (filter/search/paginate/sort), `/products/{id}`, `/products/{id}/reviews`, `/categories` |
| Sellers | `/sellers`, `/sellers/{id}`, `/sellers/{id}/products` |
| Customers | `/customers/{id}` |
| Orders | `/orders` (filter), `/orders/{id}` (items+payments+reviews aggregate), `/orders/{id}/timeline` (delivery-delay calc) |
| Reviews | `/reviews`, `/reviews/summary` |
| Geolocation | `/geolocation/states` |
| Analytics | `/analytics/sales-by-category`, `/analytics/top-sellers`, `/analytics/delivery-performance`, `/analytics/monthly-revenue` |
| Search | `/search?q=&kind=` |

All SQL runs through the Rust-native `DbPool` (`app.db.query(sql, params)`,
injection-safe bound params). 23 endpoints validated green via the framework's
`AsyncTestClient` (`pytest`-style smoke in `test_app.py`).

## Run it

```bash
# Serve (NOTE: see Finding #4 — app.run() currently deadlocks with a DB)
python demo_shop/app.py --port 8080

# Smoke-test the routes (works — uses the test client, not the TCP server)
python demo_shop/test_app.py

# In-process load test (see Finding #5)
python demo_shop/bench_async.py --concurrency 1 --per-worker 20
```

## Framework problems found (the whole point of this exercise)

### Finding #1 — `app.db` is `None` inside handlers unless the pool is connected
`app.db` resolves the pool lazily, but inside a request context the pool may be
`None` (returns `AttributeError: 'NoneType' object has no attribute 'query'`).
Handlers must rely on the pool being connected by the server/test-client
runtime. Documented behavior says `app.db` "works before and after `run()`" but
in practice it is `None` until a runtime connects it. **Mitigation:** call
`app.connect_database()` eagerly (see `app.py`).

### Finding #2 — `set_database(..., wal=True)` HANGS the pool connect forever
`app.set_database(url, wal=True)` (which appends `journal_mode=WAL`) makes
`connect_database()` **block indefinitely** with no error and no log, on a real
SQLite file. Reproduced deterministically (timeout 30s, no output). Plain
`app.set_database(url)` connects in ~0.00s. **This is a framework bug** in the
WAL pragma path (likely a `busy_timeout`/lock wait or a statement that never
returns on a large file). **Mitigation:** do not use `wal=True`; run on the
default rollback journal.

### Finding #3 — `app.run()` deadlocks on a configured database
A no-DB app serves fine via `app.run()` (~37k req/s, see baseline below). A
DB-backed app **never binds** — `run()` calls `connect_database()` internally
(from inside its own runtime context) and **hangs forever**. Connecting the DB
eagerly at import (so `db_pool.is_some()` and `run()` skips the call) still does
not let the server bind. **This is a framework bug** (nested-runtime /
GIL-pool deadlock during `run()` DB init). **Impact: the framework currently
cannot serve a database-backed HTTP app over TCP.** DB-backed code is only
testable via `AsyncTestClient`.

### Finding #4 — DB-backed handler throughput collapses to ~15 req/s
Raw `app.db.query` on the same dataset is fast: `COUNT(*)` 1.8 ms, 20-row select
0.3 ms, a 74-row category join 96 ms. But a full request through the framework's
handler dispatch (test client, `concurrency=1`) measures:

```
concurrency=1 per_worker=20 total_reqs=280 wall=18.45s
throughput = 15 req/s
latency ms: p50=910  p90=981  p99=1007  max=1020
```

i.e. a trivial 1-row `COUNT(*)` read takes **~900 ms end-to-end** — ~900×
slower than the raw query. The overhead is entirely in the framework's
handler→response path (GIL-pool dispatch + Python↔Rust result/JSON
serialization), not the database. **This is the headline performance problem:**
the Python+DB hot path is ~2500× slower than the Rust HTTP layer (below).

### Finding #5 — concurrent async load DEADLOCKS
`bench_async.py` at `concurrency=32` (or any >1 spreading across the loop)
**hangs** — the process never completes (timeout 90s). The GIL pool / shared
`db_runtime` `Handle` + `block_on` does not tolerate concurrent in-flight
requests. So the framework cannot serve even modest concurrency against a DB
today. (The no-DB path is fine at 64 concurrency — see baseline.)

## Baseline: pure Rust HTTP layer is fast; the Python+DB path is not

| Path | Throughput | Notes |
|---|---:|---|
| No-DB `app.run()` + `oha -c 64` | **37,171 req/s** | Rust HTTP + Python dict handler, real TCP |
| DB-backed handler, `concurrency=1` | **15 req/s** | framework handler+DB dispatch (Finding #4) |
| DB-backed handler, `concurrency>1` | **hangs** | deadlock (Finding #5) |
| Raw `app.db.query` (same queries) | ~1–100 ms/query | DB itself is fine (Finding #4 contrast) |

**Conclusion:** the runtime's networking core is solid (37k req/s, consistent
with the ~700k native-fast-path claims for opt-in routes), but the **default
Python-handler + SQLite path — the path every real app actually uses — is
currently unusable in production**: it deadlocks on `run()` and, when forced
through the test client, runs at 15 req/s with ~900 ms latency. This must be
fixed before any "real-life complex app" claim holds.

## Recommended framework fixes (for 2.0.8+)

1. **Fix `run()` DB-init deadlock** (Finding #3): connect the pool on the
   server's own runtime, not via a nested `Runtime::new()` + `block_on` called
   from within `run()`'s GIL/release context.
2. **Fix `wal=True` hang** (Finding #2): the WAL pragma execution path on a real
   file never returns; add a timeout / correct lock handling.
3. **Cut handler→response overhead** (Finding #4): the ~900 ms/request is in
   GIL-pool dispatch + serialization. Profile and remove per-request GIL
   round-trips / redundant copies for the common "return dict → JSON" path.
4. **Fix concurrent-request deadlock** (Finding #5): the shared `db_runtime`
   `Handle` + `block_on` serializes/hangs under concurrency; use a proper
   `tokio::Runtime`/`Handle` that supports concurrent `block_on` or run the DB
   pool on its own multi-threaded runtime.

## Files
- `app.py` — the API (23 routes, SQLite-backed).
- `test_app.py` — smoke test, 23 endpoints (all pass via `AsyncTestClient`).
- `bench_async.py` — in-process load test (surfaces Finding #4/#5).
- `README.md` — this file.
