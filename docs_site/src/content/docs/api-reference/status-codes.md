---
title: Status Codes
description: HTTP and WebSocket status code constants in JustAPI.
keywords: [JustAPI, status codes, HTTP, WebSocket, constants]
---

## HTTP Status Codes

Import from `justapi.status`:

```python
from justapi import status

@app.get("/items")
def list_items():
    if not items:
        return JSONResponse(
            content={"error": "no items"},
            status_code=status.HTTP_204_NO_CONTENT,
        )
    return {"items": items}
```

### 1xx Informational

| Constant | Code | Description |
|----------|------|-------------|
| `HTTP_100_CONTINUE` | 100 | Continue |
| `HTTP_101_SWITCHING_PROTOCOLS` | 101 | Switching Protocols |

### 2xx Success

| Constant | Code | Description |
|----------|------|-------------|
| `HTTP_200_OK` | 200 | OK |
| `HTTP_201_CREATED` | 201 | Created |
| `HTTP_202_ACCEPTED` | 202 | Accepted |
| `HTTP_204_NO_CONTENT` | 204 | No Content |

### 3xx Redirection

| Constant | Code | Description |
|----------|------|-------------|
| `HTTP_301_MOVED_PERMANENTLY` | 301 | Moved Permanently |
| `HTTP_302_FOUND` | 302 | Found |
| `HTTP_304_NOT_MODIFIED` | 304 | Not Modified |

### 4xx Client Errors

| Constant | Code | Description |
|----------|------|-------------|
| `HTTP_400_BAD_REQUEST` | 400 | Bad Request |
| `HTTP_401_UNAUTHORIZED` | 401 | Unauthorized |
| `HTTP_403_FORBIDDEN` | 403 | Forbidden |
| `HTTP_404_NOT_FOUND` | 404 | Not Found |
| `HTTP_405_METHOD_NOT_ALLOWED` | 405 | Method Not Allowed |
| `HTTP_409_CONFLICT` | 409 | Conflict |
| `HTTP_422_UNPROCESSABLE_ENTITY` | 422 | Validation Error |
| `HTTP_429_TOO_MANY_REQUESTS` | 429 | Rate Limited |

### 5xx Server Errors

| Constant | Code | Description |
|----------|------|-------------|
| `HTTP_500_INTERNAL_SERVER_ERROR` | 500 | Internal Server Error |
| `HTTP_502_BAD_GATEWAY` | 502 | Bad Gateway |
| `HTTP_503_SERVICE_UNAVAILABLE` | 503 | Service Unavailable |

## WebSocket Close Codes

| Constant | Code | Description |
|----------|------|-------------|
| `WS_1000_NORMAL_CLOSURE` | 1000 | Normal closure |
| `WS_1001_GOING_AWAY` | 1001 | Endpoint going away |
| `WS_1002_PROTOCOL_ERROR` | 1002 | Protocol error |
| `WS_1003_UNSUPPORTED_DATA` | 1003 | Unsupported data |
| `WS_1008_POLICY_VIOLATION` | 1008 | Policy violation |
| `WS_1011_INTERNAL_ERROR` | 1011 | Internal error |

## See Also

- [Response Status Code](/tutorials/response-status-code/) — setting status codes
- [Handling Errors](/tutorials/error-handling/) — error responses
- [Response Classes](/api-reference/responses/) — response types
