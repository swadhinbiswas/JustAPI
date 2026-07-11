# JustAPI Architecture

This document provides a comprehensive overview of the design, internal structure, and technical invariants of the **JustAPI** runtime.

---

## High-Level System Design

JustAPI splits responsibility between a high-performance Rust core and Python application logic:

*   **Rust Core (`justapi-core`):** Handles I/O, networking (HTTP/1.1, HTTP/2, WebSockets, gRPC, SSE), TLS termination, path routing, the middleware chain (auth, CORS, rate limiting, compression), schema validation, and serialization.
*   **Python Layer (`justapi-py`):** Provides the decorator-based framework API (matching FastAPI's developer experience), registers routes, runs dependency injection (`Depends()`), and executes the user's business logic.
*   **Zero-Copy FFI Boundary:** PyO3's buffer protocol is used to pass request parameters, headers, and bodies directly from Rust to Python as read-only memory views, eliminating serialization/deserialization overhead.

```
       [ Client Request ]
               │
               ▼
┌──────────────────────────────┐
│  Kernel (epoll / io_uring)   │
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│  Connection Manager (Tokio)  │
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│       TLS (rustls)           │
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│      HTTP Parser (hyper)     │
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│      Router (matchit)        │
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│      Middleware Chain        │ (Auth, CORS, Rate Limit, Compression)
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│  Zero-Copy FFI Boundary      │ (PyO3 buffer protocol / Arrow / DLPack)
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│    Python Handler Coroutine  │ (Jinja2, Pydantic validation, DB access)
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│   JSON Serializer (serde)    │
└──────────────┬───────────────┘
               │
               ▼
       [ Response Write ]
```

---

## Crate Layout & Dependency Graph

The workspace is organized into modular crates:

```
justapi-cli  ──→  justapi-core
justapi-py   ──→  justapi-core
justapi-bench ─→  justapi-core
```

1.  **`justapi-core`:**
    *   The engine room. Exposes the custom HTTP server built on hyper and tokio.
    *   Implements the radix-tree path router (`matchit`), middlewares, JSON Schema validation (`jsonschema` crate), static file serving, and the database connection pooling/migration runner (`sqlx`).
2.  **`justapi-py`:**
    *   The PyO3 FFI bridge. Wraps `justapi-core` server handles and request views.
    *   Includes the pure-Python framework code (`python/justapi/`) providing the `JustAPIApp` decorator interface, custom dependency injection container, responses, testing client, and templates.
3.  **`justapi-cli`:**
    *   Command-line utility (`justapi` binary). Runs development servers (with watch/hot-reload), compiles OpenAPI specs, profiles handlers, and executes migrations.
4.  **`justapi-bench`:**
    *   Internal benchmark harness. Used to prevent performance regressions by verifying throughput and latency gates.
5.  **`justapi-inference`:**
    *   Optional ML serving engine built on Candle (CUDA/CPU). Implements PagedAttention KV-caching, continuous batching, and speculative decoding.

---

## Concurrency & GIL Model

JustAPI utilizes a **GIL-free async worker architecture** (ADR-021):

*   **Tokio Worker Integration:** Incoming connections are accepted concurrently by Tokio worker threads.
*   **GIL-free attach:** When a request requires Python execution, the Tokio worker thread uses PyO3's GIL-free `Python::try_attach()` to safely lock the GIL.
*   **Execution Caching:** Python handler helper modules are compiled once at startup and cached in a global `OnceLock<Py<PyAny>>`. Invoking Python handlers does not require spawning separate OS threads or managing an external asyncio loop in Rust.
*   **GIL Release during I/O:** The GIL is immediately released (via PyO3's `py.detach()`) when Python handlers complete or yield for async Rust I/O, maximizing CPU cores and avoiding interpreter deadlocks.

---

## Key Subsystems

### Radix Router
Uses the radix-trie based `matchit` crate. Paths are resolved in `O(1)` time relative to the number of registered routes. Parameters are parsed into typed path variables during the routing step.

### Request Validation
Request bodies are validated against their JSON Schema *entirely in Rust* using the `jsonschema` crate before Python is invoked. Invalid requests receive an RFC 9457 structured error response and are rejected at the FFI boundary, saving CPU cycles.

### Database ORM
Built on `sqlx` connection pools. The migrator discovers and executes plain SQL migration files, tracking applied versions in a tracking table. Transactions are automatically managed by the middleware chain: initialized on incoming requests, committed on `2xx` statuses, and rolled back on error.

### Observability
*   **Metrics:** Prometheus-compatible endpoint serving bucket-based latency histograms (p50, p95, p99, p999 percentiles calculated client-side) and error counts.
*   **OTel Tracing:** OpenTelemetry OTLP tracing hooks propagate context across the PyO3 boundary using thread-local variables.

---

## LLM Serving Engine (`justapi-inference`)

To support fast AI deployment, JustAPI incorporates a Rust-native model server:

*   **KvBlockPool:** Implements paged KV-cache block allocation to prevent GPU memory fragmentation.
*   **PrefixCache:** A SGLang-style radix prefix cache that reuses prefill context across concurrent requests.
*   **PdScheduler:** Implements disaggregated prefill/decode scheduling. Prefill operations run on separate resource pools from decode steps, avoiding decode starvation.
*   **Speculative Decoding:** Compares generated draft tokens with a target model, verifying candidate tokens in parallel.
