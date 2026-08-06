---
title: External Links & Ecosystem
description: Related projects, community tools, and ecosystem around JustAPI.
keywords: [JustAPI, ecosystem, community, related projects, integrations]
---

## Core Dependencies

| Project | Purpose | Link |
|---------|---------|------|
| **Pydantic** | Data validation and serialization (schema bridge) | [pydantic.dev](https://pydantic.dev/) |
| **PyO3** | Python ↔ Rust FFI | [pyo3.rs](https://pyo3.rs/) |
| **maturin** | Build Rust Python packages | [maturin.rs](https://www.maturin.rs/) |

## Related Rust-Powered Frameworks

| Framework | What It Does |
|-----------|-------------|
| **Robyn** | Rust runtime, decorator API |
| **Granian** | Rust ASGI server (ASGI-based, unlike JustAPI's native runtime) |
| **Litestar** | Python-native ASGI framework |
| **FastAPI** | Python ASGI web framework (design inspiration for DX) |
| **Typer** | CLI framework (FastAPI for CLIs) |

## Community

- **GitHub**: [github.com/swadhinbiswas/JustAPI](https://github.com/swadhinbiswas/JustAPI)
- **Stack Overflow**: [tag: justapi](https://stackoverflow.com/questions/tagged/justapi)
- **Issues**: [GitHub Issues](https://github.com/swadhinbiswas/JustAPI/issues)

## Ecosystem

### Database Drivers
- **aiosqlite** — async SQLite
- **asyncpg** — async PostgreSQL
- **aiomysql** — async MySQL

### Serialization
- **orjson** — fast JSON (JustAPI has built-in `orjson` feature)
- **simd-json** — SIMD-accelerated JSON

### Testing
- **pytest** — Python testing framework
- **httpx** — async HTTP client for testing
- **criterion** — Rust benchmarking

### Deployment
- **uvicorn** — Python ASGI server (runs FastAPI/Starlette apps; not used by JustAPI)
- **gunicorn** — pre-fork server
- **Docker** — container deployment
- **Kubernetes** — orchestration
