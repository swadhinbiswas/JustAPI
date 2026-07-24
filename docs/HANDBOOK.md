# 📖 JustAPI Developer Handbook & Master Reference Guide

Welcome to the official handbook for **JustAPI** — a modern, high-performance Python web framework powered by a native **Rust core**. 

JustAPI is designed as a **drop-in, Rust-accelerated replacement for FastAPI**. It combines FastAPI’s beloved developer experience (decorators, type annotations, Pydantic/Schema validation, automatic OpenAPI docs) with a multi-threaded Rust networking and routing engine that delivers **up to 20× the throughput of FastAPI+Uvicorn**.

---

## Table of Contents

1. [Architecture & Philosophy](#1-architecture--philosophy)
2. [Installation & Setup](#2-installation--setup)
3. [Quickstart & Project Generator](#3-quickstart--project-generator)
4. [Routing & Request Handling](#4-routing--request-handling)
5. [Schemas & Request Validation](#5-schemas--request-validation)
6. [Multi-Protocol APIs (REST, GraphQL, gRPC, JSON-RPC)](#6-multi-protocol-apis)
7. [Database Backend & Fast-Path I/O](#7-database-backend--fast-path-io)
8. [Dependency Injection & Middleware](#8-dependency-injection--middleware)
9. [WebSockets, SSE & Real-Time Streaming](#9-websockets-sse--real-time-streaming)
10. [Resilience & Security Safeguards](#10-resilience--security-safeguards)
11. [Observability & Monitoring](#11-observability--monitoring)
12. [Production Deployment & CI/CD](#12-production-deployment--cicd)

---

## 1. Architecture & Philosophy

### Rust-First Performance Thesis

In standard ASGI frameworks like FastAPI + Uvicorn or Starlette, every incoming connection, HTTP header parse, route match, JSON validation, and response serialization is executed inside Python under the **Global Interpreter Lock (GIL)**.

JustAPI moves 95%+ of the request lifecycle out of Python entirely:

```
[ Client Connection ]
        │
        ▼
┌─────────────────────────────────────────────────────────────┐
│  RUST RUNTIME (justapi-core)                                │
│  • Socket I/O (tokio) & TLS termination (rustls)            │
│  • HTTP/1.1 & HTTP/2 Parsing (hyper)                        │
│  • Radix Trie Routing (matchit)                             │
│  • Middleware (Auth/JWT, CORS, GCRA Rate Limiting)          │ 🟢 GIL RELEASED
│  • Precompiled JSON Schema Validation (jsonschema-rs)      │ (Zero GIL Lock!)
│  • Connection Pool & Database Queries (sqlx / AnyPool)      │
│  • JSON Serialization (serde_json / simd-json)              │
└──────────────────────────────┬──────────────────────────────┘
                               │
       Invoked only for Python application logic:
                               ▼
┌─────────────────────────────────────────────────────────────┐
│  PYTHON APPLICATION LAYER (justapi-py)                      │
│  • Business logic execution inside dedicated gil_pool       │ 🟡 PyO3 GIL Thread
└─────────────────────────────────────────────────────────────┘
```

### Zero-GIL Fast Path Mechanics
* **Python 3.11 & 3.12:** PyO3 releases the GIL via `py.allow_threads` during all I/O, routing, DB queries, and JSON processing.
* **Python 3.13t & 3.14t (Free-Threaded):** Full support for PEP 703 free-threading — Python handler execution runs concurrently across all CPU cores without GIL locks.

---

## 2. Installation & Setup

### Installing via pip

```bash
pip install justapi
```

To install optional dependencies (e.g. Pydantic v2, Jinja2):

```bash
pip install "justapi[full]"
```

### Building from Source

Requirements: **Python 3.11+** and **Rust 1.85+**.

```bash
git clone https://github.com/swadhinbiswas/JustAPI.git
cd JustAPI
maturin develop --release --manifest-path crates/justapi-py/Cargo.toml
```

---

## 3. Quickstart & Project Generator

### Hello World Application (`app.py`)

```python
from justapi import JustAPIApp

app = JustAPIApp(title="My First JustAPI App", version="0.1.0")


@app.get("/")
def root(request):
    return {"message": "Welcome to JustAPI!"}


@app.get("/items/{item_id}")
def read_item(request, item_id: int):
    return {"item_id": item_id, "status": "active"}
```

Run directly in Python:

```bash
python app.py
```

### Interactive Project Scaffolder CLI

JustAPI includes a project generator supporting multiple database backends and API architecture templates:

```bash
# Interactive project generator (prompts for DB and API protocol choice):
justapi create my_app

# Or pass explicit configuration flags:
justapi create analytics_service --db duckdb --api-type rest
justapi create graph_service --db postgres --api-type graphql
justapi create rpc_service --db redis --api-type grpc
justapi create jsonrpc_service --db sqlite --api-type jsonrpc
```

### Hot-Reload Development Server

```bash
cd my_app
justapi serve --reload
```

---

## 4. Routing & Request Handling

### HTTP Method Decorators

JustAPI supports standard HTTP verb decorators:

```python
@app.get("/users")
def list_users(request):
    ...

@app.post("/users")
def create_user(request):
    ...

@app.put("/users/{user_id}")
def update_user(request, user_id: int):
    ...

@app.patch("/users/{user_id}")
def patch_user(request, user_id: int):
    ...

@app.delete("/users/{user_id}")
def delete_user(request, user_id: int):
    ...
```

### Sub-Routers (`APIRouter`)

Organize routes across modular files:

```python
# app/routers/products.py
from justapi import APIRouter

router = APIRouter(prefix="/products", tags=["Products"])

@router.get("/")
def list_products(request):
    return [{"id": 1, "name": "Rust Book"}]
```

Include routers in the main application:

```python
# app/main.py
from justapi import JustAPIApp
from app.routers.products import router as products_router

app = JustAPIApp()
app.include_router(products_router)
```

---

## 5. Schemas & Request Validation

### Defining Schemas (`justapi.Schema`)

JustAPI provides a lightweight, type-safe schema class:

```python
from justapi import JustAPIApp, Schema
from pydantic import Field

app = JustAPIApp()


class ItemCreate(Schema):
    name: str = Field(..., min_length=1, description="Item name")
    price: float = Field(..., gt=0, description="Item price")
    qty: int = Field(0, ge=0, description="Stock quantity")


@app.post("/items", body_schema=ItemCreate)
def create_item(request):
    data = request.json()
    return {"message": "Item validated & created", "item": data}
```

### Native Rust Fast-Path (`native=True`)

For ultimate throughput (**724,000+ RPS**), pass `native=True` to execute body validation and response generation **100% inside Rust** without touching Python:

```python
@app.post("/fast-items", body_schema=ItemCreate, native=True)
def fast_create_item(request):
    return {"status": "ok"}
```

---

## 6. Multi-Protocol APIs

JustAPI natively supports four API architecture styles out of the box:

### 1. REST API (OpenAPI 3.1 & Docs)
Auto-generated interactive API documentation available at:
* **Swagger UI:** `http://localhost:8080/docs`
* **ReDoc:** `http://localhost:8080/redoc`
* **Scalar UI:** `http://localhost:8080/scalar`
* **OpenAPI Spec:** `http://localhost:8080/openapi.json`

### 2. GraphQL API
Mount GraphiQL UI and execution engine in one line:

```python
app.graphql(path="/graphql")
# Interactive GraphiQL UI available at http://localhost:8080/graphql
```

### 3. gRPC / Protobuf RPC
High-performance RPC endpoints:

```python
class RPCRequest(Schema):
    method: str
    params: dict

@app.post("/rpc", body_schema=RPCRequest)
def handle_rpc(request):
    data = request.json()
    return {"status": "OK", "method": data["method"]}
```

### 4. JSON-RPC 2.0 Protocol
Standard JSON-RPC 2.0 endpoint:

```python
class JSONRPCRequest(Schema):
    jsonrpc: str = "2.0"
    method: str
    params: list | dict = []
    id: int | str = 1

@app.post("/jsonrpc", body_schema=JSONRPCRequest)
def handle_jsonrpc(request):
    req = request.json()
    if req["method"] == "ping":
        return {"jsonrpc": "2.0", "result": "pong", "id": req["id"]}
```

---

## 7. Database Backend & Fast-Path I/O

JustAPI provides a Rust-native connection pool manager (`app.db` / `AnyPool`) achieving **177,000+ RPS** query reads.

### Configuring Database Connection

```python
app = JustAPIApp()

# Wire PostgreSQL
app.set_database(
    "postgres://postgres:password@localhost:5432/mydb",
    init_sql="""
    CREATE TABLE IF NOT EXISTS items (
        id SERIAL PRIMARY KEY,
        name TEXT NOT NULL,
        qty INT DEFAULT 0
    )
    """,
)

@app.get("/items")
def list_items(request):
    # Runs inside Rust database connection pool
    return app.db.query("SELECT * FROM items ORDER BY id")

@app.post("/items")
def add_item(request):
    data = request.json()
    app.db.execute("INSERT INTO items (name, qty) VALUES ($1, $2)", [data["name"], data["qty"]])
    return {"message": "Inserted"}
```

### Supported Database Engines

| Engine | Scheme | Library / Driver | Usage |
|---|---|---|---|
| **PostgreSQL** | `postgres://` | Rust `sqlx-postgres` | High-throughput relational OLTP |
| **MySQL** | `mysql://` | Rust `sqlx-mysql` | Scalable relational OLTP |
| **SQLite** | `sqlite://` | Rust `sqlx-sqlite` | Zero-config embedded DB |
| **DuckDB** | `duckdb://` | Python `duckdb` | Fast in-process analytical SQL & Parquet |
| **ClickHouse**| `clickhouse://`| Python `clickhouse-driver` | High-volume analytical column store |
| **MongoDB** | `mongodb://` | Python `pymongo` | NoSQL document storage |
| **Redis** | `redis://` | Python `redis` | Key-value caching & rate limiting |

---

## 8. Dependency Injection & Middleware

### Dependency Injection (`Depends`)

Declare reusable dependencies:

```python
from justapi import JustAPIApp, Depends, HTTPException

app = JustAPIApp()

def verify_token(request):
    token = request.headers.get("authorization")
    if not token or token != "Bearer secret-token":
        raise HTTPException(status_code=401, detail="Invalid token")
    return {"user_id": 42, "role": "admin"}

@app.get("/protected")
def protected_route(request, user: dict = Depends(verify_token)):
    return {"message": "Access granted", "user": user}
```

### Starlette / FastAPI Middleware Parity

Add middleware using standard Starlette/FastAPI syntax:

```python
# Configure CORS
app.add_cors(
    allow_origins=["https://example.com"],
    allow_methods=["GET", "POST", "PUT", "DELETE"],
    allow_headers=["*"],
    allow_credentials=True,
)

# Custom HTTP middleware
@app.middleware("http")
def custom_middleware(request, call_next):
    print(f"Before request: {request.path}")
    response = call_next(request)
    print(f"After request: status {response.status_code}")
    return response
```

---

## 9. WebSockets, SSE & Real-Time Streaming

### WebSockets

Full-duplex WebSocket communication:

```python
@app.websocket("/ws")
async def websocket_handler(ws):
    await ws.accept()
    while True:
        msg = await ws.receive_text()
        await ws.send_text(f"Echo: {msg}")
```

### Server-Sent Events (SSE)

Stream events to browser clients:

```python
import asyncio
from justapi.responses import StreamingResponse

@app.get("/events")
async def event_stream(request):
    async def generate():
        for i in range(10):
            yield f"data: Event message #{i}\n\n"
            await asyncio.sleep(1)
    return StreamingResponse(generate(), media_type="text/event-stream")
```

---

## 10. Resilience & Security Safeguards

JustAPI incorporates enterprise resilience primitives directly inside the Rust engine:

### Circuit Breakers (`enable_circuit_breaker`)

Automatically open circuit breakers when failure thresholds are exceeded:

```python
# Open circuit after 5 consecutive failures, reset after 10,000 ms
app.enable_circuit_breaker(failure_threshold=5, reset_timeout_ms=10000)
```

### Request Coalescing (`enable_request_coalescing`)

Deduplicate concurrent identical requests so only 1 request hits the backend while all callers share the response:

```python
app.enable_request_coalescing(headers=["accept"])
```

### Security Headers (`enable_secure_headers`)

Apply `X-Content-Type-Options`, `X-Frame-Options`, `X-XSS-Protection`, and `Content-Security-Policy`:

```python
app.enable_secure_headers(with_hsts=True)
```

---

## 11. Observability & Monitoring

### OpenTelemetry Distributed Tracing

Initialize OTLP gRPC span exporter:

```python
from justapi.tracing import init_otlp_tracing, shutdown_tracing

init_otlp_tracing(endpoint="http://localhost:4317", service_name="my_service")
```

### Prometheus Metrics & Health Probes

Built-in system endpoints:
* `/health` — Returns JSON application status
* `/ready` — Executes readiness probes
* `/live` — Executes liveness probes
* `/metrics` — Prometheus metrics text format

### OpenTelemetry Compose Stack

Project templates generated by `justapi create` include `docker-compose.otel.yml` for Jaeger, Prometheus, and Grafana.

---

## 12. Production Deployment & CI/CD

### Multi-Worker CLI Execution

Utilize all available CPU cores:

```bash
justapi serve --host 0.0.0.0 --port 8080 --workers 4
```

### Optimized Dockerfile

```dockerfile
FROM python:3.12-slim

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

EXPOSE 8080
ENV HOST=0.0.0.0 PORT=8080

CMD ["justapi", "serve", "--host", "0.0.0.0", "--port", "8080"]
```

---

*JustAPI Runtime Manual — Version 2.0.0*
