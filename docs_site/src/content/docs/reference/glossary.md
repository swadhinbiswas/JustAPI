---
title: Glossary
description: Key terms and concepts used throughout the JustAPI documentation for a high-performance FastAPI alternative built in Rust.
keywords: glossary, terminology, FastAPI alternative, Rust web framework, JustAPI concepts
---

## A

**APIRouter** — A class for grouping related routes into modular files. Supports the same decorators as `JustAPIApp`.

**ASGI** — Asynchronous Server Gateway Interface, the Python standard for async web servers (used by Starlette/FastAPI/Uvicorn). JustAPI is a **native Rust runtime** and does not implement ASGI; it runs its own pipeline for maximum throughput.

## B

**BackgroundTasks** — Post-response task execution. Register callables to run after the HTTP response is sent.

**Bulkhead** — Resilience pattern that limits concurrent requests to prevent resource exhaustion.

## C

**Circuit Breaker** — Resilience pattern that stops calling a failing service when errors exceed a threshold.

**CORS** — Cross-Origin Resource Sharing. HTTP mechanism for controlling cross-origin requests.

## D

**Depends** — Dependency injection function. Resolves and injects callable results into route handlers.

**Dependency Injection** — Design pattern where a component's dependencies are provided externally.

## G

**GIL** — Global Interpreter Lock. CPython's mechanism preventing multiple threads from executing Python bytecode simultaneously.

**GIL Pool** — Subsystem managing GIL-bound worker threads for Python handler execution.

**gRPC** — High-performance RPC framework using Protocol Buffers.

## H

**Hyper** — Rust HTTP library used by JustAPI for HTTP/1.1 and HTTP/2 parsing.

## J

**JSON-RPC 2.0** — Lightweight remote procedure call protocol using JSON encoding.

**JustAPIApp** — The core application class. Register routes, middleware, plugins, and configuration.

## M

**matchit** — Rust radix-trie router library. Provides O(1) path matching.

**MCP** — Model Context Protocol. Protocol for exposing tools and context to AI agents.

**Middleware** — Request/response processing layer that runs before and after route handlers.

## N

**Native Fast Path** — Route execution entirely in Rust with `native=True`. Maximum performance (724k+ RPS).

## O

**OpenAPI** — Standard specification for REST API documentation. JustAPI auto-generates OpenAPI 3.1 specs.

**OTel** — OpenTelemetry. Observability framework for distributed tracing and metrics.

## P

**PyO3** — Rust bindings for Python. Used to build the JustAPI Python extension module.

**Pydantic** — Python library for data validation. JustAPI supports Pydantic v2 models as route schemas.

## R

**Radix Trie** — Tree data structure used by matchit for O(1) route matching.

**Rustls** — Rust TLS library. Provides TLS 1.2/1.3 support.

## S

**Schema** — JustAPI's built-in validation class. Supports Field constraints and Rust-side validation.

**serde_json** — Rust JSON serialization library. Used for response serialization.

**SSE** — Server-Sent Events. HTTP protocol for server-to-client streaming.

**sqlx** — Rust SQL library with compile-time query checking. Powers JustAPI's database layer.

## T

**Tokio** — Rust async runtime. Powers JustAPI's network I/O and concurrency.

## Z

**Zero-Copy** — Data transfer technique avoiding unnecessary memory copies. Used at the Rust↔Python FFI boundary.

## See Also

- [ADR Index](/reference/adr-index/) — Architecture Decision Records
- [Architecture Overview](/getting-started/overview/) — High-level design
- [Rust Core Deep Dive](/advanced/rust-core-deep-dive/) — Runtime internals
