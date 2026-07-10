# JustAPI Documentation

Welcome to the official documentation for **JustAPI**, a high-performance Python web framework built on top of a powerful Rust core.

JustAPI is designed to be a drop-in replacement for FastAPI but engineered to handle extreme throughput—achieving up to **700k Requests Per Second (RPS)** without breaking a sweat.

## Why JustAPI?

### 🚀 Rust Core Performance
The entire HTTP server, routing engine, and protocol parsing are written in Rust using highly optimized crates like `tokio` and `hyper`. This ensures that connection handling is as fast and secure as theoretically possible.

### 🐍 Python Ergonomics
You get the raw performance of Rust with the beautiful, developer-friendly ergonomics of Python. You write standard Python using familiar concepts like `JustAPIApp`, `@app.get`, and Pydantic schemas for data validation.

### ⚡ Zero-Copy Architecture
JustAPI uses a highly optimized boundary between Rust and Python (via PyO3). By implementing the Python Buffer Protocol, the framework avoids unnecessary memory allocations and copies, making it incredibly memory-efficient.

### 🔄 100% Async
Built entirely on `async/await` principles. JustAPI seamlessly bridges Rust's Tokio runtime with Python's native `asyncio` event loop.

## Performance Matrix

JustAPI obliterates traditional frameworks under load. Measured on an Intel Core i5-13600K against the Uvicorn+FastAPI baseline:

| Metric / Workload | Uvicorn + FastAPI | JustAPI (Native Python) | Improvement |
|-------------------|-------------------|-------------------------|-------------|
| **Hello-World (GET)** throughput | 36,189 req/sec | **324,130 req/sec** | **~9x Faster** |
| **Hello-World (GET)** p99 latency | 24.63 ms | **1.10 ms** | **~22x Lower** |
| **JSON-Echo (POST)** throughput | 26,000 req/sec | **267,846 req/sec** | **~10x Faster** |
| **JSON-Echo (POST)** p99 latency | 14.50 ms | **1.64 ms** | **~9x Lower** |

*(Note: JustAPI achieves these numbers in a single process without multi-worker fan-out.)*

## Where to go next?

- **[Getting Started](getting_started.md):** Build your first JustAPI application in minutes.
- **[Migrating from FastAPI](migrating_from_fastapi.md):** See how easy it is to switch your existing FastAPI project to JustAPI.
- **[API Reference](api_reference.md):** Detailed information about all the classes and functions available.
- **[Plugins](plugins.md):** Learn how to extend JustAPI using Python or Rust.
