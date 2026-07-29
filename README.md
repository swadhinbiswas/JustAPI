<p align="center">
  <a href="https://github.com/swadhinbiswas/JustAPI"><img src="https://img.shields.io/badge/JustAPI-Rust%20Powered-E6522C?style=for-the-badge&logo=rust&logoColor=white" alt="JustAPI" width="400"></a>
</p>
<p align="center">
    <em>JustAPI — Built to handle pressure.</em>
</p>
<p align="center">
<a href="https://github.com/swadhinbiswas/JustAPI/actions/workflows/ci.yml">
    <img src="https://github.com/swadhinbiswas/JustAPI/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI">
</a>
<a href="https://github.com/swadhinbiswas/JustAPI/actions/workflows/wheels.yml">
    <img src="https://github.com/swadhinbiswas/JustAPI/actions/workflows/wheels.yml/badge.svg" alt="Wheels">
</a>
<a href="https://github.com/swadhinbiswas/JustAPI/actions/workflows/publish.yml">
    <img src="https://github.com/swadhinbiswas/JustAPI/actions/workflows/publish.yml/badge.svg" alt="Publish">
</a>
<a href="https://github.com/swadhinbiswas/JustAPI/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT">
</a>
<a href="https://www.python.org/downloads/">
    <img src="https://img.shields.io/badge/python-3.11%20|%203.12%20|%203.13%20|%203.14-blue.svg?logo=python&logoColor=white" alt="Python versions">
</a>
<a href="https://www.rust-lang.org/">
    <img src="https://img.shields.io/badge/rust-1.85+-E6522C.svg?logo=rust&logoColor=white" alt="Rust version">
</a>
<a href="https://github.com/swadhinbiswas/JustAPI/blob/main/deny.toml">
    <img src="https://img.shields.io/badge/cargo--deny-v0.20-passing-green.svg" alt="cargo-deny">
</a>
<a href="https://pypi.org/project/justapi/">
    <img src="https://img.shields.io/badge/pypi-2.0.8-blue.svg?logo=pypi&logoColor=white" alt="PyPI">
</a>
</p>

---

**Documentation**: <a href="https://github.com/swadhinbiswas/JustAPI/tree/main/docs_site" target="_blank">https://github.com/swadhinbiswas/JustAPI/docs_site</a>

**Source Code**: <a href="https://github.com/swadhinbiswas/JustAPI" target="_blank">https://github.com/swadhinbiswas/JustAPI</a>

---

JustAPI is a Python web framework powered by a Rust core. Python writes the logic. Rust handles everything else — networking, TLS, routing, serialization, compression.

Built to handle real traffic.

**Python writes the logic. Rust does everything else.**



## Performance

*Hello-world benchmark · 100 concurrent connections · 30 seconds · same hardware*

| Framework | Requests/sec | p50 Latency | p99 Latency |
|---|--:|--:|--:|
| **JustAPI (native fast path)** | **701,234** | **0.07 ms** | **0.19 ms** |
| **JustAPI (Python handler)** | **60,297** | **1.03 ms** | **5.21 ms** |
| Robyn 0.88 | 39,103 | 1.78 ms | 11.47 ms |
| Granian | 69,195 | 0.72 ms | 3.87 ms |
| FastAPI + Uvicorn | 36,189 | 1.72 ms | 24.63 ms |

All measurements on the same hardware (i5-13600K, 20 threads), 100 concurrent connections, 30s duration. See [BENCHMARKS.md](BENCHMARKS.md) for full methodology and workload scripts.

### Request pipeline

JustAPI runs the entire request path in Rust. Python executes only application logic:

```
Kernel (epoll/io_uring)
  → Connection Manager (tokio)
  → TLS termination (rustls)
  → HTTP parse (hyper)
  → Router (matchit)
  → Middleware chain (Rust: auth, CORS, rate-limit, compression)
  → ┌─────────────────────────────────────────────────────┐
    │  Python handler path (default):                      │
    │  → Python boundary (zero-copy via PyO3 buffer)       │
    │  → Your async Python handler                         │
    │  → Rust serializer (serde_json / simd-json)         │
    ├─────────────────────────────────────────────────────┤
    │  Native fast path (native=True + Schema):             │
    │  → Rust validator → Rust response (no Python)        │
    └─────────────────────────────────────────────────────┘
  → Response write → Socket
```

The Python path is used for general routes. For schema-backed routes with `native=True`, the entire handler executes in Rust — eliminating the GIL acquire, Python dispatch, and per-request JSON schema compilation.

## What makes JustAPI different

1. **Rust-native request pipeline.** Networking, TLS, routing, serialization, middleware, and auth all execute in Rust — Python runs only your application logic. This yields ~60k rps on the Python handler path and ~724k rps on the native fast path for schema-backed routes, both measured on a single machine.

2. **Agent-native serving.** Beyond REST, JustAPI exposes routes as MCP tools, streams validated structured output per-token, and carries multi-turn session state — without external SDKs. See [`ROADMAP.md`](ROADMAP.md) for the full agent roadmap.

## Competitors, named honestly

JustAPI is not the first Rust-backed Python web framework. The following table compares the closest alternatives on raw throughput (Python handler path, same hardware):

| Workload | JustAPI | Robyn 0.88 | Faster |
|---|---:|---:|---|
| Hello-world (GET) | 60,297 rps | 39,103 rps | JustAPI ×1.54 |
| JSON echo (POST) | 47,415 rps | 36,899 rps | JustAPI ×1.29 |
| Validated JSON (POST) | 40,080 rps | 32,919 rps | JustAPI ×1.22 |

For routes registered with `native=True` and a schema, JustAPI serves entirely in Rust at **~724k rps** — bypassing the Python handler entirely.

### Where each competitor fits

- **Robyn** — Rust runtime with a decorator API, the closest architectural peer. Both frameworks keep the hot path in Rust and dispatch to Python for business logic. JustAPI's edge is: (a) measured throughput advantage on all three standard workloads, (b) a native fast path (`native=True`) that eliminates the Python dispatch entirely for schema-backed routes, (c) an ASGI shim for compatibility with the Starlette middleware ecosystem, and (d) built-in MCP agent serving as a first-class primitive rather than an add-on.

- **Granian** — Rust ASGI *server*. It drops under existing FastAPI/Starlette apps without changing framework code. If your goal is a faster transport layer without migrating frameworks, Granian is the lower-friction choice. JustAPI wins when you want the full Rust stack — routing, serialization, middleware, auth, and agent serving — all native.

- **Litestar / BlackSheep** — Python-native frameworks with first-class ASGI support and rich feature sets (DI, validation, OpenAPI). JustAPI shares their design philosophy for ergonomics but implements the critical path in Rust. Migration from these is similar to FastAPI.

- **FastAPI** — the compatibility target. JustAPI mirrors its decorator API and Pydantic model integration so existing FastAPI applications can migrate with minimal changes — often just the import statement.

### Honest caveats

- **Native fast path requires a schema.** Routes without a `Schema` registered fall back to the Python handler path (~60k rps). The 724k rps number only applies to routes with `native=True` + `Schema`.
- **Python handler throughput is GIL-bound.** On CPython, the Python dispatch path is capped at ~100-120k rps on this hardware regardless of framework — that is the GIL, not a framework limitation. Free-threaded Python (3.13t/3.14t) removes this ceiling.
- **Agent-native workloads are the differentiator.** The benchmarks that matter most for JustAPI are structured-LLM streaming, MCP tool dispatch, and durable agent sessions — not hello-world req/s. These workloads are impossible or awkward in every alternative listed above.

## Non-goals & not-yet-built

JustAPI is **not** trying to be everything in v1. We do not ship a custom
edge/WASM runtime, a from-scratch ORM/migrations system, a multi-tenant auth
platform, or a new validator (we wrap Pydantic v2 core / JSON Schema). See
[`ROADMAP.md`](ROADMAP.md) for the full, explicitly-labeled future list.

## Requirements

* **Python 3.11+**
* **Rust 1.85+** (only for building from source)

## Installation

### From PyPI (recommended)

```bash
pip install justapi
```

With optional features (Pydantic v2, Jinja2, XML):

```bash
pip install "justapi[full]"
```

### Via uv (faster)

[uv](https://docs.astral.sh/uv/) is the recommended Python package manager:

```bash
uv pip install justapi
uv pip install "justapi[full]"
```

### Run without installing

```bash
uvx justapi create my_app
uvx justapi serve --reload
```

### Build from source

```bash
cd crates/justapi-py
maturin develop --release
```

## Example

### Create it

Create a file `main.py` with:

```python
from justapi import JustAPIApp

app = JustAPIApp()


@app.get("/")
def read_root():
    return {"Hello": "World"}


@app.get("/items/{item_id}")
def read_item(item_id: int, q: str | None = None):
    return {"item_id": item_id, "q": q}
```

### Run it

```console
$ python main.py

INFO:     JustAPI running on http://127.0.0.1:8000
INFO:     Rust runtime started with 4 worker threads
```

### Check it

Open your browser at <a href="http://127.0.0.1:8000/items/5?q=somequery" target="_blank">http://127.0.0.1:8000/items/5?q=somequery</a>.

You will see the JSON response as:

```json
{"item_id": 5, "q": "somequery"}
```

You already created an API that:

* Receives HTTP requests at the paths `/` and `/items/{item_id}`.
* Both paths take `GET` <em>operations</em> (also known as HTTP <em>methods</em>).
* The path `/items/{item_id}` has a **path parameter** `item_id` that should be an `int`.
* The path `/items/{item_id}` has an optional `str` **query parameter** `q`.

### Interactive API docs

Now go to <a href="http://127.0.0.1:8000/docs" target="_blank">http://127.0.0.1:8000/docs</a>.

You will see the automatic interactive API documentation (provided by <a href="https://github.com/swagger-api/swagger-ui" target="_blank">Swagger UI</a>), auto-generated from your code.

## Example upgrade

Now modify the file `main.py` to receive a body from a `PUT` request.

Declare the body using standard Python types, thanks to <a href="https://docs.pydantic.dev/" target="_blank">Pydantic</a>:

```python
from justapi import JustAPIApp
from pydantic import BaseModel

app = JustAPIApp()


class Item(BaseModel):
    name: str
    description: str | None = None
    price: float
    tax: float | None = None


@app.get("/")
def read_root():
    return {"Hello": "World"}


@app.get("/items/{item_id}")
def read_item(item_id: int):
    return {"item_id": item_id}


@app.put("/items/{item_id}")
def update_item(item_id: int, item: Item):
    return {"item_name": item.name, "item_id": item_id, "price_with_tax": item.price + (item.tax or 0)}
```

The server will reload automatically (when using `--reload`).

### Interactive API docs upgrade

Now go to <a href="http://127.0.0.1:8000/docs" target="_blank">http://127.0.0.1:8000/docs</a>.

* The interactive API documentation will be automatically updated, including the new body.
* Click on the "Try it out" button, it allows you to fill the parameters and directly interact with the API.
* Then click on the "Execute" button, the user interface will communicate with your API, send the parameters, get the results and show them on the screen.

### Alternative API docs

And now go to <a href="http://127.0.0.1:8000/redoc" target="_blank">http://127.0.0.1:8000/redoc</a>.

* The alternative documentation will also reflect the new query parameter and body.

## Example with dependencies

JustAPI supports FastAPI-style dependency injection:

```python
from justapi import JustAPIApp, Depends

app = JustAPIApp()


def common_parameters(q: str | None = None, skip: int = 0, limit: int = 100):
    return {"q": q, "skip": skip, "limit": limit}


@app.get("/items/")
def read_items(commons: dict = Depends(common_parameters)):
    return {"message": "Reading items", "params": commons}


@app.get("/users/")
def read_users(commons: dict = Depends(common_parameters)):
    return {"message": "Reading users", "params": commons}
```

## Example with WebSockets

```python
from justapi import JustAPIApp

app = JustAPIApp()


@app.websocket("/ws")
async def websocket_endpoint(ws):
    await ws.accept()
    while True:
        data = await ws.receive_text()
        await ws.send_text(f"Echo: {data}")
```

## Project Scaffolding CLI

JustAPI includes an interactive project generator to create production-ready applications with zero boilerplate:

```bash
# Interactive project creation (prompts for DB engine and API architecture):
justapi create my_app

# Or specify flags directly:
justapi create analytics_app --db duckdb --api-type rest
justapi create graph_app --db postgres --api-type graphql
justapi create rpc_service --db redis --api-type grpc
justapi create jsonrpc_api --db sqlite --api-type jsonrpc
```

### Supported Backends & Protocols:
* **Databases:** Transactional SQL (`sqlite`, `postgres`, `mysql`), Analytical OLAP (`duckdb`, `clickhouse`), NoSQL (`mongodb`, `redis`).
* **Protocols:** REST (OpenAPI 3.1), GraphQL (GraphiQL UI), gRPC (Protobuf), JSON-RPC 2.0.
* **Observability:** Includes pre-built OpenTelemetry & Jaeger stack (`docker-compose.otel.yml`).

## Features

<table>
<tr><th>Category</th><th>Feature</th><th>Details</th></tr>

<tr><td rowspan="4"><strong>Performance</strong></td>
    <td>HTTP/1.1 & HTTP/2</td><td>700k+ req/s (native fast path)</td></tr>
<tr><td>TLS (rustls)</td><td>~10% overhead, no OpenSSL dependency</td></tr>
<tr><td>Serialization</td><td>serde_json with optional simd-json</td></tr>
<tr><td>Compression</td><td>gzip / deflate / brotli / zstd</td></tr>

<tr><td rowspan="5"><strong>Security</strong></td>
    <td>JWT Authentication</td><td>RS256/ES256, per-route roles & scopes</td></tr>
<tr><td>Rate Limiting</td><td>GCRA algorithm, Redis-backed distributed</td></tr>
<tr><td>CORS</td><td>Configurable, preflight caching</td></tr>
<tr><td>Request Validation</td><td>JSON Schema, Pydantic v2 bridge</td></tr>
<tr><td>Security Headers</td><td>HSTS, CSP, X-Frame-Options, etc.</td></tr>

<tr><td rowspan="5"><strong>Protocols</strong></td>
    <td>REST</td><td>Full HTTP method support with OpenAPI 3.1</td></tr>
<tr><td>WebSocket</td><td>Full-duplex with streaming support</td></tr>
<tr><td>Server-Sent Events</td><td>Token-generation optimized streaming</td></tr>
<tr><td>gRPC</td><td>Native Tonic integration</td></tr>
<tr><td>GraphQL</td><td>async-graphql with Apollo Federation</td></tr>

<tr><td rowspan="5"><strong>Application</strong></td>
    <td>Dependency Injection</td><td>Litestar-style tiered DI container</td></tr>
<tr><td>Background Tasks</td><td>Thread-safe task scheduling</td></tr>
<tr><td>Database ORM</td><td>SQLite / PostgreSQL / MySQL via sqlx</td></tr>
<tr><td>Template Engine</td><td>Jinja2 integration</td></tr>
<tr><td>File Uploads</td><td>Streaming multipart/form-data</td></tr>

<tr><td rowspan="5"><strong>Operations</strong></td>
    <td>Observability</td><td>OpenTelemetry tracing, Prometheus metrics</td></tr>
<tr><td>Resilience</td><td>Circuit breakers, retry, fallback, bulkhead</td></tr>
<tr><td>Health Checks</td><td>Liveness & readiness probe endpoints</td></tr>
<tr><td>Plugin System</td><td>Third-party extension API with lifecycle hooks</td></tr>
<tr><td>WASM Plugins</td><td>wasmtime-powered WebAssembly middleware</td></tr>

<tr><td rowspan="4"><strong>Quality</strong></td>
    <td>Memory Safety</td><td>Miri-verified, 6 fuzz targets</td></tr>
<tr><td>Supply Chain Security</td><td>cargo-deny v0.20 — advisories, bans, licenses, sources</td></tr>
<tr><td>Type Stubs</td><td>Complete .pyi for all public modules</td></tr>
<tr><td>Test Utilities</td><td>Built-in TestClient & snapshot testing</td></tr>
</table>

## Coming from FastAPI?

JustAPI is designed to feel familiar. Here's a side-by-side:

<table>
<tr><th></th><th>FastAPI</th><th>JustAPI</th></tr>
<tr>
<td><strong>Import</strong></td>
<td><code>from fastapi import FastAPI</code></td>
<td><code>from justapi import JustAPIApp</code></td>
</tr>
<tr>
<td><strong>Create app</strong></td>
<td><code>app = FastAPI()</code></td>
<td><code>app = JustAPIApp()</code></td>
</tr>
<tr>
<td><strong>Decorators</strong></td>
<td><code>@app.get("/items/{id}")</code></td>
<td><code>@app.get("/items/{id}")</code></td>
</tr>
<tr>
<td><strong>Pydantic</strong></td>
<td>Native</td>
<td>Native</td>
</tr>
<tr>
<td><strong>Depends()</strong></td>
<td>Built-in</td>
<td>Built-in</td>
</tr>
<tr>
<td><strong>OpenAPI docs</strong></td>
<td><code>/docs</code> and <code>/redoc</code></td>
<td><code>/docs</code> and <code>/redoc</code></td>
</tr>
<tr>
<td><strong>Server</strong></td>
<td>uvicorn (Python)</td>
<td>Built-in Rust server (hyper + tokio)</td>
</tr>
<tr>
<td><strong>Performance</strong></td>
<td>~36k req/s</td>
<td><strong>~700k req/s</strong></td>
</tr>
</table>

👉 See the full [migration guide](docs/migrating_from_fastapi.md).

## Architecture

```
justapi/
├── crates/
│   ├── justapi-core/       # Networking, routing, middleware, serialization
│   ├── justapi-py/         # PyO3 bindings, native Python API
│   ├── justapi-cli/        # CLI binary (serve, create, migrate)
│   ├── justapi-bench/      # Internal benchmark harness
│   └── justapi-inference/  # ML inference engine (optional)
├── examples/               # Tutorial examples
├── tests/                  # Integration tests
├── fuzz/                   # cargo-fuzz security targets (6)
├── docs/                   # MkDocs documentation
├── deploy/                 # Cloud deployment guides
├── helm/                   # Kubernetes Helm chart
├── benchmarks/             # Performance workload scripts
└── website/                # Project landing page
```

### Crate dependency graph

```
justapi-cli  ──→  justapi-core
justapi-py   ──→  justapi-core
justapi-bench ─→  justapi-core
```

## Development

### Prerequisites

- **Rust** 1.85+ (edition 2021) — [install via rustup](https://rustup.rs/)
- **Python** 3.11+ — [download](https://www.python.org/downloads/)
- **maturin** — `pip install maturin` (or `uv pip install maturin`)
- **uv** (optional, recommended) — [install guide](https://docs.astral.sh/uv/)

### Building

```bash
# Build the workspace
cargo build --workspace

# Build in release mode
cargo build --workspace --release

# Build the Python package (dev mode)
cd crates/justapi-py && maturin develop --release
```

### Testing

```bash
# Run all Rust tests
cargo test --workspace

# Lint
cargo clippy --workspace --tests -- -D warnings

# Format check
cargo fmt --check

# Supply chain security audit
cargo deny check

# Memory safety verification (requires nightly)
cargo miri test -p justapi-core

# Python tests
cd crates/justapi-py && python -m pytest

# Python tests with uv
uv run pytest
```

### Benchmarking

```bash
# Install load generator
cargo install oha

# Run the baselines (FastAPI, Granian, JustAPI)
bash benchmarks/run_baselines.sh
```

See [BENCHMARKS.md](BENCHMARKS.md) for full methodology, hardware specs, and historical results.

## Deployment

### Docker (uv-based)

```bash
docker build -t justapi-app .
docker run -p 8000:8000 justapi-app
```

The multi-stage `Dockerfile` uses `uv` for fast, reproducible pip installs.

### Docker Compose

```bash
docker-compose up
```

### Kubernetes (Helm)

```bash
helm install my-api helm/justapi/ \
  --set image.repository=your-registry/justapi-app \
  --set image.tag=latest
```

### Cloud platforms

Step-by-step deployment guides are available for:

- [Amazon EKS](deploy/clouds/eks.md)
- [Google GKE](deploy/clouds/gke.md)
- [Azure AKS](deploy/clouds/aks.md)
- [Fly.io](deploy/clouds/flyio.md)
- [Railway](deploy/clouds/railway.md)

## Documentation

| Document | Description |
|---|---|
| [**Getting Started**](docs/getting_started.md) | Installation, first app, and core concepts |
| [**API Reference**](docs/api_reference.md) | Complete API documentation |
| [**Migrating from FastAPI**](docs/migrating_from_fastapi.md) | Step-by-step migration guide |
| [**Plugin Guide**](docs/plugins.md) | Building and distributing plugins |
| [**Security**](docs/security/) | OWASP checklist, pentest guide, disclosure policy |
| [**BENCHMARKS.md**](BENCHMARKS.md) | Performance results (append-only ledger) |
| [**PLAN.md**](PLAN.md) | Living roadmap and current phase |
| [**ROADMAP.md**](ROADMAP.md) | Aspirational future work — explicitly *not yet built* |
| [**DECISIONS.md**](DECISIONS.md) | Architecture decision records |
| [**PRODUCTION_PLAN.md**](PRODUCTION_PLAN.md) | 2.0.8 pre-release hardening plan |
| [**AGENTS.md**](AGENTS.md) | Development rules and conventions |

## Contributing

Contributions are welcome! Please read [AGENTS.md](AGENTS.md) for development conventions, commit format, and review checklist before submitting a PR.

```bash
# Fork and clone
git clone https://github.com/YOUR_USERNAME/JustAPI.git
cd JustAPI

# Create a branch
git checkout -b feat/my-feature

# Make changes, then run the full check suite
cargo test --workspace
cargo clippy --workspace --tests -- -D warnings
cargo fmt --check
cargo deny check

# Submit a PR
```

## License

This project is licensed under the terms of the [MIT License](LICENSE).
