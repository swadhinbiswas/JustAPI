<p align="center">
  <img src="assets/justapi-hero.svg" alt="JustAPI — Python writes the logic. Rust does everything else." width="100%">
</p>

<p align="center">
  <a href="https://github.com/swadhinbiswas/JustAPI/actions/workflows/ci.yml">
    <img src="https://github.com/swadhinbiswas/JustAPI/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI">
  </a>
  <a href="https://github.com/swadhinbiswas/JustAPI/actions/workflows/wheels.yml">
    <img src="https://github.com/swadhinbiswas/JustAPI/actions/workflows/wheels.yml/badge.svg" alt="Wheels & Release">
  </a>
  <a href="https://pypi.org/project/justapi/">
    <img src="https://img.shields.io/badge/pypi-2.0.8-blue.svg?logo=pypi&logoColor=white" alt="PyPI">
  </a>
  <a href="https://www.python.org/downloads/">
    <img src="https://img.shields.io/badge/python-3.11%20|%203.12%20|%203.13%20|%203.14-blue.svg?logo=python&logoColor=white" alt="Python">
  </a>
  <a href="https://www.rust-lang.org/">
    <img src="https://img.shields.io/badge/rust-1.85+-E6522C.svg?logo=rust&logoColor=white" alt="Rust">
  </a>
  <a href="https://github.com/swadhinbiswas/JustAPI/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT">
  </a>
  <a href="https://img.shields.io/badge/free--threaded-3.14t-22d3ee">
    <img src="https://img.shields.io/badge/free--threaded-3.14t-22d3ee.svg" alt="Free-threaded">
  </a>
  <a href="https://github.com/swadhinbiswas/JustAPI/blob/main/deny.toml">
    <img src="https://img.shields.io/badge/cargo--deny-passing-green.svg" alt="cargo-deny">
  </a>
</p>

---

**JustAPI** is a Python web framework with a Rust core. You write Python logic.
Rust handles everything else — networking, TLS, routing, validation,
serialization, SSE, database I/O, rate limiting. One `pip install`, a server
that starts in milliseconds, and numbers that hold up under real traffic.

> **Python writes the logic. Rust does everything else.**

---

## Quickstart

```bash
pip install justapi
```

```python
# main.py
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/")
def read_root():
    return {"Hello": "World"}

@app.get("/items/{item_id}")
def read_item(item_id: int, q: str | None = None):
    return {"item_id": item_id, "q": q}

app.run()
```

```console
$ python main.py
INFO:     JustAPI running on http://127.0.0.1:8000
```

Interactive OpenAPI docs at <a href="http://127.0.0.1:8000/docs" target="_blank">http://127.0.0.1:8000/docs</a> — generated from your code, zero configuration.

### Async? Native. DB? Native. SSE? Native.

```python
import asyncio
from justapi import JustAPIApp, native_async

app = JustAPIApp()
app.set_database("postgres://user:pass@host/db")
app.sse_native("/events", count=1000, interval_ms=0)   # Rust streams events — 0 Python per event

@app.get("/users/{uid}")
@native_async
async def get_user(request, uid: int):
    row = await app.db.query_async(                     # runs on the DB's tokio runtime
        "SELECT * FROM users WHERE id = ?", [uid])      # GIL released — loop never blocks
    return row

app.run()
```

---

## Performance

<p align="center">
  <img src="assets/justapi-benchmark.svg" alt="Benchmark — JustAPI vs Granian vs FastAPI vs Robyn" width="100%">
</p>

Hello-world, 100 concurrent connections, 30s, same hardware. Full methodology in [BENCHMARKS.md](BENCHMARKS.md).

| Framework | Hello-world | JSON echo | p99 hello-world | RSS |
|---|--:|--:|--:|--:|
| **JustAPI (native fast path)** | **766k req/s** | **782k req/s** | **0.48 ms** | **12 MB** |
| Granian (ASGI) | 314k | 145k | 0.74 ms | 37 MB |
| Robyn | 39k | — | — | — |
| FastAPI + Uvicorn | 36k | 33k | 24.63 ms | 29 MB |

More numbers, honestly:

- **Route lookup:** 51 ns average on a 500-route table (matchit radix trie)
- **Rust-native CRUD:** 181k SELECT req/s — **×125 vs FastAPI** on the same DB fixture
- **Native async DB awaits** (`query_async`): **53× faster than the blocking path** on slow queries (320 vs 6 RPS) — the asyncio loop is never blocked
- **Multi-worker prefork** (`justapi serve --workers N`): **1.88× throughput at 4 workers** (99.7k RPS), auto-scale included
- **Free-threaded CPython (3.14t):** CPU-bound Python handlers **12.4× faster** — the GIL ceiling is gone

### Honest caveats

- **Native fast path requires a schema.** Routes without a `Schema` fall back to the Python handler path (~60k req/s). The ~700k numbers apply to routes with `native=True` + `Schema`.
- **Python handler throughput is GIL-bound.** On GIL-locked CPython, the Python dispatch path caps at ~100-120k req/s on this hardware regardless of framework — that is the GIL, not a framework limitation. Free-threaded Python (3.14t) removes this ceiling.
- **Light async handlers are loop-bound.** Async handlers that only sleep/echo (~4-12k req/s) are capped by the asyncio loop + thread hop. Three experiments (ADR-090/091/092) proved no coroutine driver, bridge, or multi-loop approach beats asyncio's own stepping. We win where it matters: real async workloads (1ms-sleep handlers: 3.9k vs Granian 2.9k RPS) and anything DB/SSE-heavy via native ops.
- **LLM-serving claims are NOT part of this release.** Inference phases are unverified (no real GPU run, ADR-067) — the tested story is the web framework.

---

## How a request flows

<p align="center">
  <img src="assets/justapi-pipeline.svg" alt="Request pipeline — Rust end to end" width="100%">
</p>

The entire request path runs in Rust. Python executes only your application logic:

```
Kernel (epoll/io_uring)
  → tokio connection manager
  → TLS (rustls)
  → HTTP parse (hyper)
  → Router (matchit, 51ns)
  → Middleware chain (auth, CORS, rate-limit, compression)
  → Python boundary (zero-copy PyO3, GIL pool)
  → Your async Python handler
  → Rust serializer (serde_json / simd-json)
  → Response write → socket
```

Routes marked `native=True` + `Schema` skip Python entirely — Rust validates the
body and writes the response. That is the ~700k req/s path.

---

## Feature highlights

<p align="center">
  <img src="assets/justapi-features.svg" alt="Feature matrix" width="100%">
</p>

<table>
<tr><th>Category</th><th>Feature</th><th>Details</th></tr>

<tr><td rowspan="5"><strong>Performance</strong></td>
    <td>HTTP/1.1 · HTTP/2 · HTTP/3</td><td>~766k req/s native fast path; QUIC (HTTP/3) is feature-gated — no other Python framework ships it</td></tr>
<tr><td>TLS (rustls)</td><td>~10% overhead, zero OpenSSL dependency</td></tr>
<tr><td>Serialization</td><td>serde_json with optional simd-json (89 ns/op)</td></tr>
<tr><td>Compression</td><td>gzip / deflate / brotli / zstd</td></tr>
<tr><td>Multi-worker prefork</td><td><code>justapi serve --workers N --scale</code> — shared-socket process fleet with load-based auto-scaling</td></tr>

<tr><td rowspan="5"><strong>Native ops</strong></td>
    <td>Async DB awaits</td><td><code>await app.db.query_async()</code> — SQL on the DB's tokio runtime, GIL released, loop never blocked (ADR-093)</td></tr>
<tr><td>Rust-native CRUD</td><td>Rust validates + queries + serializes — 181k SELECT req/s, ×125 vs FastAPI</td></tr>
<tr><td>Native SSE</td><td><code>app.sse_native(path, count, interval_ms)</code> — events generated in Rust, zero Python per event</td></tr>
<tr><td>@native_async</td><td>Marks async handlers for the fastest dispatch; true parallel dispatch on free-threaded CPython</td></tr>
<tr><td>Adaptive batching</td><td>Batch + window decorator for bursty workloads</td></tr>

<tr><td rowspan="5"><strong>Security</strong></td>
    <td>JWT Authentication</td><td>HS256/RS256/ES256, per-route roles & scopes, OAuth2 password flow</td></tr>
<tr><td>Rate Limiting</td><td>GCRA, Redis-backed distributed, IP + per-route</td></tr>
<tr><td>CORS</td><td>Configurable, preflight caching</td></tr>
<tr><td>Request Validation</td><td>JSON Schema compiled in Rust, Pydantic v2 bridge</td></tr>
<tr><td>Security Headers</td><td>HSTS, CSP, X-Frame-Options, nosniff, and more</td></tr>

<tr><td rowspan="5"><strong>Protocols</strong></td>
    <td>REST</td><td>Full HTTP methods with OpenAPI 3.1</td></tr>
<tr><td>WebSocket</td><td>Full-duplex with streaming support</td></tr>
<tr><td>Server-Sent Events</td><td>Generator-driven + Rust-native streaming</td></tr>
<tr><td>gRPC</td><td>Native Tonic integration</td></tr>
<tr><td>GraphQL</td><td>async-graphql wiring</td></tr>

<tr><td rowspan="5"><strong>Application</strong></td>
    <td>Dependency Injection</td><td>Litestar-style tiered DI, sync/async generators, cleanup</td></tr>
<tr><td>Background Tasks</td><td>Thread-safe task scheduling</td></tr>
<tr><td>Database</td><td>SQLite / PostgreSQL / MySQL / DuckDB via sqlx — pool, migrations, transaction middleware</td></tr>
<tr><td>Template Engine</td><td>Jinja2 integration</td></tr>
<tr><td>File Uploads</td><td>Streaming multipart/form-data</td></tr>

<tr><td rowspan="5"><strong>Operations</strong></td>
    <td>Observability</td><td>OpenTelemetry tracing, Prometheus metrics, health checks</td></tr>
<tr><td>Resilience</td><td>Circuit breakers, retry, fallback, bulkhead — 44 resilience tests</td></tr>
<tr><td>Hot Reload</td><td><code>justapi serve --reload</code> with graceful drain</td></tr>
<tr><td>Plugin System</td><td>Third-party extension API with lifecycle hooks</td></tr>
<tr><td>WASM Plugins</td><td>wasmtime-powered WebAssembly middleware</td></tr>

<tr><td rowspan="4"><strong>DX & Quality</strong></td>
    <td>Type Stubs</td><td><code>py.typed</code> + complete <code>.pyi</code> — mypy-clean on user code</td></tr>
<tr><td>Scaffolding</td><td><code>justapi create</code> — full CRUD project (7 DB backends, 4 API styles) that demos every differentiator</td></tr>
<tr><td>Memory Safety</td><td>ASan on the full suite + Miri on all unsafe code, 6 fuzz targets</td></tr>
<tr><td>Supply Chain</td><td>cargo-deny — advisories, bans, licenses, sources</td></tr>
</table>

---

## Coming from FastAPI?

JustAPI mirrors FastAPI's decorator API and Pydantic integration, so existing
apps migrate with minimal changes — often just the import:

| | FastAPI | JustAPI |
|---|---|---|
| **Import** | `from fastapi import FastAPI` | `from justapi import JustAPIApp` |
| **Create** | `app = FastAPI()` | `app = JustAPIApp()` |
| **Decorators** | `@app.get("/items/{id}")` | `@app.get("/items/{id}")` |
| **Pydantic** | Native | Native |
| **Depends()** | Built-in | Built-in (incl. generator deps + cleanup) |
| **OpenAPI** | `/docs` and `/redoc` | `/docs` and `/redoc` |
| **Server** | uvicorn (Python) | Built-in Rust server (tokio + hyper) |
| **Hello-world** | ~36k req/s | **~766k req/s** (native fast path) |

👉 Full [migration guide](docs/migrating_from_fastapi.md).

---

## Architecture

```
justapi/
├── crates/
│   ├── justapi-core/       # Networking, routing, middleware, serialization
│   ├── justapi-py/         # PyO3 bindings + Python package (python/justapi)
│   ├── justapi-cli/        # CLI binary (serve, create, check, gen-client)
│   └── justapi-bench/      # Internal benchmark harness
├── assets/                 # README diagrams (animated SVG)
├── benchmarks/             # Workload scripts for baseline comparisons
├── docs_site/              # Full documentation site (141 pages)
├── skills/                 # Reusable engineering knowledge
├── PLAN.md                 # Living roadmap (single source of truth)
├── DECISIONS.md            # Append-only ADR log
└── BENCHMARKS.md           # Append-only performance ledger
```

Crate dependency graph:

```
justapi-cli  ──→  justapi-core
justapi-py   ──→  justapi-core
justapi-bench ─→  justapi-core
```

---

## Development

### Prerequisites

- **Rust** 1.85+ — [install via rustup](https://rustup.rs/)
- **Python** 3.11+ — [download](https://www.python.org/downloads/)
- **maturin** — `pip install maturin`

### Build & test

```bash
cargo build --workspace --release
cd crates/justapi-py && maturin develop --release   # build the Python package

cargo test --workspace                              # Rust tests
cargo clippy --workspace --tests -- -D warnings     # lint
cargo fmt --check                                   # format
cargo deny check                                    # supply-chain audit
bash scripts/sanitize.sh                            # ASan + Miri memory-safety gate
python -m pytest                                    # Python tests (in the venv)
```

### Benchmark

```bash
bash benchmarks/run_baselines.sh                    # FastAPI / Granian / JustAPI A/B
```

See [BENCHMARKS.md](BENCHMARKS.md) for methodology, hardware, and history.

---

## Deployment

```bash
# Docker (multi-stage, uv-based — fast reproducible builds)
docker build -t justapi-app .
docker run -p 8000:8000 justapi-app

# Multi-worker + auto-scale (saturate all cores)
justapi serve --workers 4 --scale
```

- **Kubernetes:** `helm install my-api helm/justapi/ --set image.repository=your-registry/justapi-app`
- **Cloud guides:** [EKS](deploy/clouds/eks.md) · [GKE](deploy/clouds/gke.md) · [AKS](deploy/clouds/aks.md) · [Fly.io](deploy/clouds/flyio.md) · [Railway](deploy/clouds/railway.md)
- **Observability:** OTel tracing + Prometheus metrics + health probes out of the box

---

## Documentation

| Document | Description |
|---|---|
| [**docs_site**](docs_site/) | Full documentation site — 141 pages, 13 sections, search |
| [**Migration guide**](docs/migrating_from_fastapi.md) | Step-by-step from FastAPI |
| [**BENCHMARKS.md**](BENCHMARKS.md) | Performance ledger (append-only) |
| [**PLAN.md**](PLAN.md) | Living roadmap and current phase |
| [**DECISIONS.md**](DECISIONS.md) | Architecture decision records (ADR-001 → ADR-093) |
| [**AGENTS.md**](AGENTS.md) | Development conventions & review checklist |

---

## Contributing

Contributions welcome. Read [AGENTS.md](AGENTS.md) for conventions before opening a PR.

```bash
git clone git@github.com:swadhinbiswas/JustAPI.git
cd JustAPI
git checkout -b feat/my-feature
# ... make changes ...
cargo test --workspace
cargo clippy --workspace --tests -- -D warnings
cargo fmt --check
```

Every PR is gated on: tests green, clippy `-D warnings`, fmt clean, sanitizers
clean (core touched), benchmarks appended (perf touched), ADR recorded
(architecture touched).

---

## License

[MIT](LICENSE) © JustAPI contributors
