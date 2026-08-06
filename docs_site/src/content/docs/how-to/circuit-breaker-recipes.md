---
title: Circuit Breaker Recipes
description: Configure circuit breakers, retry policies, and bulkheads for real-world scenarios in JustAPI.
keywords: [JustAPI, circuit breaker, retry, bulkhead, resilience, patterns]
---

## Basic Circuit Breaker

```python
import time
from justapi import JustAPIApp

class CircuitBreaker:
    def __init__(self, failure_threshold=3, recovery_timeout=60):
        self.failure_count = 0
        self.failure_threshold = failure_threshold
        self.recovery_timeout = recovery_timeout
        self.state = "closed"
        self.last_failure_time = None

    def call(self, func, *args, **kwargs):
        if self.state == "open":
            if time.time() - self.last_failure_time > self.recovery_timeout:
                self.state = "half-open"
            else:
                raise Exception("Circuit breaker is open")

        try:
            result = func(*args, **kwargs)
            self.failure_count = 0
            self.state = "closed"
            return result
        except Exception as e:
            self.failure_count += 1
            self.last_failure_time = time.time()
            if self.failure_count >= self.failure_threshold:
                self.state = "open"
            raise

breaker = CircuitBreaker(failure_threshold=3, recovery_timeout=60)

def unreliable_service():
    # Simulate a failing service
    import random
    if random.random() < 0.5:
        raise Exception("Service unavailable")
    return {"status": "ok"}

@app.get("/data")
def get_data():
    return breaker.call(unreliable_service)
```

## Retry with Backoff

```python
import asyncio
from justapi import JustAPIApp

async def retry_with_backoff(func, max_retries=3, base_delay=1):
    for attempt in range(max_retries):
        try:
            return await func()
        except Exception as e:
            if attempt == max_retries - 1:
                raise
            await asyncio.sleep(base_delay * (2 ** attempt))

@app.get("/retry")
async def retry_example():
    async def call_external():
        # Simulate external call
        import random
        if random.random() < 0.7:
            raise Exception("Temporary failure")
        return {"data": "success"}

    return await retry_with_backoff(call_external)
```

## Rate Limiting

```python
import time
from collections import defaultdict

class RateLimiter:
    def __init__(self, max_requests=100, window=60):
        self.max_requests = max_requests
        self.window = window
        self.requests = defaultdict(list)

    def is_allowed(self, client_id):
        now = time.time()
        self.requests[client_id] = [
            t for t in self.requests[client_id] if now - t < self.window
        ]
        if len(self.requests[client_id]) >= self.max_requests:
            return False
        self.requests[client_id].append(now)
        return True

limiter = RateLimiter(max_requests=100, window=60)

@app.get("/api/data")
def rate_limited_data(request):
    client_id = request.client.host
    if not limiter.is_allowed(client_id):
        from justapi import JSONResponse
        return JSONResponse(content={"error": "Rate limit exceeded"}, status_code=429)
    return {"data": "ok"}
```

## See Also

- [Resilience Patterns](/advanced/resilience-patterns/) — full resilience guide
- [Rate Limiting](/tutorials/middleware/) — built-in rate limiting
- [Error Handling](/tutorials/error-handling/) — exception handling
