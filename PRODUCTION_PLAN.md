# PRODUCTION-READINESS PLAN (2.0.8 pre-release hardening)

> Goal: make JustAPI safe for real DB-backed production use and close the gaps
> vs FastAPI/Robyn that the micro-benchmarks hide. This plan is derived from the
> 2026-07-20 audit (REPORT.md) + live reproductions. Work top-down; each section
> is independently testable and gated by `cargo test --workspace` + pytest.
>
> Conventions (AGENTS.md): Rust-first, typed `thiserror` errors at crate
> boundaries, `// SAFETY:` on any `unsafe`, clippy `-D warnings`, fmt clean,
> miri on core if touched.

---

## P0 — Blockers (must fix before ANY release)

### P0.1 — BUG-2a: `normalize_db_url` breaks SQLite path resolution
**Symptom (reproduced 2026-07-20):** `app.set_database("sqlite://...")` +
`app.run()` crashes with `code 14 / unable to open database file` for *every*
path (relative and absolute). `demo_shop` reported it as a deadlock; it is a
path-resolution bug.

**Root cause:** `crates/justapi-core/src/db/pool.rs:93` `normalize_db_url`
only strips the 2-char `sqlite://` prefix and passes the remainder verbatim.
- `sqlite:///demo_shop.db` (3 slashes, relative per SQLAlchemy convention)
  → `sqlite:/demo_shop.db` → driver reads **absolute `/demo_shop.db`** (root).
- `sqlite:////tmp/foo.db` (4 slashes, absolute) → `sqlite://tmp/foo.db`
  → mangled double-slash path, also fails near root.

No unprivileged process can write to `/demo_shop.db`, hence `SQLITE_CANTOPEN`.

**Fix:** match the SQLAlchemy/FastAPI convention exactly.
```rust
fn normalize_db_url(url: &str) -> String {
    let Some(rest) = url.strip_prefix("sqlite://") else {
        return url.to_string();
    };
    if let Some(abs) = rest.strip_prefix("//") {
        format!("sqlite:/{abs}")          // sqlite:////abs -> sqlite:/abs
    } else if let Some(rel) = rest.strip_prefix('/') {
        format!("sqlite:{rel}")           // sqlite:///rel -> sqlite:rel
    } else {
        format!("sqlite:{rest}")          // sqlite://:memory:, sqlite://./x
    }
}
```
Preserves existing `:memory:` and `./explicit` cases (fall through `else`).

**Tests:** add to `pool.rs` unit tests next to `test_db_kind_from_url`:
- `sqlite:///foo.db` → `sqlite:foo.db` (relative)
- `sqlite:////tmp/foo.db` → `sqlite:/tmp/foo.db` (absolute)
- `sqlite://:memory:` → `sqlite::memory:` (unchanged)
- `sqlite://./x.db` → `sqlite:./x.db` (unchanged)
Plus an integration test: spin `AnyPool::connect` on a temp relative + temp
absolute path, assert `connect` succeeds and a `SELECT 1` works.

**Gate:** cargo test db pool + repro script (`/tmp/repro_db*.py`) now starts
and serves `/ping` with the DB up.

### P0.2 — BUG-2b / D3: SQLite has zero concurrency protection by default
**Symptom (stress test):** DB-backed handler path collapsed to ~15 req/s /
~900 ms p50 while raw query is 1–100 ms (~2500× overhead).

**Root cause (confirmed in code):** pragmas are **opt-in**. In
`pool.rs:252`, the `busy_timeout`+WAL block only runs `if (DbKind::Sqlite,
Some(pragmas))` — i.e. only when the caller passes `pragmas`. The Python
`app.set_database(db)` default is `pragmas=None`, `wal=False`, so a fresh
SQLite pool runs stock defaults: **rollback-journal (single writer blocks all
readers/writers) + `busy_timeout=0` (immediate `SQLITE_BUSY` on contention, no
wait)**. Under the default `max_connections=10` pool all hammering one file,
that is constant `SQLITE_BUSY`. (No retry/backoff loop exists in
`justapi-py`/`justapi-core` today, so the slowdown is the driver's own
contention behavior, not a bolted-on retry loop — but the fix below removes the
cause regardless.)

**Fix:** make `busy_timeout` + WAL + `synchronous=NORMAL` **unconditional** for
SQLite, with user `pragmas` appended/overriding:
```rust
if kind == DbKind::Sqlite {
    let mut all = vec!["PRAGMA busy_timeout=5000".to_string()];
    all.extend(config.pragmas.clone().unwrap_or_else(|| vec![
        "journal_mode=WAL".to_string(),
        "synchronous=NORMAL".to_string(),
    ]));
    opts = opts.after_connect(move |conn, _meta| { /* unchanged body */ });
}
```
Keep `wal=True` Python flag working (it already appends `journal_mode=WAL`;
the unconditional default makes it redundant but harmless).

**Tests:** integration test asserting a 10-connection concurrent INSERT/SELECT
load on a SQLite file sustains > some floor (e.g. > 1k RPS @ -c10) without
`SQLITE_BUSY`. Re-run `demo_shop` checkout flow under `oha -c 50` and confirm
p50 in the low-ms range, not ~900 ms.

**Gate:** `demo_shop` 23-endpoint app starts against `shop.db` and sustains
concurrent load (D1/D3/D4 all closed, not just worked around).

### P0.3 — BUG-1: top-level `"status"` key empties the response body
**Symptom (reproduced):** handler returning `{"status":"ok","products":5}`
→ `200` with **empty body**. `demo_shop` hit this on health/order endpoints.

**Root cause:** `crates/justapi-py/src/native/handlers.rs:322-348`
`serialize_response` branch 2 treats *any* dict containing a `"status"` key as
a legacy `{"status","body","headers"}` envelope. With no `"body"` key, body
becomes `Vec::new()`. A normal business payload with a `"status"` field is
silently dropped.

**Fix:** only enter the legacy-envelope branch when the dict actually looks
like an envelope — require an explicit sentinel, e.g. a `"__response__": true`
key, OR require both `status` **and** `body`/`headers`. Simplest correct rule:
treat as envelope only if it has `"body"` (the thing that distinguishes a
response envelope from a data dict). A plain `{"status": ...}` data dict falls
through to normal JSON serialization.
```rust
let is_envelope = has_body; // drop `|| has_status`
```
Add a regression test: `{"status":"ok","products":5}` round-trips unchanged;
`{"status":201,"body":"x"}` still works as an envelope.

**Gate:** repro script shows `with_status body: b'{"status":"ok","products":5}'`.

---

## P1 — Correctness / DX gaps

### P1.1 — BUG-3: `body_schema=` delivers raw `bytes`, not a Schema instance
Handlers must re-parse `request.json()` manually (demo_shop workaround). The
native fast path already validates via the schema; the *Python handler* path
should receive a validated, instantiated object (or at least the parsed dict),
not raw bytes. Decide: pass parsed dict when `body_schema` is a `Schema`/pydantic
model; keep raw bytes only for legacy callables. Add test asserting handler
receives a dict (or model) for a `Schema`-registered route.

### P1.2 — `test_graphql.py` 404s (`graphql()` builder missing)
Either implement a `graphql()` route builder or remove the test. The GraphQL
gateway (Phase 35) exists in core but the Python `app.graphql()` surface is
absent. Low priority — implement or skip.

### P1.3 — Test-dependency pinning (open issue in PLAN.md)
`pytest` gate currently **5 failed / 115 passed** due to missing
`pydantic`, `jinja2`, `websockets` in the venv, plus a `test_scheduler` flake.
Add `requirements-dev.txt` / `[project.optional-dependencies].test` and a
`test_scheduler` fix. Gate: `pytest` fully green.

### P1.4 — `cargo fmt --check` gate drift
`rustfmt.toml` uses nightly-only options (`wrap_comments`, `group_imports`,
`imports_granularity`, …) so stable CI can't enforce format. Pin CI to nightly
+ commit the format, OR drop the nightly-only options. Pick one; do not ship
the 2860-line deletion accidentally.

---

## P2 — Honest benchmarking (close the "real life" gap)

### P2.1 — Real DB-backed CRUD benchmark vs FastAPI+SQLAlchemy and Robyn
The micro-benchmarks (echo/validate) overstate production reality by ~1000× on
DB paths. Add `benchmarks/workloads_crud_app.py` (JustAPI) and
`workloads_crud_fastapi.py` (FastAPI+SQLAlchemy) + a Robyn equivalent:
- SQLite file, WAL, 10-conn pool, single-row INSERT/SELECT/UPDATE under `oha
  -c 50`. Report RPS + p50/p99 for all three. This is the only fair "real life"
  number and must be appended to BENCHMARKS.md.

### P2.2 — Reduce GIL-path overhead on the handler→response dispatch
D3's residual ~ms dispatch cost on the Python path (post-pragma fix) is the
next lever. Measure after P0.2; if still > FastAPI, shrink the per-request FFI
cost (reuse request dict, avoid rebuild).

---

## P3 — Distribution

### P3.1 — PyPI publish
Package builds (`twine check` PASSED) but is unpublished. Build portable
`manylinux_2_28` wheels (`maturin` in the ghcr container) and upload with
`MATURIN_PYPI_TOKEN`. Required for adoption.

### P3.2 — Unfreeze GPU/inference (Phase 52)
Keep frozen until 2.0.8 per ADR-067. Structural benchmarks are topology proofs,
not GPU throughput — do not market as tokens/sec until a real-weight CUDA run.

---

## Execution order
1. P0.1 (normalize_db_url) + P0.2 (unconditional pragmas) — unblock DB startup
   & concurrency. Verify `demo_shop` starts + sustains load.
2. P0.3 (`"status"` envelope) — correctness blocker.
3. P1.1 (body_schema) — DX correctness.
4. P1.2–P1.4 — test/dev hygiene.
5. P2.1–P2.2 — honest benchmark vs FastAPI/Robyn.
6. P3.1 — PyPI publish.

## Definition of Done (production-ready)
- [ ] `demo_shop` 23-endpoint app starts against SQLite/Postgres and sustains
      concurrent load with p50 in low-ms (D1/D3/D4 closed).
- [ ] `{"status": ...}` payloads return intact (BUG-1 fixed + test).
- [ ] `body_schema` routes receive parsed data, not raw bytes (BUG-3 fixed).
- [ ] `pytest` 100% green; `cargo test --workspace` + clippy `-D warnings` clean;
      `cargo fmt --check` enforceable.
- [ ] Published real DB-backed CRUD benchmark vs FastAPI+SQLAlchemy + Robyn in
      BENCHMARKS.md.
- [ ] Wheel published to PyPI (manylinux_2_28).
