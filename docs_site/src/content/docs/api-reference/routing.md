---
title: Routing API
description: "API reference for HTTP routing in JustAPI, the FastAPI alternative — method decorators and route registration."
keywords: [routing, fastapi alternative, justapi, http decorators, route registration, radix trie]
---

## Method Decorators

Each HTTP method decorator registers a route handler in the radix-trie router.

### `@app.get()`

```python
@app.get(
    path: str,
    dependencies: list | None = None,
    tags: list[str] | None = None,
)
```

### `@app.post()`

```python
@app.post(
    path: str,
    body_schema: type[Schema] | None = None,
    schema: type[Schema] | None = None,
    dependencies: list | None = None,
    tags: list[str] | None = None,
    native: bool = False,
)
```

### `@app.put()`

```python
@app.put(
    path: str,
    body_schema: type[Schema] | None = None,
    schema: type[Schema] | None = None,
    dependencies: list | None = None,
    tags: list[str] | None = None,
    native: bool = False,
)
```

### `@app.patch()`

```python
@app.patch(
    path: str,
    body_schema: type[Schema] | None = None,
    schema: type[Schema] | None = None,
    dependencies: list | None = None,
    tags: list[str] | None = None,
    native: bool = False,
)
```

### `@app.delete()`

```python
@app.delete(
    path: str,
    dependencies: list | None = None,
    tags: list[str] | None = None,
)
```

## Route Parameters

| Parameter | Applies To | Type | Description |
|---|---|---|---|
| `path` | All | `str` | URL pattern with `{param}` placeholders |
| `body_schema` | POST, PUT, PATCH | `Schema` subclass | Validate body against this schema in Rust |
| `schema` | POST, PUT, PATCH | `Schema` subclass | Alias for `body_schema` |
| `dependencies` | All | `list[Depends]` | Route-level dependency injection |
| `tags` | All | `list[str]` | OpenAPI operation tags |
| `native` | POST, PUT, PATCH | `bool` | Execute entirely in Rust (724k+ RPS) |

## Path Parameter Types

| Type | Example | Valid Input |
|---|---|---|
| `str` | `/hello/{name}` | Any string |
| `int` | `/items/{id}` | `42` |
| `float` | `/price/{val}` | `19.99` |
| `bool` | `/flag/{val}` | `true`, `1`, `yes` |
| `UUID` | `/users/{uid}` | `550e8400-...` |
| `datetime` | `/events/{ts}` | `2026-07-24T12:00:00Z` |

## Native Fast Path

When `native=True`, the handler executes entirely in Rust:

- Request body is validated against the schema in Rust
- The handler runs as a Rust callback
- The response is serialized in Rust
- Zero GIL acquisition for the entire request lifecycle

```python
@app.post("/fast-items", body_schema=ItemSchema, native=True)
def create_item(request):
    return {"status": "ok"}
```

## Route Resolution

Routes are resolved in O(1) time using a radix-trie (matchit). The route-lookup cache memoizes repeated lookups for the same path.

## OpenAPI Generation

All registered routes automatically generate OpenAPI 3.1 documentation available at `/openapi.json`, `/docs` (Swagger UI), `/redoc` (ReDoc), and `/scalar` (Scalar UI).

## See Also

- [APIRouter](/api-reference/apirouter/) — Modular route grouping
- [JustAPIApp](/api-reference/justapiapp/) — App configuration reference
- [Native Fast Path](/advanced/native-fast-path/) — Deep dive on Rust-native execution
