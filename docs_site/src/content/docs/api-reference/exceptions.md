---
title: Exceptions & Error Handling API
description: Reference for HTTPException, validation errors, and exception handlers.
---

## `HTTPException`

Raise to return an HTTP error response:

```python
from justapi import HTTPException

raise HTTPException(
    status_code=404,
    detail="Item not found",
    headers={"X-Error": "not_found"},
)
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `status_code` | `int` | required | HTTP status code |
| `detail` | `str` | `""` | Error message |
| `headers` | `dict` | `None` | Additional response headers |

## `RequestValidationError`

Raised automatically when Pydantic validation fails:

```python
from justapi import RequestValidationError

@app.exception_handler(RequestValidationError)
def handler(request, exc: RequestValidationError):
    return {
        "status": 422,
        "body": {"error": "Validation Failed", "issues": exc.errors()},
    }
```

| Attribute | Type | Description |
|---|---|---|
| `.errors()` | `list` | List of validation error details |

## Exception Handlers

Register custom handlers with `@app.exception_handler()`:

```python
@app.exception_handler(HTTPException)
def custom_handler(request, exc: HTTPException):
    return {
        "status": exc.status_code,
        "body": {"error": exc.detail, "code": exc.status_code},
    }
```

### Precedence

Exception handlers are matched in order:
1. Exact exception class handler
2. Parent class handler
3. `Exception` catch-all handler

## Default Error Codes

| Status | Error | When |
|---|---|---|
| 400 | Bad Request | Malformed request |
| 401 | Unauthorized | Missing/invalid auth |
| 403 | Forbidden | Insufficient permissions |
| 404 | Not Found | Unmatched route |
| 413 | Payload Too Large | Body exceeds `max_body_size` |
| 422 | Unprocessable Entity | Validation failure |
| 429 | Too Many Requests | Rate limit exceeded |
| 500 | Internal Server Error | Unhandled exception |
| 503 | Service Unavailable | Pool saturation |
| 504 | Gateway Timeout | Request timeout |

## Error Response Format

Default error body:

```json
{"detail": "<error message>"}
```

Custom handlers can return any format.

## See Also

- [Error Handling Tutorial](/tutorials/error-handling/) — Usage guide
- [Resilience Patterns](/advanced/resilience-patterns/) — Circuit breakers, rate limiting
- [Production Checklist](/deployment/production-checklist/) — Error handling in production
