---
title: Zero-GIL Architecture & PyO3 Threads
description: Deep dive into how JustAPI releases Python GIL for native performance.
---

## How the GIL Works

In standard CPython (3.11 & 3.12), the Global Interpreter Lock (GIL) prevents multiple OS threads from executing Python bytecode simultaneously.

However, the GIL **only locks Python interpreter execution** — it does NOT lock native C/Rust code!

## The `py.allow_threads` Mechanism

In `justapi-core` and PyO3 native bindings:

```rust
// Rust releases GIL during I/O, routing, and DB operations:
py.allow_threads(|| {
    // 1. Socket TCP read
    // 2. TLS rustls decryption
    // 3. Matchit radix trie path resolution
    // 4. Precompiled JSON Schema validation
    // 5. Rust sqlx database query
});
```

Because 95%+ of request processing occurs inside `py.allow_threads`, **JustAPI achieves true multi-core CPU scaling even on Python 3.11 & 3.12**.

## Free-Threading Support (Python 3.13t & 3.14t)

When compiled against Python 3.13t or 3.14t free-threaded builds (`Py_GIL_DISABLED=1`), PyO3 automatically disables GIL synchronization, allowing Python handler execution in `gil_pool.rs` to run fully parallel across all CPU cores.
