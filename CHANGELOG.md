# Changelog

All notable changes to JustAPI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

**FastAPI parity (Phase 53)**
- **Real-weights inference support (first real run, ADR-098):** `Engine::encode/decode/generate_text` — real models are now drivable from text; GGUF SentencePiece tokenizers are built correctly (Metaspace, `unk_token`, `fuse_unk`); `quantized_llama` logits are rank-normalized for the sampler; generation errors are surfaced instead of silently closing the stream. Verified: TinyLlama-1.1B Q4 on CPU — 22.1 tok/s, TTFT 0.14 s, fluent output (`JUSTAPI_REAL_MODEL_DIR=... cargo test -p justapi-inference --features real --test real_weights -- --ignored --nocapture`).
- **`justapi serve --workers N`** — Python-app prefork: N workers share the address via `SO_REUSEPORT`, dead workers respawn, SIGTERM/SIGINT drain cleanly. **`justapi serve --reload`** — watches `*.py` files and restarts the app on change (combinable: `--reload --workers 4`).
- **`app.enable_timeout(seconds)`** — per-request timeout (504) via the core `TimeoutMiddleware`, on both the real server and the TestClient chain.
- `@app.exception_handler(ExcClass)` decorator (FastAPI form) alongside `add_exception_handler`; `RequestValidationError.errors()` works as a method (FastAPI) *and* as an attribute (historical JustAPI) — `exc.errors` and `exc.errors()` both return the list.
- Envelope response bodies may be dicts/lists: `{"status": 418, "body": {...}}` now serializes the body as JSON instead of a Python repr.
- `Param` constraints (`gt/ge/lt/le/multiple_of/min_length/max_length/pattern`) enforced as 422 for path/query/header/cookie params; `Header(convert_underscores=...)`; bool / int / float / Enum / `List[T]` coercion (sync + async handlers). Param metadata (`title`, `description`, `deprecated`, `include_in_schema`) now works and is emitted in OpenAPI (`multipleOf`, `title`, parameter `description`/`deprecated`; hidden params still function at runtime).
- `JustAPIApp(lifespan=...)` — async or sync context manager entered before the server starts and exited on shutdown (`on_event` still supported).
- Auth schemes: `HTTPBearer`, `HTTPBasic`, `APIKeyHeader`, `APIKeyQuery`, `APIKeyCookie` as `Depends` callables, with OpenAPI security schemes.
- Multi-file uploads: repeated multipart field names yield a list; `List[UploadFile]` handlers receive every file.
- `JustAPIApp.set_jwt_auth(..., audience=[...], issuer=...)` — `aud`/`iss` are now enforced (401) instead of warn-only.
- TestClient: `get_with`/`put_with`/`patch_with`/`delete_with`/`options_with` headers, a persistent cookie jar (`cookies()`, `clear_cookies()`), the full native middleware pipeline (CORS, secure headers, JWT) and JWT claims via `request.auth`.
- OpenAPI: real parameter types + constraints, `enum` values, cookie parameters, `components.securitySchemes` + per-operation `security`, real `requestBody` from `Schema`/Pydantic models, and `info.servers/tags/contact/license/termsOfService` emission.
- `justapi create` scaffolds now enable secure headers by default.

### Fixed

- **Pydantic body models work now (the flagship FastAPI pattern).** `def create_item(item: Item)` previously returned `422 field required` — the repository's own `demo_app.py` was broken. Plain model params, `Body(..., embed=True)`, and `Form(...)` with Pydantic models (multipart *and* urlencoded) all parse into validated model instances now, with `422` + field details on validation errors.
- **`Query(...)` / `Body(...)` required params no longer leak `Ellipsis`.** A missing required parameter returned `200` with the literal `Ellipsis` as its value; it now returns `422 field required`.
- **`justapi serve` served nothing** — the Rust CLI never registered its default routes (every request, including `/live`, returned 404). Default routes now include `/hello`, `/echo`, `/health`, `/live`, `/ready`, `/metrics`, `/openapi.json`, `/events` and GraphQL; the CLI also gained a `ws` feature so the WebSocket echo route works.
- **Python CLI `justapi serve`** no longer delegates to the Rust binary (which cannot load Python apps and would serve 404s); it always runs your `app.py`/`main.py` and now accepts `--addr`. The Docker image installs the Rust CLI as `justapi-cli` so the pip console script is not clobbered.
- **CORS + JWT together:** OPTIONS preflight is no longer rejected with `401` (browsers send no credentials on preflight), and auth-error responses (`401`) now carry `Access-Control-*` headers so browsers can read them. CORS now runs outermost in the native middleware chain.
- **Middleware ordering now matches Starlette/FastAPI (ADR-095):** the **last** middleware added is the outermost (it sees the request first). JustAPI previously ran first-added-outermost — the opposite of FastAPI. Apps that relied on the old order should reverse their `@app.middleware("http")` registrations.
- `FileResponse` sets `ETag`, `Last-Modified`, and `Content-Length` (honouring a caller `stat_result`).
- SSE/WebSocket routes registered via `APIRouter` keep their prefix, tags, summary, and OpenAPI entry through `include_router`.
- Docker: the image now uses `ENTRYPOINT ["justapi"]` + `CMD ["serve", "--addr", "0.0.0.0:8080"]` with the demo mounted as `/app/main.py`, so Kubernetes/Helm `args` (`serve --addr ...`) work end to end — previously they tried to exec a nonexistent `serve` binary. Mount your app as `/app/main.py`, or override the entrypoint to run it directly.

### Security

- Dropped `rustls-pemfile` (RUSTSEC-2025-0134) in favour of `rustls-pki-types` PEM loaders.
- Bumped `h2` → 0.4.20 (RUSTSEC-2026-0258), `rustls` → 0.23.45 (RUSTSEC-2026-0285) and `wasmtime` → 49.0.2 (five RUSTSEC-2026 advisories: fuel accounting, GC heap corruption, stack overflow in async-lifted callbacks). `cargo deny check` is green again — no ignored advisories remain.
- Trivy CI job (filesystem scan: vulnerabilities, secrets, misconfigurations; HIGH/CRITICAL, fixed-only) + CycloneDX SBOM artifact; pinned Trivy binary verified by SHA256 (the `trivy-action` was compromised in 2026).
- Patched the docs-site lockfile (1 CRITICAL + 13 HIGH npm advisories).

### CI

- **Free-threaded (3.14t) tests are now a real gate.** The job previously swallowed every failure with `|| true`; it now fails on any `FAILED`/`ERROR` and only tolerates the known interpreter-teardown segfault after a clean pass. The previously skipped websocket/GraphQL/hot-reload/max-body-size files now run on free-threaded (verified: 212 passed locally on 3.14t).
- **Integration tests re-enabled.** The job was disabled after always failing (it curled a nonexistent `/` route and `justapi serve` found no app in CI). It now boots the Rust server, waits for readiness with retries, and exercises health, metrics, OpenAPI, echo and a real WebSocket round-trip.
- Per-PR short fuzz gate (`fuzz_body`, `fuzz_headers`, `fuzz_jwt`).
- p99 A/B latency workflow (warn-only until 2026-10-23).

## [2.0.10] - 2026-08-08

### Fixed

- The `justapi` CLI binary is now shipped inside the PyPI wheel — `pip install justapi` previously left the `justapi` command broken.

## [2.0.9] - 2026-08-08

### Added

**Native async DB awaits — the headliner (ADR-093)**
- `app.db.query_async(sql, params)` / `app.db.execute_async(...)` — awaitable DB operations that run on the DB's own multi-threaded tokio runtime with the GIL released for the whole execution. The asyncio loop is never blocked, so slow queries (Postgres, network DBs) no longer serialize other requests.
- **Measured: 53× faster than the blocking path on slow queries** (320 vs 6 RPS, 2M-row CTE benchmark).

**Rust-native SSE streaming (ADR-088)**
- `app.sse_native(path, count=10, interval_ms=100)` — events generated entirely in Rust (tokio + mpsc + stream). Zero Python, zero GIL, zero pump per event.

**`@native_async` decorator (ADR-089)**
- Marks async handlers for the fastest dispatch path; on free-threaded CPython (3.14t) the dispatch runs in true parallel (available_parallelism workers, ADR-087).

**Callback-driven async resolution (ADR-086)**
- `_DoneNotifier` fires the instant a coroutine completes — the async path no longer blocks a thread or polls; one less thread hop per async request.

**Type-checked DX**
- `py.typed` marker + fully-typed `.pyi` stubs — mypy-clean on user code (`app.get`, `app.db`, `native_async`, `sse_native`, `query_async` all typed). Fixed 200+ pre-existing stub errors (implicit-Optional, missing imports, duplicate defs).

**Scaffold demos the differentiators**
- `justapi create` now generates a CRUD project that exercises `query_async`, `@native_async`, and Rust-native SSE out of the box.

**Multi-worker prefork — verified with data**
- `justapi serve --workers N --scale`: 4 workers = **1.88× throughput** (99.7k RPS vs 53k, keep-alive static), auto-scale respawns verified.

**Modern README with animated SVG diagrams**
- Animated hero (matrix rain + metrics), request-pipeline diagram, benchmark chart, feature matrix — all SMIL-only (GitHub-safe, no scripts).

### CI & Release Infrastructure
- **OIDC trusted publishing** — `wheels.yml` publishes to PyPI with zero tokens (GitHub → PyPI trusted publisher, environment `pypi`). Tag push = build → publish → GitHub Release.
- **9-platform wheel matrix green end-to-end** — manylinux/musllinux × x86_64/aarch64, macOS arm64 + x86_64 (zig cross), Windows x64, free-threaded 3.14t. Every row builds, tests, and uploads.
- **Windows build fixed** — Unix-only server APIs (`run_on_uds`, `serve_unix`, `bind_unix_listener`, UDS test) are now `#[cfg(unix)]`-gated; the crate compiles clean on Windows.
- **CI hardening** — replaced PyO3/maturin-action (unsupported inputs, broken venv dance) with direct maturin + `--zig`; zig downloaded from ziglang.org (setup-zig mirrors were down); fixed GitHub bool-vs-string matrix gotchas; cross-platform wheel smoke tests.
- **Repo hygiene** — 160 MB of wheel artifacts + debug logs purged from git history (`.git` 225 MB → 61 MB).

### Changed
- `pyproject.toml` project URLs corrected to `swadhinbiswas/JustAPI`.

### Experiments recorded (no code shipped)
- ADR-090 — native-awaitables experiment: every bridge/driver approach measured, none beats the asyncio loop; HTTP A/B shows justapi already beats Granian on real async workloads (1ms-sleep: 3.9k vs 2.9k RPS).
- ADR-091 — multiplexing Rust coroutine driver: 29× slower per-await than asyncio (16µs vs 0.56µs); asyncio's stepping is the C-API floor.
- ADR-092 — multi-loop async dispatch A/B: neutral-to-worse (GIL worker is the single dispatch point); reverted, `JUSTAPI_ASYNC_LOOPS` kept for experiments.

### Fixed
- **Unix-only server APIs now cfg-gated** (Windows compile fix — see CI section).

## [2.0.8] - 2026-08-06

### Security Fixes
- **CRITICAL:** Fixed global panic hook race condition in `panic.rs` (P0-1)
- **CRITICAL:** Fixed RwLock poisoning panics in `PerRouteCircuitBreakerMiddleware` (P0-2)
- **CRITICAL:** Fixed division by zero in rate limiter (P0-3)
- **HIGH:** Added SQL identifier validation for CRUD table/column names (H-1)
- **HIGH:** Fixed WebSocket unwrap on hot path (H-2)
- **HIGH:** GraphiQL now gated on `JUSTAPI_ENABLE_GRAPHIQL` env var (H-3)
- **HIGH:** Added 5-second per-check timeout in health checks (H-5)
- **MEDIUM:** Added Mutex poison recovery to Python bindings (7 files) (M-5)
- **MEDIUM:** Fixed ChaosMiddleware potential panic on misconfigured latency (M-6)
- **MEDIUM:** Added `nosniff` header to large file streaming path (M-7)

### Added
- **HTTP/3 (QUIC) transport** — `app.enable_http3(cert_path, key_path)` serves the app over HTTP/3 on the same port as TCP (feature `http3`; h3 + h3-quinn + quinn). Python handlers, DI, schema validation, and the GIL pool all work over QUIC (ADR-079, ADR-082)
- **`Security` dependency marker** — `Security(dep, scopes=...)` is `Depends` with OAuth2-scope metadata (FastAPI parity); top-level re-exports of `JwtAuth`, `OAuth2PasswordBearer`, `OAuth2PasswordRequestForm`, `OAuth2PasswordRequestFormStrict` from `justapi.auth`
- **`JustAPP` alias** — `JustAPP()` is the same class as `JustAPIApp()` (also `JustAPI`)
- **QUERY method (RFC 10008) testability** — `TestClient::query`/`query_with`, `JustAPITestClient.query`/`query_with`, `AsyncTestClient.query` (RFC requires Content-Type; enforced with 400)
- **`__version__`** — `justapi.__version__` now exposed (2.0.8)
- **`scripts/sanitize.sh`** — fast memory-safety gate: ASan on the full core suite + targeted Miri on the only `unsafe` module (`memory.rs`); replaces the 20+ min full-suite Miri run
- **4 portable wheels** — manylinux + musllinux × x86_64 + aarch64 via `maturin --zig`; `scripts/publish.sh` builds/verifies/upload all

### Fixed
- **CRITICAL: Python-handler write-path collapse** — concurrent writes dropped to ~3 RPS at c=11 (was 150 at c=10): the request-scoped auto-transaction double-acquired pool connections (`2N` for `N` writes). Removed; SQLite BUSY/LOCK now maps to 503 Retry-After. Writes now flat ~150 RPS c=1..50 (ADR-080)
- **CRITICAL: GIL pool not fork-safe** — a forked child inherited the parent's initialized pool whose worker threads don't exist → every Python request hung (504s). Pool now tracks its PID and rebuilds after `fork(2)`; fixes prefork deployments and the long-standing `test_circuit_breaker` flake (ADR-081)
- **DI awaitedness bug** — `Depends`/`Security` on async callable *instances* (e.g. `OAuth2PasswordBearer`) were invoked without awaiting (unawaited coroutines leaked); `_is_async_callable` now checks `__call__`
- **Default-feature test gate** — `db_bridge`/`edge_cases` integration tests now carry proper `#![cfg(feature = ...)]` guards; `cargo test --workspace` passes on default features
- Added GraphQL query depth/complexity limits (depth=10, complexity=200)
- Added 4 MiB gRPC message size limit
- Added 1024-route capacity for circuit breakers with FIFO eviction
- Added symlink resolution in static file serving via `canonicalize()`
- Added `x-content-type-options: nosniff` header to all static file responses
- Fixed poisoned mutex cascading panics in request coalescer
- Redis dependency now feature-gated behind `redis-rate-limit`

### Performance
- Router FIFO cache eviction now O(1) with `VecDeque`
- Accept-Encoding now respects RFC 7231 quality values
- Health checks now run in parallel
- Metrics no longer misclassify 1xx status codes
- Environment variables cached with `LazyLock`
- CRUD query params now URL-decoded

### Changed
- **PLAN.md honesty pass** — phase table distinguishes ✅ verified vs 🟡 implemented/unverified (all AI/inference phases marked unverified: MockModel-only, no GPU/real weights); Phase 52 stays frozen
- **Docs fabrication sweep** — removed false APIs from docs (`justapi.middleware`, `PyScheduler`, `enable_rate_limiter`, `enable_compression`, `fastapi.security` imports); rewrote security tutorials, scheduler API, performance-tuning with the real API; added HTTP/3 deployment guide
- **README comparison table** — now states the honest breakdown (700k native fast path / 120k Python / 180k DB SELECT) instead of a single number
- **Release infra** — version bumped to 2.0.8 everywhere; publish consolidated to `wheels.yml` (7-platform matrix, sole PyPI publisher); duplicate publish jobs disabled; classifier set to "4 - Beta"
- Server code partially split: `ws.rs`, `sse_ws.rs`, `handler_exec.rs` extracted
- Added `#[non_exhaustive]` to public enums for forward compatibility
- ANSI injection prevention in diagnostics
- Implemented `GatewayState::get_route()` (was a stub returning `None`)

### CI/CD
- Memory-safety gate now `scripts/sanitize.sh` (ASan + targeted Miri) instead of the 20-min full-suite Miri run
- Added `--tests` flag to clippy in CI
- Added `--all-features` to cargo test in CI
- Added `fuzz_pipeline`, `fuzz_websocket`, `fuzz_grpc` targets to fuzz workflow
- Fuzzing duration increased from 60s to 5 minutes per target
- Added benchmark regression gate to CI (95% threshold)
- Added integration test job (server startup, health checks, endpoints)
- Added PyPI publish workflow with OIDC trusted publishers

### Documentation
- Created troubleshooting guide (10 common issues)
- Created performance tuning guide (10 optimization areas)
- Created migration guide from Robyn/Granian
- Created API stability guarantees document
- All docs integrated into docs_site

### Testing
- 6 new edge case tests for SQL validation, health timeouts, chaos config
- 2 new fuzz targets (WebSocket, gRPC)
- New regression tests: write-path collapse, fork-safety, async-callable DI, `JustAPP` alias, HTTP/3 e2e
- 521+ total tests passing (170 Python, workspace suites green)

## [Unreleased]

### Added
- `PerRouteRateLimiter` middleware with prefix matching, method filtering, and optional fallback (`middleware.rs`)
- `Server::add_per_route_rate_limit()` builder method
- `CoreError` typed error enum with 12 variants, `thiserror`-derived (`error.rs`)
- `CoreError::to_problem_json()` for RFC 9457 Problem Details rendering
- `CoreError::into_response()` for direct HTTP response building
- `CoreError::status_code()` for variant-to-HTTP-status mapping
- `Server::with_shutdown_timeout(Duration)` builder method (configurable drain timeout, default 30s)
- 15 new crash-prevention and feature tests

### Changed
- **BREAKING:** All error responses now follow RFC 9457 Problem Details format (`application/problem+json`) instead of `{"detail":"..."}` or `{"error":"..."}`. Clients matching on `detail` field will still work; clients matching on `error` field must update.
- `error_response()` returns `{type, title, status, detail}` instead of `{detail}`
- `service_unavailable_response()` returns `{type, title, status, detail}` with `application/problem+json` content type
- `json_error()` in middleware returns `{type, title, status, detail}` instead of `{error}`
- `Handler::Static` dispatch uses `error_response()` for non-2xx status codes
- 40+ `unwrap()` calls on request hot paths replaced with safe error handling
- Mutex locks in `metrics.rs` and `tracing_setup.rs` now recover from poisoning
- Email format validator handles missing `@` without panicking
- `openai.rs` status code conversion uses `safe_status()` fallback

### Fixed
- User-triggerable server crash via email validation without `@` character (`validate.rs:33`, `app.rs:244`)
- Potential panic from `Bound::new()` failure on every request (`handlers.rs:176`)
- Potential panic from `CString::new()` and `py.eval()` in `fast_dumps` (`handlers.rs:335-336`)
- Mutex poisoning causing panic on `/metrics` endpoint (`metrics.rs:153,278`)
- Clock skew panic in Redis rate limiter (`rate_limit.rs:78`)
- Clock skew panic in mail verification tokens (`mail/otp.rs`, `mail/verify.rs`)

## [2.0.7] - 2026-07-25

### Added
- Generator dependencies (FastAPI-style `yield` in `Depends`)
- ORJSON as optional Rust-level serializer (`orjson` feature)
- Production panic hotspot audit — fixed 6 categories of `unwrap()` panics
- Scalar API Reference documentation page (156 pages total)

### Changed
- Demo shop stress test — all D1–D4 defects fixed, 29/29 assertions pass

## [2.0.0] - 2026-07-03

### Added
- Initial release of JustAPI 2.0 "Singularity"
- Rust core (`justapi-core`): HTTP/1.1+HTTP/2, routing, middleware, compression, WebSocket, SSE, TLS, database ORM, OpenAPI, GraphQL, gRPC, WASM middleware
- Python bindings (`justapi-py`): FastAPI-compatible decorator API, DI, controllers, TestClient
- CLI (`justapi-cli`): serve, db migrate/rollback, routes, doctor, gen, new, create, profile
- Inference engine (`justapi-inference`): KV cache, RadixAttention, continuous batching, speculative decoding, OpenAI-compatible API
- 493+ tests, 8 fuzz targets, CI with Miri
