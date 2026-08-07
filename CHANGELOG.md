# Changelog

All notable changes to JustAPI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.8] - 2026-08-06

### Added (2026-08-07)
- **Native async DB awaits** — `app.db.query_async(sql, params)` / `app.db.execute_async(...)`: awaitable DB ops on the DB's own tokio runtime, GIL released, loop never blocked. **53× faster than the blocking path on slow queries** (320 vs 6 RPS, ADR-093)
- **Multi-worker prefork verified** — `justapi serve --workers N --scale`: 4 workers = 1.88× throughput (99.7k RPS), auto-scale respawns (benchmarked 2026-08-07)
- **Type-checked DX** — `py.typed` marker + mypy-clean `.pyi` stubs (routes, `app.db`, `native_async`, `sse_native` all typed)
- **Scaffold demos differentiators** — `justapi create` generates a CRUD app exercising `query_async`, `@native_async`, and Rust-native SSE
- **Rust-native SSE streaming** — `app.sse_native(path, count, interval_ms)`: events generated entirely in Rust, zero Python per event (ADR-088)
- **`@native_async` decorator** — marks async handlers for the fastest dispatch; true parallel dispatch on free-threaded CPython (ADR-089)
- **Callback-driven async resolution** — `_DoneNotifier` fires on coroutine completion; no blocking wait, no thread hop (ADR-086)

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
