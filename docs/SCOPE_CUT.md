# Scope-Cut Proposal — testable core, optional extras

> Status: **partially implemented (2026-08-06)**. The core-crate side is done;
> the Python-binding split is a planned phase with a documented blocker.

## Implementation status (2026-08-06)

### Done

1. **`justapi-core` heavy features were already optional** — `db`, `ws`,
   `wasm`, `graphql`, `grpc`, `inference`, `mail`, `redis-rate-limit`, `tls`,
   `http3`, `orjson`, `opentelemetry` are all feature-gated. `default =
   ["opentelemetry"]`.
2. **Default-feature test gate fixed** — `db_bridge.rs`/`edge_cases.rs`
   integration tests now carry `#![cfg(feature = ...)]` guards;
   `cargo test --workspace` is green on default features (was broken).
3. **CI split is the documented pattern** — fast default-features gate +
   `--all-features` matrix (see PLAN.md gates).
4. **Honest wheel story** — the Python wheel is full-featured by design
   (verified: `justapi-core` features `db, ws, wasm, graphql, grpc` are what
   the Python API exposes — `set_database`, `websocket`, `graphql`,
   `add_grpc_service`).

### Deferred (blocked, needs a focused session)

**Python-binding `lean` feature.** Making `justapi-py` build without
`db/ws/wasm/graphql/grpc` requires feature-guarding ~50+ symbols across
`native/app.rs`, `websocket.rs`, `database.rs`, `test_client.rs`, `handlers.rs`
plus the Python glue (`app.py`, `__init__.py`, `.pyi` stubs). Verified
experimentally: removing the core features breaks the build in 20+ places.
Payoff is build-time only (the product bundles everything by design), so it is
lower priority than the runtime correctness work (ADR-080, ADR-081). Tracked
here so it is not lost.

---

## The original problem

`justapi-py` unconditionally enables `justapi-core` features
`db, ws, wasm, graphql, grpc` (plus `inference`/`mail`/`redis-rate-limit`
optionally). Consequences:

- **The core test gate is coupled to every feature.** `cargo test --workspace`
  compiles sqlx+postgres/mysql, wasmtime, async-graphql, tonic+prost, and the
  whole inference stack whether or not the user uses them. A clean-machine
  "does the framework work?" check is impossible.
- **The wheel is heavy and untested in its default form.** The default install
  carries wasmtime/grpc/graphql compile time and binary size even for a
  hello-world app.
- **"Done" is not verifiable.** PLAN.md lists 50+ phases complete, but the
  default build never exercises them in isolation — regressions hide behind the
  feature soup (the `db_bridge`/`edge_cases` integration tests already fail
  without `--all-features`).

## The proposal: three tiers

### Tier 1 — `justapi-core` (always compiled, always tested)

The transport + framework core. **No feature-gated deps in the default path:**

- tokio, hyper, hyper-util, http-body-util, matchit, serde_json, jsonschema
- middleware (CORS/JWT/rate-limit/security headers/compression), router,
  serialize, validate, metrics, health, server, testing, openapi, static_files
- Existing flags stay, but `default = []` (drop `opentelemetry` from default;
  OTel becomes opt-in — it currently drags grpc-tonic into every build via
  `opentelemetry-otlp`).

**Exit criterion:** `cargo test -p justapi-core` (default features) is green in
under ~60s on a clean machine and exercises routing, middleware, serialization,
validation, static files, health, test client.

### Tier 2 — `justapi-py` (the pip package)

- `default = ["mail"]` today → make `default = []`; map every Python feature to
  the corresponding core feature flag so users install exactly what they use:
  `pip install justapi[db]`, `justapi[wasm]`, `justapi[grpc]`, `justapi[graphql]`,
  `justapi[inference]`, `justapi[http3]`, `justapi[otlp]`, `justapi[all]`.
- Python glue that touches a disabled feature must degrade gracefully at import
  time (try/except around the native submodule), not hard-fail.

**Exit criterion:** `pip install justapi` in a fresh venv → `justapi serve`
hello-world works with **no** wasmtime/grpc/graphql installed; each extra
installs only its own transitive deps.

### Tier 3 — optional feature flags to keep (already correct pattern)

Keep the existing per-feature gates and simply stop forcing them all on:

| Feature | Core flag | Why gated |
|---|---|---|
| WebSocket | `ws` | tokio-tungstenite |
| TLS | `tls` | rustls stack |
| HTTP/3 | `http3` | quinn+h3 (new, ADR-079) |
| DB | `db` | sqlx + 4 DB drivers |
| Inference | `inference` | candle |
| WASM | `wasm` | wasmtime |
| GraphQL | `graphql` | async-graphql |
| gRPC | `grpc` | tonic+prost |
| Mail | `mail` | lettre+minijinja |
| Redis rate-limit | `redis-rate-limit` | redis |
| OTel | `opentelemetry` | otlp+grpc-tonic |
| ORJSON | `orjson` | orjson binding |

## What moves where

1. **`justapi-py/Cargo.toml`:** drop `features = ["db","ws","wasm","graphql","grpc"]`
   from the `justapi-core` dependency; re-enable via `[features]` passthrough
   with `dep:` forwarding. Python `extras` in `pyproject.toml` mirror the
   feature names.
2. **`justapi-core/Cargo.toml`:** `default = []` (OTel opt-in). Add a
   `full` convenience feature = `["tls","ws","db","mail","compression","brotli-compression","zstd-compression","wasm","graphql","grpc","http3","redis-rate-limit","opentelemetry"]` for the benchmark harness and CI matrix.
3. **Fix the integration tests** (`db_bridge`, `edge_cases`) so they carry the
   right `#![cfg(feature = ...)]` guards and the default-feature gate is green
   alone; run `--all-features` only in the full matrix.
4. **CI:** two jobs — `default-features` (fast, the "does it work" gate) and
   `all-features` (the "everything still compiles" gate). BENCHMARKS records
   which feature set each number was measured with.

## Honest tradeoffs

- **Compile time for power users:** `justapi[all]` users still pull everything —
  but that's their explicit choice, not the default tax.
- **Python import behavior:** currently `from justapi import WebSocket` works
  because ws is always on. After the cut, importing a feature's symbol without
  the extra installed must raise a clear `ImportError` with an install hint
  (`pip install justapi[ws]`), not a confusing `AttributeError` from a missing
  native submodule.
- **This is a packaging/CI change, not a feature change.** It changes *when*
  things compile and *what the default is*, not what exists. Risk is low; the
  payoff is a verifiable core.

## Suggested order

1. `justapi-core` default-features-to-empty + `full` convenience flag + CI split
   (verify the fast gate).
2. `justapi-py` feature passthrough + `pyproject.toml` extras + import-hint
   degradation.
3. Fix the two integration-test guards; record a clean-machine default-feature
   run in BENCHMARKS.
4. Ship 2.0.8 with the default wheel being the lean, tested core.
