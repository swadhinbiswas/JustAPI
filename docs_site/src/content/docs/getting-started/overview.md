---
title: Overview & Core Philosophy
description: Learn why JustAPI is built on Rust and how it delivers FastAPI compatibility with 20x performance.
---

**JustAPI** is a high-performance Python web framework built on top of a multi-threaded **Rust runtime engine** (`justapi-core`).

It is engineered as a **drop-in replacement for FastAPI** — preserving your favorite Python decorator syntax, Pydantic schemas, dependency injection, and automatic OpenAPI documentation — while offloading networking, TLS, routing, request validation, and response serialization to Rust.

## Core Pillars

1. **Rust-First Architecture:** The core network stack (Tokio, Hyper, Rustls, Matchit, Serde-JSON, SQLx) is written entirely in Rust.
2. **FastAPI Parity:** `@app.get()`, `@app.post()`, `APIRouter`, `Depends()`, `HTTPException`, `Header`, `Query`, `Path`.
3. **Zero-GIL Execution:** Rust releases the Python GIL during all I/O and validation, enabling true parallel execution across multi-core systems.
4. **Batteries-Included Tooling:** Native database pool, WebSockets, SSE, GraphQL, gRPC, JSON-RPC, OpenTelemetry tracing, and built-in project scaffolding CLI.
