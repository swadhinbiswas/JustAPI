---
title: PyO3 & FFI Safety
description: How JustAPI safely crosses the Rust-Python boundary without sacrificing performance — a key advantage over other FastAPI alternatives.
keywords: JustAPI, FastAPI alternative, PyO3, FFI safety, Rust-Python boundary, performance
---

## The Challenge

The boundary between Rust and Python is the most critical part of the JustAPI runtime. Getting it wrong can cause use-after-free errors, GIL deadlocks, or undefined behavior.

## GIL Acquisition Discipline

### Rust → Python Calls

When Rust needs to call Python code, it must hold the GIL:

```rust
Python::with_gil(|py| {
    let handler = py.import("app")?.getattr("handle")?;
    let result = handler.call1((request,))?;
    result.extract::<Vec<u8>>()
})
```

### Python → Rust Calls

When Python calls into Rust (via PyO3 bindings), the GIL is already held:

```rust
#[pyfunction]
fn process(data: Vec<u8>) -> PyResult<Vec<u8>> {
    // GIL is held — safe to call Python APIs
}
```

### GIL Release During I/O

The GIL is released during all I/O operations using `py.allow_threads`:

```rust
py.allow_threads(|| {
    // Network I/O, DB queries, JSON parsing
    // No GIL — runs fully parallel
    let result = fetch_from_database(&query)?;
    Ok(result)
})
```

## Zero-Copy Buffer Protocol

JustAPI uses PyO3's buffer protocol to expose Rust memory to Python without copying:

```rust
// Rust side: expose request body as a memory view
impl PyBufferProtocol for Request {
    fn get_buffer(&self, view: &mut PyBufferView) -> PyResult<()> {
        view.fill_with_slice(&self.body)
    }
}
```

### Safety Rules

1. **Bind to `#[pyclass]`**: Rust buffers exposed to Python must be owned by a `#[pyclass]` that outlives the Python reference
2. **Don't hold across await**: Never hold a Python buffer reference across a `.await` point
3. **Extract before spawn**: Extract all data from Python objects before `tokio::spawn`

## Common Pitfalls

| Failure | Root Cause | Mitigation |
|---|---|---|
| Use-after-free | Buffer freed while Python holds view | Bind to `#[pyclass]` |
| GIL deadlock | Two threads each waiting for GIL | Never hold Rust `Mutex` across GIL |
| `!Send` violation | Python object across task boundary | Extract before `tokio::spawn` |
| Aliased `&mut` | Through FFI | Use `PyCell`/`PyRefMut` |

## The `unsafe` Policy

Every `unsafe` block in `justapi-py` must have a `// SAFETY:` comment:

```rust
// SAFETY: The GIL is held (guaranteed by `Python::with_gil`),
// so the pointer to `py_obj` is valid and won't be moved.
let ptr = py_obj.as_ptr();
```

## Free-Threaded Python (PEP 703)

When compiled for Python 3.13t/3.14t:

```rust
#[cfg(feature = "free-threaded")]
// GIL synchronization is disabled — Python handlers run truly parallel
```

## See Also

- [Zero-GIL Architecture](/advanced/zero-gil-architecture/) — Concurrency model
- [Rust Core Deep Dive](/advanced/rust-core-deep-dive/) — Runtime internals
- [Coding Standards](/contributing/coding-standards/) — unsafe policy and conventions
