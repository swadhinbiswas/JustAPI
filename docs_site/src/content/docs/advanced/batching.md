---
title: Adaptive Batching
description: Automatically batch multiple requests into a single handler call in JustAPI.
keywords: [JustAPI, batching, adaptive batch, performance, request coalescing]
---

## Basic Usage

Use `@adaptive_batch` to automatically collect and batch incoming requests:

```python
from justapi import JustAPIApp, adaptive_batch

app = JustAPIApp()

@app.post("/predict")
@adaptive_batch(max_size=32, window_ms=10)
def predict_batch(items: list[dict]):
    # Called with up to 32 items collected within 10ms
    results = [process(item) for item in items]
    return results
```

When multiple requests arrive within the `window_ms` window, JustAPI collects them and calls your handler once with all items combined.

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_size` | `int` | `32` | Maximum items per batch |
| `window_ms` | `int` | `10` | Collection window in milliseconds |

## How It Works

1. Request arrives → added to batch queue
2. If queue reaches `max_size` → handler called immediately
3. If `window_ms` elapses → handler called with whatever is queued
4. Response is sent back to each original request

## With Dependencies

```python
from justapi import Security

@app.post("/predict")
@adaptive_batch(max_size=64, window_ms=5)
def predict_batch(
    items: list[dict],
    user: dict = Security(get_current_user),
):
    # Process batch for authenticated user
    return process_batch(items, user["id"])
```

## See Also

- [Resilience Patterns](/advanced/resilience-patterns/) — rate limiting, circuit breakers
- [Performance Tuning](/advanced/performance-tuning/) — optimization
