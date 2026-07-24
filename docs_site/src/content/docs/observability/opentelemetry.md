---
title: OpenTelemetry Tracing
description: Distributed tracing with OpenTelemetry and Jaeger for JustAPI — a high-performance FastAPI alternative built in Rust.
keywords: OpenTelemetry, distributed tracing, Jaeger, FastAPI alternative, Rust web framework
---

JustAPI includes built-in OpenTelemetry tracing with OTLP gRPC export.

## Enabling Tracing

```python
from justapi.tracing import init_otlp_tracing

init_otlp_tracing(
    endpoint="http://localhost:4317",
    service_name="my-justapi-service",
)
```

## Jaeger Stack

Generated projects include `docker-compose.otel.yml`:

```yaml
services:
  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # Jaeger UI
      - "4318:4318"    # OTLP HTTP
      - "4317:4317"    # OTLP gRPC
```

```bash
docker compose -f docker-compose.otel.yml up -d
```

Access the Jaeger UI at `http://localhost:16686`.

## Trace Spans

Each request generates spans:

| Span | Description |
|---|---|
| `request` | Full HTTP request lifecycle |
| `handler.execute` | Python handler execution |
| `middleware.*` | Each middleware in the chain |
| `db.query` | Database query execution |
| `db.execute` | Database write execution |

## Context Propagation

Trace context propagates across the Python↔Rust boundary using thread-local variables. No manual instrumentation is needed.

## Environment Variables

| Variable | Description |
|---|---|
| `OTEL_SERVICE_NAME` | Service name for traces |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP collector endpoint |
| `RUST_LOG` | Log level (set to `info` for production) |

## See Also

- [Metrics & Monitoring](/observability/metrics-monitoring/) — Prometheus metrics
- [Structured Logging](/observability/structured-logging/) — Log configuration
- [Health Checks](/observability/health-checks/) — Health endpoints
