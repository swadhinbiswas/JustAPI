---
title: Performance Tuning
description: Optimize JustAPI for maximum throughput and minimum latency.
---

# JustAPI Performance Tuning Guide

Optimize JustAPI for maximum throughput and minimum latency.

---

## Table of Contents

1. [Tokio Runtime Configuration](#1-tokio-runtime-configuration)
2. [GIL Pool Tuning](#2-gil-pool-tuning)
3. [Buffer Pool Optimization](#3-buffer-pool-optimization)
4. [Connection Pool Tuning](#4-connection-pool-tuning)
5. [Middleware Optimization](#5-middleware-optimization)
6. [Compression Settings](#6-compression-settings)
7. [Caching Strategies](#7-caching-strategies)
8. [Database Performance](#8-database-performance)
9. [Profiling & Benchmarking](#9-profiling--benchmarking)
10. [Production Checklist](#10-production-checklist)

---

## 1. Tokio Runtime Configuration

### Worker Threads

JustAPI uses Tokio's multi-threaded runtime. The default worker count matches your CPU cores.

**Custom worker count:**
```python
from justapi import JustAPIApp

app = JustAPIApp()
app.run(
    addr="0.0.0.0:8000",
    workers=8  # Explicit worker count
)
```

**Environment variable:**
```bash
# Override Tokio worker count
export TOKIO_WORKER_THREADS=8
python app.py
```

**Recommendations:**
- **CPU-bound workloads:** Set workers = CPU cores
- **I/O-bound workloads:** Set workers = 2 × CPU cores
- **Mixed workloads:** Start with CPU cores, tune based on profiling

### Thread Stack Size

Default stack size is 2MB per thread. For deep recursion, increase it:

```bash
# Increase thread stack size (in bytes)
export RUST_MIN_STACK=8388608  # 8MB
```

---

## 2. GIL Pool Tuning

### Understanding the GIL Pool

JustAPI uses a dedicated GIL pool to execute Python handlers without blocking Tokio workers. The pool size is auto-detected based on Python type:

- **CPython (GIL enabled):** 1 worker (GIL serializes execution)
- **Free-threaded Python (3.13t+):** Scales with CPU cores

### Monitoring GIL Pool

```python
# Check pool status
import justapi
print(f"GIL pool workers: {justapi.gil_pool_status()}")
```

### Optimizing Python Handlers

**Minimize GIL hold time:**
```python
# BAD: Long GIL hold
@app.get("/slow")
async def slow_handler():
    result = await db.query("SELECT * FROM large_table")
    # Process for 5 seconds
    processed = heavy_computation(result)
    return processed

# GOOD: Offload to Rust, minimize Python time
@app.get("/fast")
async def fast_handler():
    # Use native fast path when possible
    return {"status": "ok"}
```

**Use native fast path for validated routes:**
```python
from justapi import JustAPIApp, Schema

app = JustAPIApp()

class UserSchema(Schema):
    name: str
    age: int

# This runs entirely in Rust (no Python GIL)
@app.post("/users", schema=UserSchema, native=True)
async def create_user():
    return {"status": "created"}
```

---

## 3. Buffer Pool Optimization

### Understanding the Buffer Pool

JustAPI uses a size-class buffer pool to reduce allocation overhead:
- 1KB bucket
- 4KB bucket
- 16KB bucket
- 64KB bucket

### Configuring Buffer Pool

```python
from justapi import JustAPIApp

app = JustAPIApp()
app.run(
    addr="0.0.0.0:8000",
    buffer_pool_max_per_bucket=256  # Default is 128
)
```

### Monitoring Buffer Usage

```bash
# Check buffer pool metrics
curl http://localhost:8000/metrics | grep buffer
```

### Recommendations

- **High-throughput APIs:** Increase `max_per_bucket` to 256-512
- **Memory-constrained:** Decrease to 64
- **Large responses:** Consider streaming instead of buffering

---

## 4. Connection Pool Tuning

### Database Connection Pool

```python
from justapi import JustAPIApp, Database

app = JustAPIApp()
db = Database(
    "postgres://user:pass@localhost/db",
    max_connections=20,      # Pool size
    min_connections=5,       # Minimum idle connections
    connect_timeout=10,      # Connection timeout (seconds)
    idle_timeout=300,        # Idle connection timeout (seconds)
)
app.set_database(db)
```

### Redis Connection Pool

```python
from justapi import JustAPIApp

app = JustAPIApp()
app.set_redis(
    url="redis://localhost:6379",
    max_connections=10,
    connect_timeout=5
)
```

### Monitoring Pool Health

```bash
# Check database pool
curl http://localhost:8000/health | jq '.database'

# Check Redis pool
redis-cli info clients
```

---

## 5. Middleware Optimization

### Prefer Native Middleware

JustAPI's CORS, security headers, and JWT auth run in the Rust middleware
chain with zero GIL overhead. Prefer them over Python middleware:

```python
from justapi import JustAPIApp

app = JustAPIApp()

# Rust-native: CORS
app.add_cors(allow_origins=["https://example.com"])

# Rust-native: security headers (HSTS optional)
app.enable_secure_headers(with_hsts=True)

# Rust-native: JWT auth (validates every request in Rust)
app.set_jwt_auth(secret="your-hmac-secret")
```

### Python Middleware for Custom Logic

Custom per-request logic is an async Python middleware. Keep it to logic that
must run in Python — the native fast path bypasses Python middleware by design:

```python
@app.middleware("http")
async def profile_middleware(request, call_next):
    start = time.perf_counter()
    response = await call_next(request)
    duration = time.perf_counter() - start

    if duration > 0.1:  # Log slow middleware
        print(f"Slow middleware: {request.get('path')} took {duration:.3f}s")

    return response
```

> **Performance note:** Python middleware runs on the Python side of the
> boundary and is bypassed (by design) by the native fast path
> (`native=True`). For hot endpoints, prefer native middleware
> (`add_cors`, `enable_secure_headers`, `set_jwt_auth`) or no middleware.

### Middleware Profiling

The built-in metrics include per-request latency histograms — see
[Observability](/advanced/observability/) for p50/p95/p99 tracking.

---

## 6. Response Compression

Response compression (Gzip, with feature-gated Brotli/Zstd) is a Rust-side
server option:

```rust
// justapi-core / CLI: add compression to the server chain
let server = Server::new(addr).add_compression();
```

For Python apps, compression is applied per-response via a streaming or
static response with the appropriate `Content-Encoding` — see the
[Streaming Output](/advanced/streaming-output/) guide. The Rust
`CompressionMiddleware` compresses responses based on the client's
`Accept-Encoding`, with Brotli/Zstd enabled by the `brotli-compression` /
`zstd-compression` feature flags.

---

## 7. Caching Strategies

### Response Caching

### Static File Caching

JustAPI automatically sets cache headers for static files (served via
`app.frontend(...)` or a static mount):
- ETag-based validation
- `Cache-Control` headers
- 304 Not Modified responses

### Response Caching for Custom Routes

For route-level response caching, hold a short-lived cache yourself (e.g. a
dict keyed by path, or Redis via the `RateLimiter`/Redis integration) and
return the cached payload from the handler. JustAPI's request coalescing
(`app.enable_request_coalescing(...)`) collapses thundering-herd traffic on
identical concurrent requests into a single upstream call — a useful
read-path optimization for expensive endpoints.

---

## 8. Database Performance

### Query Optimization

```python
# BAD: N+1 query problem
@app.get("/users")
async def get_users():
    users = await db.execute("SELECT * FROM users")
    for user in users:
        user.posts = await db.execute(
            "SELECT * FROM posts WHERE user_id = ?", user.id
        )
    return users

# GOOD: Join query
@app.get("/users")
async def get_users():
    return await db.execute("""
        SELECT u.*, p.* 
        FROM users u 
        LEFT JOIN posts p ON u.id = p.user_id
    """)
```

### Connection Pool Sizing

```python
# Rule of thumb: connections = CPU cores × 2 + disk spindles
db = Database(
    "postgres://user:pass@localhost/db",
    max_connections=16  # For 8-core machine
)
```

### Enable Query Logging

```python
import logging
logging.getLogger("sqlx").setLevel(logging.DEBUG)
```

---

## 9. Profiling & Benchmarking

### Built-in Profiler

```bash
# Profile a running server
justapi profile --duration 10 --connections 50

# Generate flamegraph
justapi profile --flamegraph --output profile.svg
```

### Python Profiling

```python
import cProfile

@app.get("/profiled")
async def profiled_handler():
    profiler = cProfile.Profile()
    profiler.enable()
    
    result = await expensive_operation()
    
    profiler.disable()
    profiler.print_stats(sort='cumulative')
    
    return result
```

### Load Testing

```bash
# Install oha (Rust-based load generator)
cargo install oha

# Run benchmark
oha -z 30s -c 100 http://localhost:8000/api/endpoint

# Compare with FastAPI
oha -z 30s -c 100 http://localhost:8001/api/endpoint
```

### Memory Profiling

```bash
# Using mprof
mprof run python app.py
mprof plot

# Using tracemalloc
import tracemalloc
tracemalloc.start()
# ... run workload ...
snapshot = tracemalloc.take_snapshot()
top_stats = snapshot.statistics('lineno')
for stat in top_stats[:10]:
    print(stat)
```

---

## 10. Production Checklist

### Pre-Deployment

- [ ] **Release build:** `cargo build --release`
- [ ] **Feature flags:** Enable only needed features
- [ ] **Environment variables:** All required vars set
- [ ] **TLS certificates:** Valid and not expired
- [ ] **Database migrations:** Run before deployment
- [ ] **Health checks:** `/health`, `/ready`, `/live` endpoints working

### Runtime Configuration

- [ ] **Worker threads:** Match CPU cores
- [ ] **Connection pools:** Sized for expected load
- [ ] **Rate limiting:** Configured for API limits
- [ ] **Compression:** Enabled for text responses
- [ ] **Caching:** Appropriate cache headers
- [ ] **Logging:** Structured JSON for production

### Monitoring

- [ ] **Metrics:** Prometheus endpoint at `/metrics`
- [ ] **Tracing:** OpenTelemetry configured
- [ ] **Alerting:** Webhook configured for critical errors
- [ ] **Health checks:** K8s probes configured

### Security

- [ ] **CORS:** Properly configured for your domain
- [ ] **Security headers:** HSTS, CSP, X-Frame-Options
- [ ] **Rate limiting:** Prevents abuse
- [ ] **Input validation:** Schema validation enabled
- [ ] **SQL injection:** Parameterized queries only

---

## Quick Reference: Performance Targets

| Metric | Target | How to Measure |
|--------|--------|----------------|
| Hello-world RPS | > 100,000 | `oha -z 10s -c 100` |
| p50 latency | < 1ms | `oha` output |
| p99 latency | < 10ms | `oha` output |
| Memory usage | < 100MB | `ps aux` |
| Startup time | < 100ms | Time first response |
| Docker image | < 200MB | `docker images` |

---

## Getting Help

For performance issues:

1. **Run the profiler:** `justapi profile`
2. **Check metrics:** `curl http://localhost:8000/metrics`
3. **Review logs:** `RUST_LOG=debug python app.py`
4. **Open a GitHub Issue** with profiling data
