---
title: Zero-GIL Architecture & PyO3 Threads
description: Deep dive into how JustAPI releases the Python GIL for native performance.
---

## How the GIL Works

In standard CPython (3.11 & 3.12), the Global Interpreter Lock (GIL) prevents multiple OS threads from executing Python bytecode simultaneously.

However, the GIL **only locks Python interpreter execution** — it does NOT lock native C/Rust code. JustAPI exploits this by moving 95% of request processing into Rust, where the GIL is released.

## The `py.allow_threads` Mechanism

In `justapi-core` and PyO3 native bindings, the GIL is explicitly released during all I/O, routing, and validation:

```rust
// Rust releases GIL during I/O, routing, and DB operations:
py.allow_threads(|| {
    // 1. Socket TCP read
    // 2. TLS rustls decryption
    // 3. Matchit radix trie path resolution
    // 4. Precompiled JSON Schema validation
    // 5. Rust sqlx database query
    // 6. serde_json response serialization
});
```

Because 95%+ of request processing occurs inside `py.allow_threads`, **JustAPI achieves true multi-core CPU scaling even on Python 3.11 & 3.12**.

## Request Lifecycle (GIL States)

```
Client Request
    │
    ▼
┌──────────────────────────────────────┐
│ Rust: Accept connection              │ 🔴 No GIL needed (native tokio)
│ Rust: TLS handshake (rustls)         │ 🔴 No GIL needed
│ Rust: HTTP parse (hyper)             │ 🔴 No GIL needed
│ Rust: Route match (matchit)          │ 🔴 No GIL needed
│ Rust: Middleware chain               │ 🔴 No GIL needed
│ Rust: JSON Schema validation         │ 🔴 No GIL needed
│ Rust: DB query (sqlx)                │ 🔴 No GIL needed
│ Rust: JSON serialize (serde_json)    │ 🔴 No GIL needed
│    │
│    ▼
│ PyO3: Acquire GIL                   │ 🟡 Brief GIL acquisition
│ Python: Execute handler              │ 🟡 GIL held
│ PyO3: Release GIL                    │ 🟡 Release
│    │
│    ▼
│ Rust: Write response                 │ 🔴 No GIL needed
└──────────────────────────────────────┘
```

## Free-Threading Support (Python 3.13t & 3.14t)

When compiled against Python 3.13t or 3.14t free-threaded builds (`Py_GIL_DISABLED=1`), PyO3 automatically disables GIL synchronization, allowing Python handler execution in `gil_pool.rs` to run fully parallel across all CPU cores.

```bash
# Run with free-threaded Python
PYTHON_GIL=0 python main.py
```

## GIL Pool Architecture

The `gil_pool` subsystem manages a pool of GIL-bound worker threads:

1. Incoming requests that need Python execution are dispatched to the GIL pool
2. Each worker acquires the GIL, executes the Python handler, and releases it
3. The pool size is auto-tuned based on workload
4. Under the standard GIL (3.11/3.12), this provides fairness across requests

## Performance Impact

On standard CPython 3.12:

| Scenario | RPS | GIL Contention |
|---|---|---|
| 100% Rust native path | 724,000 | None |
| Python handler (simple) | 60,000 | Low |
| Python handler (CPU-bound) | 12,000 | High |
| Free-threaded 3.13t | Scaling | None |

## See Also

- [Native Fast Path](/advanced/native-fast-path/) — 100% Rust route execution
- [PyO3 & FFI Safety](/advanced/pyo3-ffi-safety/) — Safe crossing between Rust and Python
- [Performance Tuning](/advanced/performance-tuning/) — Optimize your application
