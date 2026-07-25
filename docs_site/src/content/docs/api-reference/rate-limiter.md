---
title: RateLimiter
description: Redis-backed distributed rate limiting with GCRA algorithm in JustAPI.
keywords: [JustAPI, rate limiter, GCRA, Redis, distributed rate limiting]
---

## Create Rate Limiter

```python
from justapi import RateLimiter

# Async — creates Redis connection
limiter = await RateLimiter.new_redis("redis://localhost:6379")
```

## Check Rate Limit

```python
result = await limiter.check_limit(
    key="user:123",        # unique key per user/client
    capacity=100,          # max requests in window
    replenish_rate=10,     # tokens added per second
    tokens=1,              # tokens to consume
)

if result.allowed:
    # Process request
    return {"status": "ok"}
else:
    # Rate limited
    return JSONResponse(
        status_code=429,
        content={"error": "Rate limit exceeded"},
        headers={"Retry-After": str(result.retry_after_ms // 1000)},
    )
```

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `key` | `str` | Unique identifier for the rate limit bucket |
| `capacity` | `int` | Maximum burst size |
| `replenish_rate` | `float` | Tokens added per second |
| `tokens` | `int` | Tokens to consume (default: 1) |

## RateLimitResult

| Property | Type | Description |
|----------|------|-------------|
| `allowed` | `bool` | Whether the request is allowed |
| `retry_after_ms` | `int` | Milliseconds until next token is available |

## Built-in Middleware

JustAPI also provides a built-in rate limiter middleware:

```python
app.add_middleware(
    RateLimiter,
    capacity=100,
    replenish_rate=10,
)
```

## See Also

- [Resilience Patterns](/advanced/resilience-patterns/) — circuit breakers, retry
- [Security](/tutorials/security/first-steps/) — authentication
