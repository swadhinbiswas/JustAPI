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

### P0.1 — [DONE] BUG-2a: `normalize_db_url` breaks SQLite path resolution
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

### P0.2 — [DONE] BUG-2b / D3: SQLite has zero concurrency protection by default
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

### P0.3 — [DONE] BUG-1: top-level `"status"` key empties the response body
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

### P1.1 — [DONE] BUG-3: `body_schema=` delivers raw `bytes`, not a Schema instance
Handlers must re-parse `request.json()` manually (demo_shop workaround). The
native fast path already validates via the schema; the *Python handler* path
should receive a validated, instantiated object (or at least the parsed dict),
not raw bytes. Decide: pass parsed dict when `body_schema` is a `Schema`/pydantic
model; keep raw bytes only for legacy callables. Add test asserting handler
receives a dict (or model) for a `Schema`-registered route.

### P1.2 — [DONE] `test_graphql.py` 404s (`graphql()` builder implemented)
Added `app.graphql()` route builder method and `_builtin_graphql` wrapper in `app.py` backed by `justapi_core::graphql`. `test_graphql.py` passes 200 OK for GraphiQL UI and query execution.

### P1.3 — [DONE] Test suite 100% green
Updated route handling in `_JustAPIApp` to support route overriding so user routes can override built-in routes cleanly. Full `pytest` integration test suite passes 100% green (137 passed, 1 skipped).

### P1.4 — [DONE] `cargo fmt --check` gate drift
Standardized `rustfmt.toml` without nightly-only flags. Both `cargo fmt --check` and `cargo clippy --workspace --tests -- -D warnings` pass clean.


---

## P2 — Honest benchmarking (close the "real life" gap)

### P2.1 — [DONE] Real DB-backed CRUD benchmark vs FastAPI+SQLAlchemy and Robyn
The micro-benchmarks (echo/validate) overstate production reality by ~1000× on
DB paths. Added `benchmarks/crud_justapi.py` (Python-handler + Rust-native),
`crud_fastapi.py` (FastAPI+SQLAlchemy async), `crud_robyn.py` (Robyn+sqlite3),
and `bench_one.py` (oha JSON driver). Ran `oha -c 10 -z 5s` against a SQLite
file (WAL, busy_timeout=5000, 10-conn pool). Results appended to BENCHMARKS.md
(SELECT is solid & shows JustAPI/Robyn >> FastAPI; writes are SQLite-bound).
**The benchmark surfaced a blocking defect: JustAPI's Python-handler DB write
path does not await/commit writes under concurrency (INSERT flies at 362k RPS,
UPDATE/DELETE collapse to ~5–15 RPS).** Tracked as P2.2.

### P2.2 — [DONE] Fix Python-handler DB write path (durability under concurrency)
The CRUD benchmark exposed that `crates/justapi-py/src/database.rs` `query()`
(used for writes) ran via `py.detach` + `rt.block_on`, which **deadlocks the DB
runtime** from the server's dispatch thread — the connection pool exhausts
(`busy_timeout` hit) and writes silently drop (1–2 of ~50 INSERTs persisted at
`-c 50`). Fixed in `run_blocking()`: release the GIL with `py.detach` but run the
future via `rt.spawn` + an `mpsc` channel wait (no re-entrant `block_on`). Now
100 concurrent INSERTs via the test client persist 100/100, and a 100-thread
direct `app.db.execute` burst persists 200/200 with zero errors. Durability test
added in `test_db_concurrent.py`. The Rust-native CRUD path remains the fastest
write route. See ADR-074 (revised) + BENCHMARKS.md P2.2.

---

## P3 — Distribution

### P3.1 — PyPI publish
Portable `manylinux_2_28` wheel is built and verified:
`target/wheels/justapi-2.0.0-cp311-abi3-manylinux_2_28_x86_64.whl`
(built inside `quay.io/pypa/manylinux_2_28_x86_64` via `maturin build
--release`). `twine check` PASSED; the wheel installs + imports cleanly in a
fresh venv. **Remaining:** `twine upload` with `MATURIN_PYPI_TOKEN` (no token
present in this environment — blocked on credentials). Also consider building
`musllinux` + `aarch64` wheels for full coverage before publishing.

### P3.2 — Unfreeze GPU/inference (Phase 52)
Keep frozen until 2.0.8 per ADR-067. Structural benchmarks are topology proofs,
not GPU throughput — do not market as tokens/sec until a real-weight CUDA run.

---

## P4 — Built-in validator hardening (Gap #3)

### P4.1 — [DONE] `Schema` model + `Field` constraints + nested `$ref`/`$defs`
`justapi.Schema` now emits a real JSON Schema: `Field` supports
`default, gt, ge, lt, le, min_length, max_length, regex, format, enum,
description`; nested `Schema` fields (incl. `list[OtherSchema]`) are lifted into
top-level `$defs` and referenced via `$ref`. Validation runs in Rust (jsonschema
0.46, `default-features=false`) with zero Python round-trips on the hot path.
Direct `validate_value` enforced `format` from the start (pyo3-side format
registry in `crates/justapi-py/src/native/app.rs`).

### P4.2 — [DONE] Fix server-path `format` enforcement (regression)
**Root cause:** request-body validation on the live HTTP path uses
`justapi_core::validate::CompiledValidator` (`handlers.rs` fast path), which
built validators with plain `jsonschema::options().build(...)` and **never
registered the `format` checkers**. So `format: "email"` violations were silently
accepted (200) at the server boundary, even though the in-process `validate_value`
path rejected them (422). The fix moves the format registry into `justapi-core`
(`validate.rs` → `build_validator`, registering email/uri/uuid/date-time/date/
hostname/ipv4 + `should_validate_formats(true)`), making core the single source
of truth for both one-shot and precompiled validation. Verified end-to-end: a
`Schema`-validated route now returns 422 for `email:"nope"` and for a nested
`zip:"abc"` over HTTP, while a valid payload returns 201. Regression test added
in `test_schema_hardened.py::test_server_path_enforces_format`.

## P5 — DB pool saturation fast-fail (Gap #2)

### P5.1 — [DONE] Pooled-connection acquire now fails fast under saturation
**Why:** the default pool `acquire_timeout` is 30 s. Under load (DB pool smaller
than peak concurrency, e.g. `max_connections=1` behind a burst, or a slow
transaction pinning the only connection), every blocked request would queue for
up to 30 s, stacking timeouts and delaying recovery. A saturated pool should
degrade by rejecting new work immediately (503 + `Retry-After`), not by waiting.

**What changed:**
- `crates/justapi-core/src/db/pool.rs`: added `DatabaseConfig.request_acquire_timeout`
  (default 3 s). `AnyPool::connect` sets the sqlx `acquire_timeout` to
  `request_acquire_timeout.or(acquire_timeout).unwrap_or(3s)`. Added
  `AnyPool::begin_request() -> Result<Transaction, DbAcquireError>` and a typed
  `DbAcquireError` (`TimedOut` / `PoolClosed` / `Other`) with `From<sqlx::Error>`.
- `crates/justapi-core/src/lib.rs`: `service_unavailable_response()` (503 +
  `Retry-After: 1`) and `db_error_response(err)` (503 on `TimedOut`/`PoolClosed`,
  else 500).
- `crates/justapi-py/src/native/handlers.rs`: both write-path handlers call
  `pool.begin_request()` and map saturation → 503 via `db_acquire_error_response()`.
- `crates/justapi-core/src/server/crud.rs`: all 4 DB error arms return
  `db_error_response(&e)` (503 on saturation).
- `crates/justapi-py/src/database.rs`: `Database.request_acquire_timeout` is now a
  plain `f64` (default 3.0) and `app.set_database(db, request_acquire_timeout=...)`
  propagates it.

**FFI gotcha (record so we don't relearn it):** a `Database` instance crossing
the FFI to Rust hits a pyo3 reconstruction quirk — `Option<f64>` fields come back
as `None`/defaults and even `#[pyclass]` `self` field reads via `getattr`/
`call_method0("config_dict")` returned the default `3.0` for
`request_acquire_timeout` (the value passed from Python was `1.0`). The reliable
round-trip is: Python builds the `Database`, then hands `Database.config_dict()`
(a plain `dict`, read in Python where field values are correct) across the FFI,
and the Rust side extracts the `dict`. `config_dict()` is the source of truth for
the dict shape.

**Verified:** `test_db_saturated.py` boots a real HTTP server, one-connection pool,
`request_acquire_timeout=1.0`; a `/slow` handler holds the only connection for 3 s
while a concurrent `/other` DB write returns **503 in ~1.0 s** (not 30 s).

## P6 — [DONE] Untrusted-input fuzzing (Gap #5)
Added `fuzz_pipeline` fuzz target (`fuzz/fuzz_targets/fuzz_pipeline.rs`) testing JSON schema validation, query parsing, and router resolution against untrusted bytes. Documented security rationale in `DECISIONS.md` ADR-075.

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
- [x] `demo_shop` 23-endpoint app starts against SQLite/Postgres and sustains
      concurrent load with p50 in low-ms (D1/D3/D4 closed — 29/29 flow).
- [x] `{"status": ...}` payloads return intact (BUG-1 fixed + test; status-only
      envelopes still set the code).
- [x] `body_schema` routes receive parsed data, not raw bytes (BUG-3 fixed).
- [x] `pytest` 100% green (137 passed, 1 skipped); `cargo test --workspace` + clippy `-D warnings` clean; `cargo fmt --check` clean.
- [x] Published real DB-backed CRUD benchmark vs FastAPI+SQLAlchemy + Robyn in
       BENCHMARKS.md (P2.1). Found + **fixed** a blocking write-path defect (P2.2)
       — the Python-handler DB write path now awaits/commits durably under
       concurrency (`run_blocking` uses `py.detach` + `rt.spawn` + channel wait).
- [x] Built-in `justapi.Schema` validator (P4.1): `Field` constraints + nested
       `$ref`/`$defs` + array-of-model, validated in Rust with zero Python
       round-trips.
- [x] Server-path `format` enforcement fixed (P4.2): `format` keywords are now
       asserted by `justapi_core` for both one-shot and precompiled validation;
       live HTTP routes reject `format` violations (422), not just in-process
       `validate_value`. Regression test in `test_schema_hardened.py`.
- [x] DB pool fast-fail under saturation (P5.1): `request_acquire_timeout` (3 s
       default) turns a saturated pool into immediate 503 + `Retry-After` instead
       of 30 s queue-and-timeout. Verified by `test_db_saturated.py`.
- [x] Untrusted-input fuzzing target (P6): `fuzz_pipeline` target created and compiled in `fuzz/`. Security rationale documented in `DECISIONS.md` ADR-075.
- [ ] Wheel published to PyPI (manylinux_2_28).

