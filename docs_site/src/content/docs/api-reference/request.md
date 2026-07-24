---
title: Request Object
description: "API reference for the Request object in JustAPI, the FastAPI alternative — access incoming HTTP request data with zero-copy performance."
keywords: [request object, fastapi alternative, justapi, http request, zero copy, request data]
---

The `Request` object provides zero-copy access to the incoming HTTP request. It's passed as the first parameter to every route handler.

## Request Properties

```python
@app.get("/items/{item_id}")
def handler(request, item_id: int):
    method = request.method       # str: GET, POST, etc.
    path = request.path           # str: /items/42
    url = request.url             # URL object
    headers = request.headers     # Headers object
    query_params = request.query_params  # QueryParams object
    state = request.state         # State object (per-request storage)
```

### `request.method`

The HTTP method as a string: `"GET"`, `"POST"`, `"PUT"`, `"PATCH"`, `"DELETE"`.

### `request.path`

The URL path: `"/items/42"`.

### `request.url`

A `URL` object with:

| Attribute | Type | Description |
|---|---|---|
| `scheme` | `str` | `"http"` or `"https"` |
| `host` | `str` | Hostname |
| `port` | `int` | Port number |
| `path` | `str` | URL path |

### `request.headers`

A `Headers` object providing dict-like access to HTTP headers. All header names are lowercased.

```python
@app.get("/")
def handler(request):
    user_agent = request.headers.get("user-agent")
    content_type = request.headers.get("content-type")
    auth = request.headers.get("authorization")
    return {"ua": user_agent}
```

| Method | Description |
|---|---|
| `.get(key)` | Get header value (case-insensitive) |
| `.items()` | Iterate over `(key, value)` pairs |

### `request.query_params`

A `QueryParams` object providing dict-like access to query string parameters.

```python
@app.get("/search")
def handler(request):
    q = request.query_params.get("q")
    page = request.query_params.get("page", "1")
    return {"query": q, "page": int(page)}
```

| Method | Description |
|---|---|
| `.get(key, default)` | Get query parameter |
| `.items()` | Iterate over `(key, value)` pairs |

### `request.state`

A `State` object for per-request storage. Useful for passing data between middleware and handlers:

```python
@app.middleware("http")
def set_user(request, call_next):
    request.state.user = {"id": 42, "role": "admin"}
    return call_next(request)

@app.get("/profile")
def profile(request):
    user = request.state.user
    return {"user_id": user["id"]}
```

## Request Body Methods

```python
@app.post("/data")
def handler(request):
    # Parse JSON body
    data = request.json()          # dict | list

    # Raw bytes
    body = request.body()          # bytes

    # Parse form data
    form = request.form()          # dict
```

| Method | Returns | Description |
|---|---|---|
| `request.json()` | `dict` or `list` | Parse body as JSON |
| `request.body()` | `bytes` | Raw request body bytes |
| `request.form()` | `dict` | Parse as URL-encoded form |

## Zero-Copy Architecture

The `Request` object uses PyO3's buffer protocol to provide **zero-copy** access to request data:

- Headers and query parameters are parsed in Rust and exposed as read-only memory views
- No data is copied between Rust and Python
- Body reading triggers a one-time copy from the network buffer

## See Also

- [Response Classes](/api-reference/responses/) — Returning responses
- [Dependency Injection](/api-reference/dependency-injection/) — Extracting request components with Depends
- [Zero-GIL Architecture](/advanced/zero-gil-architecture/) — How the GIL is released during request processing
