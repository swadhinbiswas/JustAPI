---
title: Request IDs
description: Correlate gateway, application, and error logs with one request ID — a high-performance FastAPI alternative built in Rust.
keywords: request id, correlation id, x-request-id, tracing, FastAPI alternative, Rust web framework, observability
---

Every production incident starts with "which request was that?". A request ID
turns scattered log lines into one traceable story.

## Enable it

```python
from justapi import JustAPIApp

app = JustAPIApp()
app.enable_request_id()                    # x-request-id (default)
# or reuse the header your gateway already sets:
# app.enable_request_id("X-Correlation-ID")
```

## What it does

For each request:

1. **Honours an incoming id** — if the request already carries the header
   (from a load balancer, API gateway, or client) and it is a safe value
   (1–128 printable ASCII characters), that id is used.
2. **Generates one otherwise** — a 128-bit random id as 32 lowercase hex
   characters.
3. **Exposes it to handlers** — the header is injected into the request, so
   `request.headers.get("x-request-id")` always returns it.
4. **Echoes it on the response** — including `401`/`429`/`504` responses
   produced by middleware and CORS preflights, so failures are traceable too.
5. **Logs it** — the structured access log gains `http.request_id`.

## Reading it in a handler

```python
@app.get("/orders/{order_id}")
def get_order(order_id: int, request):
    log.info("fetching order", request_id=request.headers.get("x-request-id"))
    ...
```

Or return it to clients so support can ask for it:

```python
@app.get("/whoami")
def whoami(request):
    return {"request_id": request.headers.get("x-request-id")}
```

## Log output

```json
{
  "timestamp": "2026-10-30T08:36:49.076322Z",
  "level": "INFO",
  "fields": {
    "message": "request completed",
    "http.method": "GET",
    "http.path": "/analytics/summary",
    "http.request_id": "77491bde080d2b7b8f49aeaf87d78bfc",
    "http.status_code": 200,
    "latency_ms": "0.97"
  }
}
```

Grep one id across gateway, app, and worker logs:

```bash
grep 77491bde080d2b7b8f49aeaf87d78bfc /var/log/app.jsonl
```

## Interaction with tracing

If OpenTelemetry is enabled (`justapi.logging.init_otlp_tracing(...)`), keep
both: the request id is the *external* correlation key (safe to show clients,
stable across services that do not propagate trace headers), while the trace
id links spans internally. A common pattern is to log both.

## Notes

- The middleware runs **outermost**, so it also decorates responses from the
  timeout, rate-limit, JWT, and CORS middlewares.
- If you use CORS and want browsers to read the id, add the header to your
  exposed headers: `app.add_cors(..., expose_headers=["x-request-id"])`.
- IDs are validated before use (no control characters, max 128 bytes), so a
  malicious header cannot inject log lines.
