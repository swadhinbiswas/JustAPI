---
title: Logging Setup
description: Configure logging and tracing in JustAPI — text, JSON, file, and OpenTelemetry.
keywords: [JustAPI, logging, tracing, OpenTelemetry, JSON logging, file logging]
---

## Text Logging

```python
from justapi import init_logging

init_logging(level="info", format="text")
```

## JSON Logging

```python
from justapi import init_json_logging

init_json_logging()
```

Outputs structured JSON to stdout — ideal for production log aggregation.

## File Logging

```python
from justapi import init_file_logging

init_file_logging(path="logs/app.log")
```

JSON logging to a rolling file.

## OpenTelemetry Tracing

```python
from justapi import init_otlp_tracing

init_otlp_tracing(
    endpoint="http://localhost:4317",
    service_name="my-api",
)
```

## Shutdown Tracing

```python
from justapi import shutdown_tracing

# On app shutdown
shutdown_tracing()
```

Flushes all pending spans and shuts down the subscriber.

## Read Trace Context

```python
from justapi.tracing import get_current_trace_id, get_current_span_id

@app.get("/debug")
def debug():
    return {
        "trace_id": get_current_trace_id(),
        "span_id": get_current_span_id(),
    }
```

## See Also

- [Structured Logging](/observability/structured-logging/) — logging overview
- [OpenTelemetry](/observability/opentelemetry/) — tracing overview
- [Environment Variables](/reference/environment-variables/) — JUSTAPI_ENABLE_TRACE
