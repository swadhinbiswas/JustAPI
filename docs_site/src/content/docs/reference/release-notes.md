---
title: Release Notes - JustAPI v2.0.0
description: Official release notes for JustAPI 2.0.0.
---

## 🚀 JustAPI v2.0.0 Release Highlights

We are thrilled to announce **JustAPI 2.0.0** — the flagship production release of the Rust-powered Python web framework!

### 🌟 Key Changes in 2.0.0

1. **Multi-Database Project Scaffolder:**
   * Interactive CLI wizard (`justapi create`) supporting DuckDB, ClickHouse, PostgreSQL, MySQL, SQLite, MongoDB, and Redis.

2. **Multi-Protocol Architecture:**
   * Built-in support for REST (OpenAPI 3.1 & Scalar UI), GraphQL (`app.graphql()`), gRPC/Protobuf (`/rpc`), and JSON-RPC 2.0 (`/jsonrpc`).

3. **OpenTelemetry Jaeger Stack Generator:**
   * Automatically generates `docker-compose.otel.yml` for zero-config distributed tracing and Prometheus monitoring.

4. **Multi-Arch Binary Wheel Pipeline:**
   * Published multi-architecture PyPI wheels for Linux x86_64, Linux ARM64 (AWS Graviton), Linux Musl (Alpine), macOS x86_64/arm64, and Windows x64.

5. **Starlette Parity:**
   * Added `app.add_middleware()`, `app.add_cors()`, and FastAPI-standard `JustAPIApp(title=..., version=...)` kwargs.

6. **Hardened Security & Fuzzing:**
   * Untrusted input pipeline fuzzing targets verified clean across Miri and LLVM libFuzzer suites.
