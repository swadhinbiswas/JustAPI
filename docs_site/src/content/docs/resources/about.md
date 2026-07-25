---
title: About JustAPI
description: The story, philosophy, and future of JustAPI — a Rust-powered Python web framework built to replace FastAPI.
keywords: [JustAPI, about, history, philosophy, Rust, Python web framework]
---

## What is JustAPI?

JustAPI is a Python web framework for building APIs, powered by a Rust core. It provides the same developer experience as FastAPI — decorator syntax, Pydantic models, dependency injection — while moving the entire networking stack out of Python.

**Python writes the logic. Rust does everything else.**

## Why Rust?

Python is excellent for application logic. It's readable, has a rich ecosystem, and developers love it. But it has limitations for networking:

- The GIL prevents true parallelism in Python handlers.
- Python HTTP servers (uvicorn, gunicorn) add overhead.
- TLS termination, connection management, and routing are all CPU-bound.

JustAPI solves this by implementing these critical paths in Rust using:

- **hyper** — HTTP/1.1 and HTTP/2 implementation
- **tokio** — asynchronous runtime
- **rustls** — TLS termination without OpenSSL
- **matchit** — radix-trie routing
- **PyO3** — seamless Python ↔ Rust boundary

## Design Philosophy

1. **FastAPI-compatible.** Existing FastAPI apps migrate with minimal changes.
2. **Rust-first.** If it can be in Rust, it must be in Rust. Python is for application logic only.
3. **Production-ready from day one.** Built-in auth, rate limiting, circuit breakers, and observability.
4. **Honest performance claims.** Every benchmark number in the docs is reproducible on the documented hardware.

## Performance History

| Milestone | Date | Result |
|-----------|------|--------|
| Phase 3 | 2026-07 | 752k req/s hello-world |
| Phase 20 | 2026-07 | Beat FastAPI on all benchmarks |
| Phase 25 | 2026-07 | 1M+ RPS tuning |
| Hot-path optimization | 2026-07-13 | 60k rps Python path, 724k rps native |
| Head-to-head vs Robyn | 2026-07-13 | JustAPI ×1.54 faster |

## Architecture

```
Kernel → epoll/io_uring
  → Connection Manager (tokio)
  → TLS (rustls)
  → HTTP parse (hyper)
  → Router (matchit)
  → Middleware chain (Rust: auth, CORS, rate-limit)
  → Python boundary (PyO3)
  → Application code (async Python)
  → Rust serializer (serde_json / simd-json)
  → Response write → Socket
```

## See Also

- [Alternatives, Inspiration & Comparisons](/resources/external-links/)
- [History, Design & Future](/resources/about/)
- [Benchmarks & Performance](/reference/adr-index/)
- [Repository Management](/contributing/coding-standards/)
