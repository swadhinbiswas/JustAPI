---
title: Overview & Core Philosophy
description: Learn why JustAPI — the fastest FastAPI alternative — is built on Rust and delivers FastAPI compatibility with 20x performance. 700k+ RPS, zero-GIL.
keywords: [JustAPI, FastAPI alternative, FastAPI replacement, Rust Python web framework, high performance Python, Python API framework, drop-in FastAPI replacement]
---

**JustAPI** is a high-performance Python web framework built on top of a multi-threaded **Rust runtime engine** (`justapi-core`).

It is engineered as a **drop-in replacement for FastAPI** — preserving your favorite Python decorator syntax, Pydantic schemas, dependency injection, and automatic OpenAPI documentation — while offloading networking, TLS, routing, request validation, and response serialization to Rust.

## Who Is This For?

| Audience | Why JustAPI |
|---|---|
| **FastAPI users** | Drop-in replacement. Change one import. Get 20x throughput. |
| **Performance engineers** | 700k+ RPS in a single process. Sub-millisecond p99 latency. |
| **AI/ML teams** | Native MCP tools, streaming validated output, agent session state. |
| **Platform teams** | Built-in OTel, Prometheus metrics, health checks, circuit breakers. |
| **Startups** | One framework for REST, GraphQL, gRPC, JSON-RPC. Ship faster. |

## Core Pillars

1. **Rust-First Architecture:** The core network stack (Tokio, Hyper, Rustls, Matchit, Serde-JSON, SQLx) is written entirely in Rust.
2. **FastAPI Parity:** `@app.get()`, `@app.post()`, `APIRouter`, `Depends()`, `HTTPException`, `Header`, `Query`, `Path`.
3. **Zero-GIL Execution:** Rust releases the Python GIL during all I/O and validation, enabling true parallel execution across multi-core systems.
4. **Batteries-Included Tooling:** Native database pool, WebSockets, SSE, GraphQL, gRPC, JSON-RPC, OpenTelemetry tracing, and built-in project scaffolding CLI.

## Architecture Overview

![JustAPI Architecture](https://cdn.jsdelivr.net/gh/swadhinbiswas/JustAPI@refs/heads/initial-setup/docs_site/src/assets/architectureoverview.png)

## Performance Comparison

Measured on Intel Core i5-13600K, 31 GiB DDR5. Single process, no multi-worker fan-out.

| Framework | Requests/sec | p50 Latency | p99 Latency |
|---|---|---|---|
| **JustAPI** | **701,234** | **0.07 ms** | **0.19 ms** |
| Granian | 69,195 | 0.72 ms | 3.87 ms |
| FastAPI + Uvicorn | 36,189 | 1.72 ms | 24.63 ms |

JustAPI delivers **~20x the throughput** of FastAPI+Uvicorn on the same hardware.

## Feature Comparison

| Feature | FastAPI | JustAPI | Notes |
|---|---|---|---|
| Rust-native core | ✗ | ✓ | Tokio + Hyper + matchit |
| Zero-GIL execution | ✗ | ✓ | `py.allow_threads` on all I/O |
| Native DB pool | ✗ | ✓ | sqlx (Postgres, MySQL, SQLite) |
| GraphQL | ✗ (Torture) | ✓ | `app.graphql()` one-liner |
| gRPC | ✗ | ✓ | tonic + prost |
| JSON-RPC 2.0 | ✗ | ✓ | Built-in handler |
| MCP Agent Tools | ✗ | ✓ | `@app.tool` native registration |
| Streaming validation | ✗ | ✓ | `@app.stream_json` with Rust validation |
| Circuit breakers | ✗ | ✓ | Built-in resilience |
| Request coalescing | ✗ | ✓ | Dedup concurrent identical requests |
| OpenTelemetry | ✗ | ✓ | Built-in OTLP exporter |
| Free-threaded Python | ✗ | ✓ | Python 3.13t/3.14t support |
| Multi-arch wheels | ✗ | ✓ | Linux, macOS, Windows, ARM64 |

## System Requirements

- **Python:** 3.11, 3.12, 3.13, or 3.14 (including 3.13t/3.14t free-threaded builds)
- **Rust:** 1.85+ (only required when building from source)
- **OS:** Linux (x86_64, ARM64), macOS (x86_64, ARM64), Windows (x64)

## Next Steps

- [Installation Guide](/getting-started/installation/) — Install JustAPI in under a minute
- [First Steps](/getting-started/first-steps/) — Build your first API endpoint
- [Migrating from FastAPI](/getting-started/migrating-from-fastapi/) — Switch your existing project
