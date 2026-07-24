---
title: Testing Guide
description: How to write and run tests for JustAPI — a high-performance FastAPI alternative built in Rust.
keywords: testing guide, pytest, FastAPI alternative, Rust web framework, test automation
---

## Running Tests

```bash
# All Rust tests
cargo test --workspace

# Single crate
cargo test -p justapi-core

# Python tests
pytest

# With uv
uv run pytest

# Miri (unsafe validation)
cargo +nightly miri test -p justapi-core
```

## Writing Rust Tests

Unit tests go at the bottom of the module they test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_db_url() {
        assert_eq!(normalize_db_url("sqlite:///foo.db"), "sqlite:foo.db");
        assert_eq!(normalize_db_url("sqlite:////tmp/foo.db"), "sqlite:/tmp/foo.db");
    }
}
```

## Writing Python Tests

Use the `JustAPITestClient` for in-process testing:

```python
from justapi import JustAPIApp, JustAPITestClient


def test_hello_world():
    app = JustAPIApp()

    @app.get("/hello")
    def hello(request):
        return {"message": "Hello!"}

    client = JustAPITestClient(app)
    response = client.get("/hello")

    assert response.status == 200
    assert response.json() == {"message": "Hello!"}
```

### Testing POST Endpoints

```python
def test_create_item():
    app = JustAPIApp()

    @app.post("/items")
    def create(request):
        data = request.json()
        return {"id": 1, **data}

    client = JustAPITestClient(app)
    response = client.post("/items", body={"name": "Widget", "price": 9.99})

    assert response.status == 200
    assert response.json()["name"] == "Widget"
```

### Testing Errors

```python
def test_404():
    app = JustAPIApp()
    client = JustAPITestClient(app)
    response = client.get("/nonexistent")
    assert response.status == 404
```

## Integration Testing

For integration tests that need a running server:

```python
# tests/test_app.py
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/health")
def health(request):
    return {"status": "ok"}
```

Then use `JustAPITestClient` for end-to-end flow tests.

## Property-Based Testing

Use `proptest` for Rust property-based tests:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_route_roundtrip(path in ".*") {
        // Property: route matching should never panic
        let router = Router::new();
        // ...
    }
}
```

## Test File Layout

```
tests/
├── test_annotations.py
├── test_middleware.py
├── test_responses.py
├── test_multipart.py
├── test_exceptions.py
├── test_db_concurrent.py
└── ...
```

## CI Testing

Every PR runs:

1. `cargo test --workspace`
2. `cargo clippy -- -D warnings`
3. `cargo fmt --check`
4. `cargo miri test -p justapi-core`
5. `pytest` (Python 3.12 + free-threaded 3.14t)
6. `cargo audit`
7. Fuzz tests (7 targets, 60s each)

## See Also

- [Development Setup](/contributing/development-setup/) — Get started
- [Coding Standards](/contributing/coding-standards/) — Code conventions
- [Testing Client API](/api-reference/testing-client/) — `JustAPITestClient` reference
