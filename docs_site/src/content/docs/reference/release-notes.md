---
title: Release Notes
description: Version history and changelog for JustAPI.
---

## 2.0.0 (2026-07-24)

### Breaking Changes

- `FastAPI` → `JustAPIApp` rename
- Server startup: `app.run()` replaces uvicorn
- Middleware response manipulation uses tuple-based headers

### New Features

- Multi-database project scaffolder (`justapi create`)
- Multi-protocol support (REST, GraphQL, gRPC, JSON-RPC)
- Native MCP tool registration (`@app.tool`)
- Validated streaming output (`@app.stream_json`)
- Rust-backed agent session store
- OpenTelemetry Jaeger stack generator
- Multi-arch PyPI wheels (Linux, macOS, Windows, ARM64)
- Load-based auto-scaling (`justapi serve --scale`)
- Unix domain socket support

### Performance

- 701,234 RPS hello-world (20x FastAPI)
- 724,038 RPS native fast path
- 456,168 RPS native DB query path
- Sub-0.2 ms p99 latency

### Security

- OWASP Top 10 compliance (A01-A10 coverage)
- 7 cargo-fuzz targets in CI
- Rustls TLS (TLS 1.2/1.3 only)
- Secure-by-default CORS + security headers

## 1.0.0 (2026-07-15)

Initial release with core routing, middleware, database support, and FastAPI compatibility.

## See Also

- [ADR Index](/reference/adr-index/) — Architecture Decision Records
- [Changelog](https://github.com/swadhinbiswas/JustAPI/releases)
- [Roadmap](https://github.com/swadhinbiswas/JustAPI/blob/main/ROADMAP.md)
