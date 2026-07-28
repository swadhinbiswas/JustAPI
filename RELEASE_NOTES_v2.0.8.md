# JustAPI v2.0.8 Release Notes

## What's New

### Security Fixes (Critical)
- **P0-1:** Fixed global panic hook race condition in `panic.rs`
- **P0-2:** Fixed RwLock poisoning panics in `PerRouteCircuitBreakerMiddleware`
- **P0-3:** Fixed division by zero in rate limiter

### Security Fixes (High)
- **H-1:** Added SQL identifier validation for CRUD table/column names
- **H-2:** Fixed WebSocket unwrap on hot path
- **H-3:** GraphiQL now gated on `JUSTAPI_ENABLE_GRAPHIQL` env var
- **H-4:** Eliminated TLS service_fn duplication (~300 lines)
- **H-5:** Added 5-second per-check timeout in health checks

### Security Fixes (Medium)
- **M-5:** Added Mutex poison recovery to Python bindings (7 files)
- **M-6:** Fixed ChaosMiddleware potential panic on misconfigured latency
- **M-7:** Added `nosniff` header to large file streaming path
- **M-8:** Increased fuzzing duration from 60s to 5min per target
- **M-9:** Docker Compose credentials now via env vars
- **M-10:** Added `.dockerignore` for faster builds

### Bug Fixes
- **P1-1:** Added GraphQL query depth/complexity limits
- **P1-2:** Added 4 MiB gRPC message size limit
- **P1-3:** Added 1024-route capacity for circuit breakers
- **P1-4:** Added symlink resolution in static file serving
- **P1-5:** Added `x-content-type-options: nosniff` header
- **P1-6:** Fixed poisoned mutex cascading panics in coalesce
- **P1-7:** Redis dependency now feature-gated

### Performance Improvements
- **P2-6:** Router FIFO cache eviction now O(1) with VecDeque
- **P2-7:** Accept-Encoding now respects RFC 7231 quality values
- **P2-8:** Health checks now run in parallel
- **P2-9:** Metrics no longer misclassify 1xx status codes
- **P2-10:** Env vars now cached with LazyLock
- **P2-11:** CRUD query params now URL-decoded
- **P2-12:** Gateway config file permissions checked

### Code Quality
- **P2-1:** Server code partially split (2,560 → 2,120 lines)
  - Extracted `ws.rs`, `sse_ws.rs`, `handler_exec.rs`
- **P3-2:** ANSI injection prevention in diagnostics
- **P3-3:** Secret file permission warnings
- **P3-5:** BufferPool max_per_bucket now configurable
- **P3-6:** CookieJar now handles quoted values (RFC 6265)
- **P3-7:** Array schema handling improved
- **CQ-2:** Added `#[non_exhaustive]` to public enums

### CI/CD Improvements
- Added `--tests` flag to clippy in CI
- Added `--all-features` to cargo test in CI
- Added `fuzz_pipeline` target to fuzz workflow
- Added WebSocket and gRPC fuzz targets
- Added benchmark regression gate to CI

### Documentation
- Created troubleshooting guide
- Created performance tuning guide
- Created migration guide from Robyn/Granian
- Created API stability guarantees document
- All docs integrated into docs_site

## Installation

```bash
pip install justapi
```

Or with optional features:
```bash
pip install "justapi[full]"
```

## Upgrade Notes

This release includes breaking changes for security:
- GraphiQL is now disabled by default in production
- WebSocket upgrade now uses defensive error handling
- Health checks now timeout after 5 seconds

## Contributors

Thanks to all contributors who helped make this release possible!

## Full Changelog

See [CHANGELOG.md](CHANGELOG.md) for the complete list of changes.
