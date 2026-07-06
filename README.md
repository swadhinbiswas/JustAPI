# justapi Runtime

A production-grade Python web framework written in Rust. Rust owns the
network/protocol stack; Python executes only application logic.

**10-20x faster than FastAPI** — see [BENCHMARKS.md](BENCHMARKS.md).

## Project Status

**Phase 40 — The JustAPI 2.0 "Singularity" Release** (in progress)

All 39 prior phases complete. See [PLAN.md](PLAN.md) for the full roadmap.

## Architecture

```
Kernel → epoll/io_uring → Connection Manager → tokio task
  → TLS (rustls) → HTTP parse (hyper) → Router (matchit)
  → Middleware chain (Rust: auth/CORS/rate-limit/compression)
  → Python boundary (typed, zero-copy request view via PyO3)
  → Application code (native Python handler)
  → Rust serializer (serde_json/simd-json)
  → Response write → Socket
```

justapi provides a single native Python API (Tier B) — the FastAPI
replacement that runs entirely in Rust with zero-copy Python integration.

## Quick Start

### Python Native API

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/hello/{name}")
async def hello_handler(request):
    name = request["path_params"]["name"]
    return {"message": f"Hello {name}!"}

app.run("127.0.0.1:8080")
```

Install the package:

```bash
pip install justapi
```

Or build from source with `maturin`:

```bash
cd crates/justapi-py
maturin develop --release
```

### CLI

```bash
# Run server with Rust-defined routes
justapi serve --addr 127.0.0.1:8080

# Generate a client SDK from your API
justapi gen-client --lang python
```

## Features

| Area | Status |
|---|---|
| **HTTP/1.1 & HTTP/2** | ✅ 324k req/s hello-world |
| **WebSocket & SSE** | ✅ Streaming, token-generation optimized |
| **TLS (rustls)** | ✅ 10.7% overhead |
| **Middleware** | ✅ CORS, SecurityHeaders, JWT, RateLimiter |
| **JWT Auth** | ✅ RS256/ES256, per-route roles/scopes |
| **Serialization** | ✅ serde_json, simd-json optional |
| **Request Validation** | ✅ JSON Schema, Pydantic bridge |
| **OpenAPI Generation** | ✅ Auto-generated `/docs` and `/redoc` |
| **Database ORM** | ✅ SQLite/PostgreSQL/MySQL via sqlx |
| **gRPC** | ✅ Native Tonic integration |
| **GraphQL** | ✅ async-graphql, Apollo Federation |
| **Observability** | ✅ OTel tracing, Prometheus metrics, structured logging |
| **Resilience** | ✅ Circuit breakers, retry, fallback, bulkhead |
| **Rate Limiting** | ✅ GCRA algorithm, Redis-backed |
| **WASM Middleware** | ✅ wasmtime embedded plugins |
| **Agent DAG Engine** | ✅ DAG orchestration in Rust |
| **Background Tasks** | ✅ Thread-safe task scheduling |
| **Dependency Injection** | ✅ Litestar-style tiered DI |
| **Snapshot Testing** | ✅ Built-in test utilities |
| **Static Files** | ✅ Range requests, MIME detection |
| **Compression** | ✅ gzip/deflate/brotli/zstd |
| **Plugin System** | ✅ Third-party plugin API |
| **Memory Safety** | ✅ Miri-verified unsafe blocks, 6 fuzz targets |

## Development

### Prerequisites

- Rust 1.85+ (edition 2021)
- Python 3.11+

### Building

```bash
cargo build --workspace               # debug build
cargo build --workspace --release      # release build
cargo test --workspace                 # run all tests (236+)
cargo clippy --workspace --tests -- -D warnings # lint
cargo fmt --check                      # format check
cargo miri test -p justapi-core        # memory safety (nightly)
```

### Benchmarking

See [BENCHMARKS.md](BENCHMARKS.md) for methodology and results.

```bash
# Install load generator
cargo install oha

# Run baseline benchmarks
bash benchmarks/run_baselines.sh
```

## Project Structure

```
crates/
  justapi-core/   — networking, protocol, scheduler, memory
  justapi-py/     — PyO3 bindings, native Python API
  justapi-cli/    — `justapi` CLI binary
  justapi-bench/  — internal benchmark harness
python/
  justapi/        — pip-installable Python package
fuzz/              — cargo-fuzz targets (6 targets)
docs/              — documentation
skills/            — reusable knowledge modules for development
benchmarks/        — workload scripts for performance testing
```

## Documentation

- [PLAN.md](PLAN.md) — Living roadmap and current status
- [AGENTS.md](AGENTS.md) — Development rules and conventions
- [DECISIONS.md](DECISIONS.md) — Architecture decision records
- [BENCHMARKS.md](BENCHMARKS.md) — Performance results ledger
- [docs/security/](docs/security/) — Security policies and pentest guide
- [docs/plugins.md](docs/plugins.md) — Plugin development guide

## License

MIT
