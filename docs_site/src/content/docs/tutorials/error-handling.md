---
title: Error Handling
description: Handle errors gracefully in JustAPI with HTTPException, custom exception handlers, and validation error responses — a FastAPI alternative.
keywords: error handling, HTTPException, exception handlers, JustAPI, FastAPI alternative, validation errors
---

JustAPI provides a structured error-handling system that lets you return consistent error responses across your API.

## Raising HTTP Errors

Use `HTTPException` to return error responses from any handler or dependency:

```python
from justapi import JustAPIApp, HTTPException

app = JustAPIApp()


@app.get("/items/{item_id}")
def read_item(request, item_id: int):
    if item_id == 0:
        raise HTTPException(status_code=400, detail="Item ID must be positive")
    if item_id == 42:
        raise HTTPException(
            status_code=418,
            detail="I'm a teapot",
            headers={"X-Error": "teapot"},
        )
    return {"item_id": item_id}
```

```bash
curl http://127.0.0.1:8000/items/0
# Output: {"detail":"Item ID must be positive"}

curl http://127.0.0.1:8000/items/42
# Output: {"detail":"I'm a teapot"}
```

### `HTTPException` Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `status_code` | int | required | HTTP status code |
| `detail` | str | required | Error message |
| `headers` | dict | `None` | Additional response headers |

## Custom Exception Handlers

Override the default error format for specific exception types:

```python
from justapi import RequestValidationError


@app.exception_handler(HTTPException)
def http_exception_handler(request, exc: HTTPException):
    return {
        "status": exc.status_code,
        "body": {
            "error": "Request Failed",
            "code": exc.status_code,
            "message": exc.detail,
        },
    }


@app.exception_handler(RequestValidationError)
def validation_handler(request, exc: RequestValidationError):
    return {
        "status": 422,
        "body": {
            "error": "Validation Failed",
            "issues": exc.errors(),
        },
    }
```

## Validation Errors

When a request fails type validation (e.g., invalid path parameter type), JustAPI returns a structured 422 response:

```bash
curl http://127.0.0.1:8000/items/abc
# Output: {"detail":"validation error"}
```

With a custom handler, you can expand this to include field-level details.

## Error Response Format

By default, all error responses follow the format:

```json
{
  "detail": "<error message>"
}
```

This applies to:
- **400** — Bad request
- **401** — Unauthorized
- **403** — Forbidden
- **404** — Not found (unmatched route)
- **413** — Payload too large
- **422** — Validation error
- **429** — Rate limit exceeded
- **500** — Internal server error
- **503** — Service unavailable (pool saturation)
- **504** — Gateway timeout

## Handling Errors in Dependencies

Dependencies can raise `HTTPException` to reject requests:

```python
from justapi import Depends, HTTPException, Header


def require_admin(authorization: str = Header(...)):
    if authorization != "Bearer admin-token":
        raise HTTPException(status_code=403, detail="Admin access required")


@app.get("/admin")
def admin_panel(request, admin: None = Depends(require_admin)):
    return {"secret": "classified"}
```

## Global Error Catchers

You can register a catch-all exception handler:

```python
@app.exception_handler(Exception)
def global_handler(request, exc: Exception):
    return {
        "status": 500,
        "body": {"detail": "An unexpected error occurred"},
    }
```

In production, JustAPI's `panic = "abort"` mode ensures that unexpected Rust panics abort the process cleanly, and the supervisor restarts it. See [Production Checklist](/deployment/production-checklist/).

## Next Steps

- [Middleware](/tutorials/middleware/) — Request/response interception
- [File Uploads](/tutorials/file-uploads/) — Handle multipart form data
- [Production Checklist](/deployment/production-checklist/) — Error handling in production
