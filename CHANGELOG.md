# Changelog

All notable changes to JustAPI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
