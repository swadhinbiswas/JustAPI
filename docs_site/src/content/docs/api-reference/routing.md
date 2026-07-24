---
title: Routing API
description: API reference for HTTP routing in JustAPI, the FastAPI alternative — method decorators, route registration, and programmatic add_api_route.
keywords: [routing, fastapi alternative, justapi, http decorators, route registration, radix trie, add_api_route, url_for]
---

## Method Decorators

Each HTTP method decorator registers a route handler in the radix-trie router.

### `@app.get()`

```python
@app.get(
    path: str,
    dependencies: list[Depends] | None = None,
    middlewares: list[Callable] | None = None,
    tags: list[str] | None = None,
    summary: str | None = None,
    description: str | None = None,
    deprecated: bool = False,
    status_code: int | None = None,
    responses: dict | None = None,
    operation_id: str | None = None,
    openapi_extra: dict | None = None,
    name: str | None = None,
    include_in_schema: bool = True,
    native: bool = False,
)
```

### `@app.post()`

```python
@app.post(
    path: str,
    body_schema: type[Schema] | None = None,
    schema: type[Schema] | None = None,
    dependencies: list[Depends] | None = None,
    middlewares: list[Callable] | None = None,
    tags: list[str] | None = None,
    summary: str | None = None,
    description: str | None = None,
    deprecated: bool = False,
    status_code: int | None = None,
    responses: dict | None = None,
    operation_id: str | None = None,
    openapi_extra: dict | None = None,
    name: str | None = None,
    include_in_schema: bool = True,
    native: bool = False,
)
```

### `@app.put()`, `@app.patch()`

Same signature as `@app.post()`.

### `@app.delete()`, `@app.head()`, `@app.options()`, `@app.trace()`

```python
@app.delete(
    path: str,
    dependencies: list[Depends] | None = None,
    ...
)
```

### `@app.websocket()`

```python
@app.websocket(path: str)
```

### `@app.sse()`

```python
@app.sse(path: str)
```

### `@app.route()`

Register a route that responds to multiple HTTP methods:

```python
@app.route("/items", methods=["GET", "POST"])
def items(request):
    if request.method == "GET":
        return list_items()
    return create_item(request)
```

## Programmatic Route Registration

Register routes without decorators (FastAPI parity):

```python
def read_item(request, item_id: int):
    return {"item_id": item_id}

app.add_api_route("/items/{item_id}", read_item, methods=["GET"])
app.add_api_websocket_route("/ws", my_ws_handler)
```

## Route Parameters

| Parameter | Applies To | Type | Description |
|---|---|---|---|
| `path` | All | `str` | URL pattern with `{param}` placeholders |
| `body_schema` | POST, PUT, PATCH | `Schema` subclass | Validate body against this schema in Rust |
| `dependencies` | All | `list[Depends]` | Route-level dependency injection |
| `middlewares` | All | `list[Callable]` | Route-level middleware functions |
| `tags` | All | `list[str]` | OpenAPI operation tags |
| `summary` | All | `str` | OpenAPI operation summary |
| `description` | All | `str` | OpenAPI operation description |
| `deprecated` | All | `bool` | Mark deprecated in OpenAPI |
| `status_code` | All | `int` | Default response status code |
| `responses` | All | `dict` | Additional OpenAPI responses |
| `operation_id` | All | `str` | OpenAPI operation ID |
| `openapi_extra` | All | `dict` | Extra OpenAPI metadata |
| `include_in_schema` | All | `bool` | Exclude from OpenAPI schema |
| `name` | All | `str` | Name for `url_for()` |
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

## Named Routes & URL Building

Use the `name` parameter and `url_for()` to build URLs:

```python
@app.get("/items/{item_id}", name="get_item")
def read_item(request, item_id: int):
    ...

# Later: app.url_for("get_item", item_id=42) -> "/items/42"
```

## Native Fast Path

When `native=True`, the handler validates the body and serializes the response entirely in Rust — zero GIL acquisition:

```python
@app.post("/fast-items", body_schema=ItemSchema, native=True)
def create_item(request):
    return {"status": "ok"}
```

## Route Resolution

Routes are resolved in O(1) time using a radix-trie (`matchit`). The route-lookup cache memoizes repeated lookups for the same path.

## OpenAPI Generation

All registered routes automatically generate OpenAPI 3.1 documentation:

| URL | Description |
|---|---|
| `/docs` | Swagger UI |
| `/redoc` | ReDoc |
| `/scalar` | Scalar API Reference |
| `/openapi.json` | Raw OpenAPI 3.1 spec |

## See Also

- [APIRouter](/api-reference/apirouter/) — Modular route grouping
- [JustAPIApp](/api-reference/justapiapp/) — App configuration reference
- [Native Fast Path](/advanced/native-fast-path/) — Deep dive on Rust-native execution
- [Routing & Sub-routers](/tutorials/routing-subrouters/) — Step-by-step tutorial
