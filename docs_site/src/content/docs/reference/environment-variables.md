---
title: Environment Variables
description: Environment variables that control JustAPI behavior — profiling, tracing, background tasks, testing.
keywords: [JustAPI, environment variables, configuration, profiling, tracing]
---

JustAPI reads these environment variables at startup:

## Runtime

| Variable | Default | Description |
|----------|---------|-------------|
| `JUSTAPI_BG_MAX_QUEUE` | `100000` | Maximum number of queued background tasks |
| `JUSTAPI_ENABLE_TRACE` | `false` | Inject OpenTelemetry trace context into Python contextvars |
| `JUSTAPI_PROFILE` | `false` | Enable GIL-path profiler for performance analysis |

## Testing

| Variable | Default | Description |
|----------|---------|-------------|
| `SNAPSHOT_UPDATE` | `false` | Update snapshot files when set to `1`, `true`, or `yes` |

## Examples

```bash
# Enable tracing in production
JUSTAPI_ENABLE_TRACE=true python main.py

# Increase background task queue
JUSTAPI_BG_MAX_QUEUE=500000 python main.py

# Profile GIL contention
JUSTAPI_PROFILE=true python main.py

# Update test snapshots
SNAPSHOT_UPDATE=1 pytest tests/
```

## See Also

- [Configuration Reference](/reference/configuration/) — all config options
- [OpenTelemetry](/observability/opentelemetry/) — tracing setup
- [Background Tasks](/tutorials/background-tasks/) — task execution
