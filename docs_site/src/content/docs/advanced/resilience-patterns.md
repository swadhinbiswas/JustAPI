---
title: Resilience Patterns
description: Protect your application with circuit breakers, rate limiting, bulkheads, and graceful degradation in JustAPI, the resilient FastAPI alternative.
keywords: JustAPI, FastAPI alternative, resilience, circuit breaker, rate limiting, bulkheads
---

JustAPI incorporates enterprise resilience primitives directly in the Rust engine.

## Circuit Breakers

Automatically stop calling a failing service when errors exceed a threshold:

```python
app.enable_circuit_breaker(
    failure_threshold=5,      # Open after 5 consecutive failures
    reset_timeout_ms=30000,   # Try again after 30 seconds
)
```

States:
- **Closed** — Normal operation, requests pass through
- **Open** — Requests fail fast with 503
- **Half-Open** — Probe request allowed; success → Closed, failure → Open

## Request Coalescing

Deduplicate concurrent identical requests so only one hits the backend:

```python
app.enable_request_coalescing(
    headers=["accept"],  # Consider these headers for dedup keys
)
```

When 100 identical requests arrive simultaneously:
- Only 1 request reaches the handler
- The response is shared across all 100 callers
- Saves backend resources on cacheable/read-only endpoints

## Rate Limiting

### Global Rate Limiter

```python
app.enable_rate_limiter(max_requests=1000, window_seconds=60)
```

### Per-IP Rate Limiter

```python
app.enable_ip_rate_limiter(max_requests=100, window_seconds=60)
```

Exceeding limits returns **429 Too Many Requests** with `Retry-After` header.

## Bulkhead Pattern

Limit concurrent requests to prevent resource exhaustion:

```python
app.enable_bulkhead(max_concurrent=100)
```

When the limit is exceeded, requests receive **503 Service Unavailable**.

## Timeout Middleware

Cancel requests that exceed the time limit:

```python
app.enable_timeout(seconds=30)
```

Returns **504 Gateway Timeout** when exceeded.

## Graceful Degradation

```python
app.enable_fallback_middleware(
    fallback_response={"status": "degraded", "message": "Please try again later"},
)
```

## Graceful Shutdown

JustAPI handles SIGINT/SIGTERM automatically:

1. Stop accepting new connections
2. Drain in-flight requests (with configurable timeout)
3. Call plugin `on_shutdown()` hooks
4. Close database connections
5. Exit cleanly

## Panic Model

In production, Rust panics **abort the process** (not unwind). The supervisor (Docker, systemd, K8s) restarts the process immediately:

```toml
# Cargo.toml
[profile.release]
panic = "abort"
```

This avoids undefined behavior across the PyO3 FFI boundary during panics.

## Configuration Summary

```python
app = JustAPIApp()

# Resilience
app.enable_circuit_breaker(failure_threshold=5, reset_timeout_ms=30000)
app.enable_request_coalescing(headers=["accept"])
app.enable_rate_limiter(max_requests=1000, window_seconds=60)
app.enable_ip_rate_limiter(max_requests=100, window_seconds=60)
app.enable_bulkhead(max_concurrent=100)
app.enable_timeout(seconds=30)
```

## See Also

- [Production Checklist](/deployment/production-checklist/) — Production deployment
- [Security](/security/policy/) — Security policy and OWASP compliance
- [Observability](/observability/metrics-monitoring/) — Monitoring resilience signals
