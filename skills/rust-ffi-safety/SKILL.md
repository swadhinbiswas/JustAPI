---
name: rust-ffi-safety
description: >
  Use when writing or reviewing any PyO3 code that crosses the GIL boundary,
  holds a buffer pointer across an await point, calls into Python from a
  Rust thread not holding the GIL, or implements the buffer protocol for
  zero-copy request views.
---

# Rust FFI Safety

## When to use

- Writing `#[pyfunction]` or `#[pymethods]` that take or return raw pointers.
- Implementing `PyBuffer` protocol to share Rust-owned memory with Python.
- Spawning a `tokio` task that calls back into Python (GIL reacquisition).
- Adding `unsafe` blocks in `justapi-py/src/`.
- Reviewing any code path where a Rust `&[u8]` or `&str` is handed to
  Python without copying.
- Any `Send`/`Sync` boundary involving Python objects.

## Rules

### 1. GIL acquisition discipline

- Use `Python::with_gil(|py| { ... })` for short callbacks.
- For long-running Rust I/O, release the GIL before the `.await` point
  and reacquire it afterward.
- **Never** hold the GIL across an `.await` — doing so blocks the entire
  tokio worker thread and can deadlock the runtime.

### 2. Lifetime rules for zero-copy

- A `&[u8]` handed to Python via the buffer protocol must remain valid
  for the buffer's lifetime.
- Pin the backing allocation (arena, refcounted pool) and never move it
  while Python holds a view.
- Always Python-bind the buffer's lifecycle to a Python-owned wrapper
  (`#[pyclass]`) so Python's GC can track it.

### 3. `// SAFETY:` comments

Every `unsafe` block must explain:
- (a) The invariant being upheld
- (b) Why the invariant holds at this specific call site
- (c) What test would fail if the invariant were violated

### 4. Thread safety

- Python objects are `!Send`. Use `pyo3::PyAny::extract()` to copy data
  out before sending across threads.
- Never hold a Rust `Mutex` (or `RwLock`) across a GIL acquisition —
  this inverts the lock ordering and can deadlock.

### 5. Free-threaded CPython (PEP 703) path

- Feature-gate free-threading code behind `#[cfg(feature = "free-threaded")]`.
- When the GIL is absent, Python objects become `Send` but you must use
  `pyo3`'s atomic reference counting APIs.
- Test both paths in CI when PyO3 free-threading support matures.

## Gotchas / known failure modes

| Failure | Root cause | Mitigation |
|---|---|---|
| Use-after-free in buffer view | Request body buffer freed while Python holds `memoryview` | Bind buffer lifetime to a `#[pyclass]` wrapper |
| GIL deadlock | Two Rust threads each waiting for the other to release GIL while holding a tokio lock | Never hold Rust Mutex across GIL acquisition |
| `!Send` violation | Passing Python object across tokio task boundary | Extract data into Rust types before `tokio::spawn` |
| Silent data corruption | Aliased `&mut` through FFI boundary | Use `PyCell` / `PyRefMut` for mutable access |

## Example: correct GIL handling in async context

```rust
use pyo3::prelude::*;

/// Call a Python handler from an async Rust context.
/// GIL is held only during the Python call, not during I/O.
async fn call_python_handler(body: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    // Do async I/O without the GIL
    let processed = do_rust_io(&body).await?;

    // Acquire GIL only for the Python call
    let result = Python::with_gil(|py| -> PyResult<Vec<u8>> {
        let handler = py.import("app")?.getattr("handle")?;
        let py_bytes = pyo3::types::PyBytes::new(py, &processed);
        let result = handler.call1((py_bytes,))?;
        result.extract::<Vec<u8>>()
    })?;

    Ok(result)
}
```

## References

- PyO3 user guide: https://pyo3.rs/latest/
- PEP 703 (free-threaded CPython): https://peps.python.org/pep-0703/
- `DECISIONS.md` — GIL/concurrency strategy (to be written in Phase 4)
