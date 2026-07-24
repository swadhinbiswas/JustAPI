---
title: Rust Core Deep Dive
description: How the Rust runtime handles networking, routing, middleware, and serialization.
---

This guide explores the internals of `justapi-core`, the Rust engine that powers JustAPI.

## Architecture

```
┌──────────────────────────────────────────────────┐
│                justapi-core                       │
│                                                    │
│  ┌────────────┐  ┌──────────┐  ┌──────────────┐  │
│  │ Connection  │  │  HTTP    │  │   Router     │  │
│  │  Manager    │──│  Parser  │──│  (matchit)   │  │
│  │  (tokio)   │  │  (hyper) │  │  Radix Trie  │  │
│  └────────────┘  └──────────┘  └──────────────┘  │
│        │                                            │
│        ▼                                            │
│  ┌──────────────────────────────────────────────┐  │
│  │         Middleware Chain                      │  │
│  │  Auth → CORS → Rate Limit → Compression      │  │
│  └──────────────────────────────────────────────┘  │
│        │                                            │
│        ▼                                            │
│  ┌────────────┐  ┌──────────┐  ┌──────────────┐  │
│  │  JSON      │  │  DB      │  │  PyO3 FFI    │  │
│  │  Schema    │  │  Pool    │  │  Boundary    │  │
│  │  Validate  │  │  (sqlx)  │  │              │  │
│  └────────────┘  └──────────┘  └──────────────┘  │
│                                                    │
└──────────────────────────────────────────────────┘
```

## Server Lifecycle

1. **`Server::new(addr)`** — Create a new server instance
2. **`server.run()`** — Bind socket, start Tokio runtime, accept connections
3. **Graceful shutdown** — SIGINT/SIGTERM → stop accepting → drain in-flight → exit

```rust
// Simplified from justapi-core/src/server/
pub async fn run(self) -> Result<()> {
    let listener = TcpListener::bind(self.addr).await?;
    loop {
        tokio::select! {
            Ok((stream, _)) = listener.accept() => {
                tokio::spawn(handle_connection(stream));
            }
            _ = shutdown_signal() => break,
        }
    }
}
```

## Connection Manager

- Uses `tokio` for async I/O with epoll (Linux) or io_uring (when configured)
- Each connection is handled in a separate tokio task
- TLS is handled by `rustls` (TLS 1.2/1.3 only, no legacy protocols)
- HTTP/1.1 and HTTP/2 parsing via `hyper` 1.x

## Radix Trie Router (matchit)

Routes are stored in a radix-trie data structure that provides O(1) path matching:

```
Routes:
  /users/{id}
  /users/me
  /users/{id}/items/{item_id}

Trie:
  /users/
    ├── me
    └── {id}
          └── /items/{item_id}
```

- Parameters are extracted during the match
- Type conversion happens in Rust before Python is called
- Route-lookup cache memoizes repeated lookups

## Middleware Chain

Middleware runs in the Rust middleware chain before Python is called:

| Middleware | Type | Configurable |
|---|---|---|
| CORS | Built-in | `app.add_cors()` |
| Auth/JWT | Built-in | Via middleware function |
| Rate Limiting | Built-in | Global and per-IP |
| Compression | Built-in | gzip, brotli, zstd |
| Security Headers | Built-in | `app.enable_secure_headers()` |
| Circuit Breaker | Built-in | `app.enable_circuit_breaker()` |
| Request Coalescing | Built-in | `app.enable_request_coalescing()` |

## Zero-Copy FFI Boundary

Data crosses the Rust↔Python boundary using PyO3's buffer protocol:

- Request headers are exposed as read-only memory views
- Request body is copied once from the network buffer (if read)
- Response bodies use the same zero-copy path in reverse
- No JSON serialization between Rust and Python for internal data

## See Also

- [Architecture Overview](/getting-started/overview/) — High-level design
- [Zero-GIL Architecture](/advanced/zero-gil-architecture/) — Concurrency model
- [PyO3 & FFI Safety](/advanced/pyo3-ffi-safety/) — Safe FFI patterns
